pub mod model;
pub mod window;

use std::time::Duration;

use chrono::{DateTime, NaiveDate, Timelike, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    analysis::{AnalysisConfig, AnalysisMetrics, AnalysisRun, AnalysisStatus},
    auth::{
        decrypt_token, encrypt_token,
        refresh::{RefreshError, refresh_access_token_for},
    },
    billing::subscription_allows_access,
    http::{AppState, execute_analysis, mark_run_failed},
    mailbox::mailbox_connection_is_active,
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
        let Some(window) = window::due_window_for(now_utc, &config.timezone, &config.analysis_time)
        else {
            outcomes.push(ScheduledOutcome::Skipped {
                user_email: Some(config.user_email),
                reason: "fuera de horario programado".to_string(),
            });
            continue;
        };
        let repeat_hourly = state.config.is_privileged_account(&config.user_email);
        outcomes.push(run_for_user(state, mailer, &config, &window, repeat_hourly).await);
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
        outcomes.push(run_for_user(state, mailer, &config, window, false).await);
    }
    Ok(outcomes)
}

/// Loop interno: despierta periódicamente y dispara una vez por día hábil desde
/// la hora configurada. La idempotencia real vive en ScheduleState.
pub fn spawn(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mailer = match ResendMailer::from_config(&state.config.report) {
            Ok(mailer) => mailer,
            Err(_) => {
                tracing::error!(
                    operation = "scheduler_start",
                    "scheduler interno desactivado: mailer mal configurado"
                );
                return;
            }
        };
        tracing::info!("scheduler interno activo (hora local configurable)");
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
                        let completed = outcomes
                            .iter()
                            .filter(|outcome| matches!(outcome, ScheduledOutcome::Completed { .. }))
                            .count();
                        let failed = outcomes
                            .iter()
                            .filter(|outcome| matches!(outcome, ScheduledOutcome::Failed { .. }))
                            .count();
                        let skipped = outcomes.len() - completed - failed;
                        tracing::info!(
                            completed,
                            failed,
                            skipped,
                            "tick del análisis programado completado"
                        );
                    }
                }
                Err(_) => {
                    tracing::error!(
                        operation = "scheduler_tick",
                        "tick del análisis programado falló"
                    );
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
        analysis_time: window::DEFAULT_ANALYSIS_TIME.to_string(),
        gmail_max_threads: state.config.scheduler.seed_gmail_max_threads,
        updated_at: Utc::now(),
    };
    state.storage.upsert_schedule_config(&seeded).await?;
    tracing::info!(
        operation = "scheduler_seed",
        "scheduleConfig sembrado desde variables de entorno"
    );
    Ok(vec![seeded])
}

