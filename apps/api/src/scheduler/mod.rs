pub mod model;
pub mod window;

use std::time::Duration;

use chrono::{NaiveDate, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    analysis::{AnalysisConfig, AnalysisMetrics, AnalysisRun, AnalysisStatus},
    auth::{
        decrypt_token, encrypt_token,
        refresh::{RefreshError, refresh_google_access_token},
    },
    http::{AppState, execute_analysis, mark_run_failed},
    report::{
        ReportMailer, ResendMailer,
        template::{ReviewItem, build_failure_email, build_report_email},
    },
    scheduler::{
        model::{ScheduleConfig, ScheduleRunStatus, ScheduleState},
        window::AnalysisWindow,
    },
};

/// Si un claim `Running` quedó colgado (reinicio a mitad de análisis), se
/// considera obsoleto después de este tiempo y un nuevo tick puede reintentar.
const CLAIM_TTL_MINUTES: i64 = 60;
const POLL_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum ScheduledOutcome {
    Skipped {
        #[serde(skip_serializing_if = "Option::is_none")]
        user_email: Option<String>,
        reason: String,
    },
    Completed {
        user_email: String,
        run_id: String,
        email_sent: bool,
    },
    Failed {
        user_email: String,
        error: String,
        notice_sent: bool,
    },
}

/// Núcleo compartido por el loop interno y el endpoint /internal/scheduled-analysis.
/// `as_of_date` es la fecha local (America/Santiago) en la que ocurre el tick.
pub async fn run_scheduled_analysis(
    state: &AppState,
    mailer: &dyn ReportMailer,
    as_of_date: NaiveDate,
) -> anyhow::Result<Vec<ScheduledOutcome>> {
    let Some(window) = window::analysis_window_for(as_of_date) else {
        return Ok(vec![ScheduledOutcome::Skipped {
            user_email: None,
            reason: "fin de semana: sin ventana de análisis".to_string(),
        }]);
    };
    let configs = load_or_seed_configs(state).await?;
    if configs.is_empty() {
        return Ok(vec![ScheduledOutcome::Skipped {
            user_email: None,
            reason:
                "sin configuración programada (scheduleConfigs vacío y sin SCHEDULE_USER_EMAIL)"
                    .to_string(),
        }]);
    }
    let mut outcomes = Vec::new();
    for config in configs {
        if !config.enabled {
            outcomes.push(ScheduledOutcome::Skipped {
                user_email: Some(config.user_email),
                reason: "configuración deshabilitada".to_string(),
            });
            continue;
        }
        outcomes.push(run_for_user(state, mailer, &config, &window).await);
    }
    Ok(outcomes)
}

/// Loop interno: despierta cada minuto y dispara una vez por día hábil desde
/// las 08:00 de America/Santiago. La idempotencia real vive en ScheduleState,
/// el memo en memoria solo evita relecturas de Firestore durante el día.
pub fn spawn(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mailer = match ResendMailer::from_config(&state.config.report) {
            Ok(mailer) => mailer,
            Err(error) => {
                tracing::error!(
                    ?error,
                    "scheduler interno desactivado: mailer mal configurado"
                );
                return;
            }
        };
        tracing::info!("scheduler interno activo (lunes a viernes 08:00 America/Santiago)");
        let mut last_attempt_date: Option<NaiveDate> = None;
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            let now_scl = Utc::now().with_timezone(&window::SCL);
            if !window::is_fire_time(now_scl) {
                continue;
            }
            let today = now_scl.date_naive();
            if last_attempt_date == Some(today) {
                continue;
            }
            match run_scheduled_analysis(&state, &mailer, today).await {
                Ok(outcomes) => {
                    let summary = serde_json::to_string(&outcomes).unwrap_or_default();
                    tracing::info!(%summary, "tick del análisis programado completado");
                    last_attempt_date = Some(today);
                }
                Err(error) => {
                    // Error duro antes de tocar correo/estado (p. ej. Firestore
                    // caído): no memorizar el día para reintentar al minuto.
                    tracing::error!(?error, "tick del análisis programado falló");
                }
            }
        }
    })
}

