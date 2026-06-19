pub mod model;
pub mod window;

use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    analysis::{AnalysisConfig, AnalysisMetrics, AnalysisRun, AnalysisStatus},
    auth::{
        decrypt_token, encrypt_token,
        refresh::{RefreshError, refresh_google_access_token},
    },
    http::{AppState, execute_analysis, mark_run_failed},
    policies::{ReportMode, retention_expires_at, setup_state},
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
    run_configs_for_window(state, mailer, configs, &window).await
}

pub async fn run_due_scheduled_analysis(
    state: &AppState,
    mailer: &dyn ReportMailer,
    now_utc: DateTime<Utc>,
) -> anyhow::Result<Vec<ScheduledOutcome>> {
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
        let Some(window) = window::due_window_for(now_utc, &config.timezone) else {
            outcomes.push(ScheduledOutcome::Skipped {
                user_email: Some(config.user_email),
                reason: "fuera de horario programado".to_string(),
            });
            continue;
        };
        outcomes.push(run_for_user(state, mailer, &config, &window).await);
    }
    Ok(outcomes)
}

async fn run_configs_for_window(
    state: &AppState,
    mailer: &dyn ReportMailer,
    configs: Vec<ScheduleConfig>,
    window: &AnalysisWindow,
) -> anyhow::Result<Vec<ScheduledOutcome>> {
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
        outcomes.push(run_for_user(state, mailer, &config, window).await);
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
        tracing::info!("scheduler interno activo (preset weekdays_08_local por timezone)");
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            match run_due_scheduled_analysis(&state, &mailer, Utc::now()).await {
                Ok(outcomes) => {
                    if outcomes.iter().any(|outcome| {
                        !matches!(
                            outcome,
                            ScheduledOutcome::Skipped { reason, .. }
                                if reason == "fuera de horario programado"
                        )
                    }) {
                        let summary = serde_json::to_string(&outcomes).unwrap_or_default();
                        tracing::info!(%summary, "tick del análisis programado completado");
                    }
                }
                Err(error) => {
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
    let review_items = collect_review_items_for_report(state, &run).await?;
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
    if let Some(bundle) = state
        .storage
        .get_org_config_for_user(&config.user_email)
        .await?
    {
        if !bundle.draft.schedule_report_policy.scheduler_enabled {
            return Err(anyhow::anyhow!(
                "scheduler deshabilitado en la política de organización"
            ));
        }
        let current_setup = setup_state(&bundle.draft);
        if !current_setup.ready_for_analysis {
            return Err(anyhow::anyhow!(
                "configuración incompleta para análisis programado: {}",
                current_setup.missing.join(", ")
            ));
        }
        let snapshot = bundle.policy_version.snapshot.clone();
        let analysis = &snapshot.analysis_policy;
        let now = Utc::now();
        let run = AnalysisRun {
            id: Uuid::new_v4().to_string(),
            user_email: config.user_email.clone(),
            org_id: Some(bundle.org.id),
            mailbox_id: Some(snapshot.mailbox.id.clone()),
            trigger_type: Some(crate::analysis::TriggerType::Scheduled),
            policy_version_id: Some(bundle.policy_version.id),
            policy_hash: Some(bundle.policy_version.policy_hash),
            policy_snapshot: Some(snapshot.clone()),
            gmail_scope_snapshot: snapshot.mailbox.gmail_scope_snapshot.clone(),
            retention_expires_at: Some(retention_expires_at(now, &snapshot)),
            data_minimization_mode: Some("metadata_snippets_excerpts_only".to_string()),
            config: AnalysisConfig {
                date_from: window.date_from.clone(),
                date_to: window.date_to.clone(),
                time_from: analysis.default_time_from.clone(),
                time_to: analysis.default_time_to.clone(),
                timezone: analysis.timezone.clone(),
                internal_domains: analysis.internal_domains.clone(),
                ignored_senders: analysis.ignored_senders.clone(),
                ignored_domains: analysis.ignored_domains.clone(),
                ignored_keywords: analysis.ignored_keywords.clone(),
                include_labels: analysis.include_labels.clone(),
                exclude_labels: analysis.exclude_labels.clone(),
            },
            status: AnalysisStatus::Running,
            progress_message: "Análisis programado iniciando".to_string(),
            processed_threads: 0,
            total_candidate_threads: 0,
            metrics: AnalysisMetrics::default(),
            created_at: now,
            completed_at: None,
            error_message: None,
        };
        state.storage.create_analysis_run(&run).await?;
        return Ok(run);
    }

    let run = AnalysisRun {
        id: Uuid::new_v4().to_string(),
        user_email: config.user_email.clone(),
        org_id: None,
        mailbox_id: None,
        trigger_type: Some(crate::analysis::TriggerType::Scheduled),
        policy_version_id: None,
        policy_hash: None,
        policy_snapshot: None,
        gmail_scope_snapshot: vec![],
        retention_expires_at: None,
        data_minimization_mode: Some("metadata_snippets_excerpts_only".to_string()),
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
            include_labels: vec![],
            exclude_labels: vec![],
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

async fn collect_review_items_for_report(
    state: &AppState,
    run: &AnalysisRun,
) -> anyhow::Result<Vec<ReviewItem>> {
    let Some(snapshot) = &run.policy_snapshot else {
        return collect_review_items(state, &run.id, true, true).await;
    };
    let content = &snapshot.schedule_report_policy.report_content;
    if content.mode == ReportMode::MetricsOnly {
        return Ok(Vec::new());
    }
    collect_review_items(
        state,
        &run.id,
        content.include_subjects,
        content.include_senders,
    )
    .await
}

async fn collect_review_items(
    state: &AppState,
    run_id: &str,
    include_subjects: bool,
    include_senders: bool,
) -> anyhow::Result<Vec<ReviewItem>> {
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
            subject: if include_subjects {
                thread.subject
            } else {
                "Asunto oculto por política de reporte".to_string()
            },
            from_email: if include_senders {
                from_email
            } else {
                "Remitente oculto por política de reporte".to_string()
            },
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
        policies::{ReportMode, policy_version_from_draft, provision_default_config},
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

    fn thread(id: &str, run_id: &str) -> crate::analysis::EmailThread {
        let now = Utc::now();
        crate::analysis::EmailThread {
            id: id.to_string(),
            analysis_run_id: run_id.to_string(),
            gmail_thread_id: format!("gmail-{id}"),
            subject: "Ayuda sensible".to_string(),
            normalized_subject: "ayuda sensible".to_string(),
            classification: crate::analysis::Classification::Ambiguous,
            classification_source: crate::analysis::ClassificationSource::Rules,
            classification_confidence: 0.5,
            is_valid_client_request: true,
            is_answered: false,
            first_message_at: Some(now),
            first_client_message_id: Some("msg-a".to_string()),
            first_internal_reply_message_id: None,
            last_internal_message_id: None,
            first_client_message_at: Some(now),
            first_internal_reply_at: None,
            last_internal_message_at: None,
            response_time_minutes: None,
            resolution_time_minutes: None,
            manual_review_required: true,
            manual_override_applied: false,
            reasons: vec!["Confianza baja".to_string()],
            created_at: now,
            updated_at: now,
        }
    }

    fn message(id: &str, from: &str) -> crate::analysis::EmailMessage {
        crate::analysis::EmailMessage {
            id: id.to_string(),
            gmail_message_id: format!("gmail-{id}"),
            from_email: from.to_string(),
            from_name: None,
            to_emails: vec!["help@x.cl".to_string()],
            cc_emails: vec![],
            date: Utc::now(),
            subject: "Ayuda sensible".to_string(),
            snippet: "Necesito ayuda".to_string(),
            headers: serde_json::json!({}),
            is_internal: false,
            is_external: true,
            is_automated: false,
            body_text: None,
        }
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
    async fn scheduled_policy_run_uses_current_snapshot() {
        let (state, storage) = app_state(None);
        let mut bundle = provision_default_config("a@x.cl", Utc::now());
        bundle.draft.analysis_policy.valid_request_criteria =
            vec!["Clientes externos solicitan soporte".to_string()];
        bundle.draft.analysis_policy.default_time_from = "08:30".to_string();
        bundle.draft.analysis_policy.default_time_to = "18:15".to_string();
        bundle.draft.schedule_report_policy.scheduler_enabled = true;
        bundle.draft.schedule_report_policy.report_recipients = vec!["ops@x.cl".to_string()];
        bundle.policy_version =
            policy_version_from_draft(&bundle.mailbox, &bundle.draft, 2, "a@x.cl", Utc::now());
        storage.upsert_org_config(&bundle).await.unwrap();

        let run = create_scheduled_run(
            &state,
            &schedule_config("a@x.cl"),
            &AnalysisWindow {
                date_from: "2026-06-11".to_string(),
                date_to: "2026-06-11".to_string(),
            },
        )
        .await
        .unwrap();

        assert_eq!(run.org_id, Some(bundle.org.id));
        assert_eq!(run.policy_version_id, Some(bundle.policy_version.id));
        assert!(run.policy_snapshot.is_some());
        assert_eq!(run.config.time_from, "08:30");
        assert_eq!(run.config.time_to, "18:15");
        assert_eq!(run.config.internal_domains, vec!["x.cl"]);
        assert!(run.retention_expires_at.is_some());
    }

    #[tokio::test]
    async fn scheduled_policy_report_respects_metrics_only_mode() {
        let (state, storage) = app_state(None);
        let mut run = AnalysisRun {
            id: "run-policy".to_string(),
            user_email: "a@x.cl".to_string(),
            org_id: None,
            mailbox_id: None,
            trigger_type: Some(crate::analysis::TriggerType::Scheduled),
            policy_version_id: None,
            policy_hash: None,
            policy_snapshot: None,
            gmail_scope_snapshot: vec![],
            retention_expires_at: None,
            data_minimization_mode: None,
            config: AnalysisConfig {
                date_from: "2026-06-11".to_string(),
                date_to: "2026-06-11".to_string(),
                time_from: "00:00".to_string(),
                time_to: "23:59".to_string(),
                timezone: "America/Santiago".to_string(),
                internal_domains: vec!["x.cl".to_string()],
                ignored_senders: vec![],
                ignored_domains: vec![],
                ignored_keywords: vec![],
                include_labels: vec![],
                exclude_labels: vec![],
            },
            status: AnalysisStatus::Completed,
            progress_message: String::new(),
            processed_threads: 1,
            total_candidate_threads: 1,
            metrics: AnalysisMetrics::default(),
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
            error_message: None,
        };
        let bundle = provision_default_config("a@x.cl", Utc::now());
        run.policy_snapshot = Some(bundle.policy_version.snapshot);
        storage.create_analysis_run(&run).await.unwrap();
        storage
            .upsert_thread(
                &thread("thread-policy", "run-policy"),
                &[message("msg-policy", "cliente@example.com")],
            )
            .await
            .unwrap();

        let items = collect_review_items_for_report(&state, &run).await.unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn scheduled_policy_report_redacts_subjects_and_senders_when_disabled() {
        let (state, storage) = app_state(None);
        let mut bundle = provision_default_config("a@x.cl", Utc::now());
        bundle.draft.schedule_report_policy.report_content.mode = ReportMode::MetricsAndReviewItems;
        bundle
            .draft
            .schedule_report_policy
            .report_content
            .include_subjects = false;
        bundle
            .draft
            .schedule_report_policy
            .report_content
            .include_senders = false;
        bundle.policy_version =
            policy_version_from_draft(&bundle.mailbox, &bundle.draft, 2, "a@x.cl", Utc::now());
        let run = AnalysisRun {
            id: "run-redacted".to_string(),
            user_email: "a@x.cl".to_string(),
            org_id: Some(bundle.org.id.clone()),
            mailbox_id: Some(bundle.mailbox.id.clone()),
            trigger_type: Some(crate::analysis::TriggerType::Scheduled),
            policy_version_id: Some(bundle.policy_version.id.clone()),
            policy_hash: Some(bundle.policy_version.policy_hash.clone()),
            policy_snapshot: Some(bundle.policy_version.snapshot.clone()),
            gmail_scope_snapshot: bundle.mailbox.gmail_scope_snapshot.clone(),
            retention_expires_at: None,
            data_minimization_mode: None,
            config: AnalysisConfig {
                date_from: "2026-06-11".to_string(),
                date_to: "2026-06-11".to_string(),
                time_from: "00:00".to_string(),
                time_to: "23:59".to_string(),
                timezone: "America/Santiago".to_string(),
                internal_domains: vec!["x.cl".to_string()],
                ignored_senders: vec![],
                ignored_domains: vec![],
                ignored_keywords: vec![],
                include_labels: vec![],
                exclude_labels: vec![],
            },
            status: AnalysisStatus::Completed,
            progress_message: String::new(),
            processed_threads: 1,
            total_candidate_threads: 1,
            metrics: AnalysisMetrics::default(),
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
            error_message: None,
        };
        storage.create_analysis_run(&run).await.unwrap();
        storage
            .upsert_thread(
                &thread("thread-redacted", "run-redacted"),
                &[message("msg-redacted", "cliente@example.com")],
            )
            .await
            .unwrap();

        let items = collect_review_items_for_report(&state, &run).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].subject, "Asunto oculto por política de reporte");
        assert_eq!(
            items[0].from_email,
            "Remitente oculto por política de reporte"
        );
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