async fn run_for_user(
    state: &AppState,
    mailer: &dyn ReportMailer,
    config: &ScheduleConfig,
    window: &AnalysisWindow,
    repeat_completed_hourly: bool,
) -> ScheduledOutcome {
    match check_idempotency(state, config, window, repeat_completed_hourly).await {
        Ok(Some(reason)) => {
            return ScheduledOutcome::Skipped {
                user_email: Some(config.user_email.clone()),
                reason,
            };
        }
        Ok(None) => {}
        Err(_) => {
            return ScheduledOutcome::Failed {
                user_email: config.user_email.clone(),
                error: "scheduler_state_persist_failed".to_string(),
                notice_sent: false,
            };
        }
    }
    match analyze_and_report(state, mailer, config, window).await {
        Ok(outcome) => outcome,
        Err(_) => {
            // Log ERROR dedicado: permite distinguir en Cloud Monitoring un
            // fallo programado de uno manual, sin datos del correo.
            tracing::error!(
                operation = "scheduled_analysis",
                "scheduled analysis failed"
            );
            let message = "scheduled_analysis_failed".to_string();
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

/// Reserva atómicamente la ventana o devuelve por qué otra ejecución ya la
/// cubrió. El backend toma un lock de fila (`SELECT … FOR UPDATE` dentro de la
/// transacción del claim), por lo que dos instancias no pueden obtener el mismo
/// claim sobre un `schedule_state` ya existente.
async fn check_idempotency(
    state: &AppState,
    config: &ScheduleConfig,
    window: &AnalysisWindow,
    repeat_completed_hourly: bool,
) -> anyhow::Result<Option<String>> {
    let now = Utc::now();
    let repeat_completed_before = repeat_completed_hourly.then(|| {
        now.with_minute(0)
            .and_then(|value| value.with_second(0))
            .and_then(|value| value.with_nanosecond(0))
            .expect("UTC always has a valid hour boundary")
    });
    let claim = state
        .storage
        .claim_schedule_window(
            &ScheduleState {
                user_email: config.user_email.clone(),
                window_date_from: window.date_from.clone(),
                window_date_to: window.date_to.clone(),
                status: ScheduleRunStatus::Running,
                run_id: None,
                email_sent: false,
                error_message: None,
                started_at: now,
                updated_at: now,
            },
            now - chrono::Duration::minutes(CLAIM_TTL_MINUTES),
            repeat_completed_before,
        )
        .await?;
    Ok(match claim {
        crate::storage::ScheduleWindowClaim::Claimed => None,
        crate::storage::ScheduleWindowClaim::AlreadyCompleted => {
            Some("ventana ya analizada".to_string())
        }
        crate::storage::ScheduleWindowClaim::AlreadyRunning => {
            Some("análisis en curso".to_string())
        }
    })
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
        Err(_) => {
            tracing::error!(operation = "report_delivery", run_id = %run.id, "no se pudo enviar el reporte programado");
            (false, Some("report_delivery_failed".to_string()))
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
    let connection = state
        .storage
        .get_gmail_connection(&config.user_email)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no hay una conexión de casilla activa para {}; vuelve a conectarla",
                config.user_email
            )
        })?;
    if !mailbox_connection_is_active(Some(&connection)) {
        return Err(anyhow::anyhow!(
            "la conexión de la casilla está revocada; vuelve a conectarla"
        ));
    }
    let encrypted_refresh = connection.refresh_token_encrypted.clone().ok_or_else(|| {
        anyhow::anyhow!("la conexión de la casilla no tiene refresh token; vuelve a conectarla")
    })?;
    let refresh_token = decrypt_token(&encrypted_refresh, &state.config.encryption_key)?;
    let token = refresh_access_token_for(
        &state.http,
        connection.provider,
        &state.config.google,
        state.config.microsoft.as_ref(),
        &refresh_token,
    )
    .await
    .map_err(|error| match error {
        RefreshError::InvalidGrant => anyhow::anyhow!(
            "el proveedor rechazó el refresh token; vuelve a conectar la casilla para renovar el acceso"
        ),
        RefreshError::Other(inner) => inner,
    })?;

    let mut updated = connection.clone();
    updated.access_token_encrypted =
        encrypt_token(&token.access_token, &state.config.encryption_key)?;
    if let Some(rotated) = &token.refresh_token {
        updated.refresh_token_encrypted =
            Some(encrypt_token(rotated, &state.config.encryption_key)?);
    }
    updated.updated_at = Utc::now();
    if state
        .storage
        .refresh_gmail_connection(&connection, &updated)
        .await?
        != crate::storage::MailboxConnectionRefresh::Updated
    {
        return Err(anyhow::anyhow!(
            "la conexión de la casilla cambió durante el refresh; se canceló el análisis programado"
        ));
    }
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
        let unrestricted = state.config.is_privileged_account(&config.user_email);
        if !current_setup.ready_for_analysis && !unrestricted {
            return Err(anyhow::anyhow!(
                "configuración incompleta para análisis programado: {}",
                current_setup.missing.join(", ")
            ));
        }
        if state.config.billing.enforcement_enabled && !unrestricted {
            let subscription = state
                .storage
                .get_subscription_for_org(&bundle.org.id)
                .await?;
            if !subscription_allows_access(subscription.as_ref(), Utc::now()) {
                return Err(anyhow::anyhow!(
                    "subscription_required: plan activo o trial vigente requerido para scheduler"
                ));
            }
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
        tracing::info!(
            operation = "scheduled_analysis_run_created",
            run_id = %run.id,
            "scheduled analysis run created"
        );
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
    tracing::info!(
        operation = "scheduled_analysis_run_created",
        run_id = %run.id,
        "scheduled analysis run created"
    );
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
        Err(_) => {
            tracing::error!(
                operation = "failure_notice",
                "no se pudo enviar el aviso de fallo"
            );
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
    if result.is_err() {
        tracing::error!(
            operation = "scheduler_state_persist",
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
        config.billing.enforcement_enabled = false;
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
            analysis_time: window::DEFAULT_ANALYSIS_TIME.to_string(),
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
            thread_id: format!("gmail-{id}"),
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
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn message(id: &str, from: &str) -> crate::analysis::EmailMessage {
        crate::analysis::EmailMessage {
            id: id.to_string(),
            message_id: format!("gmail-{id}"),
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
    async fn privileged_account_can_create_scheduled_run_without_subscription() {
        let email = "alice@example.com";
        let storage = Arc::new(MemoryStorage::default());
        let mut config = test_config(None);
        config.billing.enforcement_enabled = true;
        config.internal_full_access_emails = vec![email.to_string()];
        let state = AppState::new(config, storage.clone());
        let mut bundle = provision_default_config(email, Utc::now());
        bundle.draft.schedule_report_policy.scheduler_enabled = true;
        bundle.draft.schedule_report_policy.report_recipients = vec![email.to_string()];
        bundle.draft.analysis_policy.valid_request_criteria =
            vec!["Solicitudes de soporte".to_string()];
        bundle.policy_version =
            policy_version_from_draft(&bundle.mailbox, &bundle.draft, 2, email, Utc::now());
        storage.upsert_org_config(&bundle).await.unwrap();

        let run = create_scheduled_run(
            &state,
            &schedule_config(email),
            &AnalysisWindow {
                date_from: "2026-07-22".to_string(),
                date_to: "2026-07-22".to_string(),
            },
        )
        .await
        .unwrap();

        assert_eq!(run.user_email, email);
        assert_eq!(run.org_id.as_deref(), Some(bundle.org.id.as_str()));
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
                assert_eq!(error, "scheduled_analysis_failed");
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
        assert_eq!(
            saved.error_message.as_deref(),
            Some("scheduled_analysis_failed")
        );
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
    async fn concurrent_claims_only_allow_one_scheduled_run() {
        let (state, _) = app_state(None);
        let config = schedule_config("a@x.cl");
        let window = AnalysisWindow {
            date_from: "2026-06-11".to_string(),
            date_to: "2026-06-11".to_string(),
        };

        let (first, second) = tokio::join!(
            check_idempotency(&state, &config, &window, false),
            check_idempotency(&state, &config, &window, false),
        );
        let outcomes = [first.unwrap(), second.unwrap()];
        assert_eq!(
            outcomes.iter().filter(|outcome| outcome.is_none()).count(),
            1
        );
        assert!(
            outcomes
                .iter()
                .any(|outcome| { outcome.as_deref() == Some("análisis en curso") })
        );
    }

    #[tokio::test]
    async fn privileged_schedule_can_repeat_once_per_hour() {
        let (state, storage) = app_state(None);
        let config = schedule_config("tester@example.com");
        let window = AnalysisWindow {
            date_from: "2026-06-11".to_string(),
            date_to: "2026-06-11".to_string(),
        };
        storage
            .upsert_schedule_state(&schedule_state(
                &config.user_email,
                &window.date_from,
                &window.date_to,
                ScheduleRunStatus::Completed,
                70,
            ))
            .await
            .unwrap();

        assert!(
            check_idempotency(&state, &config, &window, true)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            check_idempotency(&state, &config, &window, true)
                .await
                .unwrap()
                .as_deref(),
            Some("análisis en curso")
        );
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
                workos_user_id: Some("workos-s1".to_string()),
                workos_session_id: None,
                google_account_email: "a@x.cl".to_string(),
                gmail_account_email: Some("a@x.cl".to_string()),
                access_token_encrypted: "x".to_string(),
                refresh_token_encrypted: None,
                gmail_access_token_encrypted: None,
                gmail_refresh_token_encrypted: None,
                expires_at: None,
                revoked_at: None,
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