async fn load_or_seed_configs(state: &AppState) -> anyhow::Result<Vec<ScheduleConfig>> {
    let configs = state.storage.list_schedule_configs().await?;
    if !configs.is_empty() {
        return Ok(configs);
    }
    let Some(email) = &state.config.scheduler.seed_user_email else {
        return Ok(Vec::new());
    };
    let seeded = ScheduleConfig {
        user_email: email.clone(),
        enabled: true,
        recipients: state.config.report.to_emails.clone(),
        internal_domains: state.config.scheduler.seed_internal_domains.clone(),
        ignored_senders: Vec::new(),
        ignored_domains: Vec::new(),
        ignored_keywords: Vec::new(),
        timezone: "America/Santiago".to_string(),
        gmail_max_threads: state.config.scheduler.seed_gmail_max_threads,
        updated_at: Utc::now(),
    };
    state.storage.upsert_schedule_config(&seeded).await?;
    tracing::info!(user_email = %seeded.user_email, "scheduleConfig sembrado desde variables de entorno");
    Ok(vec![seeded])
}

async fn run_for_user(
    state: &AppState,
    mailer: &dyn ReportMailer,
    config: &ScheduleConfig,
    window: &AnalysisWindow,
) -> ScheduledOutcome {
    match check_idempotency(state, config, window).await {
        Ok(Some(reason)) => {
            return ScheduledOutcome::Skipped {
                user_email: Some(config.user_email.clone()),
                reason,
            };
        }
        Ok(None) => {}
        Err(error) => {
            return ScheduledOutcome::Failed {
                user_email: config.user_email.clone(),
                error: format!("no se pudo registrar el estado del análisis: {error}"),
                notice_sent: false,
            };
        }
    }
    match analyze_and_report(state, mailer, config, window).await {
        Ok(outcome) => outcome,
        Err(error) => {
            let message = error.to_string();
            store_state(
                state,
                config,
                window,
                ScheduleRunStatus::Failed,
                None,
                false,
                Some(message.clone()),
            )
            .await;
            let notice_sent = send_failure_notice(state, mailer, config, window, &message).await;
            ScheduledOutcome::Failed {
                user_email: config.user_email.clone(),
                error: message,
                notice_sent,
            }
        }
    }
}

/// Devuelve Some(motivo) si la ventana ya está cubierta; si no, escribe el
/// claim `Running`. Lectura-verificación-escritura sin transacción: suficiente
/// para una instancia única; con múltiples instancias el hardening sería la
/// precondición `currentDocument` de Firestore.
async fn check_idempotency(
    state: &AppState,
    config: &ScheduleConfig,
    window: &AnalysisWindow,
) -> anyhow::Result<Option<String>> {
    if let Some(existing) = state.storage.get_schedule_state(&config.user_email).await? {
        let same_window = existing.window_date_from == window.date_from
            && existing.window_date_to == window.date_to;
        if same_window {
            match existing.status {
                ScheduleRunStatus::Completed => {
                    return Ok(Some("ventana ya analizada".to_string()));
                }
                ScheduleRunStatus::Running
                    if Utc::now() - existing.started_at
                        < chrono::Duration::minutes(CLAIM_TTL_MINUTES) =>
                {
                    return Ok(Some("análisis en curso".to_string()));
                }
                _ => {}
            }
        }
    }
    let now = Utc::now();
    state
        .storage
        .upsert_schedule_state(&ScheduleState {
            user_email: config.user_email.clone(),
            window_date_from: window.date_from.clone(),
            window_date_to: window.date_to.clone(),
            status: ScheduleRunStatus::Running,
            run_id: None,
            email_sent: false,
            error_message: None,
            started_at: now,
            updated_at: now,
        })
        .await?;
    Ok(None)
}

async fn analyze_and_report(
    state: &AppState,
    mailer: &dyn ReportMailer,
    config: &ScheduleConfig,
    window: &AnalysisWindow,
) -> anyhow::Result<ScheduledOutcome> {
    let access_token = fresh_access_token(state, config).await?;
    let run = create_scheduled_run(state, config, window).await?;

    let mut exec_state = state.clone();
    if let Some(max_threads) = config.gmail_max_threads {
        exec_state.config.google.gmail_max_threads = max_threads;
    }
    if let Err(error) = execute_analysis(exec_state, run.id.clone(), access_token).await {
        mark_run_failed(state, &run.id, &error).await;
        return Err(error.context("el análisis programado falló"));
    }

    let run = state
        .storage
        .get_analysis_run(&run.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("el run programado desapareció del storage"))?;
    let review_items = collect_review_items(state, &run.id).await?;
    let email = build_report_email(&run, &review_items, &state.config.web_base_url);

    let recipients = recipients_for(state, config);
    let send_result = if recipients.is_empty() {
        Err(anyhow::anyhow!(
            "sin destinatarios: define recipients en scheduleConfigs o REPORT_TO_EMAIL"
        ))
    } else {
        mailer.send(&recipients, &email.subject, &email.html).await
    };
    let (email_sent, error_message) = match send_result {
        Ok(provider_id) => {
            tracing::info!(%provider_id, run_id = %run.id, "reporte programado enviado");
            (true, None)
        }
        Err(error) => {
            tracing::error!(?error, run_id = %run.id, "no se pudo enviar el reporte programado");
            (false, Some(format!("el envío del reporte falló: {error}")))
        }
    };
    // El análisis ya corrió: el estado queda Completed aunque el correo falle,
    // para que un reintento del scheduler no duplique la ventana.
    store_state(
        state,
        config,
        window,
        ScheduleRunStatus::Completed,
        Some(run.id.clone()),
        email_sent,
        error_message,
    )
    .await;
    Ok(ScheduledOutcome::Completed {
        user_email: config.user_email.clone(),
        run_id: run.id,
        email_sent,
    })
}

async fn fresh_access_token(state: &AppState, config: &ScheduleConfig) -> anyhow::Result<String> {
    let session = state
        .storage
        .find_latest_session_with_refresh_token(&config.user_email)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no hay una sesión con refresh token para {}; inicia sesión de nuevo en la aplicación",
                config.user_email
            )
        })?;
    let encrypted_refresh = session
        .refresh_token_encrypted
        .clone()
        .expect("session filtered by refresh token presence");
    let refresh_token = decrypt_token(&encrypted_refresh, &state.config.encryption_key)?;
    let token = refresh_google_access_token(&state.http, &state.config.google, &refresh_token)
        .await
        .map_err(|error| match error {
            RefreshError::InvalidGrant => anyhow::anyhow!(
                "Google rechazó el refresh token; vuelve a iniciar sesión en la aplicación para renovar el acceso a Gmail"
            ),
            RefreshError::Other(inner) => inner,
        })?;

    let mut updated = session;
    updated.access_token_encrypted =
        encrypt_token(&token.access_token, &state.config.encryption_key)?;
    if let Some(rotated) = &token.refresh_token {
        updated.refresh_token_encrypted =
            Some(encrypt_token(rotated, &state.config.encryption_key)?);
    }
    updated.updated_at = Utc::now();
    state.storage.upsert_user_session(&updated).await?;
    Ok(token.access_token)
}

async fn create_scheduled_run(
    state: &AppState,
    config: &ScheduleConfig,
    window: &AnalysisWindow,
) -> anyhow::Result<AnalysisRun> {
    let run = AnalysisRun {
        id: Uuid::new_v4().to_string(),
        user_email: config.user_email.clone(),
        config: AnalysisConfig {
            date_from: window.date_from.clone(),
            date_to: window.date_to.clone(),
            time_from: "00:00".to_string(),
            time_to: "23:59".to_string(),
            timezone: config.timezone.clone(),
            internal_domains: config.internal_domains.clone(),
            ignored_senders: config.ignored_senders.clone(),
            ignored_domains: config.ignored_domains.clone(),
            ignored_keywords: config.ignored_keywords.clone(),
        },
        status: AnalysisStatus::Running,
        progress_message: "Análisis programado iniciando".to_string(),
        processed_threads: 0,
        total_candidate_threads: 0,
        metrics: AnalysisMetrics::default(),
        created_at: Utc::now(),
        completed_at: None,
        error_message: None,
    };
    state.storage.create_analysis_run(&run).await?;
    Ok(run)
}

async fn collect_review_items(state: &AppState, run_id: &str) -> anyhow::Result<Vec<ReviewItem>> {
    let threads = state.storage.list_threads(run_id).await?;
    let mut items = Vec::new();
    for thread in threads
        .into_iter()
        .filter(|thread| thread.manual_review_required)
    {
        let messages = state.storage.list_messages(run_id, &thread.id).await?;
        let from_email = messages
            .iter()
            .min_by_key(|message| message.date)
            .map(|message| message.from_email.clone())
            .unwrap_or_default();
        items.push(ReviewItem {
            subject: thread.subject,
            from_email,
            reason: thread
                .reasons
                .last()
                .cloned()
                .unwrap_or_else(|| "Revisión manual requerida".to_string()),
        });
    }
    Ok(items)
}

fn recipients_for(state: &AppState, config: &ScheduleConfig) -> Vec<String> {
    if config.recipients.is_empty() {
        state.config.report.to_emails.clone()
    } else {
        config.recipients.clone()
    }
}

async fn send_failure_notice(
    state: &AppState,
    mailer: &dyn ReportMailer,
    config: &ScheduleConfig,
    window: &AnalysisWindow,
    reason: &str,
) -> bool {
    let recipients = recipients_for(state, config);
    if recipients.is_empty() {
        return false;
    }
    let email = build_failure_email(
        &window.date_from,
        &window.date_to,
        reason,
        &state.config.web_base_url,
    );
    match mailer.send(&recipients, &email.subject, &email.html).await {
        Ok(_) => true,
        Err(error) => {
            tracing::error!(?error, "no se pudo enviar el aviso de fallo");
            false
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn store_state(
    state: &AppState,
    config: &ScheduleConfig,
    window: &AnalysisWindow,
    status: ScheduleRunStatus,
    run_id: Option<String>,
    email_sent: bool,
    error_message: Option<String>,
) {
    let now = Utc::now();
    let started_at = state
        .storage
        .get_schedule_state(&config.user_email)
        .await
        .ok()
        .flatten()
        .map(|existing| existing.started_at)
        .unwrap_or(now);
    let result = state
        .storage
        .upsert_schedule_state(&ScheduleState {
            user_email: config.user_email.clone(),
            window_date_from: window.date_from.clone(),
            window_date_to: window.date_to.clone(),
            status,
            run_id,
            email_sent,
            error_message,
            started_at,
            updated_at: now,
        })
        .await;
    if let Err(error) = result {
        tracing::error!(
            ?error,
            "no se pudo guardar el estado del análisis programado"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use chrono::Duration as ChronoDuration;
    use tokio::sync::Mutex;

    use super::*;
    use crate::{
        auth::UserSession,
        config::{AppConfig, test_app_config},
        storage::{MemoryStorage, StorageRepository},
    };

    #[derive(Default)]
    struct FakeMailer {
        sent: Mutex<Vec<(Vec<String>, String)>>,
    }

    #[async_trait]
    impl ReportMailer for FakeMailer {
        async fn send(&self, to: &[String], subject: &str, _html: &str) -> anyhow::Result<String> {
            self.sent
                .lock()
                .await
                .push((to.to_vec(), subject.to_string()));
            Ok("fake-id".to_string())
        }
    }

    fn test_config(seed_email: Option<&str>) -> AppConfig {
        let mut config = test_app_config();
        config.scheduler.seed_user_email = seed_email.map(ToOwned::to_owned);
        config
    }

    fn app_state(seed_email: Option<&str>) -> (AppState, Arc<MemoryStorage>) {
        let storage = Arc::new(MemoryStorage::default());
        let state = AppState::new(test_config(seed_email), storage.clone());
        (state, storage)
    }

    fn schedule_config(email: &str) -> ScheduleConfig {
        ScheduleConfig {
            user_email: email.to_string(),
            enabled: true,
            recipients: vec!["admin@x.cl".to_string()],
            internal_domains: vec![],
            ignored_senders: vec![],
            ignored_domains: vec![],
            ignored_keywords: vec![],
            timezone: "America/Santiago".to_string(),
            gmail_max_threads: None,
            updated_at: Utc::now(),
        }
    }

    fn schedule_state(
        email: &str,
        from: &str,
        to: &str,
        status: ScheduleRunStatus,
        started_minutes_ago: i64,
    ) -> ScheduleState {
        let started = Utc::now() - ChronoDuration::minutes(started_minutes_ago);
        ScheduleState {
            user_email: email.to_string(),
            window_date_from: from.to_string(),
            window_date_to: to.to_string(),
            status,
            run_id: None,
            email_sent: false,
            error_message: None,
            started_at: started,
            updated_at: started,
        }
    }

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn skip_reason(outcomes: &[ScheduledOutcome]) -> &str {
        match &outcomes[0] {
            ScheduledOutcome::Skipped { reason, .. } => reason,
            other => panic!("expected Skipped, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn weekend_tick_is_skipped() {
        let (state, _) = app_state(None);
        let mailer = FakeMailer::default();
        let outcomes = run_scheduled_analysis(&state, &mailer, date("2026-06-13"))
            .await
            .unwrap();
        assert!(skip_reason(&outcomes).contains("fin de semana"));
    }

    #[tokio::test]
    async fn missing_config_without_seed_is_skipped() {
        let (state, _) = app_state(None);
        let mailer = FakeMailer::default();
        let outcomes = run_scheduled_analysis(&state, &mailer, date("2026-06-12"))
            .await
            .unwrap();
        assert!(skip_reason(&outcomes).contains("sin configuración"));
    }

    #[tokio::test]
    async fn seed_env_creates_config_and_missing_session_fails_with_notice() {
        let (state, storage) = app_state(Some("a@x.cl"));
        let mailer = FakeMailer::default();
        let outcomes = run_scheduled_analysis(&state, &mailer, date("2026-06-12"))
            .await
            .unwrap();

        let configs = storage.list_schedule_configs().await.unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].internal_domains, vec!["x.cl"]);
        assert_eq!(configs[0].recipients, vec!["admin@x.cl"]);

        match &outcomes[0] {
            ScheduledOutcome::Failed {
                user_email,
                error,
                notice_sent,
            } => {
                assert_eq!(user_email, "a@x.cl");
                assert!(error.contains("refresh token"));
                assert!(notice_sent);
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        let sent = mailer.sent.lock().await;
        assert_eq!(sent.len(), 1);
        assert!(sent[0].1.contains("el análisis falló"));

        let saved = storage
            .get_schedule_state("a@x.cl")
            .await
            .unwrap()
            .expect("state expected");
        assert_eq!(saved.status, ScheduleRunStatus::Failed);
    }

    #[tokio::test]
    async fn completed_window_is_not_repeated() {
        let (state, storage) = app_state(None);
        storage
            .upsert_schedule_config(&schedule_config("a@x.cl"))
            .await
            .unwrap();
        storage
            .upsert_schedule_state(&schedule_state(
                "a@x.cl",
                "2026-06-11",
                "2026-06-11",
                ScheduleRunStatus::Completed,
                10,
            ))
            .await
            .unwrap();

        let mailer = FakeMailer::default();
        let outcomes = run_scheduled_analysis(&state, &mailer, date("2026-06-12"))
            .await
            .unwrap();
        assert!(skip_reason(&outcomes).contains("ya analizada"));
        assert!(mailer.sent.lock().await.is_empty());
    }

    #[tokio::test]
    async fn fresh_running_claim_is_skipped_but_stale_claim_retries() {
        let (state, storage) = app_state(None);
        storage
            .upsert_schedule_config(&schedule_config("a@x.cl"))
            .await
            .unwrap();
        storage
            .upsert_schedule_state(&schedule_state(
                "a@x.cl",
                "2026-06-11",
                "2026-06-11",
                ScheduleRunStatus::Running,
                5,
            ))
            .await
            .unwrap();
        let mailer = FakeMailer::default();
        let outcomes = run_scheduled_analysis(&state, &mailer, date("2026-06-12"))
            .await
            .unwrap();
        assert!(skip_reason(&outcomes).contains("en curso"));

        storage
            .upsert_schedule_state(&schedule_state(
                "a@x.cl",
                "2026-06-11",
                "2026-06-11",
                ScheduleRunStatus::Running,
                CLAIM_TTL_MINUTES + 30,
            ))
            .await
            .unwrap();
        let outcomes = run_scheduled_analysis(&state, &mailer, date("2026-06-12"))
            .await
            .unwrap();
        // Sin sesión disponible el reintento falla, pero ya no se salta.
        assert!(matches!(outcomes[0], ScheduledOutcome::Failed { .. }));
    }

    #[tokio::test]
    async fn disabled_config_is_skipped() {
        let (state, storage) = app_state(None);
        let mut config = schedule_config("a@x.cl");
        config.enabled = false;
        storage.upsert_schedule_config(&config).await.unwrap();

        let mailer = FakeMailer::default();
        let outcomes = run_scheduled_analysis(&state, &mailer, date("2026-06-12"))
            .await
            .unwrap();
        assert!(skip_reason(&outcomes).contains("deshabilitada"));
    }

    #[tokio::test]
    async fn failure_notice_uses_config_recipients_over_fallback() {
        let (state, storage) = app_state(None);
        let mut config = schedule_config("a@x.cl");
        config.recipients = vec!["jefa@x.cl".to_string()];
        storage.upsert_schedule_config(&config).await.unwrap();
        // Sesión sin refresh token: fuerza el fallo después del claim.
        storage
            .upsert_user_session(&UserSession {
                id: "s1".to_string(),
                google_account_email: "a@x.cl".to_string(),
                access_token_encrypted: "x".to_string(),
                refresh_token_encrypted: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .unwrap();

        let mailer = FakeMailer::default();
        let outcomes = run_scheduled_analysis(&state, &mailer, date("2026-06-12"))
            .await
            .unwrap();
        assert!(matches!(outcomes[0], ScheduledOutcome::Failed { .. }));
        let sent = mailer.sent.lock().await;
        assert_eq!(sent[0].0, vec!["jefa@x.cl".to_string()]);
    }
}
