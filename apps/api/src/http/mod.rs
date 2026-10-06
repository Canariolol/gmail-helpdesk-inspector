mod internal;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    convert::Infallible,
    fmt,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context as _;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{
        IntoResponse, Redirect, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{delete, get, patch, post, put},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use futures_util::{StreamExt, stream};
use hmac::{Hmac, Mac};
use rand::RngCore;
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    analysis::{
        AiUsageAttempt, AiUsageAttemptOutcome, AnalysisConfig, AnalysisFunnel, AnalysisMetrics,
        AnalysisRun, AnalysisStatus, Classification, ClassificationSource, DroppedThreadInfo,
        EmailMessage, EmailThread, ManualReview, ManualReviewOverride, RequestScope,
        ThreadDisposition, TriggerType, apply_manual_review_override, calculate_metrics,
        classify_thread, message_fingerprint, message_is_inside_analysis_window,
        refine_classification_with_folders, refine_classification_with_policy_hints,
        rescue_classification_with_valid_signals,
    },
    auth::{
        GoogleTokenResponse, MICROSOFT_MAIL_SCOPE, UserSession, clear_oauth_cookie,
        clear_session_cookie, decrypt_token, encrypt_token, oauth_cookie,
        refresh::{RefreshError, refresh_access_token_for},
        session_cookie, session_expires_at, session_is_active, sign_session_id,
        verify_session_cookie,
    },
    billing::{
        Account, BillingInterval, BillingPlan, BillingPlanId, CheckoutSession,
        CheckoutSessionStatus, EntitlementSnapshot, Subscription, SubscriptionStatus,
        UNLIMITED_REPORTED_PER_RUN, UsageLedger, active_subscription_for_trial, free_plan,
        plan_by_id, public_plans, subscription_allows_access,
    },
    config::{AppConfig, MicrosoftConfig},
    gmail::GmailClient,
    graph::GraphClient,
    mailbox::{
        FilterPreset, MailboxConnection, MailboxProvider, MailboxProviderKind, MailboxProviders,
        mailbox_connection_is_active,
    },
    policies::{
        AiPolicy, AnalysisPolicy, MailboxPurpose, MembershipStatus, OrgConfigBundle,
        OrgConfigResponse, OrgRole, OrganizationStatus, PolicyVersion, ScheduleReportPolicy,
        apply_ai_defaults_migration, apply_ai_prompt_version_migration,
        apply_ai_threshold_migration, hash_owner_email, normalize_domains, normalize_list,
        policy_version_from_draft, provision_default_config, retention_expires_at, setup_state,
        validate_timezone,
    },
    provider_detect::{self, DetectedProvider},
    report::{ReportMailer, ResendMailer},
    scheduler::{
        model::{ScheduleConfig, ScheduleState},
        window::next_fire_time_for_days,
    },
    storage::{
        AnalysisDataDeletionAudit, AnalysisDataDeletionStatus, StorageRepository, UsageAmounts,
    },
};

#[derive(Debug, Deserialize)]
struct GmailProfileResponse {
    #[serde(rename = "emailAddress")]
    email_address: String,
}

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub storage: Arc<dyn StorageRepository>,
    pub http: Client,
    pub mailbox: Arc<MailboxProviders>,
    analysis_slots: Arc<tokio::sync::Semaphore>,
    rate_limiter: RateLimiter,
}

#[derive(Clone, Default)]
struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, VecDeque<chrono::DateTime<Utc>>>>>,
}

impl AppState {
    pub fn new(config: AppConfig, storage: Arc<dyn StorageRepository>) -> Self {
        let microsoft = config.microsoft.is_some();
        Self {
            config,
            storage,
            http: Client::builder()
                .timeout(Duration::from_secs(30))
                .connect_timeout(Duration::from_secs(10))
                .build()
                .expect("failed to build http client"),
            // `microsoft` queda en None mientras no haya credenciales de Azure
            // AD: la ausencia degrada una función, no impide arrancar.
            mailbox: Arc::new(MailboxProviders::new(
                GmailClient::default(),
                microsoft.then(GraphClient::default),
            )),
            rate_limiter: RateLimiter::default(),
            analysis_slots: Arc::new(tokio::sync::Semaphore::new(4)),
        }
    }
}

impl RateLimiter {
    async fn check(&self, key: String, limit: usize) -> bool {
        if limit == 0 {
            return true;
        }
        let now = Utc::now();
        let cutoff = now - chrono::Duration::hours(1);
        let mut inner = self.inner.lock().await;
        let entries = inner.entry(key).or_default();
        while entries.front().is_some_and(|value| *value < cutoff) {
            entries.pop_front();
        }
        if entries.len() >= limit {
            return false;
        }
        entries.push_back(now);
        true
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/health/ready", get(readiness))
        .route("/me/report", get(consolidated_report))
        .route("/mailbox/connect/imap", post(imap_connect))
        .route("/auth/workos/login", get(auth_workos_login))
        .route("/auth/workos/callback", get(auth_workos_callback))
        .route("/auth/workos/webhook", post(workos_webhook))
        .route("/auth/google/login", get(gmail_connect_login))
        .route("/auth/google/callback", get(gmail_connect_callback))
        .route("/gmail/connect/login", get(gmail_connect_login))
        .route("/gmail/connect/callback", get(gmail_connect_callback))
        .route("/gmail/disconnect", post(gmail_disconnect))
        .route(
            "/mailbox/connect/microsoft/login",
            get(microsoft_connect_login),
        )
        .route(
            "/mailbox/connect/microsoft/callback",
            get(microsoft_connect_callback),
        )
        .route("/mailbox/providers", get(mailbox_providers))
        .route("/mailbox/detect", get(detect_mailbox_provider))
        .route("/auth/logout", post(auth_logout))
        .route("/auth/logout-all", post(auth_logout_all))
        .route("/auth/me", get(auth_me))
        .route("/me/account", get(get_account_status))
        .route("/me/usage", get(get_usage))
        .route("/public/plans", get(get_public_plans))
        .route(
            "/checkout/subscriptions",
            post(create_checkout_subscription),
        )
        .route("/me/subscription/cancel", post(cancel_subscription))
        .route("/me/subscription/change-plan", post(change_plan))
        .route("/me/subscription/reconcile", post(reconcile_subscription))
        .route("/billing/mercadopago/webhook", post(mercadopago_webhook))
        .route("/me/data-summary", get(get_data_summary))
        .route("/me/analysis-data", delete(delete_analysis_data))
        .route("/me/operations/status", get(get_operations_status))
        .route("/me/operations/history", get(get_operations_history))
        .route("/me/org/config", get(get_org_config).put(update_org_config))
        .route(
            "/me/filter-presets",
            get(list_filter_presets_handler).post(create_filter_preset),
        )
        .route(
            "/me/filter-presets/{id}",
            put(update_filter_preset_handler).delete(delete_filter_preset_handler),
        )
        .route(
            "/analysis-runs",
            post(create_analysis_run).get(list_analysis_runs),
        )
        .route("/analysis-runs/{id}", get(get_analysis_run))
        .route("/analysis-runs/{id}/start", post(start_analysis_run))
        .route("/analysis-runs/{id}/status", get(get_analysis_run))
        .route("/analysis-runs/{id}/events", get(analysis_events))
        .route("/analysis-runs/{id}/metrics", get(get_metrics))
        .route("/analysis-runs/{id}/threads", get(list_threads))
        .route(
            "/analysis-runs/{run_id}/threads/{thread_id}",
            get(get_thread_for_run),
        )
        .route(
            "/analysis-runs/{run_id}/threads/{thread_id}/manual-review",
            patch(manual_review_for_run),
        )
        .route("/threads/{thread_id}", get(get_thread_legacy))
        .route(
            "/threads/{thread_id}/manual-review",
            patch(manual_review_legacy),
        )
        .route(
            "/internal/scheduled-analysis",
            post(internal::scheduled_analysis),
        )
        .route("/internal/maintenance", post(internal::maintenance))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true }))
}

/// Readiness: liveness más un ping liviano al storage activo. No consulta
/// Gmail, Bedrock ni proveedores externos, y no expone detalles internos.
async fn readiness(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .storage
        .ping()
        .await
        .map_err(|_| ApiError::service_unavailable("El servicio no está listo"))?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct ReportQuery {
    date_from: String,
    date_to: String,
}

#[derive(Serialize)]
struct ConsolidatedReport {
    threads: Vec<EmailThread>,
    metrics: AnalysisMetrics,
    timezone: String,
    run_count: usize,
}

async fn consolidated_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ReportQuery>,
) -> Result<Json<ConsolidatedReport>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = require_active_entitlement(&state, &session).await?;
    validate_date_range(&query.date_from, &query.date_to)?;
    let timezone = bundle.draft.analysis_policy.timezone.clone();
    let tz = timezone
        .parse::<chrono_tz::Tz>()
        .map_err(|_| ApiError::bad_request("zona horaria inválida"))?;
    let mut runs = state
        .storage
        .list_analysis_runs(&session.google_account_email)
        .await?;
    runs.sort_by_key(|run| std::cmp::Reverse(run.created_at));
    let mut threads: HashMap<_, EmailThread> = HashMap::new();
    let mut run_count = 0;
    for run in runs.into_iter().filter(|run| {
        run.status == AnalysisStatus::Completed
            && run.retention_deadline() > Utc::now()
            && run.config.date_from <= query.date_to
            && run.config.date_to >= query.date_from
    }) {
        if run.org_id.as_deref().is_some_and(|id| id != bundle.org.id) {
            continue;
        }
        run_count += 1;
        for thread in state.storage.list_threads(&run.id).await? {
            let focus = if let Some(date) = thread.first_client_message_at {
                Some(date)
            } else {
                state
                    .storage
                    .list_messages(&run.id, &thread.id)
                    .await?
                    .iter()
                    .filter(|message| message_is_inside_analysis_window(message, &run.config))
                    .map(|message| message.date)
                    .min()
                    .or(thread.first_message_at)
            };
            let date = focus
                .unwrap_or(thread.created_at)
                .with_timezone(&tz)
                .format("%Y-%m-%d")
                .to_string();
            if date < query.date_from || date > query.date_to {
                continue;
            }
            let provider = if run
                .gmail_scope_snapshot
                .iter()
                .any(|scope| scope.starts_with("Mail.Read"))
            {
                "microsoft"
            } else if run
                .gmail_scope_snapshot
                .iter()
                .any(|scope| scope.starts_with("IMAP"))
            {
                "imap"
            } else {
                "google"
            };
            let mailbox = run
                .policy_snapshot
                .as_ref()
                .map(|snapshot| {
                    snapshot
                        .mailbox
                        .google_account_email
                        .trim()
                        .to_ascii_lowercase()
                })
                .unwrap_or_else(|| {
                    run.mailbox_id
                        .clone()
                        .unwrap_or_else(|| run.user_email.clone())
                });
            let key = (
                mailbox,
                provider,
                thread.thread_id.clone(),
                thread.first_client_message_id.clone(),
            );
            if threads
                .get(&key)
                .is_none_or(|previous| thread.updated_at > previous.updated_at)
            {
                threads.insert(key, thread);
            }
        }
    }
    let mut threads = threads.into_values().collect::<Vec<_>>();
    threads.sort_by_key(|thread| thread.first_client_message_at.or(thread.first_message_at));
    let metrics = calculate_metrics(&threads, 0, 0);
    Ok(Json(ConsolidatedReport {
        threads,
        metrics,
        timezone,
        run_count,
    }))
}

/// Limita el `screen_hint` a los valores válidos de AuthKit; cualquier otra cosa se
/// ignora (deja que WorkOS muestre su pantalla por defecto). Evita reenviar entrada
/// arbitraria del usuario al proveedor de identidad.
fn normalize_screen_hint(raw: Option<&str>) -> Option<&'static str> {
    match raw {
        Some("sign-up") => Some("sign-up"),
        Some("sign-in") => Some("sign-in"),
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
struct WorkosLoginQuery {
    /// "sign-up" lleva a la pantalla de creación de cuenta; "sign-in" a la de ingreso.
    #[serde(default)]
    screen_hint: Option<String>,
}

async fn auth_workos_login(
    State(state): State<AppState>,
    Query(query): Query<WorkosLoginQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if state.config.workos.client_id.trim().is_empty() {
        return Err(ApiError::service_unavailable("WorkOS no está configurado"));
    }
    let oauth_state = random_urlsafe(24);
    let signed_oauth = sign_session_id(&oauth_state, &state.config.workos.cookie_secret)?;
    let mut params = vec![
        ("response_type", "code"),
        ("provider", "authkit"),
        ("client_id", state.config.workos.client_id.as_str()),
        ("redirect_uri", state.config.workos.redirect_uri.as_str()),
        ("state", oauth_state.as_str()),
    ];
    if let Some(hint) = normalize_screen_hint(query.screen_hint.as_deref()) {
        params.push(("screen_hint", hint));
    }
    let url =
        url::Url::parse_with_params("https://api.workos.com/user_management/authorize", &params)
            .expect("valid workos auth url");
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&oauth_cookie(
            &signed_oauth,
            &state.config.cookie_same_site,
            state.config.cookie_secure,
        ))
        .unwrap(),
    );
    Ok((headers, Redirect::temporary(url.as_str())))
}

#[derive(Debug, Deserialize)]
struct WorkosCallback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkosAuthenticateResponse {
    user: WorkosUser,
    #[serde(default)]
    access_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkosAccessTokenClaims {
    #[serde(default)]
    sid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkosUser {
    id: String,
    email: String,
    #[serde(default)]
    first_name: Option<String>,
    #[serde(default)]
    last_name: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

/// El token llega directamente del intercambio servidor-a-servidor con WorkOS;
/// solo se lee `sid` para relacionar la sesión local con un evento revocado.
fn workos_session_id_from_access_token(access_token: Option<&str>) -> Option<String> {
    let payload = access_token?.split('.').nth(1)?;
    let claims: WorkosAccessTokenClaims =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()?;
    claims.sid.filter(|sid| !sid.trim().is_empty())
}

async fn auth_workos_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WorkosCallback>,
) -> Result<impl IntoResponse, ApiError> {
    if query.error.is_some() {
        tracing::warn!(
            operation = "workos_login_callback",
            error_code = "workos_login_rejected",
            "WorkOS rechazó el inicio de sesión"
        );
        return Err(ApiError::bad_request(
            "No se pudo completar el inicio de sesión. Inténtalo nuevamente.",
        ));
    }
    let verifier_payload = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| extract_named_cookie(cookies, "ghmi_oauth"))
        .and_then(|cookie| verify_session_cookie(&cookie, &state.config.workos.cookie_secret))
        .ok_or_else(|| ApiError::bad_request("missing or invalid WorkOS state cookie"))?;
    if query.state.as_deref() != Some(verifier_payload.as_str()) {
        return Err(ApiError::bad_request("WorkOS state mismatch"));
    }
    let code = query
        .code
        .ok_or_else(|| ApiError::bad_request("missing WorkOS code"))?;

    let auth: WorkosAuthenticateResponse = state
        .http
        .post("https://api.workos.com/user_management/authenticate")
        .json(&json!({
            "client_id": state.config.workos.client_id,
            "client_secret": state.config.workos.api_key,
            "grant_type": "authorization_code",
            "code": code,
        }))
        .send()
        .await
        .map_err(|_| workos_callback_internal_error("authenticate_request"))?
        .json_or_external_error("WorkOS authenticate")
        .await
        .map_err(|_| workos_callback_internal_error("authenticate_response"))?;

    let now = Utc::now();
    let existing_account = state
        .storage
        .get_account_by_workos_user_id(&auth.user.id)
        .await
        .map_err(|_| workos_callback_internal_error("storage_lookup"))?;
    let account_email = auth.user.email.trim().to_lowercase();
    let existing_account = match existing_account {
        Some(account) => Some(account),
        None => state
            .storage
            .get_account_by_email(&account_email)
            .await
            .map_err(|_| workos_callback_internal_error("storage_email_lookup"))?,
    };
    let org_id = existing_account
        .as_ref()
        .map(|account| account.org_id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let account = Account {
        workos_user_id: auth.user.id.clone(),
        email: account_email,
        name: auth.user.name.clone().or_else(|| {
            match (&auth.user.first_name, &auth.user.last_name) {
                (Some(first), Some(last)) => Some(format!("{first} {last}")),
                (Some(first), None) => Some(first.clone()),
                _ => None,
            }
        }),
        org_id,
        created_at: existing_account
            .as_ref()
            .map(|account| account.created_at)
            .unwrap_or(now),
        updated_at: now,
    };
    if let Some(previous) = existing_account
        .as_ref()
        .filter(|previous| previous.workos_user_id != account.workos_user_id)
    {
        state
            .storage
            .rebind_account_workos_user_id(previous, &account, now)
            .await
            .map_err(|_| workos_callback_internal_error("storage_account_rebind"))?;
    } else {
        state
            .storage
            .upsert_account(&account)
            .await
            .map_err(|_| workos_callback_internal_error("storage_account"))?;
    }
    let session = UserSession {
        id: Uuid::new_v4().to_string(),
        workos_user_id: Some(auth.user.id),
        workos_session_id: workos_session_id_from_access_token(auth.access_token.as_deref()),
        google_account_email: account.email,
        gmail_account_email: None,
        access_token_encrypted: String::new(),
        refresh_token_encrypted: None,
        gmail_access_token_encrypted: None,
        gmail_refresh_token_encrypted: None,
        expires_at: Some(session_expires_at(now)),
        revoked_at: None,
        created_at: now,
        updated_at: now,
    };
    state
        .storage
        .upsert_user_session(&session)
        .await
        .map_err(|_| workos_callback_internal_error("storage_session"))?;

    let signed = sign_session_id(&session.id, &state.config.session_secret)?;
    let mut headers = HeaderMap::new();
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie(
            &signed,
            &state.config.cookie_same_site,
            state.config.cookie_secure,
        ))
        .unwrap(),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&clear_oauth_cookie(
            &state.config.cookie_same_site,
            state.config.cookie_secure,
        ))
        .unwrap(),
    );
    headers.insert(
        header::LOCATION,
        HeaderValue::from_str(&state.config.web_base_url).unwrap(),
    );
    Ok((StatusCode::FOUND, headers))
}

fn workos_callback_internal_error(stage: &'static str) -> ApiError {
    tracing::error!(
        operation = "workos_login_callback",
        stage,
        "No se pudo completar el callback WorkOS"
    );
    ApiError::internal()
}

#[derive(Debug, Deserialize)]
struct WorkosWebhookEvent {
    event: String,
    data: serde_json::Value,
}

async fn workos_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<StatusCode, ApiError> {
    verify_workos_webhook(&state, &headers, &body)?;
    let event: WorkosWebhookEvent =
        serde_json::from_str(&body).map_err(|_| ApiError::bad_request("evento WorkOS inválido"))?;

    match event.event.as_str() {
        "session.revoked" => {
            let session_id = workos_event_data_id(&event.data)?;
            state
                .storage
                .revoke_user_session_by_workos_session_id(session_id, Utc::now())
                .await?;
        }
        "user.deleted" => {
            let user_id = workos_event_data_id(&event.data)?;
            if let Some(account) = state.storage.get_account_by_workos_user_id(user_id).await? {
                let now = Utc::now();
                state
                    .storage
                    .revoke_user_sessions(&account.email, now)
                    .await?;
                state.storage.disconnect_gmail(&account.email, now).await?;
                let mut bundle = get_or_provision_org_config(&state, &account.email).await?;
                bundle.mailbox.revoked_at = Some(now);
                bundle.draft.schedule_report_policy.scheduler_enabled = false;
                state.storage.upsert_org_config(&bundle).await?;
                sync_schedule_config_from_policy(&state, &bundle).await?;
            }
        }
        _ => {}
    }

    tracing::info!(operation = "workos_webhook", event_type = %event.event, "WorkOS webhook processed");
    Ok(StatusCode::NO_CONTENT)
}

fn workos_event_data_id(data: &serde_json::Value) -> Result<&str, ApiError> {
    data.get("id")
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| ApiError::bad_request("evento WorkOS sin identificador"))
}

#[derive(Debug, Deserialize, Default)]
struct ConnectLoginQuery {
    #[serde(default)]
    target_mailbox: Option<String>,
    /// Correo que el usuario escribió en la opción "no estoy seguro". Preselecciona
    /// la casilla en el proveedor para que el flujo se sienta como el botón directo.
    #[serde(default)]
    login_hint: Option<String>,
}

/// Correo a preseleccionar: el que escribió el usuario si es válido, si no la
/// cuenta con la que inició sesión en Mira.
fn login_hint_for<'a>(query: &'a ConnectLoginQuery, session: &'a UserSession) -> &'a str {
    query
        .login_hint
        .as_deref()
        .map(str::trim)
        .filter(|hint| provider_detect::domain_of(hint).is_some())
        .unwrap_or(&session.google_account_email)
}

async fn gmail_connect_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ConnectLoginQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = require_active_entitlement(&state, &session).await?;
    require_org_role(&bundle, &[OrgRole::Owner, OrgRole::Admin])?;
    let login_hint = login_hint_for(&query, &session);
    let scope = "https://www.googleapis.com/auth/gmail.readonly";
    let oauth = MailboxOAuthState::new(&session, &bundle.org.id, MailboxProviderKind::Google, None);
    let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(oauth.code_verifier.as_bytes()));
    let signed_oauth = oauth.signed(&state.config.session_secret)?;
    let url = url::Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/v2/auth",
        &[
            ("client_id", state.config.google.client_id.as_str()),
            ("redirect_uri", state.config.google.redirect_url.as_str()),
            ("response_type", "code"),
            ("scope", scope),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("state", oauth.state.as_str()),
            ("code_challenge", code_challenge.as_str()),
            ("code_challenge_method", "S256"),
            // Preselecciona la casilla para que el selector de cuentas no aparezca
            // sin necesidad.
            ("login_hint", login_hint),
        ],
    )
    .expect("valid oauth url");
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&oauth_cookie(
            &signed_oauth,
            &state.config.cookie_same_site,
            state.config.cookie_secure,
        ))
        .unwrap(),
    );
    Ok((headers, Redirect::temporary(url.as_str())))
}

#[derive(Debug, Deserialize)]
struct OAuthCallback {
    code: String,
    state: Option<String>,
}

async fn gmail_connect_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OAuthCallback>,
) -> Result<impl IntoResponse, ApiError> {
    let existing_session = require_session(&state, &headers).await?;
    let bundle = require_active_entitlement(&state, &existing_session).await?;
    require_org_role(&bundle, &[OrgRole::Owner, OrgRole::Admin])?;
    let oauth = verified_mailbox_oauth_state(
        &state,
        &headers,
        query.state.as_deref(),
        &existing_session,
        &bundle.org.id,
        MailboxProviderKind::Google,
    )?;
    let code_verifier = oauth.code_verifier;

    let token: GoogleTokenResponse = state
        .http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", query.code.as_str()),
            ("client_id", state.config.google.client_id.as_str()),
            ("client_secret", state.config.google.client_secret.as_str()),
            ("redirect_uri", state.config.google.redirect_url.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", code_verifier.as_str()),
        ])
        .send()
        .await?
        .json_or_google_error("Google OAuth code exchange")
        .await?;

    let profile: GmailProfileResponse = state
        .http
        .get("https://gmail.googleapis.com/gmail/v1/users/me/profile")
        .bearer_auth(&token.access_token)
        .send()
        .await?
        .json_or_google_error("Gmail profile fetch")
        .await?;

    let now = Utc::now();
    let previous = state
        .storage
        .get_gmail_connection(&existing_session.google_account_email)
        .await?;
    let previous_same_mailbox = matching_previous_connection(
        &previous,
        MailboxProviderKind::Google,
        &profile.email_address,
    );
    let connection = MailboxConnection {
        owner_email: existing_session.google_account_email.clone(),
        provider: MailboxProviderKind::Google,
        microsoft_target_email: None,
        imap_config: None,
        imap_password_encrypted: None,
        needs_reauth_at: None,
        mailbox_email: profile.email_address.clone(),
        access_token_encrypted: encrypt_token(&token.access_token, &state.config.encryption_key)?,
        refresh_token_encrypted: token
            .refresh_token
            .as_deref()
            .map(|refresh| encrypt_token(refresh, &state.config.encryption_key))
            .transpose()?
            .or_else(|| {
                previous_same_mailbox
                    .and_then(|connection| connection.refresh_token_encrypted.clone())
            }),
        connected_at: previous_same_mailbox.map_or(now, |connection| connection.connected_at),
        updated_at: now,
        revoked_at: None,
    };
    state.storage.upsert_gmail_connection(&connection).await?;
    let mut bundle =
        get_or_provision_org_config(&state, &existing_session.google_account_email).await?;
    prepare_connected_policy(&state, &mut bundle, &connection, previous.as_ref()).await?;
    bundle.mailbox.google_account_email = profile.email_address.clone();
    bundle.mailbox.display_name = profile.email_address.clone();
    bundle.mailbox.authorized_by_user_email = existing_session.google_account_email.clone();
    // El scope se estampa al conectar, no al aprovisionar: la vista de
    // Privacidad lo muestra como la declaración del permiso vigente.
    bundle.mailbox.gmail_scope_snapshot =
        vec![MailboxProviderKind::Google.read_scope().to_string()];
    bundle.mailbox.connected_at = now;
    bundle.mailbox.revoked_at = None;
    bundle.policy_version = policy_version_from_draft(
        &bundle.mailbox,
        &bundle.draft,
        bundle.policy_version.version + 1,
        &existing_session.google_account_email,
        now,
    );

    // Best-effort: lee metadata de la cuenta (etiquetas, alias, perfil, filtros)
    // AL CONECTAR, en segundo plano para no demorar el redirect ni romper el login
    // si una llamada de Gmail falla.
    {
        let mailbox = state.mailbox.clone();
        let storage = state.storage.clone();
        let access_token = token.access_token.clone();
        let connection = connection.clone();
        tokio::spawn(async move {
            let Ok(provider) = mailbox.for_connection(&connection) else {
                return;
            };
            let metadata = provider.fetch_mailbox_metadata(&access_token, now).await;
            if storage
                .save_current_mailbox_metadata(&connection, &metadata)
                .await
                .is_err()
            {
                tracing::warn!(
                    operation = "gmail_metadata_persist",
                    "no se pudo guardar metadata de Gmail al conectar"
                );
            }
        });
    }
    state.storage.upsert_org_config(&bundle).await?;
    sync_schedule_config_from_policy(&state, &bundle).await?;

    let mut headers = HeaderMap::new();
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&clear_oauth_cookie(
            &state.config.cookie_same_site,
            state.config.cookie_secure,
        ))
        .unwrap(),
    );
    headers.insert(
        header::LOCATION,
        HeaderValue::from_str(&state.config.web_base_url).unwrap(),
    );
    Ok((StatusCode::FOUND, headers))
}

/// Proveedores que este despliegue puede ofrecer. El frontend no debe mostrar un
/// botón que no puede funcionar.
async fn mailbox_providers(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "providers": state
            .mailbox
            .available()
            .into_iter()
            .map(|kind| kind.as_str())
            .collect::<Vec<_>>(),
    }))
}

#[derive(Deserialize)]
struct ImapConnectRequest {
    #[serde(flatten)]
    config: crate::imap::ImapConfig,
    mailbox_email: String,
    password: String,
}

async fn imap_connect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ImapConnectRequest>,
) -> Result<StatusCode, ApiError> {
    let session = require_session(&state, &headers).await?;
    let mut bundle = require_active_entitlement(&state, &session).await?;
    require_org_role(&bundle, &[OrgRole::Owner, OrgRole::Admin])?;
    enforce_rate_limit(&state, &session.google_account_email, "mailbox_connect", 6).await?;
    let mailbox_email = request.mailbox_email.trim().to_ascii_lowercase();
    validate_emails(std::slice::from_ref(&mailbox_email))?;
    if request.password.is_empty() || request.password.len() > 4096 {
        return Err(ApiError::bad_request(
            "ingresa una contraseña válida para IMAP",
        ));
    }
    let mut config = request.config;
    config.host = config.host.trim().to_ascii_lowercase();
    config.username = config.username.trim().to_string();
    let provider = crate::imap::ImapClient::new(config.clone()).map_err(|_| {
        ApiError::bad_request(
            "IMAP requiere un servidor DNS público con TLS en el puerto 993 y un usuario válido",
        )
    })?;
    provider
        .validate_connection(&request.password)
        .await
        .map_err(|_| {
            ApiError::bad_request(
                "no se pudo validar IMAP; revisa servidor, credenciales y acceso a las carpetas",
            )
        })?;
    let now = Utc::now();
    let previous = state
        .storage
        .get_gmail_connection(&session.google_account_email)
        .await?;
    let connection = MailboxConnection {
        owner_email: session.google_account_email.clone(),
        provider: MailboxProviderKind::Imap,
        microsoft_target_email: None,
        imap_config: Some(config),
        imap_password_encrypted: Some(encrypt_token(
            &request.password,
            &state.config.encryption_key,
        )?),
        needs_reauth_at: None,
        mailbox_email: mailbox_email.clone(),
        access_token_encrypted: String::new(),
        refresh_token_encrypted: None,
        connected_at: now,
        updated_at: now,
        revoked_at: None,
    };
    state.storage.upsert_gmail_connection(&connection).await?;
    prepare_connected_policy(&state, &mut bundle, &connection, previous.as_ref()).await?;
    bundle.mailbox.google_account_email = mailbox_email.clone();
    bundle.mailbox.display_name = mailbox_email;
    bundle.mailbox.connected_at = now;
    bundle.mailbox.revoked_at = None;
    bundle.mailbox.authorized_by_user_email = session.google_account_email.clone();
    bundle.mailbox.gmail_scope_snapshot = vec![MailboxProviderKind::Imap.read_scope().to_string()];
    bundle.policy_version = policy_version_from_draft(
        &bundle.mailbox,
        &bundle.draft,
        bundle.policy_version.version + 1,
        &session.google_account_email,
        now,
    );
    state.storage.upsert_org_config(&bundle).await?;
    sync_schedule_config_from_policy(&state, &bundle).await?;
    let metadata = provider
        .fetch_mailbox_metadata(&request.password, now)
        .await;
    state
        .storage
        .save_current_mailbox_metadata(&connection, &metadata)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct DetectProviderQuery {
    email: String,
}

#[derive(Debug, Serialize)]
struct DetectProviderResponse {
    provider: DetectedProvider,
}

/// Opción "No estoy seguro / otro": el usuario escribe su correo y resolvemos el
/// proveedor por los registros MX del dominio, para encaminarlo al OAuth correcto
/// sin que tenga que saber si su casilla es Google o Microsoft.
async fn detect_mailbox_provider(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DetectProviderQuery>,
) -> Result<Json<DetectProviderResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    // El endpoint hace DNS saliente con un dominio que aporta el usuario: se acota
    // por cuenta para que no sirva de amplificador.
    enforce_rate_limit(
        &state,
        &session.google_account_email,
        "detect_provider",
        PROVIDER_DETECT_PER_HOUR,
    )
    .await?;
    let domain = provider_detect::domain_of(&query.email)
        .ok_or_else(|| ApiError::bad_request("escribe un correo válido"))?;
    let detection = provider_detect::detect(&state.http, &domain)
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(
                operation = "detect_provider",
                ?error,
                "no se pudo resolver el DNS del dominio"
            );
            provider_detect::Detection {
                provider: DetectedProvider::Unknown,
            }
        });

    Ok(Json(DetectProviderResponse {
        provider: detection.provider,
    }))
}

/// Detecciones por hora y por cuenta.
const PROVIDER_DETECT_PER_HOUR: usize = 20;

fn microsoft_config(state: &AppState) -> Result<&MicrosoftConfig, ApiError> {
    state.config.microsoft.as_ref().ok_or_else(|| {
        ApiError::bad_request("la conexión con Microsoft no está habilitada en este entorno")
    })
}

/// Espejo de `gmail_connect_login`: redirect de página completa a Microsoft, con
/// PKCE y la misma cookie temporal de verificación.
async fn microsoft_connect_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ConnectLoginQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = require_active_entitlement(&state, &session).await?;
    require_org_role(&bundle, &[OrgRole::Owner, OrgRole::Admin])?;
    let target = query
        .target_mailbox
        .as_deref()
        .map(str::trim)
        .filter(|target| !target.is_empty())
        .map(str::to_ascii_lowercase);
    if let Some(target) = &target {
        validate_emails(std::slice::from_ref(target))?;
    }
    let scope = microsoft_mailbox_scope(target.as_deref());
    let login_hint = login_hint_for(&query, &session);
    let microsoft = microsoft_config(&state)?;
    let oauth = MailboxOAuthState::new(
        &session,
        &bundle.org.id,
        MailboxProviderKind::Microsoft,
        target.as_deref(),
    );
    let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(oauth.code_verifier.as_bytes()));
    let signed_oauth = oauth.signed(&state.config.session_secret)?;
    let domain_hint = login_hint
        .split_once('@')
        .map(|(_, domain)| domain.to_string())
        .unwrap_or_default();
    let url = url::Url::parse_with_params(
        &format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
            microsoft.tenant
        ),
        &[
            ("client_id", microsoft.client_id.as_str()),
            ("redirect_uri", microsoft.redirect_url.as_str()),
            ("response_type", "code"),
            ("scope", scope),
            ("response_mode", "query"),
            ("state", oauth.state.as_str()),
            ("code_challenge", code_challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("login_hint", login_hint),
            ("domain_hint", domain_hint.as_str()),
        ],
    )
    .expect("valid microsoft oauth url");
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&oauth_cookie(
            &signed_oauth,
            &state.config.cookie_same_site,
            state.config.cookie_secure,
        ))
        .unwrap(),
    );
    Ok((headers, Redirect::temporary(url.as_str())))
}

async fn microsoft_connect_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OAuthCallback>,
) -> Result<impl IntoResponse, ApiError> {
    let existing_session = require_session(&state, &headers).await?;
    let bundle = require_active_entitlement(&state, &existing_session).await?;
    require_org_role(&bundle, &[OrgRole::Owner, OrgRole::Admin])?;
    let microsoft = microsoft_config(&state)?;
    let oauth = verified_mailbox_oauth_state(
        &state,
        &headers,
        query.state.as_deref(),
        &existing_session,
        &bundle.org.id,
        MailboxProviderKind::Microsoft,
    )?;
    let code_verifier = oauth.code_verifier;
    let target = oauth.target_mailbox;
    let scope = microsoft_mailbox_scope(target.as_deref());

    let token: GoogleTokenResponse = state
        .http
        .post(format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            microsoft.tenant
        ))
        .form(&[
            ("code", query.code.as_str()),
            ("client_id", microsoft.client_id.as_str()),
            ("client_secret", microsoft.client_secret.as_str()),
            ("redirect_uri", microsoft.redirect_url.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", code_verifier.as_str()),
            ("scope", scope),
        ])
        .send()
        .await?
        .json_or_google_error("Microsoft OAuth code exchange")
        .await?;
    let refresh_token = required_microsoft_refresh_token(&token)?;

    let graph = match target.as_deref() {
        Some(email) => GraphClient::for_mailbox(email)?,
        None => GraphClient::default(),
    };
    graph.validate_mailbox_access(&token.access_token).await.map_err(|_|ApiError::bad_request("Microsoft no concedió lectura de la casilla elegida; revisa los permisos del buzón"))?;
    let profile = graph.get_profile(&token.access_token).await.map_err(|_| {
        ApiError::bad_request("no se pudo leer la identidad de la casilla en Microsoft")
    })?;

    let now = Utc::now();
    let previous = state
        .storage
        .get_gmail_connection(&existing_session.google_account_email)
        .await?;
    let previous_same_mailbox = matching_previous_connection(
        &previous,
        MailboxProviderKind::Microsoft,
        target.as_deref().unwrap_or(&profile.email_address),
    );
    let connection = MailboxConnection {
        owner_email: existing_session.google_account_email.clone(),
        provider: MailboxProviderKind::Microsoft,
        microsoft_target_email: target.clone(),
        imap_config: None,
        imap_password_encrypted: None,
        needs_reauth_at: None,
        mailbox_email: target
            .clone()
            .unwrap_or_else(|| profile.email_address.clone()),
        access_token_encrypted: encrypt_token(&token.access_token, &state.config.encryption_key)?,
        // Se guarda el refresh del usuario que acaba de consentir. Emitirlo no
        // revoca el anterior, pero nunca se mezclan grants de distintos delegados.
        refresh_token_encrypted: Some(encrypt_token(refresh_token, &state.config.encryption_key)?),
        connected_at: previous_same_mailbox.map_or(now, |connection| connection.connected_at),
        updated_at: now,
        revoked_at: None,
    };
    state.storage.upsert_gmail_connection(&connection).await?;

    // Misma actualización de casilla y política que el callback de Google: sin
    // esto la organización quedaría declarando la casilla y el scope anteriores.
    let mut bundle =
        get_or_provision_org_config(&state, &existing_session.google_account_email).await?;
    prepare_connected_policy(&state, &mut bundle, &connection, previous.as_ref()).await?;
    bundle.mailbox.google_account_email = connection.mailbox_email.clone();
    bundle.mailbox.display_name = connection.mailbox_email.clone();
    bundle.mailbox.authorized_by_user_email = existing_session.google_account_email.clone();
    bundle.mailbox.gmail_scope_snapshot =
        vec![MailboxProviderKind::Microsoft.read_scope().to_string()];
    if connection.microsoft_target_email.is_some() {
        bundle
            .mailbox
            .gmail_scope_snapshot
            .push("Mail.Read.Shared".to_string());
    }
    bundle.mailbox.connected_at = now;
    bundle.mailbox.revoked_at = None;
    bundle.policy_version = policy_version_from_draft(
        &bundle.mailbox,
        &bundle.draft,
        bundle.policy_version.version + 1,
        &existing_session.google_account_email,
        now,
    );
    state.storage.upsert_org_config(&bundle).await?;
    sync_schedule_config_from_policy(&state, &bundle).await?;

    {
        let mailbox = state.mailbox.clone();
        let storage = state.storage.clone();
        let access_token = token.access_token.clone();
        let connection = connection.clone();
        tokio::spawn(async move {
            let Ok(provider) = mailbox.for_connection(&connection) else {
                return;
            };
            let metadata = provider.fetch_mailbox_metadata(&access_token, now).await;
            if storage
                .save_current_mailbox_metadata(&connection, &metadata)
                .await
                .is_err()
            {
                tracing::warn!(
                    operation = "mailbox_metadata_persist",
                    provider = "microsoft",
                    "no se pudo guardar metadata de la casilla al conectar"
                );
            }
        });
    }

    let mut headers = HeaderMap::new();
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&clear_oauth_cookie(
            &state.config.cookie_same_site,
            state.config.cookie_secure,
        ))
        .unwrap(),
    );
    headers.insert(
        header::LOCATION,
        HeaderValue::from_str(&state.config.web_base_url).unwrap(),
    );
    Ok((StatusCode::FOUND, headers))
}

/// PKCE vinculado al usuario, sesión, tenant y proveedor que iniciaron el flujo.
/// Base64url mantiene la cookie libre de puntos del correo antes de firmarla.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MailboxOAuthState {
    state: String,
    code_verifier: String,
    session_id: String,
    owner_email: String,
    org_id: String,
    provider: MailboxProviderKind,
    target_mailbox: Option<String>,
    expires_at: i64,
}

impl MailboxOAuthState {
    fn new(
        session: &UserSession,
        org_id: &str,
        provider: MailboxProviderKind,
        target: Option<&str>,
    ) -> Self {
        Self {
            state: random_urlsafe(24),
            code_verifier: random_urlsafe(48),
            session_id: session.id.clone(),
            owner_email: session.google_account_email.clone(),
            org_id: org_id.to_string(),
            provider,
            target_mailbox: target.map(str::to_string),
            expires_at: Utc::now().timestamp() + 600,
        }
    }

    fn signed(&self, secret: &str) -> Result<String, ApiError> {
        let encoded =
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(self).map_err(anyhow::Error::from)?);
        Ok(sign_session_id(&encoded, secret)?)
    }

    fn verify(
        signed_cookie: &str,
        secret: &str,
        provider_state: Option<&str>,
        session: &UserSession,
        org_id: &str,
        provider: MailboxProviderKind,
    ) -> Result<Self, ApiError> {
        let invalid = || {
            ApiError::bad_request(
                "La conexión OAuth venció o es inválida; vuelve a conectar la casilla",
            )
        };
        let encoded = verify_session_cookie(signed_cookie, secret).ok_or_else(invalid)?;
        let payload = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| invalid())?;
        let oauth: Self = serde_json::from_slice(&payload).map_err(|_| invalid())?;
        if oauth.state.is_empty()
            || oauth.code_verifier.is_empty()
            || provider_state != Some(oauth.state.as_str())
        {
            return Err(invalid());
        }
        if oauth.session_id != session.id
            || oauth.owner_email != session.google_account_email
            || oauth.org_id != org_id
            || oauth.provider != provider
        {
            return Err(ApiError::bad_request(
                "La sesión o la organización cambió durante OAuth; vuelve a conectar la casilla",
            ));
        }
        let now = Utc::now().timestamp();
        if oauth.expires_at <= now || oauth.expires_at > now + 600 {
            return Err(invalid());
        }
        if let Some(target) = &oauth.target_mailbox {
            validate_emails(std::slice::from_ref(target))?;
            if provider != MailboxProviderKind::Microsoft {
                return Err(invalid());
            }
        }
        Ok(oauth)
    }
}

fn verified_mailbox_oauth_state(
    state: &AppState,
    headers: &HeaderMap,
    provider_state: Option<&str>,
    session: &UserSession,
    org_id: &str,
    provider: MailboxProviderKind,
) -> Result<MailboxOAuthState, ApiError> {
    let cookie = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| extract_named_cookie(cookies, "ghmi_oauth"))
        .ok_or_else(|| {
            ApiError::bad_request("Falta la cookie OAuth; vuelve a conectar la casilla")
        })?;
    MailboxOAuthState::verify(
        &cookie,
        &state.config.session_secret,
        provider_state,
        session,
        org_id,
        provider,
    )
}

fn microsoft_mailbox_scope(target: Option<&str>) -> &'static str {
    if target.is_some() {
        "offline_access User.Read Mail.Read Mail.Read.Shared"
    } else {
        MICROSOFT_MAIL_SCOPE
    }
}

fn required_microsoft_refresh_token(token: &GoogleTokenResponse) -> Result<&str, ApiError> {
    token.refresh_token.as_deref().filter(|refresh| !refresh.trim().is_empty())
        .ok_or_else(|| ApiError::bad_request("Microsoft no concedió acceso offline; vuelve a conectar la casilla y acepta los permisos solicitados"))
}

async fn prepare_connected_policy(
    state: &AppState,
    bundle: &mut OrgConfigBundle,
    connection: &MailboxConnection,
    previous: Option<&MailboxConnection>,
) -> Result<(), ApiError> {
    let changed = previous.is_some_and(|previous| {
        previous.provider != connection.provider
            || !previous
                .mailbox_email
                .eq_ignore_ascii_case(&connection.mailbox_email)
    });
    if changed {
        bundle.mailbox.id = Uuid::new_v4().to_string();
        bundle.draft.mailbox_id = bundle.mailbox.id.clone();
        bundle.draft.analysis_policy.include_labels.clear();
        bundle.draft.analysis_policy.exclude_labels.clear();
        for preset in state
            .storage
            .list_filter_presets(&connection.owner_email)
            .await?
        {
            state
                .storage
                .delete_filter_preset(&connection.owner_email, &preset.id)
                .await?;
        }
        state
            .storage
            .upsert_mailbox_metadata(
                &connection.owner_email,
                &crate::mailbox::MailboxMetadata {
                    profile: None,
                    labels: vec![],
                    send_as: vec![],
                    filters_count: 0,
                    folders_truncated: false,
                    synced_at: Utc::now(),
                },
            )
            .await?;
    }
    bundle
        .draft
        .analysis_policy
        .internal_domains
        .retain(|domain| !crate::policies::is_public_mail_domain(domain));
    if changed && let Some(previous) = previous {
        bundle
            .draft
            .analysis_policy
            .responder_emails
            .retain(|email| !email.eq_ignore_ascii_case(&previous.mailbox_email));
    }
    if bundle.draft.analysis_policy.responder_emails.is_empty()
        && (bundle.draft.analysis_policy.internal_domains.is_empty()
            || bundle.draft.analysis_policy.request_scope != RequestScope::External)
    {
        bundle
            .draft
            .analysis_policy
            .responder_emails
            .push(connection.mailbox_email.clone());
    }
    Ok(())
}

/// Un token sólo puede reutilizarse al reconectar el mismo proveedor. Al
/// reemplazar Google por Microsoft (o viceversa), el token anterior es inválido.
fn matching_previous_connection<'a>(
    previous: &'a Option<MailboxConnection>,
    provider: MailboxProviderKind,
    mailbox_email: &str,
) -> Option<&'a MailboxConnection> {
    previous.as_ref().filter(|connection| {
        connection.provider == provider
            && connection.mailbox_email.eq_ignore_ascii_case(mailbox_email)
    })
}

fn google_revocation_token(connection: Option<&MailboxConnection>) -> Option<&str> {
    connection
        .filter(|connection| connection.provider == MailboxProviderKind::Google)
        .and_then(|connection| {
            connection
                .refresh_token_encrypted
                .as_deref()
                .or((!connection.access_token_encrypted.trim().is_empty())
                    .then_some(connection.access_token_encrypted.as_str()))
        })
}

async fn gmail_disconnect(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    require_org_role(&bundle, &[OrgRole::Owner, OrgRole::Admin])?;
    let connection = state
        .storage
        .get_gmail_connection(&session.google_account_email)
        .await?;
    // Google ofrece revocación OAuth directa. Microsoft no tiene un endpoint
    // equivalente para este flujo; en ese caso se revoca sólo la conexión local.
    let encrypted_token = google_revocation_token(connection.as_ref());

    if let Some(encrypted_token) = encrypted_token {
        let token = decrypt_token(encrypted_token, &state.config.encryption_key).map_err(|_| {
            tracing::error!(
                operation = "gmail_revoke",
                "no se pudo descifrar el token de Gmail para revocarlo"
            );
            ApiError::service_unavailable("No pudimos desconectar Gmail; inténtalo de nuevo")
        })?;
        let response = state
            .http
            .post("https://oauth2.googleapis.com/revoke")
            .form(&[("token", token.as_str())])
            .send()
            .await
            .map_err(|_| {
                tracing::warn!(operation = "gmail_revoke", "falló la revocación de Gmail");
                ApiError::service_unavailable("No pudimos desconectar Gmail; inténtalo de nuevo")
            })?;
        if !response.status().is_success() && response.status().as_u16() != 400 {
            tracing::warn!(status = %response.status(), "Google rechazó la revocación de Gmail");
            return Err(ApiError::service_unavailable(
                "No pudimos desconectar Gmail; inténtalo de nuevo",
            ));
        }
    }

    let now = Utc::now();
    state
        .storage
        .disconnect_gmail(&session.google_account_email, now)
        .await?;
    let mut bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    bundle.mailbox.revoked_at = Some(now);
    state.storage.upsert_org_config(&bundle).await?;
    sync_schedule_config_from_policy(&state, &bundle).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn auth_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let mut session = require_session(&state, &headers).await?;
    let logout_url = workos_logout_url(&session, &state.config.web_base_url);
    session.revoked_at = Some(Utc::now());
    session.updated_at = Utc::now();
    state.storage.upsert_user_session(&session).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&clear_session_cookie(
            &state.config.cookie_same_site,
            state.config.cookie_secure,
        ))
        .unwrap(),
    );
    Ok((
        StatusCode::OK,
        headers,
        Json(json!({ "logout_url": logout_url })),
    ))
}

/// WorkOS termina su cookie hospedada solo cuando el navegador visita este URL.
/// Las sesiones locales antiguas sin `sid` aún cierran sesión en Mira y vuelven
/// a la landing.
fn workos_logout_url(session: &UserSession, web_base_url: &str) -> String {
    let Some(session_id) = session.workos_session_id.as_deref() else {
        return web_base_url.to_string();
    };
    url::Url::parse_with_params(
        "https://api.workos.com/user_management/sessions/logout",
        [("session_id", session_id), ("return_to", web_base_url)],
    )
    .expect("valid WorkOS logout URL")
    .to_string()
}

async fn auth_logout_all(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let session = require_session(&state, &headers).await?;
    let logout_url = workos_logout_url(&session, &state.config.web_base_url);
    state
        .storage
        .revoke_user_sessions(&session.google_account_email, Utc::now())
        .await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&clear_session_cookie(
            &state.config.cookie_same_site,
            state.config.cookie_secure,
        ))
        .unwrap(),
    );
    Ok((
        StatusCode::OK,
        headers,
        Json(json!({ "logout_url": logout_url })),
    ))
}

async fn auth_me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let connection = state
        .storage
        .get_gmail_connection(&session.google_account_email)
        .await?;
    Ok(Json(json!({
        "email": session.google_account_email,
        "workos_user_id": session.workos_user_id,
        "gmail_connected": mailbox_connection_is_active(connection.as_ref()),
        "gmail_account_email": connection
            .filter(|connection| mailbox_connection_is_active(Some(connection)))
            .map(|connection| connection.mailbox_email),
    })))
}

#[derive(Debug, Serialize)]
struct AccountStatusResponse {
    account_email: String,
    workos_user_id: Option<String>,
    org_id: String,
    gmail_connected: bool,
    mailbox_needs_reauth: bool,
    gmail_account_email: Option<String>,
    /// Proveedor de la casilla conectada ("google" | "microsoft"). `None`
    /// cuando no hay conexión activa.
    mailbox_provider: Option<String>,
    entitlement: EntitlementSnapshot,
}

async fn get_account_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AccountStatusResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    let entitlement =
        entitlement_snapshot(&state, &bundle.org.id, &session.google_account_email).await?;
    let connection = state
        .storage
        .get_gmail_connection(&session.google_account_email)
        .await?;
    Ok(Json(AccountStatusResponse {
        account_email: session.google_account_email.clone(),
        workos_user_id: session.workos_user_id.clone(),
        org_id: bundle.org.id,
        gmail_connected: mailbox_connection_is_active(connection.as_ref()),
        mailbox_needs_reauth: connection.as_ref().is_some_and(|connection| {
            connection.revoked_at.is_none() && connection.needs_reauth_at.is_some()
        }),
        mailbox_provider: connection
            .as_ref()
            .filter(|connection| connection.revoked_at.is_none())
            .map(|connection| connection.provider.as_str().to_string()),
        gmail_account_email: connection
            .filter(|connection| connection.revoked_at.is_none())
            .map(|connection| connection.mailbox_email),
        entitlement,
    }))
}

async fn get_public_plans() -> Json<Vec<BillingPlan>> {
    Json(public_plans())
}

#[derive(Debug, Deserialize)]
struct CreateCheckoutSubscriptionRequest {
    plan_id: String,
    /// Token de tarjeta generado por el SDK de Mercado Pago en el navegador
    /// (checkout embebido). Los datos PCI nunca pasan por la API.
    #[serde(default)]
    card_token_id: Option<String>,
    /// Email del pagador capturado por el Brick; puede diferir de la cuenta.
    #[serde(default)]
    payer_email: Option<String>,
    /// "monthly" (default) o "annual". Ausente = mensual, para no romper
    /// clientes viejos del checkout embebido.
    #[serde(default)]
    billing_interval: Option<String>,
}

#[derive(Debug, Serialize)]
struct CheckoutSessionResponse {
    session: CheckoutSession,
}

async fn claim_billing_operation(state: &AppState, org_id: &str) -> Result<String, ApiError> {
    let token = Uuid::new_v4().to_string();
    let now = Utc::now();
    if !state
        .storage
        .claim_billing_operation(org_id, &token, now, now - chrono::Duration::minutes(5))
        .await?
    {
        return Err(ApiError::service_unavailable(
            "Hay otra operación de pago en curso; inténtalo nuevamente",
        ));
    }
    Ok(token)
}

async fn finish_billing_operation<T>(
    state: &AppState,
    org_id: &str,
    token: &str,
    result: Result<T, ApiError>,
) -> Result<T, ApiError> {
    if state
        .storage
        .release_billing_operation(org_id, token)
        .await
        .is_err()
    {
        tracing::error!(
            operation = "billing_lock_release",
            "No se pudo liberar la operación de pago"
        );
    }
    result
}

async fn create_checkout_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateCheckoutSubscriptionRequest>,
) -> Result<Json<CheckoutSessionResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    require_org_role(
        &bundle,
        &[
            crate::policies::OrgRole::Owner,
            crate::policies::OrgRole::Admin,
        ],
    )?;
    let token = claim_billing_operation(&state, &bundle.org.id).await?;
    let result = async {
        let plan_id = BillingPlanId::parse(&request.plan_id)
            .ok_or_else(|| ApiError::bad_request("plan_id inválido"))?;
        let billing_interval = match request.billing_interval.as_deref() {
            None => BillingInterval::Monthly,
            Some(value) => BillingInterval::parse(value)
                .ok_or_else(|| ApiError::bad_request("billing_interval inválido"))?,
        };
        let plan = plan_by_id(&plan_id);
        let amount_clp = plan
            .amount_for(&billing_interval)
            .ok_or_else(|| ApiError::bad_request("este plan no se compra con tarjeta"))?;
        let card_token_id = request
            .card_token_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::bad_request("debes completar los datos de tu tarjeta"))?;
        let checkout_id =
            checkout_idempotency_key(&bundle.org.id, card_token_id, &plan_id, &billing_interval);
        let mut previous_checkout = state.storage.get_checkout_session(&checkout_id).await?;
        if previous_checkout.is_none() {
            previous_checkout = state.storage.find_incomplete_checkout_for_org(&bundle.org.id).await?;
            if previous_checkout.as_ref().is_some_and(|checkout| checkout.plan_id != plan_id || checkout.billing_interval != billing_interval) {
                return Err(ApiError::conflict("hay una compra pendiente; termínala antes de elegir otro plan"));
            }
        }
        if let Some(checkout) = previous_checkout.as_ref()
            && checkout.status == CheckoutSessionStatus::Activated
        {
            return Ok(Json(CheckoutSessionResponse {
                session: checkout.clone(),
            }));
        }
        let existing = state
            .storage
            .get_subscription_for_org(&bundle.org.id)
            .await?;
        let resuming = previous_checkout.as_ref().is_some_and(|checkout| {
            checkout.provider_subscription_id.is_some()
                && existing.as_ref().is_some_and(|subscription| {
                    subscription.provider_subscription_id == checkout.provider_subscription_id
                })
        });
        if !resuming && checkout_blocked_by_active_subscription(existing.as_ref(), Utc::now()) {
            return Err(ApiError::conflict(
                "ya hay una suscripción; revísala o cancélala desde tu cuenta antes de comprar otra",
            ));
        }
        let now = Utc::now();
        let mut checkout = previous_checkout.unwrap_or(CheckoutSession {
            id: checkout_id,
            org_id: bundle.org.id.clone(),
            account_email: session.google_account_email.clone(),
            plan_id: plan_id.clone(),
            status: CheckoutSessionStatus::Pending,
            provider: "mercadopago".to_string(),
            provider_subscription_id: None,
            billing_interval: billing_interval.clone(),
            currency_id: "CLP".to_string(),
            amount_clp,
            usd_reference_monthly: plan.usd_reference_monthly,
            trial_days: if existing.is_none() {
                plan.trial_days
            } else {
                0
            },
            created_at: now,
            updated_at: now,
        });
        let access_token = state
            .config
            .billing
            .mercadopago_access_token
            .as_deref()
            .ok_or_else(|| ApiError::service_unavailable("Mercado Pago no está configurado"))?;
        state.storage.upsert_checkout_session(&checkout).await?;
        let payer_email = request
            .payer_email
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&checkout.account_email);
        let provider = match checkout.provider_subscription_id.as_deref() {
            Some(id) => get_mercadopago_preapproval(&state, access_token, id).await?,
            None => {
                create_mercadopago_preapproval(
                    &state,
                    access_token,
                    &checkout,
                    card_token_id,
                    payer_email,
                )
                .await?
            }
        };
        if provider.status.as_deref() != Some("authorized") {
            checkout.status = CheckoutSessionStatus::Failed;
            checkout.provider_subscription_id = Some(provider.id);
            state.storage.upsert_checkout_session(&checkout).await?;
            return Err(ApiError::bad_request(
                "Mercado Pago no pudo autorizar la tarjeta; revisa los datos e inténtalo otra vez",
            ));
        }
        checkout.provider_subscription_id = Some(provider.id.clone());
        checkout.status = CheckoutSessionStatus::ProviderCreated;
        state.storage.upsert_checkout_session(&checkout).await?;
        let subscription = existing
            .filter(|subscription| subscription.provider_subscription_id.as_deref() == Some(provider.id.as_str()))
            .unwrap_or_else(|| {
                let started = provider.date_created.unwrap_or(checkout.created_at);
                let mut subscription = active_subscription_for_trial(
                    checkout.org_id.clone(), checkout.plan_id.clone(), Some(provider.id.clone()),
                    checkout.billing_interval.clone(), started,
                );
                subscription.trial_ends_at = (checkout.trial_days > 0)
                    .then(|| started + chrono::Duration::days(i64::from(checkout.trial_days)));
                subscription
            });
        state.storage.upsert_subscription(&subscription).await?;
        reconcile_mercadopago_subscription_locked(&state, access_token, &provider, None).await?;
        checkout.status = CheckoutSessionStatus::Activated;
        checkout.updated_at = Utc::now();
        state.storage.upsert_checkout_session(&checkout).await?;
        Ok(Json(CheckoutSessionResponse { session: checkout }))
    }
    .await;
    finish_billing_operation(&state, &bundle.org.id, &token, result).await
}

fn checkout_idempotency_key(
    org_id: &str,
    card_token: &str,
    plan: &BillingPlanId,
    interval: &BillingInterval,
) -> String {
    let digest = Sha256::digest(
        format!(
            "{org_id}\0{card_token}\0{}\0{}",
            plan.as_str(),
            interval.as_str()
        )
        .as_bytes(),
    );
    format!("checkout-{}", URL_SAFE_NO_PAD.encode(digest))
}

fn checkout_blocked_by_active_subscription(
    subscription: Option<&Subscription>,
    now: chrono::DateTime<Utc>,
) -> bool {
    subscription.is_some_and(|subscription| {
        subscription_allows_access(Some(subscription), now)
            || !subscription.cancel_at_period_end
                && subscription.status != SubscriptionStatus::Cancelled
                && (subscription.provider_subscription_id.is_some()
                    || matches!(
                        subscription.status,
                        SubscriptionStatus::Pending
                            | SubscriptionStatus::Active
                            | SubscriptionStatus::Trialing
                    ))
    })
}

async fn cancel_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<EntitlementSnapshot>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    require_org_role(
        &bundle,
        &[
            crate::policies::OrgRole::Owner,
            crate::policies::OrgRole::Admin,
        ],
    )?;
    let token = claim_billing_operation(&state, &bundle.org.id).await?;
    let result = async {
        let mut subscription = state
            .storage
            .get_subscription_for_org(&bundle.org.id)
            .await?
            .ok_or_else(|| ApiError::conflict("no hay una suscripción para cancelar"))?;
        if !subscription.cancel_at_period_end {
            if let Some(provider_id) = subscription.provider_subscription_id.as_deref() {
                let access_token = state
                    .config
                    .billing
                    .mercadopago_access_token
                    .as_deref()
                    .ok_or_else(|| {
                        ApiError::service_unavailable("Mercado Pago no está configurado")
                    })?;
                update_mercadopago_preapproval(
                    &state,
                    access_token,
                    provider_id,
                    json!({ "status": "cancelled" }),
                )
                .await?;
            }
            subscription.cancel_at_period_end = true;
            subscription.status = SubscriptionStatus::Cancelled;
            subscription.updated_at = Utc::now();
            state.storage.upsert_subscription(&subscription).await?;
        }
        Ok(Json(
            entitlement_snapshot(&state, &bundle.org.id, &session.google_account_email).await?,
        ))
    }
    .await;
    finish_billing_operation(&state, &bundle.org.id, &token, result).await
}

#[derive(Debug, Deserialize)]
struct ChangePlanRequest {
    plan_id: String,
}

async fn change_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ChangePlanRequest>,
) -> Result<Json<EntitlementSnapshot>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    require_org_role(
        &bundle,
        &[
            crate::policies::OrgRole::Owner,
            crate::policies::OrgRole::Admin,
        ],
    )?;
    let token = claim_billing_operation(&state, &bundle.org.id).await?;
    let result = async {
        let plan_id = BillingPlanId::parse(&request.plan_id)
            .ok_or_else(|| ApiError::bad_request("plan_id inválido"))?;
        let plan = plan_by_id(&plan_id);
        let mut subscription = state
            .storage
            .get_subscription_for_org(&bundle.org.id)
            .await?
            .ok_or_else(|| ApiError::payment_required("no tienes una suscripción activa"))?;
        if !subscription_allows_access(Some(&subscription), Utc::now())
            || subscription.cancel_at_period_end
        {
            return Err(ApiError::conflict(
                "reactiva tu suscripción antes de cambiar de plan",
            ));
        }
        let amount = plan
            .amount_for(&subscription.billing_interval)
            .ok_or_else(|| ApiError::bad_request("este plan no se compra con tarjeta"))?;
        let provider_id = subscription
            .provider_subscription_id
            .as_deref()
            .ok_or_else(|| {
                ApiError::conflict("la suscripción aún no está confirmada por Mercado Pago")
            })?;
        let access_token = state
            .config
            .billing
            .mercadopago_access_token
            .as_deref()
            .ok_or_else(|| ApiError::service_unavailable("Mercado Pago no está configurado"))?;
        update_mercadopago_preapproval(
            &state,
            access_token,
            provider_id,
            json!({
                "auto_recurring": { "transaction_amount": amount, "currency_id": "CLP" },
                "reason": format!("{} - Helpdesk Inspector", plan.name)
            }),
        )
        .await?;
        subscription.plan_id = plan_id;
        subscription.updated_at = Utc::now();
        state.storage.upsert_subscription(&subscription).await?;
        Ok(Json(
            entitlement_snapshot(&state, &bundle.org.id, &session.google_account_email).await?,
        ))
    }
    .await;
    finish_billing_operation(&state, &bundle.org.id, &token, result).await
}

#[derive(Debug, Deserialize)]
struct MercadoPagoWebhook {
    #[serde(default)]
    #[serde(rename = "type")]
    event_type: Option<String>,
    #[serde(default)]
    data: Option<MercadoPagoWebhookData>,
}

#[derive(Debug, Deserialize)]
struct MercadoPagoWebhookData {
    id: serde_json::Value,
}

fn mercadopago_webhook_resource_id(
    query: &HashMap<String, String>,
    payload: &MercadoPagoWebhook,
) -> Result<String, ApiError> {
    let signed = query
        .get("data.id")
        .or_else(|| query.get("data_id"))
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .ok_or_else(ApiError::unauthorized)?;
    let body_id = payload
        .data
        .as_ref()
        .and_then(|data| match &data.id {
            serde_json::Value::String(id) => Some(id.clone()),
            serde_json::Value::Number(id) => Some(id.to_string()),
            _ => None,
        })
        .ok_or_else(|| ApiError::bad_request("evento Mercado Pago sin identificador de recurso"))?;
    if !body_id.eq_ignore_ascii_case(signed)
        || !signed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ApiError::unauthorized());
    }
    Ok(signed.to_string())
}

async fn mercadopago_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Json(payload): Json<MercadoPagoWebhook>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let resource_id = mercadopago_webhook_resource_id(&query, &payload)?;
    verify_mercadopago_webhook(&state, &headers, Some(&resource_id))?;
    let event_type = payload
        .event_type
        .as_deref()
        .or_else(|| query.get("type").map(String::as_str))
        .unwrap_or_default();
    let access_token = state
        .config
        .billing
        .mercadopago_access_token
        .as_deref()
        .ok_or_else(|| ApiError::service_unavailable("Mercado Pago no está configurado"))?;
    let payment_id = (event_type == "payment").then(|| resource_id.clone());
    let provider_id = match event_type {
        "subscription_preapproval" | "preapproval" => resource_id,
        "subscription_authorized_payment" => {
            get_mercadopago_invoice(&state, access_token, &resource_id)
                .await?
                .preapproval_id
        }
        "payment" => {
            let invoices =
                search_mercadopago_invoices(&state, access_token, "payment_id", &resource_id)
                    .await?;
            let Some(invoice) = invoices.into_iter().find(|invoice| {
                invoice
                    .payment
                    .as_ref()
                    .is_some_and(|payment| payment.id.to_string() == resource_id)
            }) else {
                return Ok(Json(json!({ "ok": true, "ignored": true })));
            };
            invoice.preapproval_id
        }
        _ => return Ok(Json(json!({ "ok": true, "ignored": true }))),
    };
    let provider = get_mercadopago_preapproval(&state, access_token, &provider_id).await?;
    let Some(checkout) = find_mercadopago_checkout(&state, &provider).await? else {
        return Ok(Json(json!({ "ok": true, "ignored": true })));
    };
    let token = claim_billing_operation(&state, &checkout.org_id).await?;
    let result = async {
        // El webhook solo identifica qué reconciliar. El estado actual se lee
        // después de tomar el lock, sin aplicar snapshots antiguos del evento.
        let provider = get_mercadopago_preapproval(&state, access_token, &provider_id).await?;
        let payment = match payment_id.as_deref() {
            Some(id) => Some(get_mercadopago_payment(&state, access_token, id).await?),
            None => None,
        };
        reconcile_mercadopago_subscription_locked(
            &state,
            access_token,
            &provider,
            payment.as_ref(),
        )
        .await?;
        Ok(Json(json!({ "ok": true })))
    }
    .await;
    finish_billing_operation(&state, &checkout.org_id, &token, result).await
}

async fn reconcile_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<EntitlementSnapshot>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    require_org_role(
        &bundle,
        &[
            crate::policies::OrgRole::Owner,
            crate::policies::OrgRole::Admin,
        ],
    )?;
    let token = claim_billing_operation(&state, &bundle.org.id).await?;
    let result = async {
        let subscription = state
            .storage
            .get_subscription_for_org(&bundle.org.id)
            .await?
            .ok_or_else(|| ApiError::conflict("no hay una suscripción para reconciliar"))?;
        let provider_id = subscription
            .provider_subscription_id
            .as_deref()
            .ok_or_else(|| {
                ApiError::conflict("la suscripción no tiene identificador de Mercado Pago")
            })?;
        let access_token = state
            .config
            .billing
            .mercadopago_access_token
            .as_deref()
            .ok_or_else(|| ApiError::service_unavailable("Mercado Pago no está configurado"))?;
        let provider = get_mercadopago_preapproval(&state, access_token, provider_id).await?;
        reconcile_mercadopago_subscription_locked(&state, access_token, &provider, None).await?;
        Ok(Json(
            entitlement_snapshot(&state, &bundle.org.id, &session.google_account_email).await?,
        ))
    }
    .await;
    finish_billing_operation(&state, &bundle.org.id, &token, result).await
}

#[derive(Debug, Serialize)]
struct UsageResponse {
    period_key: String,
    usage: UsageLedger,
    limits: Option<crate::billing::PlanLimits>,
}

async fn get_usage(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<UsageResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    let period_key = current_period_key();
    let usage = state
        .storage
        .get_usage_ledger(&bundle.org.id, &period_key)
        .await?
        .unwrap_or_else(|| empty_usage(&bundle.org.id, &period_key));
    let limits = if state
        .config
        .is_privileged_account(&session.google_account_email)
    {
        None
    } else {
        Some(effective_plan_for_org(&state, &bundle.org.id).await?.limits)
    };
    Ok(Json(UsageResponse {
        period_key,
        usage,
        limits,
    }))
}

#[derive(Debug, Serialize)]
struct DataSummaryResponse {
    account: DataSummaryAccount,
    org: DataSummaryOrg,
    privacy: DataSummaryPrivacy,
    stored_data: StoredDataSummary,
    actions: DataActionAvailability,
}

#[derive(Debug, Serialize)]
struct DataSummaryAccount {
    google_account_email: String,
    gmail_scope_snapshot: Vec<String>,
    mailbox_connected: bool,
    mailbox_revoked_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
struct DataSummaryOrg {
    id: String,
    name: String,
    role: crate::policies::OrgRole,
    policy_version: u64,
    setup_ready: bool,
    setup_missing: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DataSummaryPrivacy {
    data_minimization_mode: String,
    ai_enabled: bool,
    ai_consent_granted_at: Option<chrono::DateTime<Utc>>,
    retention_days: u32,
    report_mode: crate::policies::ReportMode,
}

#[derive(Debug, Serialize)]
struct StoredDataSummary {
    analysis_runs_count: usize,
    threads_count: usize,
    messages_count: usize,
    ai_audit_records_count: Option<usize>,
}

#[derive(Debug, Serialize)]
struct DataActionAvailability {
    disconnect_gmail: DataActionStatus,
    delete_analysis_data: DataActionStatus,
    delete_account_data: DataActionStatus,
}

#[derive(Debug, Serialize)]
struct DataActionStatus {
    available: bool,
    reason: &'static str,
}

#[derive(Debug, Deserialize)]
struct DeleteAnalysisDataRequest {
    confirmation: String,
}

async fn delete_analysis_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<DeleteAnalysisDataRequest>,
) -> Result<StatusCode, ApiError> {
    let session = require_session(&state, &headers).await?;
    if request.confirmation.trim() != "BORRAR MIS ANALISIS" {
        return Err(ApiError::bad_request(
            "escribe BORRAR MIS ANALISIS para confirmar",
        ));
    }
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    require_org_role(&bundle, &[OrgRole::Owner, OrgRole::Admin])?;
    let mut audit = AnalysisDataDeletionAudit {
        id: worker_request_id().unwrap_or_else(|| Uuid::new_v4().to_string()),
        owner_hash: hash_owner_email(&session.google_account_email),
        status: AnalysisDataDeletionStatus::Requested,
        requested_at: Utc::now(),
        completed_at: None,
    };
    state.storage.record_analysis_data_deletion(&audit).await?;

    match state
        .storage
        .delete_analysis_data(&session.google_account_email)
        .await
    {
        Ok(()) => {
            audit.status = AnalysisDataDeletionStatus::Completed;
            audit.completed_at = Some(Utc::now());
            // ponytail: el borrado y esta auditoría no comparten transacción, así que
            // la escritura del audit puede fallar después de borrar los datos. Dejar el
            // registro en pending es más seguro que afirmar un borrado no confirmado.
            state.storage.record_analysis_data_deletion(&audit).await?;
            Ok(StatusCode::NO_CONTENT)
        }
        Err(error) => {
            tracing::error!(
                operation = "analysis_data_deletion",
                error_code = "data_deletion_failed",
                owner_hash = %audit.owner_hash,
                "analysis data deletion failed"
            );
            audit.status = AnalysisDataDeletionStatus::Failed;
            if state
                .storage
                .record_analysis_data_deletion(&audit)
                .await
                .is_err()
            {
                tracing::error!(
                    operation = "record_analysis_data_deletion",
                    "audit update failed"
                );
            }
            Err(error.into())
        }
    }
}

async fn get_data_summary(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DataSummaryResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    let setup = setup_state(&bundle.draft);
    let runs = state
        .storage
        .list_analysis_runs(&session.google_account_email)
        .await?;
    let mut threads_count = 0usize;
    let mut messages_count = 0usize;
    for run in &runs {
        let threads = state.storage.list_threads(&run.id).await?;
        threads_count += threads.len();
        for thread in &threads {
            messages_count += state
                .storage
                .list_messages(&run.id, &thread.id)
                .await?
                .len();
        }
    }
    let mailbox_connected = bundle.mailbox.revoked_at.is_none();

    Ok(Json(DataSummaryResponse {
        account: DataSummaryAccount {
            google_account_email: session.google_account_email,
            gmail_scope_snapshot: bundle.mailbox.gmail_scope_snapshot.clone(),
            mailbox_connected,
            mailbox_revoked_at: bundle.mailbox.revoked_at,
        },
        org: DataSummaryOrg {
            id: bundle.org.id,
            name: bundle.org.name,
            role: bundle.membership.role,
            policy_version: bundle.policy_version.version,
            setup_ready: setup.ready_for_analysis,
            setup_missing: setup.missing,
        },
        privacy: DataSummaryPrivacy {
            data_minimization_mode: "metadata_snippets_excerpts_only".to_string(),
            ai_enabled: bundle.draft.ai_policy.enabled,
            ai_consent_granted_at: bundle.draft.ai_policy.consent_granted_at,
            retention_days: bundle.draft.retention_policy.retention_days,
            report_mode: bundle.draft.schedule_report_policy.report_content.mode,
        },
        stored_data: StoredDataSummary {
            analysis_runs_count: runs.len(),
            threads_count,
            messages_count,
            ai_audit_records_count: None,
        },
        actions: DataActionAvailability {
            disconnect_gmail: DataActionStatus {
                available: mailbox_connected,
                reason: if mailbox_connected {
                    "requires_confirmation"
                } else {
                    "already_disconnected"
                },
            },
            delete_analysis_data: DataActionStatus {
                available: true,
                reason: "requires_confirmation",
            },
            delete_account_data: DataActionStatus {
                available: false,
                reason: "account_deletion_policy_pending",
            },
        },
    }))
}

#[derive(Debug, Serialize)]
struct OperationsStatusResponse {
    scheduler: SchedulerStatusSummary,
    policy: OperationsPolicySummary,
}

#[derive(Debug, Serialize)]
struct OperationsHistoryResponse {
    entries: Vec<OperationsHistoryEntry>,
    total_count: usize,
}

#[derive(Debug, Serialize)]
struct OperationsHistoryEntry {
    id: String,
    kind: String,
    status: String,
    started_at: chrono::DateTime<Utc>,
    finished_at: Option<chrono::DateTime<Utc>>,
    run_id: Option<String>,
    trigger_type: Option<String>,
    window_date_from: Option<String>,
    window_date_to: Option<String>,
    processed_threads: Option<u64>,
    total_candidate_threads: Option<u64>,
    error_category: Option<String>,
    error_redacted: Option<String>,
}

#[derive(Debug, Serialize)]
struct SchedulerStatusSummary {
    enabled: bool,
    timezone: String,
    analysis_time: String,
    preset: String,
    recipients_count: usize,
    next_run_estimate: Option<String>,
    last_state: Option<ScheduleState>,
    last_error_redacted: Option<String>,
}

#[derive(Debug, Serialize)]
struct OperationsPolicySummary {
    org_id: String,
    policy_version: u64,
    policy_hash: String,
    setup_ready: bool,
    setup_missing: Vec<String>,
}

async fn get_operations_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OperationsStatusResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    let setup = setup_state(&bundle.draft);
    let schedule_config = state
        .storage
        .list_schedule_configs()
        .await?
        .into_iter()
        .find(|config| {
            config
                .user_email
                .eq_ignore_ascii_case(&session.google_account_email)
        });
    let schedule_policy = &bundle.draft.schedule_report_policy;
    let timezone = schedule_config
        .as_ref()
        .map(|config| config.timezone.clone())
        .unwrap_or_else(|| schedule_policy.timezone.clone());
    let enabled = schedule_config
        .as_ref()
        .map(|config| config.enabled)
        .unwrap_or(schedule_policy.scheduler_enabled);
    let analysis_time = schedule_config
        .as_ref()
        .map(|config| config.analysis_time.clone())
        .unwrap_or_else(|| schedule_policy.analysis_time.clone());
    let recipients_count = schedule_config
        .as_ref()
        .map(|config| config.recipients.len())
        .unwrap_or(schedule_policy.report_recipients.len());
    let last_state = state
        .storage
        .get_schedule_state(&session.google_account_email)
        .await?;
    let last_error_redacted = last_state
        .as_ref()
        .and_then(|state| state.error_message.as_deref())
        .map(redact_public_error);

    Ok(Json(OperationsStatusResponse {
        scheduler: SchedulerStatusSummary {
            enabled,
            timezone: timezone.clone(),
            analysis_time: analysis_time.clone(),
            preset: "weekdays_custom_hour_local".to_string(),
            recipients_count,
            next_run_estimate: if enabled {
                next_fire_time_for_days(
                    Utc::now(),
                    &timezone,
                    &analysis_time,
                    &bundle.draft.schedule_report_policy.days_of_week,
                )
            } else {
                None
            },
            last_state,
            last_error_redacted,
        },
        policy: OperationsPolicySummary {
            org_id: bundle.org.id,
            policy_version: bundle.policy_version.version,
            policy_hash: bundle.policy_version.policy_hash,
            setup_ready: setup.ready_for_analysis,
            setup_missing: setup.missing,
        },
    }))
}

async fn get_operations_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<OperationsHistoryResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let limit = query.limit.unwrap_or(10).clamp(1, 50);
    let offset = decode_page_token(query.page_token.as_deref())?;
    let runs = state
        .storage
        .list_analysis_runs(&session.google_account_email)
        .await?;
    let mut entries: Vec<OperationsHistoryEntry> = runs
        .into_iter()
        .map(|run| {
            let error_redacted = run.error_message.as_deref().map(redact_public_error);
            let error_category = run.error_message.as_deref().map(error_category);
            OperationsHistoryEntry {
                id: format!("run:{}", run.id),
                kind: "analysis_run".to_string(),
                status: serde_json::to_value(&run.status)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_else(|| "unknown".to_string()),
                started_at: run.created_at,
                finished_at: run.completed_at,
                run_id: Some(run.id),
                trigger_type: run
                    .trigger_type
                    .and_then(|trigger| serde_json::to_value(trigger).ok())
                    .and_then(|value| value.as_str().map(str::to_string)),
                window_date_from: Some(run.config.date_from),
                window_date_to: Some(run.config.date_to),
                processed_threads: Some(run.processed_threads),
                total_candidate_threads: Some(run.total_candidate_threads),
                error_category,
                error_redacted,
            }
        })
        .collect();

    if let Some(schedule_state) = state
        .storage
        .get_schedule_state(&session.google_account_email)
        .await?
        && !entries
            .iter()
            .any(|entry| entry.run_id.as_deref() == schedule_state.run_id.as_deref())
    {
        let error_redacted = schedule_state
            .error_message
            .as_deref()
            .map(redact_public_error);
        let error_category = schedule_state.error_message.as_deref().map(error_category);
        entries.push(OperationsHistoryEntry {
            id: format!(
                "scheduler:{}:{}",
                schedule_state.window_date_from, schedule_state.window_date_to
            ),
            kind: "scheduler_attempt".to_string(),
            status: serde_json::to_value(&schedule_state.status)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string()),
            started_at: schedule_state.started_at,
            finished_at: Some(schedule_state.updated_at),
            run_id: schedule_state.run_id,
            trigger_type: Some("scheduled".to_string()),
            window_date_from: Some(schedule_state.window_date_from),
            window_date_to: Some(schedule_state.window_date_to),
            processed_threads: None,
            total_candidate_threads: None,
            error_category,
            error_redacted,
        });
    }

    entries.sort_by_key(|entry| entry.started_at);
    entries.reverse();
    let total_count = entries.len();
    let entries = entries.into_iter().skip(offset).take(limit).collect();
    Ok(Json(OperationsHistoryResponse {
        entries,
        total_count,
    }))
}

fn error_category(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    if lower.contains("oauth") || lower.contains("token") || lower.contains("unauthorized") {
        "auth".to_string()
    } else if lower.contains("gmail") || lower.contains("google") {
        "gmail_api".to_string()
    } else if lower.contains("ai") || lower.contains("bedrock") || lower.contains("worker") {
        "ai_worker".to_string()
    } else if lower.contains("resend") || lower.contains("report") || lower.contains("email") {
        "report_delivery".to_string()
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "timeout".to_string()
    } else if lower.contains("rate") || lower.contains("429") {
        "rate_limited".to_string()
    } else if lower.contains("config") || lower.contains("schedule") {
        "configuration".to_string()
    } else {
        "unknown".to_string()
    }
}

fn redact_public_error(message: &str) -> String {
    let mut redacted = message.replace(&['\n', '\r'][..], " ");
    for marker in ["Bearer ", "refresh_token", "access_token", "client_secret"] {
        if redacted
            .to_ascii_lowercase()
            .contains(&marker.to_ascii_lowercase())
        {
            redacted = "Error operativo redacted; revisa logs internos".to_string();
            break;
        }
    }
    redacted.chars().take(240).collect()
}

async fn get_org_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OrgConfigResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    let unrestricted = state
        .config
        .is_privileged_account(&session.google_account_email);
    let metadata = state
        .storage
        .get_mailbox_metadata(&session.google_account_email)
        .await
        .unwrap_or(None);
    Ok(Json(bundle.response(unrestricted, metadata)))
}

#[derive(Debug, Deserialize)]
struct FilterPresetRequest {
    name: String,
    #[serde(default)]
    include_labels: Vec<String>,
    #[serde(default)]
    exclude_labels: Vec<String>,
    #[serde(default)]
    ignored_senders: Vec<String>,
    #[serde(default)]
    ignored_domains: Vec<String>,
    #[serde(default)]
    ignored_keywords: Vec<String>,
    #[serde(default)]
    is_default: bool,
}

async fn list_filter_presets_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<FilterPreset>>, ApiError> {
    let session = require_session(&state, &headers).await?;
    Ok(Json(
        state
            .storage
            .list_filter_presets(&session.google_account_email)
            .await?,
    ))
}

async fn create_filter_preset(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FilterPresetRequest>,
) -> Result<Json<FilterPreset>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    require_org_role(&bundle, &[OrgRole::Owner, OrgRole::Admin, OrgRole::Analyst])?;
    let name = request.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("el preset necesita un nombre"));
    }
    let now = Utc::now();
    let preset = FilterPreset {
        id: Uuid::new_v4().to_string(),
        owner_email: session.google_account_email.clone(),
        name,
        include_labels: normalize_text_list(request.include_labels),
        exclude_labels: normalize_text_list(request.exclude_labels),
        ignored_senders: normalize_list(request.ignored_senders),
        ignored_domains: normalize_domains(request.ignored_domains),
        ignored_keywords: normalize_text_list(request.ignored_keywords),
        is_default: request.is_default,
        created_at: now,
        updated_at: now,
    };
    if preset.is_default {
        clear_other_default_presets(&state, &session.google_account_email, &preset.id).await?;
    }
    state.storage.upsert_filter_preset(&preset).await?;
    Ok(Json(preset))
}

async fn update_filter_preset_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<FilterPresetRequest>,
) -> Result<Json<FilterPreset>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    require_org_role(&bundle, &[OrgRole::Owner, OrgRole::Admin, OrgRole::Analyst])?;
    let mut preset = state
        .storage
        .list_filter_presets(&session.google_account_email)
        .await?
        .into_iter()
        .find(|preset| preset.id == id)
        .ok_or(ApiError::not_found("preset no encontrado"))?;
    let name = request.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("el preset necesita un nombre"));
    }
    preset.name = name;
    preset.include_labels = normalize_text_list(request.include_labels);
    preset.exclude_labels = normalize_text_list(request.exclude_labels);
    preset.ignored_senders = normalize_list(request.ignored_senders);
    preset.ignored_domains = normalize_domains(request.ignored_domains);
    preset.ignored_keywords = normalize_text_list(request.ignored_keywords);
    preset.is_default = request.is_default;
    preset.updated_at = Utc::now();
    if preset.is_default {
        clear_other_default_presets(&state, &session.google_account_email, &preset.id).await?;
    }
    state.storage.upsert_filter_preset(&preset).await?;
    Ok(Json(preset))
}

async fn delete_filter_preset_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    require_org_role(&bundle, &[OrgRole::Owner, OrgRole::Admin, OrgRole::Analyst])?;
    state
        .storage
        .delete_filter_preset(&session.google_account_email, &id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Garantiza un solo preset por defecto: desmarca el resto del usuario.
async fn clear_other_default_presets(
    state: &AppState,
    owner_email: &str,
    keep_id: &str,
) -> Result<(), ApiError> {
    let presets = state.storage.list_filter_presets(owner_email).await?;
    for mut preset in presets {
        if preset.id != keep_id && preset.is_default {
            preset.is_default = false;
            preset.updated_at = Utc::now();
            state.storage.upsert_filter_preset(&preset).await?;
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize, Default)]
struct OrgConfigUpdateRequest {
    #[serde(default)]
    finalize: bool,
    org: Option<OrgUpdateRequest>,
    mailbox: Option<MailboxUpdateRequest>,
    analysis_policy: Option<AnalysisPolicyUpdateRequest>,
    ai_policy: Option<AiPolicyUpdateRequest>,
    schedule_report_policy: Option<ScheduleReportPolicyUpdateRequest>,
    retention_policy: Option<RetentionPolicyUpdateRequest>,
}

#[derive(Debug, Deserialize)]
struct OrgUpdateRequest {
    name: Option<String>,
    default_timezone: Option<String>,
    locale: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MailboxUpdateRequest {
    display_name: Option<String>,
    purpose: Option<MailboxPurpose>,
}

#[derive(Debug, Deserialize)]
struct AnalysisPolicyUpdateRequest {
    request_scope: Option<RequestScope>,
    timezone: Option<String>,
    internal_domains: Option<Vec<String>>,
    responder_emails: Option<Vec<String>>,
    mailbox_aliases: Option<Vec<String>>,
    valid_request_criteria: Option<Vec<String>>,
    non_responsibility_rules: Option<Vec<String>>,
    ignored_senders: Option<Vec<String>>,
    ignored_domains: Option<Vec<String>>,
    ignored_keywords: Option<Vec<String>>,
    valid_signal_keywords: Option<Vec<String>>,
    count_historical_closures_as_valid: Option<bool>,
    count_previous_request_followups_as_valid: Option<bool>,
    count_org_hosted_training_as_valid: Option<bool>,
    include_labels: Option<Vec<String>>,
    exclude_labels: Option<Vec<String>>,
    default_time_from: Option<String>,
    default_time_to: Option<String>,
    max_threads_per_run: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct AiPolicyUpdateRequest {
    enabled: Option<bool>,
    consent_confirmed: Option<bool>,
    auto_apply_threshold: Option<f64>,
    manual_review_threshold: Option<f64>,
    max_audit_messages: Option<u32>,
    max_body_chars_per_message: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ScheduleReportPolicyUpdateRequest {
    days_of_week: Option<Vec<u8>>,
    scheduler_enabled: Option<bool>,
    timezone: Option<String>,
    analysis_time: Option<String>,
    report_recipients: Option<Vec<String>>,
    report_content: Option<crate::policies::ReportContentPolicy>,
    failure_notice_enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RetentionPolicyUpdateRequest {
    retention_days: Option<u32>,
}

#[derive(Debug, Serialize)]
struct UpdateOrgConfigResponse {
    policy_version: PolicyVersion,
    setup_state: crate::policies::SetupState,
}

async fn update_org_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<OrgConfigUpdateRequest>,
) -> Result<Json<UpdateOrgConfigResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let mut bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    require_org_role(&bundle, &[OrgRole::Owner, OrgRole::Admin])?;
    let unrestricted = state
        .config
        .is_privileged_account(&session.google_account_email);
    let finalize = request.finalize;
    apply_org_config_update(&mut bundle, request, unrestricted)?;
    let next_setup = setup_state(&bundle.draft);
    if finalize && !next_setup.ready_for_analysis {
        return Err(ApiError::incomplete_config(next_setup.missing));
    }
    let now = Utc::now();
    bundle.org.updated_at = now;
    bundle.draft.updated_at = now;
    bundle.draft.updated_by_user_email = session.google_account_email.clone();
    let next_version = bundle.policy_version.version + 1;
    let new_version = policy_version_from_draft(
        &bundle.mailbox,
        &bundle.draft,
        next_version,
        &session.google_account_email,
        now,
    );
    if new_version.policy_hash != bundle.policy_version.policy_hash {
        bundle.policy_version = new_version;
    }
    state.storage.upsert_org_config(&bundle).await?;
    sync_schedule_config_from_policy(&state, &bundle).await?;
    Ok(Json(UpdateOrgConfigResponse {
        policy_version: bundle.policy_version,
        setup_state: next_setup,
    }))
}

async fn sync_schedule_config_from_policy(
    state: &AppState,
    bundle: &OrgConfigBundle,
) -> Result<(), ApiError> {
    let analysis = &bundle.draft.analysis_policy;
    let schedule = &bundle.draft.schedule_report_policy;
    let gmail_connection = state
        .storage
        .get_gmail_connection(&bundle.membership.user_email)
        .await?;
    // Una configuración leída antes de una desconexión o de `user.deleted` no
    // puede volver a activar el scheduler. La preferencia se conserva y se
    // aplicará al reconectar Gmail.
    let enabled =
        schedule.scheduler_enabled && mailbox_connection_is_active(gmail_connection.as_ref());
    state
        .storage
        .upsert_schedule_config(&ScheduleConfig {
            user_email: bundle.membership.user_email.clone(),
            enabled,
            recipients: schedule.report_recipients.clone(),
            internal_domains: analysis.internal_domains.clone(),
            ignored_senders: analysis.ignored_senders.clone(),
            ignored_domains: analysis.ignored_domains.clone(),
            ignored_keywords: analysis.ignored_keywords.clone(),
            timezone: schedule.timezone.clone(),
            analysis_time: schedule.analysis_time.clone(),
            days_of_week: schedule.days_of_week.clone(),
            gmail_max_threads: Some(analysis.max_threads_per_run),
            updated_at: Utc::now(),
        })
        .await?;
    Ok(())
}

fn apply_org_config_update(
    bundle: &mut OrgConfigBundle,
    request: OrgConfigUpdateRequest,
    bypass_ai_consent: bool,
) -> Result<(), ApiError> {
    if let Some(org) = request.org {
        if let Some(name) = org.name.map(|value| value.trim().to_string())
            && !name.is_empty()
        {
            bundle.org.name = name;
        }
        if let Some(timezone) = org.default_timezone {
            validate_policy_timezone(&timezone)?;
            bundle.org.default_timezone = timezone.clone();
            bundle.draft.analysis_policy.timezone = timezone.clone();
            bundle.draft.schedule_report_policy.timezone = timezone;
        }
        if let Some(locale) = org.locale.map(|value| value.trim().to_string())
            && !locale.is_empty()
        {
            bundle.org.locale = locale;
        }
    }
    if let Some(mailbox) = request.mailbox {
        if let Some(display_name) = mailbox.display_name.map(|value| value.trim().to_string())
            && !display_name.is_empty()
        {
            bundle.mailbox.display_name = display_name;
        }
        if let Some(purpose) = mailbox.purpose {
            bundle.mailbox.purpose = purpose;
        }
    }
    if let Some(policy) = request.analysis_policy {
        if policy.mailbox_aliases.is_some() {
            bundle.draft.mailbox_aliases_configured = true;
        }
        apply_analysis_policy_update(&mut bundle.draft.analysis_policy, policy)?;
    }
    if let Some(policy) = request.ai_policy {
        apply_ai_policy_update(&mut bundle.draft.ai_policy, policy, bypass_ai_consent)?;
    }
    if let Some(policy) = request.schedule_report_policy {
        apply_schedule_report_policy_update(&mut bundle.draft.schedule_report_policy, policy)?;
    }
    if let Some(policy) = request.retention_policy
        && let Some(days) = policy.retention_days
    {
        if !(7..=365).contains(&days) {
            return Err(ApiError::bad_request(
                "retention_days debe estar entre 7 y 365",
            ));
        }
        bundle.draft.retention_policy.retention_days = days;
    }
    Ok(())
}

fn apply_analysis_policy_update(
    current: &mut AnalysisPolicy,
    update: AnalysisPolicyUpdateRequest,
) -> Result<(), ApiError> {
    if let Some(scope) = update.request_scope {
        current.request_scope = scope;
    }
    if let Some(timezone) = update.timezone {
        validate_policy_timezone(&timezone)?;
        current.timezone = timezone;
    }
    if let Some(values) = update.internal_domains {
        current.internal_domains = normalize_domains(values);
        if current
            .internal_domains
            .iter()
            .any(|domain| crate::policies::is_public_mail_domain(domain))
        {
            return Err(ApiError::bad_request(
                "los dominios de correo público no identifican al equipo; configura responsables por dirección",
            ));
        }
    }
    if let Some(values) = update.responder_emails {
        current.responder_emails = normalize_list(values);
        validate_emails(&current.responder_emails)?;
    }
    if let Some(values) = update.mailbox_aliases {
        current.mailbox_aliases = normalize_list(values);
    }
    if let Some(values) = update.valid_request_criteria {
        current.valid_request_criteria = normalize_text_list(values);
    }
    if let Some(values) = update.non_responsibility_rules {
        current.non_responsibility_rules = normalize_text_list(values);
    }
    if let Some(values) = update.ignored_senders {
        current.ignored_senders = normalize_list(values);
    }
    if let Some(values) = update.ignored_domains {
        current.ignored_domains = normalize_domains(values);
    }
    if let Some(values) = update.ignored_keywords {
        current.ignored_keywords = normalize_text_list(values);
    }
    if let Some(values) = update.valid_signal_keywords {
        current.valid_signal_keywords = normalize_text_list(values);
    }
    if let Some(value) = update.count_historical_closures_as_valid {
        current.count_historical_closures_as_valid = value;
    }
    if let Some(value) = update.count_previous_request_followups_as_valid {
        current.count_previous_request_followups_as_valid = value;
    }
    if let Some(value) = update.count_org_hosted_training_as_valid {
        current.count_org_hosted_training_as_valid = value;
    }
    if let Some(values) = update.include_labels {
        current.include_labels = normalize_text_list(values);
    }
    if let Some(values) = update.exclude_labels {
        current.exclude_labels = normalize_text_list(values);
    }
    if let Some(value) = update.default_time_from {
        current.default_time_from = validate_time(&value)?;
    }
    if let Some(value) = update.default_time_to {
        current.default_time_to = validate_time(&value)?;
    }
    if let Some(value) = update.max_threads_per_run {
        current.max_threads_per_run = value.clamp(1, 500);
    }
    Ok(())
}

fn apply_ai_policy_update(
    current: &mut AiPolicy,
    update: AiPolicyUpdateRequest,
    bypass_consent: bool,
) -> Result<(), ApiError> {
    if let Some(threshold) = update.auto_apply_threshold {
        validate_threshold(threshold, "auto_apply_threshold")?;
        current.auto_apply_threshold = threshold;
    }
    if let Some(threshold) = update.manual_review_threshold {
        validate_threshold(threshold, "manual_review_threshold")?;
        current.manual_review_threshold = threshold;
    }
    if let Some(value) = update.max_audit_messages {
        current.max_audit_messages = value.clamp(1, 50);
    }
    if let Some(value) = update.max_body_chars_per_message {
        current.max_body_chars_per_message = value.clamp(80, 2000);
    }
    if let Some(enabled) = update.enabled {
        if enabled && !current.enabled && !bypass_consent && update.consent_confirmed != Some(true)
        {
            return Err(ApiError::bad_request(
                "para activar IA debes confirmar consentimiento explícito",
            ));
        }
        current.enabled = enabled;
        current.consent_granted_at = if enabled {
            current.consent_granted_at.or_else(|| Some(Utc::now()))
        } else {
            None
        };
    }
    Ok(())
}

fn apply_schedule_report_policy_update(
    current: &mut ScheduleReportPolicy,
    update: ScheduleReportPolicyUpdateRequest,
) -> Result<(), ApiError> {
    if let Some(mut days) = update.days_of_week {
        if days.is_empty() || days.iter().any(|day| !(1..=7).contains(day)) {
            return Err(ApiError::bad_request(
                "days_of_week debe contener días ISO entre 1 y 7",
            ));
        }
        days.sort_unstable();
        days.dedup();
        current.days_of_week = days;
    }
    if let Some(timezone) = update.timezone {
        validate_policy_timezone(&timezone)?;
        current.timezone = timezone;
    }
    if let Some(value) = update.analysis_time {
        let value = validate_time(&value)?;
        current.analysis_time = value;
    }
    if let Some(values) = update.report_recipients {
        current.report_recipients = normalize_list(values);
        validate_emails(&current.report_recipients)?;
    }
    if let Some(content) = update.report_content {
        current.report_content = content;
    }
    if let Some(value) = update.failure_notice_enabled {
        current.failure_notice_enabled = value;
    }
    if let Some(enabled) = update.scheduler_enabled {
        if enabled && current.report_recipients.is_empty() {
            return Err(ApiError::bad_request(
                "agrega destinatarios antes de activar reportes programados",
            ));
        }
        current.scheduler_enabled = enabled;
    }
    Ok(())
}

fn normalize_text_list(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let value = value.trim().to_string();
        if !value.is_empty() && !out.contains(&value) {
            out.push(value);
        }
    }
    out
}

fn validate_policy_timezone(timezone: &str) -> Result<(), ApiError> {
    if validate_timezone(timezone) {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "timezone inválida; usa una zona IANA",
        ))
    }
}

fn validate_emails(values: &[String]) -> Result<(), ApiError> {
    if values.iter().any(|email| {
        email.len() > 254
            || email.chars().any(char::is_whitespace)
            || email.split_once('@').is_none_or(|(local, domain)| {
                local.is_empty() || !domain.contains('.') || domain.contains('@')
            })
    }) {
        return Err(ApiError::bad_request("dirección de correo inválida"));
    }
    Ok(())
}

fn validate_date_range(from: &str, to: &str) -> Result<(), ApiError> {
    let parse = |value: &str| {
        chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map_err(|_| ApiError::bad_request("fecha inválida; usa YYYY-MM-DD"))
    };
    let from = parse(from)?;
    let to = parse(to)?;
    if from > to || (to - from).num_days() > 365 {
        return Err(ApiError::bad_request(
            "elige un período ordenado de hasta 366 días",
        ));
    }
    Ok(())
}

fn require_org_role(bundle: &OrgConfigBundle, roles: &[OrgRole]) -> Result<(), ApiError> {
    if bundle.org.status == OrganizationStatus::Disabled
        || bundle.membership.status != MembershipStatus::Active
        || bundle.membership.org_id != bundle.org.id
        || !roles.contains(&bundle.membership.role)
    {
        return Err(ApiError::forbidden(
            "no tienes permiso para realizar esta acción en la organización",
        ));
    }
    Ok(())
}

fn validate_time(value: &str) -> Result<String, ApiError> {
    chrono::NaiveTime::parse_from_str(value, "%H:%M")
        .map(|_| value.to_string())
        .map_err(|_| ApiError::bad_request("hora inválida; usa formato HH:MM"))
}

fn validate_threshold(value: f64, label: &str) -> Result<(), ApiError> {
    if (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(ApiError::bad_request(&format!(
            "{label} debe estar entre 0 y 1"
        )))
    }
}

async fn get_or_provision_org_config(
    state: &AppState,
    user_email: &str,
) -> Result<OrgConfigBundle, ApiError> {
    if let Some(mut bundle) = state.storage.get_org_config_for_user(user_email).await? {
        if !bundle
            .membership
            .user_email
            .eq_ignore_ascii_case(user_email)
        {
            return Err(ApiError::forbidden("membresía inválida"));
        }
        require_org_role(
            &bundle,
            &[
                OrgRole::Owner,
                OrgRole::Admin,
                OrgRole::Analyst,
                OrgRole::Viewer,
            ],
        )?;
        let now = Utc::now();
        // Migraciones perezosas e idempotentes: preservan decisiones explícitas
        // y crean una versión de política que el próximo análisis sí puede usar.
        let defaults_changed = apply_ai_defaults_migration(&mut bundle.draft.ai_policy, now);
        let threshold_changed = apply_ai_threshold_migration(&mut bundle.draft.ai_policy);
        let prompt_changed = apply_ai_prompt_version_migration(&mut bundle.draft.ai_policy);
        let previous_domains = bundle.draft.analysis_policy.internal_domains.len();
        bundle
            .draft
            .analysis_policy
            .internal_domains
            .retain(|domain| !crate::policies::is_public_mail_domain(domain));
        let domains_changed =
            previous_domains != bundle.draft.analysis_policy.internal_domains.len();
        if defaults_changed || threshold_changed || prompt_changed || domains_changed {
            bundle.draft.updated_at = now;
            bundle.policy_version = policy_version_from_draft(
                &bundle.mailbox,
                &bundle.draft,
                bundle.policy_version.version + 1,
                user_email,
                now,
            );
            state.storage.upsert_org_config(&bundle).await?;
        }
        return Ok(bundle);
    }
    let mut bundle = provision_default_config(user_email, Utc::now());
    if let Some(account) = state.storage.get_account_by_email(user_email).await? {
        bundle.org.id = account.org_id.clone();
        bundle.membership.org_id = account.org_id.clone();
        bundle.mailbox.org_id = account.org_id.clone();
        bundle.draft.org_id = account.org_id.clone();
        bundle.policy_version.org_id = account.org_id;
    }
    state.storage.upsert_org_config(&bundle).await?;
    Ok(bundle)
}

#[derive(Debug, Deserialize)]
struct CreateAnalysisRunRequest {
    date_from: String,
    date_to: String,
    time_from: Option<String>,
    time_to: Option<String>,
    timezone: Option<String>,
    #[serde(default)]
    internal_domains: Vec<String>,
    #[serde(default)]
    ignored_senders: Vec<String>,
    #[serde(default)]
    ignored_domains: Vec<String>,
    #[serde(default)]
    ignored_keywords: Vec<String>,
    #[serde(default)]
    include_labels: Vec<String>,
    #[serde(default)]
    exclude_labels: Vec<String>,
    #[serde(default)]
    policy_version_id: Option<String>,
}

async fn create_analysis_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateAnalysisRunRequest>,
) -> Result<Json<AnalysisRun>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = require_active_entitlement(&state, &session).await?;
    enforce_usage_allows_run(&state, &bundle.org.id, &session.google_account_email).await?;
    require_mailbox_connected(
        state
            .storage
            .get_gmail_connection(&session.google_account_email)
            .await?
            .as_ref(),
    )?;
    enforce_rate_limit(
        &state,
        &session.google_account_email,
        "analysis_create",
        state.config.rate_limit.analysis_create_per_hour,
    )
    .await?;
    let now = Utc::now();
    require_org_role(&bundle, &[OrgRole::Owner, OrgRole::Admin, OrgRole::Analyst])?;
    let run = build_policy_run(&state, &session.google_account_email, request, now).await?;
    reserve_run_creation(&state, &run).await?;
    if let Err(error) = state.storage.create_analysis_run(&run).await {
        state
            .storage
            .settle_analysis_usage(&run.id, UsageAmounts::default())
            .await?;
        return Err(error.into());
    }
    tracing::info!(
        operation = "analysis_run_created",
        run_id = %run.id,
        "analysis run created"
    );
    Ok(Json(run))
}

async fn build_policy_run(
    state: &AppState,
    user_email: &str,
    request: CreateAnalysisRunRequest,
    now: chrono::DateTime<Utc>,
) -> Result<AnalysisRun, ApiError> {
    validate_date_range(&request.date_from, &request.date_to)?;
    let bundle = get_or_provision_org_config(state, user_email).await?;
    if request.timezone.is_some()
        || !request.internal_domains.is_empty()
        || !request.ignored_senders.is_empty()
        || !request.ignored_domains.is_empty()
        || !request.ignored_keywords.is_empty()
    {
        return Err(ApiError::bad_request(
            "configura zona horaria, responsables y exclusiones en la política de organización",
        ));
    }
    let policy_version = if let Some(policy_version_id) = request.policy_version_id.as_deref() {
        if policy_version_id != bundle.policy_version.id {
            return Err(ApiError::conflict(
                "la política cambió; actualiza la configuración antes de analizar",
            ));
        }
        state
            .storage
            .get_policy_version(&bundle.org.id, policy_version_id)
            .await?
            .ok_or(ApiError::not_found("policy version not found"))?
    } else {
        bundle.policy_version.clone()
    };
    let current_setup = setup_state(&bundle.draft);
    if !current_setup.ready_for_analysis && !state.config.is_privileged_account(user_email) {
        return Err(ApiError::bad_request(
            "completa la configuración antes de crear análisis desde policy",
        ));
    }
    let snapshot = policy_version.snapshot.clone();
    let analysis = &snapshot.analysis_policy;
    let time_from = validate_time(
        request
            .time_from
            .as_deref()
            .unwrap_or(&analysis.default_time_from),
    )?;
    let time_to = validate_time(
        request
            .time_to
            .as_deref()
            .unwrap_or(&analysis.default_time_to),
    )?;
    if time_from > time_to {
        return Err(ApiError::bad_request(
            "la hora inicial no puede ser posterior a la final",
        ));
    }
    // La selección de etiquetas del request (por-run) tiene prioridad; si viene
    // vacía, se usa la persistida en la política.
    let include_labels = if request.include_labels.is_empty() {
        analysis.include_labels.clone()
    } else {
        request.include_labels.clone()
    };
    let exclude_labels = if request.exclude_labels.is_empty() {
        analysis.exclude_labels.clone()
    } else {
        request.exclude_labels.clone()
    };
    Ok(AnalysisRun {
        id: Uuid::new_v4().to_string(),
        user_email: user_email.to_string(),
        org_id: Some(bundle.org.id),
        mailbox_id: Some(snapshot.mailbox.id.clone()),
        trigger_type: Some(TriggerType::Manual),
        policy_version_id: Some(policy_version.id),
        policy_hash: Some(policy_version.policy_hash),
        policy_snapshot: Some(snapshot.clone()),
        gmail_scope_snapshot: snapshot.mailbox.gmail_scope_snapshot.clone(),
        retention_expires_at: Some(retention_expires_at(now, &snapshot)),
        data_minimization_mode: Some("metadata_snippets_excerpts_only".to_string()),
        config: AnalysisConfig {
            date_from: request.date_from,
            date_to: request.date_to,
            time_from,
            time_to,
            timezone: analysis.timezone.clone(),
            internal_domains: analysis.internal_domains.clone(),
            responder_emails: crate::policies::responders_for_mailbox(
                analysis,
                &snapshot.mailbox.google_account_email,
            ),
            request_scope: analysis.request_scope,
            ignored_senders: analysis.ignored_senders.clone(),
            ignored_domains: analysis.ignored_domains.clone(),
            ignored_keywords: analysis.ignored_keywords.clone(),
            include_labels,
            exclude_labels,
        },
        status: AnalysisStatus::Pending,
        progress_message: "Listo para analizar".to_string(),
        processed_threads: 0,
        total_candidate_threads: 0,
        metrics: AnalysisMetrics::default(),
        created_at: now,
        completed_at: None,
        error_message: None,
    })
}

async fn list_analysis_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PaginationQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let session = require_session(&state, &headers).await?;
    require_entitlement(&state, &session).await?;
    let runs = state
        .storage
        .list_analysis_runs(&session.google_account_email)
        .await?
        .into_iter()
        .filter(|run| run.retention_deadline() > Utc::now())
        .collect();
    paginated_or_legacy_response(runs, &query)
}

async fn get_analysis_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<AnalysisRun>, ApiError> {
    let session = require_session(&state, &headers).await?;
    require_entitlement(&state, &session).await?;
    Ok(Json(require_owned_run(&state, &id, &session).await?))
}

async fn start_analysis_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<AnalysisRun>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = require_active_entitlement(&state, &session).await?;
    require_org_role(&bundle, &[OrgRole::Owner, OrgRole::Admin, OrgRole::Analyst])?;
    let connection = state
        .storage
        .get_gmail_connection(&session.google_account_email)
        .await?
        .filter(|connection| mailbox_connection_is_active(Some(connection)))
        .ok_or_else(|| ApiError::forbidden("conecta una casilla antes de analizar"))?;
    let run = require_owned_run(&state, &id, &session).await?;
    if run.status != AnalysisStatus::Pending {
        return Err(ApiError::conflict(
            "el análisis solo puede iniciarse cuando está pendiente",
        ));
    }
    if run
        .mailbox_id
        .as_deref()
        .is_none_or(|mailbox_id| mailbox_id != bundle.mailbox.id)
    {
        return Err(ApiError::conflict(
            "la casilla cambió; crea un análisis con la configuración actual",
        ));
    }
    enforce_rate_limit(
        &state,
        &session.google_account_email,
        "analysis_start",
        state.config.rate_limit.analysis_start_per_hour,
    )
    .await?;
    let access_token = fresh_mailbox_access_token(&state, &connection).await?;
    let run = state
        .storage
        .claim_pending_analysis_run(&run.id)
        .await?
        .ok_or_else(|| {
            ApiError::conflict("el análisis solo puede iniciarse cuando está pendiente")
        })?;

    let worker_state = state.clone();
    let run_id = run.id.clone();
    tokio::spawn(async move {
        if let Err(error) =
            execute_analysis(worker_state.clone(), run_id.clone(), access_token).await
        {
            mark_run_failed(&worker_state, &run_id, &error).await;
        }
    });

    Ok(Json(run))
}

async fn analysis_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let session = require_session(&state, &headers).await?;
    require_entitlement(&state, &session).await?;
    require_owned_run(&state, &id, &session).await?;
    let stream = stream::unfold((), move |_| {
        let state = state.clone();
        let id = id.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let payload = match state.storage.get_analysis_run(&id).await {
                Ok(Some(run)) => serde_json::to_string(&run).unwrap_or_else(|_| "{}".to_string()),
                Ok(None) => json!({
                    "error": {
                        "code": "NOT_FOUND",
                        "message": "No se encontró el análisis solicitado."
                    }
                })
                .to_string(),
                Err(_) => json!({
                    "error": {
                        "code": "ANALYSIS_STATE_UNAVAILABLE",
                        "message": "No se pudo consultar el estado del análisis."
                    }
                })
                .to_string(),
            };
            Some((Ok(Event::default().data(payload)), ()))
        }
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn get_metrics(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<AnalysisMetrics>, ApiError> {
    let session = require_session(&state, &headers).await?;
    require_entitlement(&state, &session).await?;
    let run = require_owned_run(&state, &id, &session).await?;
    Ok(Json(run.metrics))
}

async fn list_threads(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<ThreadQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let session = require_session(&state, &headers).await?;
    require_entitlement(&state, &session).await?;
    let run = require_owned_run(&state, &id, &session).await?;
    let mut threads = state.storage.list_threads(&run.id).await?;
    if let Some(classification) = &query.classification {
        threads.retain(|thread| {
            serde_json::to_value(&thread.classification).ok() == Some(json!(classification))
        });
    }
    if let Some(answered) = query.answered {
        threads.retain(|thread| thread.is_answered == answered);
    }
    if let Some(manual_review_required) = query.manual_review_required {
        threads.retain(|thread| thread.manual_review_required == manual_review_required);
    }
    paginated_or_legacy_response(threads, &query.pagination())
}

#[derive(Debug, Deserialize, Default)]
struct PaginationQuery {
    limit: Option<usize>,
    page_token: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ThreadQuery {
    classification: Option<String>,
    answered: Option<bool>,
    manual_review_required: Option<bool>,
    limit: Option<usize>,
    page_token: Option<String>,
}

impl ThreadQuery {
    fn pagination(&self) -> PaginationQuery {
        PaginationQuery {
            limit: self.limit,
            page_token: self.page_token.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct PaginatedResponse<T> {
    items: Vec<T>,
    next_page_token: Option<String>,
    total_count: usize,
}

fn paginated_or_legacy_response<T: Serialize>(
    items: Vec<T>,
    query: &PaginationQuery,
) -> Result<axum::response::Response, ApiError> {
    if query.limit.is_none() && query.page_token.is_none() {
        return Ok(Json(items).into_response());
    }
    let total_count = items.len();
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = decode_page_token(query.page_token.as_deref())?;
    let page_items: Vec<T> = items.into_iter().skip(offset).take(limit).collect();
    let next_offset = offset + page_items.len();
    let next_page_token = if next_offset < total_count {
        Some(encode_page_token(next_offset))
    } else {
        None
    };
    Ok(Json(PaginatedResponse {
        items: page_items,
        next_page_token,
        total_count,
    })
    .into_response())
}

fn encode_page_token(offset: usize) -> String {
    URL_SAFE_NO_PAD.encode(offset.to_string())
}

fn decode_page_token(token: Option<&str>) -> Result<usize, ApiError> {
    let Some(token) = token else {
        return Ok(0);
    };
    let raw = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| ApiError::bad_request("page_token inválido"))?;
    let decoded =
        String::from_utf8(raw).map_err(|_| ApiError::bad_request("page_token inválido"))?;
    decoded
        .parse::<usize>()
        .map_err(|_| ApiError::bad_request("page_token inválido"))
}

async fn get_thread_for_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((run_id, thread_id)): Path<(String, String)>,
) -> Result<Json<ThreadDetailResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    require_entitlement(&state, &session).await?;
    let thread = require_owned_thread(&state, &run_id, &thread_id, &session).await?;
    thread_detail_response(&state, thread).await
}

async fn get_thread_legacy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<String>,
) -> Result<Json<ThreadDetailResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    require_entitlement(&state, &session).await?;
    let thread = require_unique_owned_thread(&state, &thread_id, &session).await?;
    thread_detail_response(&state, thread).await
}

async fn thread_detail_response(
    state: &AppState,
    mut thread: EmailThread,
) -> Result<Json<ThreadDetailResponse>, ApiError> {
    let messages = state
        .storage
        .list_messages(&thread.analysis_run_id, &thread.id)
        .await?;
    if thread.first_message_at.is_none() {
        thread.first_message_at = messages.iter().map(|message| message.date).min();
    }
    Ok(Json(ThreadDetailResponse { thread, messages }))
}

#[derive(Debug, Serialize)]
struct ThreadDetailResponse {
    thread: EmailThread,
    messages: Vec<EmailMessage>,
}

#[derive(Debug, Deserialize)]
struct ManualReviewRequest {
    new_classification: crate::analysis::Classification,
    is_answered: bool,
    first_client_message_id: Option<String>,
    first_internal_reply_message_id: Option<String>,
    last_internal_message_id: Option<String>,
    notes: Option<String>,
}

async fn manual_review_for_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((run_id, thread_id)): Path<(String, String)>,
    Json(request): Json<ManualReviewRequest>,
) -> Result<Json<AnalysisRun>, ApiError> {
    let session = require_session(&state, &headers).await?;
    require_entitlement(&state, &session).await?;
    let thread = require_owned_thread(&state, &run_id, &thread_id, &session).await?;
    apply_manual_review_request(&state, &session, thread, request).await
}

async fn manual_review_legacy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<String>,
    Json(request): Json<ManualReviewRequest>,
) -> Result<Json<AnalysisRun>, ApiError> {
    let session = require_session(&state, &headers).await?;
    require_entitlement(&state, &session).await?;
    let thread = require_unique_owned_thread(&state, &thread_id, &session).await?;
    apply_manual_review_request(&state, &session, thread, request).await
}

async fn apply_manual_review_request(
    state: &AppState,
    session: &UserSession,
    thread: EmailThread,
    request: ManualReviewRequest,
) -> Result<Json<AnalysisRun>, ApiError> {
    let bundle = require_active_entitlement(state, session).await?;
    require_org_role(&bundle, &[OrgRole::Owner, OrgRole::Admin, OrgRole::Analyst])?;
    let run = require_owned_run(state, &thread.analysis_run_id, session).await?;
    let messages = state
        .storage
        .list_messages(&thread.analysis_run_id, &thread.id)
        .await?;
    let find = |id: &Option<String>| -> Result<Option<&EmailMessage>, ApiError> {
        id.as_ref()
            .map(|id| {
                messages
                    .iter()
                    .find(|message| &message.id == id)
                    .ok_or_else(|| {
                        ApiError::bad_request("el mensaje elegido no pertenece a la conversación")
                    })
            })
            .transpose()
    };
    let client = find(&request.first_client_message_id)?;
    let reply = find(&request.first_internal_reply_message_id)?;
    let last = find(&request.last_internal_message_id)?;
    if client.is_some_and(|m| !crate::analysis::message_is_requester(m, &run.config))
        || reply.is_some_and(|m| !m.is_internal || m.is_automated)
        || last.is_some_and(|m| !m.is_internal || m.is_automated)
    {
        return Err(ApiError::bad_request(
            "selecciona una solicitud humana y respuestas del equipo",
        ));
    }
    if client.is_some_and(|m| !message_is_inside_analysis_window(m, &run.config))
        || matches!(
            request.new_classification,
            crate::analysis::Classification::ValidClientRequest
        ) && client.is_none()
    {
        return Err(ApiError::bad_request(
            "selecciona la solicitud válida dentro del período analizado",
        ));
    }
    if client.is_some_and(|c| {
        reply.into_iter().chain(last).any(|r| {
            !crate::analysis::reply_reaches_requester(r, c)
                && !crate::analysis::reply_recipient_unknown(r, c)
        })
    }) {
        return Err(ApiError::bad_request(
            "la respuesta elegida debe estar dirigida al solicitante",
        ));
    }
    if request.is_answered && (client.is_none() || reply.is_none())
        || !request.is_answered && reply.is_some()
        || client.zip(reply).is_some_and(|(c, r)| r.date < c.date)
        || client.zip(last).is_some_and(|(c, l)| l.date < c.date)
        || reply.zip(last).is_some_and(|(r, l)| l.date < r.date)
    {
        return Err(ApiError::bad_request(
            "los hitos de respuesta deben ser coherentes y estar ordenados",
        ));
    }
    let first_client_message_at = request
        .first_client_message_id
        .as_ref()
        .and_then(|id| messages.iter().find(|message| &message.id == id))
        .map(|message| message.date);
    let first_internal_reply_at = request
        .first_internal_reply_message_id
        .as_ref()
        .and_then(|id| messages.iter().find(|message| &message.id == id))
        .map(|message| message.date);
    let last_internal_message_at = request
        .last_internal_message_id
        .as_ref()
        .and_then(|id| messages.iter().find(|message| &message.id == id))
        .map(|message| message.date);
    let response_time_minutes = first_client_message_at
        .zip(first_internal_reply_at)
        .map(|(client, reply)| (reply - client).num_minutes());
    let resolution_time_minutes = first_client_message_at
        .zip(last_internal_message_at)
        .map(|(client, last)| (last - client).num_minutes());
    // La validez se deriva de la clasificación elegida: ya no es un control
    // independiente (la casilla "Solicitud válida" se eliminó del formulario).
    // Así clasificación e is_valid_client_request no pueden quedar desincronizados.
    let is_valid_client_request = matches!(
        request.new_classification,
        crate::analysis::Classification::ValidClientRequest
    );
    let review = ManualReview {
        id: Uuid::new_v4().to_string(),
        email_thread_id: thread.id.clone(),
        reviewer_label: session.google_account_email.clone(),
        new_classification: request.new_classification,
        is_valid_client_request,
        is_answered: request.is_answered,
        first_client_message_id: request.first_client_message_id,
        first_internal_reply_message_id: request.first_internal_reply_message_id,
        last_internal_message_id: request.last_internal_message_id,
        first_client_message_at,
        first_internal_reply_at,
        last_internal_message_at,
        response_time_minutes,
        resolution_time_minutes,
        notes: request.notes,
        created_at: Utc::now(),
    };
    state
        .storage
        .add_manual_review(&thread.analysis_run_id, &review)
        .await?;
    state
        .storage
        .upsert_manual_review_override(&ManualReviewOverride {
            owner_email: session.google_account_email.clone(),
            thread_id: thread.thread_id.clone(),
            source_run_id: thread.analysis_run_id.clone(),
            message_fingerprint: message_fingerprint(&messages),
            review_context: Some(crate::analysis::manual_review_context(
                &run.config,
                run.policy_snapshot.as_ref(),
            )),
            reviewer_label: session.google_account_email.clone(),
            classification: review.new_classification.clone(),
            is_answered: review.is_answered,
            first_client_message_id: review.first_client_message_id.clone(),
            first_internal_reply_message_id: review.first_internal_reply_message_id.clone(),
            last_internal_message_id: review.last_internal_message_id.clone(),
            notes: review.notes.clone(),
            created_at: review.created_at,
        })
        .await?;
    recalculate_run_metrics(state, &thread.analysis_run_id).await?;
    let run = state
        .storage
        .get_analysis_run(&thread.analysis_run_id)
        .await?
        .ok_or(ApiError::not_found("analysis run not found"))?;
    Ok(Json(run))
}

#[derive(Debug)]
struct AnalysisFailureStage(&'static str);

impl fmt::Display for AnalysisFailureStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for AnalysisFailureStage {}

fn analysis_failure_stage(error: &anyhow::Error) -> &'static str {
    error
        .downcast_ref::<AnalysisFailureStage>()
        .map(|stage| stage.0)
        .unwrap_or("unknown")
}

pub(crate) async fn mark_run_failed(state: &AppState, run_id: &str, error: &anyhow::Error) {
    tracing::error!(
        operation = "analysis_run",
        run_id,
        stage = analysis_failure_stage(error),
        "analysis failed"
    );
    if let Ok(Some(mut failed)) = state.storage.get_analysis_run(run_id).await {
        failed.status = AnalysisStatus::Failed;
        if analysis_failure_stage(error) == "analysis_memory_budget" {
            failed.error_message = Some("analysis_memory_budget_exceeded".to_string());
            failed.progress_message =
                "El análisis superó el límite de memoria; reduce el período o el número de hilos"
                    .to_string();
        } else {
            failed.error_message = Some("analysis_failed".to_string());
            failed.progress_message = "El análisis falló".to_string();
        }
        let _ = state.storage.update_analysis_run(&failed).await;
    }
}

pub(crate) async fn execute_analysis(
    state: AppState,
    run_id: String,
    access_token: String,
) -> anyhow::Result<()> {
    let mut spent = UsageAmounts {
        runs: 1,
        ..Default::default()
    };
    let queued_at = std::time::Instant::now();
    let _permit =
        match tokio::time::timeout(Duration::from_secs(5 * 60), state.analysis_slots.acquire())
            .await
        {
            Ok(permit) => permit?,
            Err(_) => {
                state.storage.settle_analysis_usage(&run_id, spent).await?;
                anyhow::bail!("analysis_capacity_timeout");
            }
        };
    let access_token = if queued_at.elapsed() > Duration::from_secs(120) {
        let run = state
            .storage
            .get_analysis_run(&run_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("analysis_cancelled_or_deleted"))?;
        let connection = state
            .storage
            .get_gmail_connection(&run.user_email)
            .await?
            .ok_or_else(|| anyhow::anyhow!("mailbox_connection_missing"))?;
        fresh_mailbox_access_token(&state, &connection)
            .await
            .map_err(|_| anyhow::anyhow!("mailbox_authentication_unavailable"))?
    } else {
        access_token
    };
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(45 * 60),
        execute_analysis_inner(state.clone(), run_id.clone(), access_token, &mut spent),
    )
    .await
    .unwrap_or_else(|_| Err(anyhow::anyhow!("analysis_timeout")));
    if result.as_ref().is_err_and(|error| {
        error
            .downcast_ref::<crate::imap::ImapAuthenticationRejected>()
            .is_some()
    }) && let Some(run) = state.storage.get_analysis_run(&run_id).await?
        && let Some(connection) = state.storage.get_gmail_connection(&run.user_email).await?
    {
        let mut updated = connection.clone();
        updated.needs_reauth_at = Some(Utc::now());
        updated.updated_at = Utc::now();
        state
            .storage
            .refresh_gmail_connection(&connection, &updated)
            .await?;
    }
    state.storage.settle_analysis_usage(&run_id, spent).await?;
    result
}

async fn execute_analysis_inner(
    state: AppState,
    run_id: String,
    access_token: String,
    spent: &mut UsageAmounts,
) -> anyhow::Result<()> {
    let mut run = state
        .storage
        .get_analysis_run(&run_id)
        .await
        .context(AnalysisFailureStage("run_lookup"))?
        .ok_or_else(|| anyhow::anyhow!("analysis run not found"))
        .context(AnalysisFailureStage("run_lookup"))?;
    if run.status != AnalysisStatus::Running {
        anyhow::bail!("analysis_cancelled_or_deleted");
    }
    let current_bundle = state
        .storage
        .get_org_config_for_user(&run.user_email)
        .await?
        .ok_or_else(|| anyhow::anyhow!("analysis_missing_organization"))?;
    require_org_role(
        &current_bundle,
        &[OrgRole::Owner, OrgRole::Admin, OrgRole::Analyst],
    )
    .map_err(|_| anyhow::anyhow!("organization_permission_denied"))?;
    if run.org_id.as_deref() != Some(current_bundle.org.id.as_str())
        || run.mailbox_id.as_deref() != Some(current_bundle.mailbox.id.as_str())
    {
        anyhow::bail!("mailbox_connection_changed");
    }
    // Plan vigente para este run (pagado o Mira Free por defecto). El tope del plan
    // El plan aplica tres cupos distintos: recuperados de la casilla (mensual,
    // holgado), hilos que entran al informe (por análisis) y hilos enviados a la
    // IA (mensual, estricto: es el único que cuesta dinero).
    let plan = plan_for_run(&state, run.org_id.as_deref(), &run.user_email)
        .await
        .context(AnalysisFailureStage("plan_lookup"))?;
    let policy_max = run
        .policy_snapshot
        .as_ref()
        .map(|snapshot| snapshot.analysis_policy.max_threads_per_run)
        .unwrap_or(state.config.google.gmail_max_threads);
    // En dev (`enforcement_enabled=false`) no hay tope: análisis ilimitado como antes.
    let enforce = state.config.billing.enforcement_enabled
        && !state.config.is_privileged_account(&run.user_email);
    // Tope de hilos que entran al informe: es por análisis, no mensual.
    let reported_cap = if enforce {
        plan.limits.reported_threads_per_run
    } else {
        u32::MAX
    };
    let has_finite_run_cap = enforce && reported_cap != UNLIMITED_REPORTED_PER_RUN;
    // La recuperación se acota por lo que queda del cupo MENSUAL de recuperados, y
    // con holgura sobre el tope del informe para que pueda llenarse pese al embudo.
    let requested_retrieval = retrieval_max_for(has_finite_run_cap, reported_cap, policy_max);
    let retrieval_max = if enforce {
        state
            .storage
            .reserve_analysis_usage(
                &run,
                &current_period_key(),
                UsageAmounts {
                    runs: 1,
                    retrieved: requested_retrieval,
                    ai: 0,
                },
                &plan.limits,
            )
            .await?
            .retrieved
    } else {
        requested_retrieval
    };
    if retrieval_max == 0 {
        anyhow::bail!("usage_quota_exceeded");
    }

    // El proveedor se resuelve una vez por run desde la conexión de la casilla,
    // no por hilo: dentro del run no puede cambiar.
    let connection = state
        .storage
        .get_gmail_connection(&run.user_email)
        .await
        .context(AnalysisFailureStage("mailbox_connection_lookup"))?
        .ok_or_else(|| anyhow::anyhow!("mailbox_connection_missing"))?;
    if !mailbox_connection_is_active(Some(&connection))
        || !connection
            .mailbox_email
            .eq_ignore_ascii_case(&current_bundle.mailbox.google_account_email)
    {
        anyhow::bail!("mailbox_connection_changed");
    }
    let provider = state
        .mailbox
        .for_connection(&connection)
        .context(AnalysisFailureStage("mailbox_provider_unavailable"))?;

    let page = provider
        .list_thread_ids(&access_token, &run.config, retrieval_max)
        .await
        .context(AnalysisFailureStage("mailbox_list_threads"))?;
    let more_beyond_retrieved = page.next_page_token.is_some();
    let thread_ids = page.ids;
    spent.retrieved = thread_ids.len() as u32;
    run.total_candidate_threads = thread_ids.len() as u64;
    run.progress_message = format!("{} hilos encontrados en la casilla", thread_ids.len());
    state
        .storage
        .update_analysis_run(&run)
        .await
        .context(AnalysisFailureStage("initial_run_progress"))?;

    let mut ai_input_tokens = 0u64;
    let mut ai_output_tokens = 0u64;
    let mut ai_unique_thread_ids = HashSet::new();
    // Embudo de diagnóstico: contamos los descartes previos a la clasificación
    // (que no se guardan en ningún lado) y guardamos una muestra acotada para que
    // el usuario vea CUÁLES hilos se cayeron y por qué.
    let mut funnel = AnalysisFunnel::default();

    // Primera fase: casilla + filtros + heurística local. No se llama a IA todavía,
    // para poder agrupar los candidatos humanos y reutilizar la política por lote.
    let config = run.config.clone();
    let policy_snapshot = run.policy_snapshot.clone();
    let owner_email = run.user_email.clone();
    let run_id = run.id.clone();
    let total = run.total_candidate_threads;
    let mut stored = 0u64;
    let mut skipped_by_plan_cap = 0u64;
    let mut prepared_threads = Vec::new();
    let mut prepared_bytes = 0usize;
    {
        let state_ref = &state;
        let provider_ref = provider.as_ref();
        let access_ref = access_token.as_str();
        let config_ref = &config;
        let policy_ref = policy_snapshot.as_ref();
        let run_id_ref = run_id.as_str();
        let owner_email_ref = owner_email.as_str();
        let analyzed_slot = std::sync::atomic::AtomicU32::new(0);
        let analyzed_slot_ref = &analyzed_slot;
        let mut task_stream = stream::iter(thread_ids.into_iter().map(|thread_id| async move {
            prepare_one_thread(
                state_ref,
                provider_ref,
                access_ref,
                run_id_ref,
                config_ref,
                ThreadProcessingContext {
                    policy_snapshot: policy_ref,
                    owner_email: owner_email_ref,
                    analyzed_slot: analyzed_slot_ref,
                    reported_cap,
                },
                thread_id,
            )
            .await
        }))
        .buffer_unordered(ANALYSIS_CONCURRENCY);

        let mut processed = 0u64;
        while let Some(result) = task_stream.next().await {
            processed += 1;
            match result {
                Ok(outcome) => {
                    if outcome.truncated {
                        funnel.truncated_threads += 1;
                    }
                    match outcome.disposition {
                        ThreadDisposition::Stored => {
                            stored += 1;
                        }
                        ThreadDisposition::DroppedNotPrimaryInbox => {
                            funnel.dropped_not_primary_inbox += 1;
                        }
                        ThreadDisposition::DroppedNoExternalInWindow => {
                            funnel.dropped_no_external_in_window += 1;
                        }
                        ThreadDisposition::SkippedByPlanCap => {
                            skipped_by_plan_cap += 1;
                        }
                    }
                    if let Some(prepared) = outcome.prepared {
                        reserve_prepared_bytes(&mut prepared_bytes, &prepared)
                            .context(AnalysisFailureStage("analysis_memory_budget"))?;
                        prepared_threads.push(prepared);
                    }
                    if let Some(dropped) = outcome.dropped
                        && funnel.dropped_samples.len() < FUNNEL_DROPPED_SAMPLE_CAP
                    {
                        funnel.dropped_samples.push(dropped);
                    }
                }
                Err(error) => {
                    if analysis_failure_stage(&error) == "analysis_memory_budget"
                        || error
                            .downcast_ref::<crate::imap::ImapAuthenticationRejected>()
                            .is_some()
                        || matches!(
                            error.to_string().as_str(),
                            "analysis_cancelled_or_deleted" | "mailbox_connection_changed"
                        )
                    {
                        return Err(error);
                    }
                    funnel.failed_threads += 1;
                    tracing::warn!(
                        operation = "thread_processing",
                        "el procesamiento de un hilo falló; se omite"
                    );
                }
            }
            if processed.is_multiple_of(PROGRESS_UPDATE_EVERY) {
                run.processed_threads = processed;
                run.progress_message = format!(
                    "Filtrados {}/{} hilos · {} candidatos listos para IA",
                    processed,
                    total,
                    prepared_threads
                        .iter()
                        .filter(|prepared| prepared.should_batch)
                        .count()
                );
                state.storage.update_analysis_run(&run).await?;
            }
        }
        run.processed_threads = processed;
    }

    let ai_enabled = policy_snapshot.as_ref().is_some_and(|snapshot| {
        snapshot.ai_policy.enabled && snapshot.ai_policy.consent_granted_at.is_some()
    }) && state
        .storage
        .get_org_config_for_user(&run.user_email)
        .await?
        .is_some_and(|bundle| {
            bundle.draft.ai_policy.enabled && bundle.draft.ai_policy.consent_granted_at.is_some()
        });
    let (auto_apply_threshold, manual_review_threshold) = policy_snapshot
        .as_ref()
        .map(|snapshot| {
            (
                snapshot.ai_policy.auto_apply_threshold,
                snapshot.ai_policy.manual_review_threshold,
            )
        })
        .unwrap_or((state.config.ai.apply_confidence_threshold, 0.72));
    let batch_indexes = prepared_threads
        .iter()
        .enumerate()
        .filter_map(|(index, prepared)| (ai_enabled && prepared.should_batch).then_some(index))
        .collect::<Vec<_>>();
    // Cupo mensual de IA (el costo real: tokens de Bedrock). Se agota de forma
    // elegante: el análisis igual corre y clasifica con heurística, solo deja de
    // auditar. Bloquear el run completo sería peor producto que degradar.
    let ai_budget = if enforce {
        state
            .storage
            .reserve_analysis_usage(
                &run,
                &current_period_key(),
                UsageAmounts {
                    runs: 1,
                    retrieved: spent.retrieved,
                    ai: batch_indexes.len() as u32,
                },
                &plan.limits,
            )
            .await?
            .ai as usize
    } else {
        usize::MAX
    };
    let (ai_audited_now, ai_skipped_by_budget) = split_by_ai_budget(batch_indexes.len(), ai_budget);
    let (batch_indexes, skipped_indexes) = batch_indexes.split_at(ai_audited_now);
    for index in skipped_indexes {
        mark_ai_audit_unavailable(&mut prepared_threads[*index].thread);
    }

    // Segunda fase: una única auditoría completa por lotes. Los errores se reintentan
    // una vez dividiendo el lote; no existe una segunda llamada por hilo.
    for index_chunk in batch_indexes.chunks(AI_BATCH_SIZE) {
        if state
            .storage
            .get_analysis_run(&run_id)
            .await?
            .is_none_or(|run| run.status != AnalysisStatus::Running)
        {
            anyhow::bail!("analysis_cancelled_or_deleted");
        }
        let current_bundle = state
            .storage
            .get_org_config_for_user(&run.user_email)
            .await?
            .ok_or_else(|| anyhow::anyhow!("analysis_missing_organization"))?;
        require_org_role(
            &current_bundle,
            &[OrgRole::Owner, OrgRole::Admin, OrgRole::Analyst],
        )
        .map_err(|_| anyhow::anyhow!("organization_permission_denied"))?;
        let consent_current = current_bundle.draft.ai_policy.enabled
            && current_bundle.draft.ai_policy.consent_granted_at.is_some();
        if !consent_current {
            for index in index_chunk {
                mark_ai_audit_unavailable(&mut prepared_threads[*index].thread);
            }
            continue;
        }
        spent.ai = spent.ai.saturating_add(index_chunk.len() as u32);
        for index in index_chunk {
            ai_unique_thread_ids.insert(prepared_threads[*index].thread.thread_id.clone());
        }
        let batch = audit_batch_with_split(
            &state,
            &mut run,
            &prepared_threads,
            index_chunk,
            policy_snapshot.as_ref(),
        )
        .await?;
        ai_input_tokens += batch.input_tokens;
        ai_output_tokens += batch.output_tokens;
        funnel.ai_calls += batch.calls;
        funnel.ai_invalid_output_calls += batch.invalid_output_calls;
        funnel.ai_unconfirmed_calls += batch.unconfirmed_calls;
        funnel.ai_batch_classified += batch.decisions.len() as u64;

        for index in index_chunk {
            let prepared = &mut prepared_threads[*index];
            if let Some(decision) = batch.decisions.get(&prepared.thread.thread_id) {
                apply_batch_decision(
                    &mut prepared.thread,
                    &prepared.messages,
                    decision,
                    auto_apply_threshold,
                    manual_review_threshold,
                );
                if prepared.truncated {
                    defer_incomplete_conversation(&mut prepared.thread);
                }
            } else {
                mark_ai_audit_unavailable(&mut prepared.thread);
            }
        }
    }

    // Los hilos que no necesitaban IA y los resultados ya resueltos se persisten al
    // final; los cuerpos se descartan como antes.
    let current_bundle = state
        .storage
        .get_org_config_for_user(&run.user_email)
        .await?
        .ok_or_else(|| anyhow::anyhow!("analysis_missing_organization"))?;
    require_org_role(
        &current_bundle,
        &[OrgRole::Owner, OrgRole::Admin, OrgRole::Analyst],
    )
    .map_err(|_| anyhow::anyhow!("organization_permission_denied"))?;
    if run.mailbox_id.as_deref() != Some(current_bundle.mailbox.id.as_str()) {
        anyhow::bail!("mailbox_connection_changed");
    }
    for prepared in &prepared_threads {
        let sanitized = messages_for_persistence(&prepared.messages);
        state
            .storage
            .upsert_thread(&prepared.thread, &sanitized)
            .await?;
    }

    // El tope del plan recortó hilos analizables: lo dejamos explícito para que el
    // reporte pueda decir honestamente "Analizamos N de M" en vez de aparentar falla.
    funnel.skipped_by_plan_cap = skipped_by_plan_cap;
    funnel.truncated_by_plan = skipped_by_plan_cap > 0;
    funnel.more_beyond_retrieved = more_beyond_retrieved;
    funnel.would_be_reported = Some(stored + skipped_by_plan_cap);
    funnel.plan_reported_cap = (skipped_by_plan_cap > 0).then_some(reported_cap);
    funnel.ai_unique_threads = ai_unique_thread_ids.len() as u64;
    funnel.ai_skipped_by_budget = ai_skipped_by_budget as u64;

    let threads = state.storage.list_threads(&run.id).await?;
    run.metrics = calculate_metrics(&threads, ai_input_tokens, ai_output_tokens);
    // Los descartados no están en `threads` (nunca se guardan), así que el embudo
    // se adjunta aparte, después de recomputar las métricas de los almacenados.
    run.metrics.funnel = funnel;
    run.status = AnalysisStatus::Completed;
    run.progress_message =
        if run.metrics.funnel.failed_threads > 0 || run.metrics.funnel.truncated_threads > 0 {
            "Análisis completado con cobertura incompleta".to_string()
        } else {
            "Análisis completado".to_string()
        };
    run.completed_at = Some(Utc::now());
    state.storage.update_analysis_run(&run).await?;
    // Se cobran las dos magnitudes con cupo mensual: los hilos RECUPERADOS de la
    // casilla y los enviados a la IA. Los reportados se topan por análisis, no al mes.
    if let Some(org_id) = run.org_id.as_deref() {
        check_quota_alerts(&state, org_id, &current_period_key(), &run.user_email).await;
    }
    Ok(())
}

/// Plan vigente para un run en background (versión `anyhow` de `effective_plan_for_org`).
async fn plan_for_run(
    state: &AppState,
    org_id: Option<&str>,
    user_email: &str,
) -> anyhow::Result<BillingPlan> {
    if !state.config.billing.enforcement_enabled || state.config.is_privileged_account(user_email) {
        return Ok(plan_by_id(&BillingPlanId::Pro));
    }
    let Some(org_id) = org_id else {
        anyhow::bail!("analysis_missing_organization");
    };
    let subscription = state.storage.get_subscription_for_org(org_id).await?;
    if subscription_allows_access(subscription.as_ref(), Utc::now()) {
        Ok(plan_by_id(&subscription.expect("checked above").plan_id))
    } else {
        Ok(free_plan())
    }
}

/// Reparte los hilos candidatos a IA contra el cupo mensual restante: devuelve
/// (auditados ahora, saltados por cupo agotado).
fn split_by_ai_budget(candidates: usize, budget: usize) -> (usize, usize) {
    let audited = candidates.min(budget);
    (audited, candidates - audited)
}

/// Cuántos candidatos recuperar de la casilla. Con tope finito (Free) recuperamos con
/// holgura para que el tope de analizados pueda llenarse pese a los descartes del
/// embudo; sin tope (planes de pago) respetamos el `max_threads_per_run` de policy.
fn retrieval_max_for(has_finite_run_cap: bool, reported_cap: u32, policy_max: u32) -> u32 {
    if has_finite_run_cap {
        reported_cap
            .saturating_mul(FREE_RETRIEVAL_FACTOR)
            .min(FREE_RETRIEVAL_HARD_MAX)
            .max(reported_cap.min(FREE_RETRIEVAL_HARD_MAX))
    } else {
        policy_max
    }
}

/// Holgura de recuperación para planes con tope finito de analizados.
const FREE_RETRIEVAL_FACTOR: u32 = 3;
/// Techo duro de recuperación para no disparar las llamadas al proveedor en Free.
const FREE_RETRIEVAL_HARD_MAX: u32 = 300;

/// One provider response per run; the instance permits four concurrent runs.
const ANALYSIS_CONCURRENCY: usize = 1;
/// Retained metadata and compact AI excerpts, excluding bodies discarded during preparation.
/// ponytail: larger runs fail explicitly; stream batches before raising this ceiling.
const MAX_PREPARED_RUN_BYTES: usize = 32 * 1024 * 1024;
/// Write run progress to storage every N processed threads (instead of every one).
const PROGRESS_UPDATE_EVERY: u64 = 5;
/// Máximo de hilos descartados que guardamos como muestra en el embudo, para
/// acotar el tamaño del documento del run sin perder utilidad de diagnóstico.
const FUNNEL_DROPPED_SAMPLE_CAP: usize = 100;
const AI_BATCH_SIZE: usize = 20;
const DEFAULT_AI_MESSAGE_CAP: usize = 4;
const DEFAULT_AI_BODY_CHARS: usize = 280;

#[derive(Default)]
struct ThreadOutcome {
    truncated: bool,
    /// Dónde terminó el hilo: almacenado o descartado (y por qué). Alimenta el
    /// embudo de diagnóstico para que los descartes dejen de ser invisibles.
    disposition: ThreadDisposition,
    /// Metadatos del descartado (solo cuando `disposition` es un descarte).
    dropped: Option<DroppedThreadInfo>,
    /// Hilo ya filtrado y clasificado localmente, pendiente de batch IA/persistencia.
    prepared: Option<PreparedThread>,
}

struct PreparedThread {
    truncated: bool,
    thread: EmailThread,
    messages: Vec<EmailMessage>,
    label_ids: Vec<String>,
    should_batch: bool,
    batch_messages: Vec<BatchMessageSummary>,
}

impl PreparedThread {
    fn new(
        thread: EmailThread,
        mut messages: Vec<EmailMessage>,
        label_ids: Vec<String>,
        should_batch: bool,
        truncated: bool,
        audit_limits: (usize, usize),
    ) -> Self {
        let batch_messages = if should_batch {
            batch_messages_for_thread(&thread, &messages, audit_limits.0, audit_limits.1)
        } else {
            Vec::new()
        };
        // Heuristics have consumed the full body. Keep only the existing bounded AI
        // excerpts and the metadata required for milestones, fingerprints and storage.
        for message in &mut messages {
            message.body_text = None;
        }
        Self {
            truncated,
            thread,
            messages,
            label_ids,
            should_batch,
            batch_messages,
        }
    }

    fn retained_bytes(&self) -> usize {
        let thread = &self.thread;
        let mut bytes = std::mem::size_of::<Self>()
            + self.messages.capacity() * std::mem::size_of::<EmailMessage>()
            + self.batch_messages.capacity() * std::mem::size_of::<BatchMessageSummary>()
            + string_list_bytes(&self.label_ids)
            + string_list_bytes(&thread.reasons);
        for value in [
            Some(&thread.id),
            Some(&thread.analysis_run_id),
            Some(&thread.thread_id),
            Some(&thread.subject),
            Some(&thread.normalized_subject),
            thread.first_client_message_id.as_ref(),
            thread.first_internal_reply_message_id.as_ref(),
            thread.last_internal_message_id.as_ref(),
            thread.notes.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            bytes += value.capacity();
        }
        for message in &self.messages {
            bytes += message.id.capacity()
                + message.message_id.capacity()
                + message.from_email.capacity()
                + message.from_name.as_ref().map_or(0, String::capacity)
                + string_list_bytes(&message.to_emails)
                + string_list_bytes(&message.cc_emails)
                + message.subject.capacity()
                + message.snippet.capacity()
                + json_heap_bytes(&message.headers)
                + message.body_text.as_ref().map_or(0, String::capacity);
        }
        for message in &self.batch_messages {
            bytes += message.message_id.capacity()
                + message.from_email.capacity()
                + string_list_bytes(&message.to_emails)
                + string_list_bytes(&message.cc_emails)
                + message.content.capacity();
        }
        bytes
    }
}

fn string_list_bytes(values: &Vec<String>) -> usize {
    values.capacity() * std::mem::size_of::<String>()
        + values.iter().map(String::capacity).sum::<usize>()
}

fn json_heap_bytes(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::String(value) => value.capacity(),
        serde_json::Value::Array(values) => {
            values.capacity() * std::mem::size_of::<serde_json::Value>()
                + values.iter().map(json_heap_bytes).sum::<usize>()
        }
        // A conservative allowance for each map node, key and inline value; JSON
        // serialization would allocate another large buffer just to count its size.
        serde_json::Value::Object(values) => values
            .iter()
            .map(|(key, value)| 256 + key.capacity() + json_heap_bytes(value))
            .sum(),
        _ => 0,
    }
}

fn reserve_prepared_bytes(bytes: &mut usize, prepared: &PreparedThread) -> anyhow::Result<()> {
    let next = bytes
        .checked_add(prepared.retained_bytes())
        .filter(|next| *next <= MAX_PREPARED_RUN_BYTES)
        .context("analysis_memory_budget_exceeded: reduce el período o el número de hilos")?;
    *bytes = next;
    Ok(())
}

/// Construye el registro de un hilo descartado antes de clasificarse, usando los
/// mensajes ya normalizados (sin tocar cuerpos). El asunto/fecha salen del primer
/// mensaje del hilo.
fn dropped_thread_info(
    thread_id: &str,
    messages: &[EmailMessage],
    reason: ThreadDisposition,
) -> DroppedThreadInfo {
    let first = messages.iter().min_by_key(|message| message.date);
    DroppedThreadInfo {
        thread_id: thread_id.to_string(),
        subject: first
            .map(|message| message.subject.clone())
            .unwrap_or_else(|| "(sin asunto)".to_string()),
        first_message_at: first.map(|message| message.date),
        reason,
    }
}

struct ThreadProcessingContext<'a> {
    policy_snapshot: Option<&'a crate::policies::PolicySnapshot>,
    owner_email: &'a str,
    /// Contador atómico compartido de hilos analizables ya reclamados, para repartir
    /// los cupos del tope del plan entre las tareas concurrentes.
    analyzed_slot: &'a std::sync::atomic::AtomicU32,
    /// Tope de hilos analizados de este run (del plan vigente).
    reported_cap: u32,
}

#[allow(clippy::too_many_arguments)]
async fn prepare_one_thread(
    state: &AppState,
    provider: &dyn MailboxProvider,
    access_token: &str,
    run_id: &str,
    config: &AnalysisConfig,
    processing: ThreadProcessingContext<'_>,
    thread_id: String,
) -> anyhow::Result<ThreadOutcome> {
    if state
        .storage
        .get_analysis_run(run_id)
        .await?
        .is_none_or(|run| run.status != AnalysisStatus::Running)
    {
        anyhow::bail!("analysis_cancelled_or_deleted");
    }
    let current = state
        .storage
        .get_gmail_connection(processing.owner_email)
        .await?;
    if !mailbox_connection_is_active(current.as_ref())
        || processing.policy_snapshot.is_some_and(|snapshot| {
            current.as_ref().is_none_or(|current| {
                !current
                    .mailbox_email
                    .eq_ignore_ascii_case(&snapshot.mailbox.google_account_email)
            })
        })
    {
        anyhow::bail!("mailbox_connection_changed");
    }
    let data = provider
        .fetch_thread(access_token, &thread_id, config)
        .await
        .map_err(|error| {
            if error
                .downcast_ref::<crate::mailbox::MailboxMemoryBudgetExceeded>()
                .is_some()
            {
                error.context(AnalysisFailureStage("analysis_memory_budget"))
            } else {
                error
            }
        })?;
    if !data.is_primary_inbox && config.include_labels.is_empty() {
        return Ok(ThreadOutcome {
            disposition: ThreadDisposition::DroppedNotPrimaryInbox,
            dropped: Some(dropped_thread_info(
                &data.id,
                &data.messages,
                ThreadDisposition::DroppedNotPrimaryInbox,
            )),
            ..Default::default()
        });
    }
    // The report counts "requests received in the window". Keep the thread only if some
    // EXTERNAL message lands inside the window — a new client request, a follow-up, or
    // (for visibility) system/automated external mail. A thread whose only in-window
    // activity is internal (the desk acting on an older request) is out of scope for this
    // window and is skipped. classify_thread then anchors its metrics on the in-window
    // client message and routes re-opened-after-answered threads to review.
    let has_external_in_window = data.messages.iter().any(|message| {
        crate::analysis::message_is_requester_candidate(message, config)
            && message_is_inside_analysis_window(message, config)
    });
    if !has_external_in_window {
        return Ok(ThreadOutcome {
            disposition: ThreadDisposition::DroppedNoExternalInWindow,
            dropped: Some(dropped_thread_info(
                &data.id,
                &data.messages,
                ThreadDisposition::DroppedNoExternalInWindow,
            )),
            ..Default::default()
        });
    }

    // El hilo sobrevivió el embudo: es analizable. Reclama un cupo bajo el tope del
    // plan. Si ya se agotó, no se guarda ni se audita (cuenta como saltado por tope).
    let slot = processing
        .analyzed_slot
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if slot >= processing.reported_cap {
        return Ok(ThreadOutcome {
            disposition: ThreadDisposition::SkippedByPlanCap,
            ..Default::default()
        });
    }

    let mut thread = classify_thread(run_id, &data.id, &data.messages, config);
    if data.truncated {
        thread.manual_review_required = true;
        thread.reasons.push(
            "La conversación supera el límite de recuperación; la cobertura es incompleta."
                .to_string(),
        );
    }
    // Señal extra: refina el veredicto heurístico con las pestañas/categorías de Gmail.
    refine_classification_with_folders(&mut thread, &data.folder_ids);
    // Señal extra: enruta a revisión los hilos cuyo asunto/remitente coincide con una
    // regla de no-responsabilidad configurada (antes solo alimentaban a la IA).
    if let Some(snapshot) = processing.policy_snapshot {
        let request_sender = thread
            .first_client_message_id
            .as_ref()
            .and_then(|id| data.messages.iter().find(|message| &message.id == id))
            .map(|message| message.from_email.as_str())
            .unwrap_or("");
        refine_classification_with_policy_hints(
            &mut thread,
            request_sender,
            &snapshot.analysis_policy.non_responsibility_rules,
        );
        // Las palabras de ticket sólo señalan candidatos, pendientes de confirmación.
        rescue_classification_with_valid_signals(
            &mut thread,
            &data.messages,
            &snapshot.analysis_policy.valid_signal_keywords,
            config,
        );
    }
    if let Some(review) = state
        .storage
        .get_manual_review_override(processing.owner_email, &thread.thread_id)
        .await?
        && review.message_fingerprint == message_fingerprint(&data.messages)
        && review.review_context.as_deref()
            == Some(
                crate::analysis::manual_review_context(config, processing.policy_snapshot).as_str(),
            )
    {
        apply_manual_review_override(&mut thread, &data.messages, &review);
        let sanitized = messages_for_persistence(&data.messages);
        state.storage.upsert_thread(&thread, &sanitized).await?;
        return Ok(ThreadOutcome {
            disposition: ThreadDisposition::Stored,
            truncated: data.truncated,
            ..Default::default()
        });
    }
    let should_batch = thread.first_client_message_id.is_some()
        && matches!(
            thread.classification,
            Classification::ValidClientRequest | Classification::Ambiguous
        );
    if should_batch
        && !processing
            .policy_snapshot
            .is_some_and(|snapshot| snapshot.ai_policy.enabled)
    {
        mark_semantic_review_pending(&mut thread);
    }
    Ok(ThreadOutcome {
        truncated: data.truncated,
        disposition: ThreadDisposition::Stored,
        prepared: Some(PreparedThread::new(
            thread,
            data.messages,
            data.folder_ids,
            should_batch,
            data.truncated,
            (
                processing
                    .policy_snapshot
                    .map(|snapshot| snapshot.ai_policy.max_audit_messages as usize)
                    .unwrap_or(DEFAULT_AI_MESSAGE_CAP),
                processing
                    .policy_snapshot
                    .map(|snapshot| snapshot.ai_policy.max_body_chars_per_message as usize)
                    .unwrap_or(DEFAULT_AI_BODY_CHARS),
            ),
        )),
        ..Default::default()
    })
}

#[derive(Serialize)]
struct AiWorkerPolicyContext<'a> {
    mailbox_email: &'a str,
    mailbox_display_name: &'a str,
    workspace_domain: &'a str,
    internal_domains: &'a [String],
    responder_emails: &'a [String],
    request_scope: crate::analysis::RequestScope,
    mailbox_aliases: &'a [String],
    valid_request_criteria: &'a [String],
    non_responsibility_rules: &'a [String],
    ignored_senders: &'a [String],
    ignored_domains: &'a [String],
    ignored_keywords: &'a [String],
    count_historical_closures_as_valid: bool,
    count_previous_request_followups_as_valid: bool,
    count_org_hosted_training_as_valid: bool,
    prompt_version: &'a str,
    allowed_fields: &'a [String],
}

fn ai_worker_policy_context(
    snapshot: &crate::policies::PolicySnapshot,
) -> AiWorkerPolicyContext<'_> {
    AiWorkerPolicyContext {
        mailbox_email: &snapshot.mailbox.google_account_email,
        mailbox_display_name: &snapshot.mailbox.display_name,
        workspace_domain: &snapshot.mailbox.workspace_domain,
        internal_domains: &snapshot.analysis_policy.internal_domains,
        responder_emails: &snapshot.analysis_policy.responder_emails,
        request_scope: snapshot.analysis_policy.request_scope,
        mailbox_aliases: &snapshot.analysis_policy.mailbox_aliases,
        valid_request_criteria: &snapshot.analysis_policy.valid_request_criteria,
        non_responsibility_rules: &snapshot.analysis_policy.non_responsibility_rules,
        ignored_senders: &snapshot.analysis_policy.ignored_senders,
        ignored_domains: &snapshot.analysis_policy.ignored_domains,
        ignored_keywords: &snapshot.analysis_policy.ignored_keywords,
        count_historical_closures_as_valid: snapshot
            .analysis_policy
            .count_historical_closures_as_valid,
        count_previous_request_followups_as_valid: snapshot
            .analysis_policy
            .count_previous_request_followups_as_valid,
        count_org_hosted_training_as_valid: snapshot
            .analysis_policy
            .count_org_hosted_training_as_valid,
        prompt_version: &snapshot.ai_policy.prompt_version,
        allowed_fields: &snapshot.ai_policy.allowed_fields,
    }
}

#[derive(Clone, Serialize)]
struct BatchMessageSummary {
    message_id: String,
    from_email: String,
    to_emails: Vec<String>,
    cc_emails: Vec<String>,
    date: chrono::DateTime<Utc>,
    is_internal: bool,
    is_automated: bool,
    content: String,
}

#[derive(Serialize)]
struct BatchThreadSummary {
    thread_id: String,
    subject: String,
    gmail_labels: Vec<String>,
    focus_message_id: Option<String>,
    messages: Vec<BatchMessageSummary>,
}

#[derive(Serialize)]
struct BatchAuditRequest<'a> {
    run_id: &'a str,
    attempt_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy_context: Option<AiWorkerPolicyContext<'a>>,
    threads: Vec<BatchThreadSummary>,
}

#[derive(Debug, Clone, Deserialize)]
struct BatchAuditDecision {
    thread_id: String,
    classification: Classification,
    is_valid_client_request: bool,
    is_answered: bool,
    first_client_message_id: Option<String>,
    first_internal_reply_message_id: Option<String>,
    last_internal_message_id: Option<String>,
    confidence: f64,
    manual_review_required: bool,
    #[serde(default)]
    issues: Vec<String>,
}

#[derive(Deserialize)]
struct BatchAuditResponse {
    decisions: Vec<BatchAuditDecision>,
    input_tokens: u64,
    output_tokens: u64,
    #[serde(default)]
    outcome: BatchAuditOutcome,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    aws_request_id: Option<String>,
}

#[derive(Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum BatchAuditOutcome {
    #[default]
    Valid,
    InvalidOutput,
}

#[derive(Default)]
struct BatchExecution {
    decisions: HashMap<String, BatchAuditDecision>,
    input_tokens: u64,
    output_tokens: u64,
    calls: u64,
    invalid_output_calls: u64,
    unconfirmed_calls: u64,
}

struct BatchCallError {
    kind: &'static str,
    retryable: bool,
}

enum BatchAttemptResult {
    Completed(BatchAuditResponse),
    Failed(BatchCallError),
}

fn should_retry_batch_error(error: &BatchCallError, attempt_number: usize) -> bool {
    error.retryable && attempt_number == 0
}

fn should_split_missing_batch(missing: usize) -> bool {
    missing > 1
}

fn batch_messages_for_thread(
    thread: &EmailThread,
    messages: &[EmailMessage],
    max_messages: usize,
    max_body_chars: usize,
) -> Vec<BatchMessageSummary> {
    let max_messages = max_messages.max(1);
    let focus = thread
        .first_client_message_id
        .as_ref()
        .and_then(|id| messages.iter().find(|message| &message.id == id));
    let first_client = messages
        .iter()
        .filter(|message| message.is_external && !message.is_automated)
        .min_by_key(|message| message.date);
    let first_reply = focus.or(first_client).and_then(|client| {
        messages
            .iter()
            .filter(|message| crate::analysis::reply_reaches_requester(message, client))
            .min_by_key(|message| message.date)
    });
    let mut selected = Vec::with_capacity(max_messages);
    for message in [focus, first_client, first_reply]
        .into_iter()
        .flatten()
        .chain(messages.iter().rev())
    {
        if selected.len() == max_messages {
            break;
        }
        if !selected
            .iter()
            .any(|selected: &&EmailMessage| selected.id == message.id)
        {
            selected.push(message);
        }
    }
    selected.sort_by_key(|message| message.date);
    selected
        .into_iter()
        .map(|message| {
            let source = message
                .body_text
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(&message.snippet);
            BatchMessageSummary {
                message_id: message.id.clone(),
                from_email: message.from_email.clone(),
                to_emails: message.to_emails.clone(),
                cc_emails: message.cc_emails.clone(),
                date: message.date,
                is_internal: message.is_internal,
                is_automated: message.is_automated,
                content: truncate_chars(source, max_body_chars),
            }
        })
        .collect()
}

fn batch_summary(prepared: &PreparedThread) -> BatchThreadSummary {
    BatchThreadSummary {
        thread_id: prepared.thread.thread_id.clone(),
        subject: prepared.thread.subject.clone(),
        gmail_labels: prepared.label_ids.clone(),
        focus_message_id: prepared.thread.first_client_message_id.clone(),
        messages: prepared.batch_messages.clone(),
    }
}

async fn audit_batch_once(
    state: &AppState,
    run_id: &str,
    attempt_id: &str,
    prepared: &[PreparedThread],
    indexes: &[usize],
    policy_snapshot: Option<&crate::policies::PolicySnapshot>,
) -> Result<BatchAuditResponse, BatchCallError> {
    let policy_context = policy_snapshot.map(ai_worker_policy_context);
    let threads = indexes
        .iter()
        .map(|index| batch_summary(&prepared[*index]))
        .collect();
    let mut request = state
        .http
        .post(format!("{}/audit/batch", state.config.ai.worker_url))
        .json(&BatchAuditRequest {
            run_id,
            attempt_id,
            policy_context,
            threads,
        });
    request = request.header("x-request-id", attempt_id);
    if let Some(audience) = &state.config.ai.worker_audience {
        let token = fetch_cloud_run_identity_token(&state.http, audience)
            .await
            .map_err(|_| BatchCallError {
                kind: "worker_auth",
                retryable: true,
            })?;
        request = request.bearer_auth(token);
    }
    let response = request
        .timeout(Duration::from_secs(190))
        .send()
        .await
        .map_err(|error| BatchCallError {
            kind: if error.is_timeout() {
                "worker_timeout"
            } else {
                "worker_transport"
            },
            retryable: true,
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(BatchCallError {
            kind: if status == StatusCode::TOO_MANY_REQUESTS {
                "worker_rate_limited"
            } else if status.is_server_error() {
                "worker_server_error"
            } else {
                "worker_client_error"
            },
            retryable: status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error(),
        });
    }
    response
        .json::<BatchAuditResponse>()
        .await
        .map_err(|_| BatchCallError {
            kind: "worker_invalid_response",
            retryable: false,
        })
}

async fn recorded_batch_attempt(
    state: &AppState,
    run: &mut AnalysisRun,
    prepared: &[PreparedThread],
    indexes: &[usize],
    policy_snapshot: Option<&crate::policies::PolicySnapshot>,
) -> anyhow::Result<BatchAttemptResult> {
    let attempt_id = Uuid::new_v4().to_string();
    let started_at = Utc::now();
    let result = audit_batch_once(
        state,
        &run.id,
        &attempt_id,
        prepared,
        indexes,
        policy_snapshot,
    )
    .await;
    let completed_at = Utc::now();
    let (outcome, input_tokens, output_tokens, stop_reason, aws_request_id, error_kind) =
        match &result {
            Ok(response) => (
                if response.outcome == BatchAuditOutcome::Valid {
                    AiUsageAttemptOutcome::Valid
                } else {
                    AiUsageAttemptOutcome::InvalidOutput
                },
                response.input_tokens,
                response.output_tokens,
                response.stop_reason.clone(),
                response.aws_request_id.clone(),
                None,
            ),
            Err(error) => (
                AiUsageAttemptOutcome::Unconfirmed,
                0,
                0,
                None,
                None,
                Some(error.kind.to_string()),
            ),
        };
    let attempt = AiUsageAttempt {
        id: attempt_id,
        run_id: run.id.clone(),
        batch_size: indexes.len() as u32,
        outcome: outcome.clone(),
        input_tokens,
        output_tokens,
        stop_reason,
        aws_request_id,
        error_kind: error_kind.clone(),
        started_at,
        completed_at,
    };
    state.storage.record_ai_usage_attempt(&attempt).await?;
    run.metrics.ai_input_tokens = run.metrics.ai_input_tokens.saturating_add(input_tokens);
    run.metrics.ai_output_tokens = run.metrics.ai_output_tokens.saturating_add(output_tokens);
    run.metrics.funnel.ai_calls = run.metrics.funnel.ai_calls.saturating_add(1);
    match outcome {
        AiUsageAttemptOutcome::InvalidOutput => {
            run.metrics.funnel.ai_invalid_output_calls =
                run.metrics.funnel.ai_invalid_output_calls.saturating_add(1);
        }
        AiUsageAttemptOutcome::Unconfirmed => {
            run.metrics.funnel.ai_unconfirmed_calls =
                run.metrics.funnel.ai_unconfirmed_calls.saturating_add(1);
        }
        AiUsageAttemptOutcome::Valid => {}
    }
    state.storage.update_analysis_run(run).await?;
    tracing::info!(
        operation = "ai_usage_attempt",
        run_id = %run.id,
        attempt_id = %attempt.id,
        batch_size = attempt.batch_size,
        outcome = ?attempt.outcome,
        input_tokens,
        output_tokens,
        stop_reason = attempt.stop_reason.as_deref().unwrap_or(""),
        aws_request_id = attempt.aws_request_id.as_deref().unwrap_or(""),
        error_kind = error_kind.as_deref().unwrap_or(""),
        "intento IA registrado"
    );
    Ok(match result {
        Ok(response) => BatchAttemptResult::Completed(response),
        Err(error) => BatchAttemptResult::Failed(error),
    })
}

async fn batch_attempt_with_retry(
    state: &AppState,
    run: &mut AnalysisRun,
    prepared: &[PreparedThread],
    indexes: &[usize],
    policy_snapshot: Option<&crate::policies::PolicySnapshot>,
    execution: &mut BatchExecution,
) -> anyhow::Result<Option<BatchAuditResponse>> {
    for attempt_number in 0..=1 {
        execution.calls += 1;
        match recorded_batch_attempt(state, run, prepared, indexes, policy_snapshot).await? {
            BatchAttemptResult::Completed(response) => {
                execution.input_tokens += response.input_tokens;
                execution.output_tokens += response.output_tokens;
                if response.outcome == BatchAuditOutcome::InvalidOutput {
                    execution.invalid_output_calls += 1;
                }
                return Ok(Some(response));
            }
            BatchAttemptResult::Failed(error) => {
                execution.unconfirmed_calls += 1;
                tracing::warn!(
                    operation = "ai_batch",
                    batch_size = indexes.len(),
                    error_kind = error.kind,
                    retryable = error.retryable,
                    "falló auditoría IA batch"
                );
                if !should_retry_batch_error(&error, attempt_number) {
                    return Ok(None);
                }
            }
        }
    }
    Ok(None)
}

async fn audit_batch_with_split(
    state: &AppState,
    run: &mut AnalysisRun,
    prepared: &[PreparedThread],
    indexes: &[usize],
    policy_snapshot: Option<&crate::policies::PolicySnapshot>,
) -> anyhow::Result<BatchExecution> {
    let mut execution = BatchExecution::default();
    if let Some(response) = batch_attempt_with_retry(
        state,
        run,
        prepared,
        indexes,
        policy_snapshot,
        &mut execution,
    )
    .await?
    {
        merge_batch_response(&mut execution, response, prepared, indexes);
    } else {
        return Ok(execution);
    }
    let missing = indexes
        .iter()
        .copied()
        .filter(|index| {
            !execution
                .decisions
                .contains_key(&prepared[*index].thread.thread_id)
        })
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(execution);
    }
    if !should_split_missing_batch(missing.len()) {
        tracing::warn!(
            operation = "ai_batch_singleton_invalid",
            "salida IA inválida para un único hilo; no se repite el mismo prompt"
        );
        return Ok(execution);
    }
    tracing::warn!(
        operation = "ai_batch_partial",
        expected = indexes.len(),
        received = execution.decisions.len(),
        missing = missing.len(),
        "auditoría IA incompleta; reintentando solo hilos faltantes"
    );
    let middle = missing.len().div_ceil(2);
    for half in [&missing[..middle], &missing[middle..]] {
        if half.is_empty() {
            continue;
        }
        if let Some(response) =
            batch_attempt_with_retry(state, run, prepared, half, policy_snapshot, &mut execution)
                .await?
        {
            merge_batch_response(&mut execution, response, prepared, half);
        }
    }
    Ok(execution)
}

fn merge_batch_response(
    execution: &mut BatchExecution,
    response: BatchAuditResponse,
    prepared: &[PreparedThread],
    indexes: &[usize],
) {
    let expected = indexes
        .iter()
        .map(|index| prepared[*index].thread.thread_id.as_str())
        .collect::<HashSet<_>>();
    for decision in response.decisions {
        if expected.contains(decision.thread_id.as_str()) {
            execution
                .decisions
                .insert(decision.thread_id.clone(), decision);
        }
    }
}

fn batch_message_ids_known(decision: &BatchAuditDecision, messages: &[EmailMessage]) -> bool {
    let known = |id: &Option<String>| {
        id.as_ref()
            .map(|id| messages.iter().any(|message| &message.id == id))
            .unwrap_or(true)
    };
    known(&decision.first_client_message_id)
        && known(&decision.first_internal_reply_message_id)
        && known(&decision.last_internal_message_id)
}

fn defer_incomplete_conversation(thread: &mut EmailThread) {
    if thread.ai_suggestion.is_none() {
        thread.ai_suggestion = Some(crate::analysis::AiSuggestion {
            classification: thread.classification.clone(),
            is_answered: thread.is_answered,
            confidence: thread.classification_confidence,
            first_client_message_id: thread.first_client_message_id.clone(),
            first_internal_reply_message_id: thread.first_internal_reply_message_id.clone(),
            last_internal_message_id: thread.last_internal_message_id.clone(),
            issues: vec!["La conversación recuperada está incompleta".into()],
        });
    }
    thread.classification = Classification::Ambiguous;
    thread.is_valid_client_request = false;
    thread.manual_review_required = true;
    thread.is_answered = false;
    thread.first_internal_reply_message_id = None;
    thread.last_internal_message_id = None;
    thread.first_internal_reply_at = None;
    thread.last_internal_message_at = None;
    thread.response_time_minutes = None;
    thread.resolution_time_minutes = None;
}

fn apply_batch_decision(
    thread: &mut EmailThread,
    messages: &[EmailMessage],
    decision: &BatchAuditDecision,
    auto_apply_threshold: f64,
    manual_review_threshold: f64,
) {
    let ids_known = batch_message_ids_known(decision, messages);
    let classification_is_valid = decision.classification == Classification::ValidClientRequest;
    let decision_is_consistent = decision.is_valid_client_request == classification_is_valid;
    let focus = thread
        .first_client_message_id
        .as_ref()
        .and_then(|id| messages.iter().find(|m| &m.id == id));
    let response_is_uncertain = focus.is_some_and(|request| {
        messages
            .iter()
            .any(|m| crate::analysis::reply_recipient_unknown(m, request))
    });
    let valid_trace_is_consistent = !classification_is_valid
        || (focus.is_some_and(|request| !request.is_internal && !request.is_automated)
            && decision.first_client_message_id == thread.first_client_message_id
            && decision.is_answered == thread.is_answered
            && [
                &decision.first_internal_reply_message_id,
                &decision.last_internal_message_id,
            ]
            .into_iter()
            .all(|id| {
                id.as_ref().is_none_or(|id| {
                    focus.is_some_and(|request| {
                        messages.iter().any(|m| {
                            &m.id == id && crate::analysis::reply_reaches_requester(m, request)
                        })
                    })
                })
            }));
    let review_required = decision.manual_review_required
        || decision.classification == Classification::Ambiguous
        || decision.confidence < auto_apply_threshold
        || !ids_known
        || !decision_is_consistent
        || !valid_trace_is_consistent
        || response_is_uncertain;
    thread.ai_suggestion = review_required.then(|| crate::analysis::AiSuggestion {
        classification: decision.classification.clone(),
        is_answered: decision.is_answered,
        confidence: decision.confidence,
        first_client_message_id: decision.first_client_message_id.clone(),
        first_internal_reply_message_id: decision.first_internal_reply_message_id.clone(),
        last_internal_message_id: decision.last_internal_message_id.clone(),
        issues: decision.issues.clone(),
    });
    thread.classification = if review_required {
        Classification::Ambiguous
    } else {
        decision.classification.clone()
    };
    thread.classification_source = ClassificationSource::Ai;
    thread.classification_confidence = decision.confidence;
    thread.is_valid_client_request = !review_required && classification_is_valid;
    if !review_required && !classification_is_valid {
        // Un cierre antiguo puede ser "ignorado y contestado": no afecta las
        // métricas válidas, pero conserva el estado que ve la persona revisora.
        thread.is_answered = decision.is_answered;
    }
    // La IA clasifica la actividad, pero no puede mover las métricas fuera de la
    // ventana: estos hitos ya fueron calculados contra el primer cliente recibido
    // dentro del período.
    apply_trace_dates(thread, messages);
    thread.manual_review_required = review_required;
    thread.reasons.push(if !ids_known {
        "Mira clasificó el hilo, pero entregó referencias de mensajes inválidas; revisa la trazabilidad."
            .to_string()
    } else if !decision_is_consistent {
        "Mira entregó una clasificación inconsistente; se dejó como propuesta para revisión."
            .to_string()
    } else if !valid_trace_is_consistent || response_is_uncertain {
        "La propuesta no confirma los hitos de respuesta dirigida al solicitante; requiere revisión."
            .to_string()
    } else if decision.classification == Classification::Ambiguous {
        "Mira encontró más de una clasificación plausible; revisa este hilo.".to_string()
    } else if decision.confidence < manual_review_threshold {
        "Mira clasificó el hilo con baja confianza; revisa este resultado.".to_string()
    } else if thread.manual_review_required {
        "Mira sugiere un estado que requiere confirmación manual.".to_string()
    } else {
        "Mira aplicó la auditoría por alta confianza.".to_string()
    });
    thread.reasons.extend(decision.issues.iter().cloned());
}

fn mark_semantic_review_pending(thread: &mut EmailThread) {
    thread.classification = Classification::Ambiguous;
    thread.is_valid_client_request = false;
    thread.classification_confidence = 0.0;
    thread.manual_review_required = true;
    thread.reasons.push(
        "La IA está desactivada: los filtros detectaron un candidato, pero la validez según los criterios del tenant debe confirmarse manualmente."
            .to_string(),
    );
}

fn mark_ai_audit_unavailable(thread: &mut EmailThread) {
    thread.classification = Classification::Ambiguous;
    thread.classification_confidence = 0.0;
    thread.is_valid_client_request = false;
    thread.manual_review_required = true;
    thread
        .reasons
        .push("Mira no pudo auditar este hilo; revísalo manualmente.".to_string());
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("\n[truncado]");
    truncated
}

fn apply_trace_dates(thread: &mut EmailThread, messages: &[EmailMessage]) {
    thread.first_client_message_at = thread
        .first_client_message_id
        .as_ref()
        .and_then(|id| messages.iter().find(|message| &message.id == id))
        .map(|message| message.date);
    thread.first_internal_reply_at = thread
        .first_internal_reply_message_id
        .as_ref()
        .and_then(|id| messages.iter().find(|message| &message.id == id))
        .map(|message| message.date);
    thread.last_internal_message_at = thread
        .last_internal_message_id
        .as_ref()
        .and_then(|id| messages.iter().find(|message| &message.id == id))
        .map(|message| message.date);
    thread.response_time_minutes = thread
        .first_client_message_at
        .zip(thread.first_internal_reply_at)
        .map(|(client, reply)| (reply - client).num_minutes());
    thread.resolution_time_minutes = thread
        .first_client_message_at
        .zip(thread.last_internal_message_at)
        .map(|(client, last)| (last - client).num_minutes());
}

fn worker_request_id() -> Option<String> {
    crate::REQUEST_ID.try_with(Clone::clone).ok()
}

fn messages_for_persistence(messages: &[EmailMessage]) -> Vec<EmailMessage> {
    messages
        .iter()
        .cloned()
        .map(|mut message| {
            message.body_text = None;
            message
        })
        .collect()
}

async fn fetch_cloud_run_identity_token(client: &Client, audience: &str) -> anyhow::Result<String> {
    let token = client
        .get("http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/identity")
        .header("Metadata-Flavor", "Google")
        .query(&[("audience", audience)])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(token)
}

async fn recalculate_run_metrics(state: &AppState, run_id: &str) -> anyhow::Result<()> {
    let mut run = state
        .storage
        .get_analysis_run(run_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("analysis run not found"))?;
    let threads = state.storage.list_threads(run_id).await?;
    // El embudo se calcula solo durante el run (los descartados no se almacenan),
    // así que lo preservamos al recomputar métricas tras una revisión manual.
    let funnel = run.metrics.funnel.clone();
    run.metrics = calculate_metrics(
        &threads,
        run.metrics.ai_input_tokens,
        run.metrics.ai_output_tokens,
    );
    run.metrics.funnel = funnel;
    state.storage.update_analysis_run(&run).await
}

async fn enforce_rate_limit(
    state: &AppState,
    user_email: &str,
    action: &str,
    limit: usize,
) -> Result<(), ApiError> {
    // Las cuentas internas privilegiadas no consumen cuota.
    if state.config.is_privileged_account(user_email) {
        return Ok(());
    }
    let key = format!("{}:{}", action, user_email.trim().to_lowercase());
    if state.rate_limiter.check(key, limit).await {
        Ok(())
    } else {
        Err(ApiError::too_many_requests(
            "Demasiados análisis solicitados; intenta nuevamente más tarde",
        ))
    }
}

pub(crate) async fn fresh_mailbox_access_token(
    state: &AppState,
    connection: &MailboxConnection,
) -> Result<String, ApiError> {
    if !mailbox_connection_is_active(Some(connection)) {
        return Err(ApiError::forbidden(
            "vuelve a conectar la casilla antes de analizar",
        ));
    }
    if connection.provider == MailboxProviderKind::Imap {
        return decrypt_token(
            connection
                .imap_password_encrypted
                .as_deref()
                .ok_or_else(|| ApiError::forbidden("vuelve a conectar la casilla IMAP"))?,
            &state.config.encryption_key,
        )
        .map_err(Into::into);
    }
    let encrypted_refresh = connection
        .refresh_token_encrypted
        .as_deref()
        .ok_or_else(|| {
            ApiError::forbidden("la conexión con la casilla expiró; vuelve a conectarla")
        })?;
    let refresh_token = decrypt_token(encrypted_refresh, &state.config.encryption_key)?;
    let token = match refresh_access_token_for(
        &state.http,
        connection.provider,
        &state.config.google,
        state.config.microsoft.as_ref(),
        &refresh_token,
    )
    .await
    {
        Ok(token) => token,
        Err(RefreshError::InvalidGrant) => {
            let mut updated = connection.clone();
            updated.needs_reauth_at = Some(Utc::now());
            updated.updated_at = Utc::now();
            state
                .storage
                .refresh_gmail_connection(connection, &updated)
                .await?;
            return Err(mailbox_refresh_error(RefreshError::InvalidGrant));
        }
        Err(error) => return Err(mailbox_refresh_error(error)),
    };

    let mut updated = connection.clone();
    updated.access_token_encrypted =
        encrypt_token(&token.access_token, &state.config.encryption_key)?;
    if let Some(rotated) = token.refresh_token.as_deref() {
        updated.refresh_token_encrypted =
            Some(encrypt_token(rotated, &state.config.encryption_key)?);
    }
    updated.updated_at = Utc::now();
    if state
        .storage
        .refresh_gmail_connection(connection, &updated)
        .await?
        != crate::storage::MailboxConnectionRefresh::Updated
    {
        return Err(ApiError::conflict(
            "la conexión de la casilla cambió mientras se iniciaba el análisis; inténtalo nuevamente",
        ));
    }
    Ok(token.access_token)
}

fn mailbox_refresh_error(error: RefreshError) -> ApiError {
    match error {
        RefreshError::InvalidGrant => {
            ApiError::forbidden("el proveedor rechazó la conexión; vuelve a conectar la casilla")
        }
        RefreshError::Other(_) => ApiError::service_unavailable(
            "no se pudo renovar la conexión con la casilla; inténtalo nuevamente",
        ),
    }
}

fn require_mailbox_connected(connection: Option<&MailboxConnection>) -> Result<(), ApiError> {
    if mailbox_connection_is_active(connection) {
        Ok(())
    } else {
        Err(ApiError::forbidden("conecta una casilla antes de analizar"))
    }
}

/// Plan vigente de una org: el de su suscripción si da acceso, o Mira Free por
/// defecto (cuentas nuevas y churned caen a Free, sin filas ni migración). En modo
/// dev (`enforcement_enabled=false`) se usa Pro para no toparse con nada.
async fn effective_plan_for_org(state: &AppState, org_id: &str) -> Result<BillingPlan, ApiError> {
    if !state.config.billing.enforcement_enabled {
        return Ok(plan_by_id(&BillingPlanId::Pro));
    }
    let subscription = state.storage.get_subscription_for_org(org_id).await?;
    if subscription_allows_access(subscription.as_ref(), Utc::now()) {
        Ok(plan_by_id(&subscription.expect("checked above").plan_id))
    } else {
        Ok(free_plan())
    }
}

async fn entitlement_snapshot(
    state: &AppState,
    org_id: &str,
    account_email: &str,
) -> Result<EntitlementSnapshot, ApiError> {
    if !state.config.billing.enforcement_enabled
        || state.config.is_privileged_account(account_email)
    {
        return Ok(EntitlementSnapshot {
            allowed: true,
            reason: None,
            subscription_status: Some(SubscriptionStatus::Active),
            plan: Some(plan_by_id(&BillingPlanId::Pro)),
            cancel_at_period_end: false,
            current_period_end: None,
            trial_ends_at: None,
        });
    }
    let subscription = state.storage.get_subscription_for_org(org_id).await?;
    // Capa gratuita: el acceso SIEMPRE está permitido. Quien no tiene suscripción
    // que dé acceso (nuevo o churned) usa Mira Free; quien la tiene, su plan pagado.
    let has_paid_access = subscription_allows_access(subscription.as_ref(), Utc::now());
    let plan = if has_paid_access {
        plan_by_id(&subscription.as_ref().expect("checked above").plan_id)
    } else {
        free_plan()
    };
    let cancel_at_period_end = subscription
        .as_ref()
        .map(|subscription| subscription.cancel_at_period_end)
        .unwrap_or(false);
    let current_period_end = subscription
        .as_ref()
        .and_then(|subscription| subscription.current_period_end);
    let trial_ends_at = subscription
        .as_ref()
        .and_then(|subscription| subscription.trial_ends_at);
    // Estado real de la suscripción, para que el front pueda mostrar un nudge de
    // recuperación (dunning) sin bloquear: una cancelación agendada ya vencida se
    // reporta como Cancelled.
    let subscription_status = subscription.map(|subscription| {
        if !has_paid_access
            && subscription.status == SubscriptionStatus::Active
            && subscription.cancel_at_period_end
        {
            SubscriptionStatus::Cancelled
        } else {
            subscription.status
        }
    });
    Ok(EntitlementSnapshot {
        allowed: true,
        reason: None,
        subscription_status,
        plan: Some(plan),
        cancel_at_period_end,
        current_period_end,
        trial_ends_at,
    })
}

async fn require_active_entitlement(
    state: &AppState,
    session: &UserSession,
) -> Result<OrgConfigBundle, ApiError> {
    let bundle = get_or_provision_org_config(state, &session.google_account_email).await?;
    require_org_role(
        &bundle,
        &[
            OrgRole::Owner,
            OrgRole::Admin,
            OrgRole::Analyst,
            OrgRole::Viewer,
        ],
    )?;
    let entitlement =
        entitlement_snapshot(state, &bundle.org.id, &session.google_account_email).await?;
    if entitlement.allowed {
        Ok(bundle)
    } else {
        Err(ApiError::payment_required(
            "necesitas un plan activo o trial vigente para usar la app",
        ))
    }
}

/// Guard de lectura: bloquea el historial/datos de análisis cuando el plan no está vigente.
async fn require_entitlement(state: &AppState, session: &UserSession) -> Result<(), ApiError> {
    require_active_entitlement(state, session).await.map(|_| ())
}

fn current_period_key() -> String {
    Utc::now().format("%Y-%m").to_string()
}

fn empty_usage(org_id: &str, period_key: &str) -> UsageLedger {
    UsageLedger {
        org_id: org_id.to_string(),
        period_key: period_key.to_string(),
        runs_created: 0,
        retrieved_threads: 0,
        ai_analyzed_threads: 0,
        updated_at: Utc::now(),
    }
}

async fn enforce_usage_allows_run(
    state: &AppState,
    org_id: &str,
    account_email: &str,
) -> Result<(), ApiError> {
    if !state.config.billing.enforcement_enabled
        || state.config.is_privileged_account(account_email)
    {
        return Ok(());
    }
    // Plan vigente (pagado o Mira Free por defecto). Free nunca tiene fila de
    // suscripción, así que NO podemos exigir una aquí.
    let plan = effective_plan_for_org(state, org_id).await?;
    let period_key = current_period_key();
    let usage = state
        .storage
        .get_usage_ledger(org_id, &period_key)
        .await?
        .unwrap_or_else(|| empty_usage(org_id, &period_key));
    if usage.runs_created >= plan.limits.runs_per_month {
        return Err(ApiError::payment_required(
            "alcanzaste el límite mensual de análisis de tu plan",
        ));
    }
    if usage.retrieved_threads >= plan.limits.retrieved_threads_per_month {
        return Err(ApiError::payment_required(
            "agotaste tu cupo mensual de hilos analizados — sube de plan para seguir",
        ));
    }
    Ok(())
}

pub(crate) async fn reserve_run_creation(
    state: &AppState,
    run: &AnalysisRun,
) -> Result<(), ApiError> {
    if !state.config.billing.enforcement_enabled
        || state.config.is_privileged_account(&run.user_email)
    {
        return Ok(());
    }
    let org_id = run
        .org_id
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("el análisis debe pertenecer a una organización"))?;
    let plan = effective_plan_for_org(state, org_id).await?;
    state
        .storage
        .reserve_analysis_usage(
            run,
            &current_period_key(),
            UsageAmounts {
                runs: 1,
                ..Default::default()
            },
            &plan.limits,
        )
        .await
        .map_err(|error| {
            if error.to_string() == "usage_quota_exceeded" {
                ApiError::payment_required("alcanzaste el límite mensual de análisis de tu plan")
            } else {
                error.into()
            }
        })?;
    check_quota_alerts(state, org_id, &current_period_key(), &run.user_email).await;
    Ok(())
}

#[cfg(test)]
async fn increment_runs_usage(
    state: &AppState,
    org_id: Option<&str>,
    account_email: &str,
) -> Result<(), ApiError> {
    if state.config.is_privileged_account(account_email) {
        return Ok(());
    }
    let Some(org_id) = org_id else {
        return Ok(());
    };
    let period_key = current_period_key();
    state
        .storage
        .add_usage(org_id, &period_key, 1, 0, 0)
        .await?;
    check_quota_alerts(state, org_id, &period_key, account_email).await;
    Ok(())
}

/// Avisos de consumo al 80% y 100% de cada eje con cupo mensual (plan §8 de
/// docs/plan-monetizacion-mira-helpdesk.md). Nunca falla el run que lo dispara:
/// un email que no sale es una degradación silenciosa aceptable, un run que
/// se cae por eso no lo es.
async fn check_quota_alerts(state: &AppState, org_id: &str, period_key: &str, account_email: &str) {
    if !state.config.billing.enforcement_enabled
        || state.config.is_privileged_account(account_email)
    {
        return;
    }
    let plan = match effective_plan_for_org(state, org_id).await {
        Ok(plan) => plan,
        Err(error) => {
            tracing::warn!(?error, %org_id, "no se pudo resolver el plan para avisos de cuota");
            return;
        }
    };
    let usage = match state.storage.get_usage_ledger(org_id, period_key).await {
        Ok(Some(usage)) => usage,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(%error, %org_id, "no se pudo leer el consumo para avisos de cuota");
            return;
        }
    };
    let axes: [(&str, u32, u32); 3] = [
        ("runs", usage.runs_created, plan.limits.runs_per_month),
        (
            "retrieved_threads",
            usage.retrieved_threads,
            plan.limits.retrieved_threads_per_month,
        ),
        (
            "ai_analyzed_threads",
            usage.ai_analyzed_threads,
            plan.limits.ai_analyzed_threads_per_month,
        ),
    ];
    for (axis, used, limit) in axes {
        if limit == 0 {
            continue;
        }
        let percent = (u64::from(used) * 100) / u64::from(limit);
        for threshold in [80u16, 100u16] {
            if percent < u64::from(threshold) {
                continue;
            }
            match state
                .storage
                .record_quota_alert_if_new(org_id, period_key, axis, threshold)
                .await
            {
                Ok(true) => {
                    if !send_quota_alert_email(state, account_email, &plan, axis, threshold).await
                        && let Err(error) = state
                            .storage
                            .release_quota_alert(org_id, period_key, axis, threshold)
                            .await
                    {
                        tracing::warn!(%error, %org_id, axis, threshold, "no se pudo liberar el aviso fallido")
                    }
                }
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!(%error, %org_id, axis, threshold, "no se pudo registrar el aviso de cuota")
                }
            }
        }
    }
}

fn quota_axis_label(axis: &str) -> &'static str {
    match axis {
        "runs" => "análisis mensuales",
        "retrieved_threads" => "correos sincronizados",
        "ai_analyzed_threads" => "conversaciones clasificadas por IA",
        _ => "consumo del plan",
    }
}

async fn send_quota_alert_email(
    state: &AppState,
    account_email: &str,
    plan: &BillingPlan,
    axis: &str,
    threshold: u16,
) -> bool {
    let mailer = match ResendMailer::from_config(&state.config.report) {
        Ok(mailer) => mailer,
        Err(error) => {
            tracing::warn!(%error, "Resend no configurado; se omite el aviso de cuota");
            return false;
        }
    };
    let axis_label = quota_axis_label(axis);
    let subject = format!(
        "Mira Helpdesk: {threshold}% de tu cuota de {axis_label} ({})",
        plan.name
    );
    let html = if threshold >= 100 {
        format!(
            "<p>Tu organización alcanzó el 100% de la cuota mensual de <strong>{axis_label}</strong> del plan {}. \
             No se procesarán más análisis en este eje hasta el próximo ciclo o hasta subir de plan.</p>",
            plan.name
        )
    } else {
        format!(
            "<p>Tu organización usó el {threshold}% de la cuota mensual de <strong>{axis_label}</strong> del plan {}.</p>",
            plan.name
        )
    };
    if let Err(error) = mailer
        .send(&[account_email.to_string()], &subject, &html)
        .await
    {
        tracing::warn!(%error, %account_email, "no se pudo enviar el aviso de cuota");
        return false;
    }
    true
}

#[derive(Debug, Deserialize)]
struct MercadoPagoPreapprovalResponse {
    id: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    date_created: Option<chrono::DateTime<Utc>>,
    #[serde(default)]
    external_reference: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MercadoPagoInvoice {
    preapproval_id: String,
    debit_date: chrono::DateTime<Utc>,
    #[serde(default)]
    payment: Option<MercadoPagoPayment>,
}

#[derive(Debug, Deserialize)]
struct MercadoPagoPayment {
    id: serde_json::Number,
    status: String,
}

#[derive(Debug, Deserialize)]
struct MercadoPagoInvoiceSearch {
    results: Vec<MercadoPagoInvoice>,
    paging: MercadoPagoPaging,
}

#[derive(Debug, Deserialize)]
struct MercadoPagoPaging {
    total: usize,
}

async fn get_mercadopago_invoice(
    state: &AppState,
    access_token: &str,
    id: &str,
) -> Result<MercadoPagoInvoice, ApiError> {
    let response = state
        .http
        .get(format!(
            "https://api.mercadopago.com/authorized_payments/{id}"
        ))
        .bearer_auth(access_token)
        .send()
        .await?;
    mercadopago_json(response, "get_invoice").await
}

async fn get_mercadopago_payment(
    state: &AppState,
    access_token: &str,
    id: &str,
) -> Result<MercadoPagoPayment, ApiError> {
    let response = state
        .http
        .get(format!("https://api.mercadopago.com/v1/payments/{id}"))
        .bearer_auth(access_token)
        .send()
        .await?;
    let payment: MercadoPagoPayment = mercadopago_json(response, "get_payment").await?;
    if payment.id.to_string() != id {
        return Err(ApiError::external_service_unavailable());
    }
    Ok(payment)
}

async fn search_mercadopago_invoices(
    state: &AppState,
    access_token: &str,
    filter: &str,
    id: &str,
) -> Result<Vec<MercadoPagoInvoice>, ApiError> {
    let mut invoices = Vec::new();
    for _ in 0..6 {
        let response = state
            .http
            .get("https://api.mercadopago.com/authorized_payments/search")
            .query(&[
                (filter, id),
                ("limit", "100"),
                ("offset", &invoices.len().to_string()),
            ])
            .bearer_auth(access_token)
            .send()
            .await?;
        let page: MercadoPagoInvoiceSearch = mercadopago_json(response, "search_invoices").await?;
        // ponytail: seis páginas cubren 50 años de cobros mensuales y caben en
        // el lease; rechazar histories mayores en vez de reconciliar incompleto.
        if page.paging.total > 600 || page.results.is_empty() && invoices.len() < page.paging.total
        {
            return Err(ApiError::external_service_unavailable());
        }
        invoices.extend(page.results);
        if invoices.len() >= page.paging.total {
            return Ok(invoices);
        }
    }
    Err(ApiError::external_service_unavailable())
}

async fn find_mercadopago_checkout(
    state: &AppState,
    provider: &MercadoPagoPreapprovalResponse,
) -> Result<Option<CheckoutSession>, ApiError> {
    if let Some(checkout) = state
        .storage
        .find_checkout_session_by_provider_id(&provider.id)
        .await?
    {
        return Ok(Some(checkout));
    }
    let Some(reference) = provider.external_reference.as_deref() else {
        return Ok(None);
    };
    Ok(state
        .storage
        .get_checkout_session(reference)
        .await?
        .filter(|checkout| {
            checkout.provider == "mercadopago"
                && checkout
                    .provider_subscription_id
                    .as_deref()
                    .is_none_or(|id| id == provider.id)
        }))
}

async fn reconcile_mercadopago_subscription_locked(
    state: &AppState,
    access_token: &str,
    provider: &MercadoPagoPreapprovalResponse,
    payment: Option<&MercadoPagoPayment>,
) -> Result<(), ApiError> {
    let mut checkout = find_mercadopago_checkout(state, provider)
        .await?
        .ok_or_else(|| ApiError::not_found("checkout session not found"))?;
    let current = state
        .storage
        .get_subscription_for_org(&checkout.org_id)
        .await?;
    // Una notificación tardía de la suscripción anterior no reemplaza la nueva.
    if current.as_ref().is_some_and(|subscription| {
        subscription
            .provider_subscription_id
            .as_deref()
            .is_some_and(|id| id != provider.id)
    }) {
        return Ok(());
    }
    let mut subscription = current.unwrap_or_else(|| {
        let started = provider.date_created.unwrap_or(checkout.created_at);
        let mut subscription = active_subscription_for_trial(
            checkout.org_id.clone(),
            checkout.plan_id.clone(),
            Some(provider.id.clone()),
            checkout.billing_interval.clone(),
            started,
        );
        subscription.trial_ends_at = (checkout.trial_days > 0
            && provider.status.as_deref() == Some("authorized"))
        .then(|| started + chrono::Duration::days(i64::from(checkout.trial_days)));
        subscription
    });
    let mut invoices =
        search_mercadopago_invoices(state, access_token, "preapproval_id", &provider.id).await?;
    if invoices
        .iter()
        .any(|invoice| invoice.preapproval_id != provider.id)
    {
        return Err(ApiError::external_service_unavailable());
    }
    if let Some(payment) = payment {
        for invoice in &mut invoices {
            if let Some(existing) = invoice.payment.as_mut()
                && existing.id == payment.id
            {
                existing.status.clone_from(&payment.status);
            }
        }
    }
    apply_mercadopago_reconciliation(&mut subscription, provider, &invoices, Utc::now());
    state.storage.upsert_subscription(&subscription).await?;
    if provider.status.as_deref() == Some("authorized")
        && checkout.status != CheckoutSessionStatus::Activated
    {
        checkout.provider_subscription_id = Some(provider.id.clone());
        checkout.status = CheckoutSessionStatus::Activated;
        checkout.updated_at = Utc::now();
        state.storage.upsert_checkout_session(&checkout).await?;
    }
    Ok(())
}

fn apply_mercadopago_reconciliation(
    subscription: &mut Subscription,
    provider: &MercadoPagoPreapprovalResponse,
    invoices: &[MercadoPagoInvoice],
    now: chrono::DateTime<Utc>,
) {
    // debit_date pertenece al ciclo facturado. La autorización de la tarjeta y
    // next_payment_date no prueban que se pagó ningún período.
    let eligible = invoices.iter().filter(|invoice| invoice.debit_date <= now);
    let paid = eligible
        .clone()
        .filter(|invoice| {
            invoice
                .payment
                .as_ref()
                .is_some_and(|payment| payment.status == "approved")
        })
        .max_by_key(|invoice| invoice.debit_date);
    subscription.current_period_start = paid.map(|invoice| invoice.debit_date);
    subscription.current_period_end =
        paid.map(|invoice| subscription.billing_interval.period_end(invoice.debit_date));
    let failed = eligible
        .max_by_key(|invoice| invoice.debit_date)
        .is_some_and(|invoice| {
            invoice.payment.as_ref().is_some_and(|payment| {
                matches!(
                    payment.status.as_str(),
                    "rejected" | "cancelled" | "refunded" | "charged_back"
                )
            })
        });
    subscription.status = match provider.status.as_deref() {
        Some("cancelled" | "canceled") => {
            subscription.cancel_at_period_end = true;
            SubscriptionStatus::Cancelled
        }
        Some("paused") => SubscriptionStatus::PastDue,
        Some("authorized") if subscription.cancel_at_period_end => SubscriptionStatus::Cancelled,
        Some("authorized") if failed => SubscriptionStatus::PastDue,
        Some("authorized") if subscription.current_period_end.is_some_and(|end| end > now) => {
            SubscriptionStatus::Active
        }
        Some("authorized") if subscription.trial_ends_at.is_some_and(|end| end > now) => {
            SubscriptionStatus::Trialing
        }
        Some("authorized") if paid.is_some() || subscription.trial_ends_at.is_some() => {
            SubscriptionStatus::PastDue
        }
        _ => SubscriptionStatus::Pending,
    };
    subscription.updated_at = now;
}

async fn create_mercadopago_preapproval(
    state: &AppState,
    access_token: &str,
    checkout: &CheckoutSession,
    card_token_id: &str,
    payer_email: &str,
) -> Result<MercadoPagoPreapprovalResponse, ApiError> {
    let body = mercadopago_preapproval_body(
        checkout,
        payer_email,
        card_token_id,
        &state.config.web_base_url,
        &state.config.api_base_url,
    );
    let response = state
        .http
        .post("https://api.mercadopago.com/preapproval")
        .header("X-Idempotency-Key", &checkout.id)
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await?;
    if matches!(response.status().as_u16(), 400 | 422) {
        let mut failed = checkout.clone();
        failed.status = CheckoutSessionStatus::Failed;
        failed.updated_at = Utc::now();
        state.storage.upsert_checkout_session(&failed).await?;
    }
    mercadopago_json(response, "create_preapproval").await
}

fn mercadopago_preapproval_body(
    checkout: &CheckoutSession,
    payer_email: &str,
    card_token_id: &str,
    web_base_url: &str,
    api_base_url: &str,
) -> serde_json::Value {
    let plan = plan_by_id(&checkout.plan_id);
    let frequency = match checkout.billing_interval {
        BillingInterval::Monthly => 1,
        BillingInterval::Annual => 12,
    };
    let mut auto_recurring = json!({
        "frequency": frequency,
        "frequency_type": "months",
        "start_date": checkout.created_at.to_rfc3339(),
        "transaction_amount": checkout.amount_clp,
        "currency_id": "CLP"
    });
    if checkout.trial_days > 0 {
        auto_recurring["free_trial"] = json!({
            "frequency": checkout.trial_days,
            "frequency_type": "days"
        });
    }
    json!({
        "reason": format!("{} - Helpdesk Inspector", plan.name),
        "external_reference": checkout.id,
        "payer_email": payer_email,
        "auto_recurring": auto_recurring,
        "back_url": web_base_url,
        "notification_url": format!("{api_base_url}/billing/mercadopago/webhook"),
        "card_token_id": card_token_id,
        "status": "authorized"
    })
}

async fn update_mercadopago_preapproval(
    state: &AppState,
    access_token: &str,
    provider_id: &str,
    body: serde_json::Value,
) -> Result<MercadoPagoPreapprovalResponse, ApiError> {
    let response = state
        .http
        .put(format!(
            "https://api.mercadopago.com/preapproval/{provider_id}"
        ))
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await?;
    mercadopago_json(response, "update_preapproval").await
}

async fn get_mercadopago_preapproval(
    state: &AppState,
    access_token: &str,
    provider_id: &str,
) -> Result<MercadoPagoPreapprovalResponse, ApiError> {
    let response = state
        .http
        .get(format!(
            "https://api.mercadopago.com/preapproval/{provider_id}"
        ))
        .bearer_auth(access_token)
        .send()
        .await?;
    let provider: MercadoPagoPreapprovalResponse =
        mercadopago_json(response, "get_preapproval").await?;
    if provider.id != provider_id {
        return Err(ApiError::external_service_unavailable());
    }
    Ok(provider)
}

async fn mercadopago_json<T: DeserializeOwned>(
    response: reqwest::Response,
    operation: &str,
) -> Result<T, ApiError> {
    let status = response.status();
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("missing")
        .to_string();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        let provider_message = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|body| body.get("message")?.as_str().map(str::to_string))
            .unwrap_or_else(|| "unavailable".to_string());
        tracing::warn!(
            %operation,
            provider_status = status.as_u16(),
            %request_id,
            %provider_message,
            "Mercado Pago rechazó la operación"
        );
        return Err(mercadopago_api_error(status));
    }
    serde_json::from_str(&text).map_err(|error| {
        tracing::warn!(
            %operation,
            %request_id,
            %error,
            "Mercado Pago devolvió una respuesta inválida"
        );
        ApiError::external_service_unavailable()
    })
}

fn mercadopago_api_error(status: StatusCode) -> ApiError {
    match status {
        StatusCode::BAD_REQUEST => ApiError::bad_request(
            "Mercado Pago rechazó la suscripción; revisa el correo del pagador y los datos de la tarjeta",
        ),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            ApiError::service_unavailable("Mercado Pago rechazó las credenciales configuradas")
        }
        StatusCode::TOO_MANY_REQUESTS => ApiError::service_unavailable(
            "Mercado Pago limitó temporalmente las solicitudes; inténtalo nuevamente",
        ),
        _ => ApiError::external_service_unavailable(),
    }
}

fn verify_mercadopago_webhook(
    state: &AppState,
    headers: &HeaderMap,
    signed_data_id: Option<&str>,
) -> Result<(), ApiError> {
    let Some(expected) = &state.config.billing.mercadopago_webhook_secret else {
        // Sin secreto configurado, producción nunca acepta webhooks sin firma;
        // en desarrollo se permite para pruebas locales sin credenciales.
        if state.config.is_production() {
            return Err(ApiError::unauthorized());
        }
        return Ok(());
    };
    let x_signature = headers
        .get("x-signature")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let x_request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    validate_mercadopago_signature(x_signature, x_request_id, signed_data_id, expected)
}

fn validate_mercadopago_signature(
    x_signature: &str,
    x_request_id: &str,
    signed_data_id: Option<&str>,
    secret: &str,
) -> Result<(), ApiError> {
    let (timestamp, received_hash) =
        parse_webhook_signature(x_signature).ok_or_else(ApiError::unauthorized)?;
    let signed = timestamp
        .parse::<u128>()
        .map_err(|_| ApiError::unauthorized())?;
    // Mercado Pago publica ejemplos tanto en segundos como en milisegundos.
    // La firma siempre usa el timestamp original; solo normalizamos su edad.
    let milliseconds = if signed < 1_000_000_000_000 {
        signed * 1000
    } else {
        signed
    };
    validate_webhook_timestamp(&milliseconds.to_string(), SystemTime::now())?;
    let mut parts = Vec::new();
    if let Some(data_id) = signed_data_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(format!("id:{}", data_id.to_lowercase()));
    }
    let request_id = x_request_id.trim();
    if !request_id.is_empty() {
        parts.push(format!("request-id:{request_id}"));
    }
    parts.push(format!("ts:{timestamp}"));
    let manifest = format!("{};", parts.join(";"));

    type HmacSha256 = Hmac<Sha256>;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| ApiError::unauthorized())?;
    mac.update(manifest.as_bytes());
    let received = hex_to_bytes(received_hash).ok_or_else(ApiError::unauthorized)?;
    mac.verify_slice(&received)
        .map_err(|_| ApiError::unauthorized())
}

fn verify_workos_webhook(
    state: &AppState,
    headers: &HeaderMap,
    body: &str,
) -> Result<(), ApiError> {
    let secret = state
        .config
        .workos
        .webhook_secret
        .as_deref()
        .ok_or_else(ApiError::unauthorized)?;
    let signature = headers
        .get("workos-signature")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    validate_workos_signature(signature, body, secret)
}

fn validate_workos_signature(signature: &str, body: &str, secret: &str) -> Result<(), ApiError> {
    let (timestamp, received_hash) =
        parse_webhook_signature(signature).ok_or_else(ApiError::unauthorized)?;
    validate_webhook_timestamp(timestamp, SystemTime::now())?;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| ApiError::unauthorized())?;
    mac.update(format!("{timestamp}.{body}").as_bytes());
    let received = hex_to_bytes(received_hash).ok_or_else(ApiError::unauthorized)?;
    mac.verify_slice(&received)
        .map_err(|_| ApiError::unauthorized())
}

fn validate_webhook_timestamp(timestamp: &str, now: SystemTime) -> Result<(), ApiError> {
    let signed_ms = timestamp
        .parse::<u128>()
        .map_err(|_| ApiError::unauthorized())?;
    let now_ms = now
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ApiError::unauthorized())?
        .as_millis();
    let drift_ms = signed_ms.abs_diff(now_ms);
    if drift_ms > Duration::from_secs(5 * 60).as_millis() {
        return Err(ApiError::unauthorized());
    }
    Ok(())
}

fn parse_webhook_signature(header_value: &str) -> Option<(&str, &str)> {
    let mut timestamp = None;
    let mut v1 = None;
    for part in header_value.split(',') {
        let (key, value) = part.split_once('=')?;
        match key.trim().to_ascii_lowercase().as_str() {
            "ts" | "t" if !value.trim().is_empty() => timestamp = Some(value.trim()),
            "v1" if !value.trim().is_empty() => v1 = Some(value.trim()),
            _ => {}
        }
    }
    Some((timestamp?, v1?))
}

#[cfg(test)]
fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn hex_to_bytes(value: &str) -> Option<Vec<u8>> {
    let trimmed = value.trim();
    if !trimmed.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(trimmed.len() / 2);
    let mut chars = trimmed.chars();
    while let (Some(high), Some(low)) = (chars.next(), chars.next()) {
        let high = high.to_digit(16)?;
        let low = low.to_digit(16)?;
        bytes.push(((high << 4) | low) as u8);
    }
    Some(bytes)
}

async fn require_owned_run(
    state: &AppState,
    run_id: &str,
    session: &UserSession,
) -> Result<AnalysisRun, ApiError> {
    let run = state
        .storage
        .get_analysis_run(run_id)
        .await?
        .ok_or(ApiError::not_found("analysis run not found"))?;
    if run.retention_deadline() <= Utc::now() {
        return Err(ApiError::not_found("analysis run not found"));
    }
    if let Some(org_id) = &run.org_id {
        if !state
            .storage
            .user_is_org_member(org_id, &session.google_account_email)
            .await?
        {
            return Err(ApiError::not_found("analysis run not found"));
        }
        return Ok(run);
    }
    if run.user_email != session.google_account_email {
        return Err(ApiError::not_found("analysis run not found"));
    }
    Ok(run)
}

async fn require_owned_thread(
    state: &AppState,
    run_id: &str,
    thread_id: &str,
    session: &UserSession,
) -> Result<EmailThread, ApiError> {
    require_owned_run(state, run_id, session)
        .await
        .map_err(|_| ApiError::not_found("thread not found"))?;
    let thread = state
        .storage
        .get_thread(run_id, thread_id)
        .await?
        .ok_or(ApiError::not_found("thread not found"))?;
    Ok(thread)
}

async fn require_unique_owned_thread(
    state: &AppState,
    thread_id: &str,
    session: &UserSession,
) -> Result<EmailThread, ApiError> {
    let mut owned = Vec::new();
    for thread in state.storage.find_threads_by_id(thread_id).await? {
        if require_owned_run(state, &thread.analysis_run_id, session)
            .await
            .is_ok()
        {
            owned.push(thread);
        }
    }
    match owned.len() {
        0 => Err(ApiError::not_found("thread not found")),
        1 => Ok(owned.remove(0)),
        _ => Err(ApiError::conflict(
            "thread_id aparece en varios análisis; usa la ruta con run_id",
        )),
    }
}

async fn require_session(state: &AppState, headers: &HeaderMap) -> Result<UserSession, ApiError> {
    let Some(cookie) = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| extract_named_cookie(cookies, "ghmi_session"))
    else {
        tracing::warn!("auth rejected: ghmi_session cookie missing");
        return Err(ApiError::unauthorized());
    };

    let Some(session_id) = verify_session_cookie(&cookie, &state.config.session_secret) else {
        tracing::warn!("auth rejected: ghmi_session cookie signature invalid");
        return Err(ApiError::unauthorized());
    };

    let session = state.storage.get_user_session(&session_id).await?;
    if session.is_none() {
        tracing::warn!("auth rejected: session not found in storage");
    }
    let session = session.ok_or(ApiError::unauthorized())?;
    if !session_is_active(&session, Utc::now()) {
        tracing::warn!("auth rejected: session expired or revoked");
        return Err(ApiError::unauthorized());
    }
    Ok(session)
}

fn extract_named_cookie(cookies: &str, expected_name: &str) -> Option<String> {
    cookies.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == expected_name).then(|| value.to_string())
    })
}

fn random_urlsafe(bytes: usize) -> String {
    let mut raw = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut raw);
    URL_SAFE_NO_PAD.encode(raw)
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
    details: Option<serde_json::Value>,
}

impl ApiError {
    fn not_found(message: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.to_string(),
            details: None,
        }
    }

    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: "Autenticación requerida".to_string(),
            details: None,
        }
    }

    pub(crate) fn forbidden(message: &str) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.to_string(),
            details: None,
        }
    }

    fn payment_required(message: &str) -> Self {
        Self {
            status: StatusCode::PAYMENT_REQUIRED,
            message: message.to_string(),
            details: None,
        }
    }

    fn bad_request(message: &str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.to_string(),
            details: None,
        }
    }

    fn incomplete_config(missing: Vec<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: "la configuración sigue incompleta".to_string(),
            details: Some(json!({ "missing": missing })),
        }
    }

    fn conflict(message: &str) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.to_string(),
            details: None,
        }
    }

    fn too_many_requests(message: &str) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: message.to_string(),
            details: None,
        }
    }

    fn service_unavailable(message: &str) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.to_string(),
            details: None,
        }
    }

    fn internal() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Ocurrió un error inesperado. Inténtalo nuevamente.".to_string(),
            details: None,
        }
    }

    fn external_service_unavailable() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: "Un servicio externo no respondió correctamente. Inténtalo nuevamente."
                .to_string(),
            details: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let request_id = crate::REQUEST_ID.try_with(Clone::clone).ok();
        let mut error = json!({
            "code": error_code(self.status),
            "message": self.message,
            "request_id": request_id,
        });
        if let Some(details) = self.details {
            error["details"] = details;
        }
        (self.status, Json(json!({ "error": error }))).into_response()
    }
}

fn error_code(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "BAD_REQUEST",
        StatusCode::UNAUTHORIZED => "AUTHENTICATION_REQUIRED",
        StatusCode::FORBIDDEN => "FORBIDDEN",
        StatusCode::PAYMENT_REQUIRED => "SUBSCRIPTION_REQUIRED",
        StatusCode::NOT_FOUND => "NOT_FOUND",
        StatusCode::CONFLICT => "CONFLICT",
        StatusCode::TOO_MANY_REQUESTS => "RATE_LIMITED",
        StatusCode::BAD_GATEWAY => "EXTERNAL_SERVICE_UNAVAILABLE",
        StatusCode::SERVICE_UNAVAILABLE => "SERVICE_UNAVAILABLE",
        _ => "INTERNAL_ERROR",
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(_error: anyhow::Error) -> Self {
        tracing::error!("la solicitud API falló con un error interno");
        Self::internal()
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(_error: reqwest::Error) -> Self {
        tracing::warn!("un proveedor externo no respondió correctamente");
        Self::external_service_unavailable()
    }
}

trait GoogleResponseExt {
    async fn json_or_google_error<T: DeserializeOwned>(self, label: &str) -> anyhow::Result<T>;
    async fn json_or_external_error<T: DeserializeOwned>(self, label: &str) -> anyhow::Result<T>;
}

impl GoogleResponseExt for reqwest::Response {
    async fn json_or_google_error<T: DeserializeOwned>(self, label: &str) -> anyhow::Result<T> {
        let status = self.status();
        let text = self.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow::anyhow!("{label} failed with {status}"));
        }
        serde_json::from_str(&text)
            .map_err(|error| anyhow::anyhow!("{label} returned invalid JSON: {error}"))
    }

    async fn json_or_external_error<T: DeserializeOwned>(self, label: &str) -> anyhow::Result<T> {
        let status = self.status();
        let text = self.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow::anyhow!("{label} failed with {status}"));
        }
        serde_json::from_str(&text)
            .map_err(|error| anyhow::anyhow!("{label} returned invalid JSON: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        body::Body,
        http::{Method, Request, StatusCode, header},
    };
    use chrono::Utc;
    use http_body_util::BodyExt;
    use serde_json::json;
    use tower::ServiceExt;

    use super::*;
    use crate::{
        analysis::{AnalysisConfig, Classification},
        auth::sign_session_id,
        billing::{Account, BillingPlanId, Subscription, SubscriptionStatus},
        config::{AppConfig, test_app_config},
        policies::OrgConfigResponse,
        storage::MemoryStorage,
    };

    #[test]
    fn ai_budget_audits_up_to_the_remaining_quota_and_skips_the_rest() {
        // Cupo de sobra: se auditan todos.
        assert_eq!(split_by_ai_budget(10, 100), (10, 0));
        // Cupo justo: se audita hasta el tope y el resto queda heurístico.
        assert_eq!(split_by_ai_budget(10, 4), (4, 6));
        // Cupo agotado: nada va a IA, pero el análisis igual corre.
        assert_eq!(split_by_ai_budget(10, 0), (0, 10));
        // Dev (`enforce=false`) usa usize::MAX: nunca recorta.
        assert_eq!(split_by_ai_budget(10, usize::MAX), (10, 0));
    }

    #[test]
    fn batch_retry_policy_does_not_repeat_invalid_singletons_or_permanent_errors() {
        assert!(!should_split_missing_batch(1));
        assert!(should_split_missing_batch(2));
        let transient = BatchCallError {
            kind: "worker_timeout",
            retryable: true,
        };
        let permanent = BatchCallError {
            kind: "worker_client_error",
            retryable: false,
        };
        assert!(should_retry_batch_error(&transient, 0));
        assert!(!should_retry_batch_error(&transient, 1));
        assert!(!should_retry_batch_error(&permanent, 0));
    }

    #[test]
    fn retrieval_max_gives_headroom_for_finite_cap_and_respects_policy_otherwise() {
        // Free (tope finito): recupera con holgura (factor 3) acotado por el techo.
        assert_eq!(retrieval_max_for(true, 40, 50), 120);
        assert_eq!(retrieval_max_for(true, 200, 50), FREE_RETRIEVAL_HARD_MAX);
        // Cupo pequeño: nunca recupera menos que el propio tope.
        assert_eq!(retrieval_max_for(true, 5, 50), 15);
        // Plan de pago (sin tope): respeta el max_threads_per_run de policy.
        assert_eq!(retrieval_max_for(false, 999, 50), 50);
    }

    #[tokio::test]
    async fn unexpected_errors_do_not_expose_internal_details() {
        let response =
            ApiError::from(anyhow::anyhow!("provider body contains secret-token")).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(
            body["error"]["message"],
            "Ocurrió un error inesperado. Inténtalo nuevamente."
        );
        assert_eq!(body["error"]["code"], "INTERNAL_ERROR");
        assert!(!body.to_string().contains("secret-token"));
    }

    #[tokio::test]
    async fn public_errors_include_a_code_and_the_server_request_id() {
        let test = seeded_app().await;
        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/analysis-runs/does-not-exist",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .unwrap()
            .to_string();
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["error"]["code"], "NOT_FOUND");
        assert_eq!(body["error"]["request_id"], request_id);
    }

    #[tokio::test]
    async fn current_request_id_is_available_for_worker_requests() {
        let expected = Uuid::new_v4().to_string();
        let forwarded = crate::REQUEST_ID
            .scope(expected.clone(), async { worker_request_id() })
            .await;
        assert_eq!(forwarded.as_deref(), Some(expected.as_str()));
    }

    #[tokio::test]
    async fn protected_errors_keep_the_public_contract_and_hide_foreign_runs() {
        let test = seeded_app().await;

        let response = test
            .app
            .clone()
            .oneshot(request(Method::GET, "/auth/me", None, None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["error"]["code"], "AUTHENTICATION_REQUIRED");

        let response = test
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/auth/logout")
                    .header(header::COOKIE, &test.alice_cookie)
                    .header(header::ORIGIN, "https://untrusted.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["error"]["code"], "FORBIDDEN");

        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::GET,
                "/analysis-runs/run-alice",
                Some(&test.bob_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let foreign: serde_json::Value = response_json(response).await;

        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/analysis-runs/does-not-exist",
                Some(&test.bob_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let absent: serde_json::Value = response_json(response).await;

        assert_eq!(foreign["error"]["code"], "NOT_FOUND");
        assert_eq!(foreign["error"]["message"], absent["error"]["message"]);
    }

    #[tokio::test]
    async fn failed_runs_persist_a_safe_error_code() {
        let test = seeded_app().await;
        let state = AppState::new(test_app_config(), Arc::new(test.storage.clone()));

        mark_run_failed(
            &state,
            "run-alice",
            &anyhow::anyhow!("provider body contains secret-token"),
        )
        .await;

        let stored = test
            .storage
            .get_analysis_run("run-alice")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.error_message.as_deref(), Some("analysis_failed"));
        assert!(
            !stored
                .error_message
                .as_deref()
                .unwrap_or_default()
                .contains("secret-token")
        );

        mark_run_failed(
            &state,
            "run-bob",
            &anyhow::anyhow!("provider body contains secret-token")
                .context(AnalysisFailureStage("analysis_memory_budget")),
        )
        .await;
        let stored = test
            .storage
            .get_analysis_run("run-bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.status, AnalysisStatus::Failed);
        assert_eq!(
            stored.error_message.as_deref(),
            Some("analysis_memory_budget_exceeded")
        );
        assert!(stored.progress_message.contains("reduce el período"));
        assert!(!stored.progress_message.contains("secret-token"));
    }

    #[test]
    fn analysis_failure_stage_is_safe_and_specific() {
        let staged = anyhow::anyhow!("provider body contains secret-token")
            .context(AnalysisFailureStage("mailbox_list_threads"));

        assert_eq!(analysis_failure_stage(&staged), "mailbox_list_threads");
        assert_eq!(
            analysis_failure_stage(&anyhow::anyhow!("provider body contains secret-token")),
            "unknown"
        );
    }

    #[test]
    fn mailbox_refresh_errors_are_safe_and_actionable() {
        let rejected = mailbox_refresh_error(RefreshError::InvalidGrant);
        assert_eq!(rejected.status, StatusCode::FORBIDDEN);
        assert!(rejected.message.contains("vuelve a conectar"));

        let unavailable = mailbox_refresh_error(RefreshError::Other(anyhow::anyhow!(
            "provider body contains secret-token"
        )));
        assert_eq!(unavailable.status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(!unavailable.message.contains("secret-token"));
    }

    #[test]
    fn credentials_are_reused_only_for_the_same_provider_and_mailbox() {
        let mut previous = Some(MailboxConnection {
            microsoft_target_email: None,
            imap_config: None,
            imap_password_encrypted: None,
            needs_reauth_at: None,
            owner_email: "owner@example.com".to_string(),
            provider: MailboxProviderKind::Google,
            mailbox_email: "mailbox@example.com".to_string(),
            access_token_encrypted: "access".to_string(),
            refresh_token_encrypted: Some("google-refresh".to_string()),
            connected_at: Utc::now(),
            updated_at: Utc::now(),
            revoked_at: None,
        });

        let same = matching_previous_connection(
            &previous,
            MailboxProviderKind::Google,
            "MAILBOX@example.com",
        )
        .unwrap();
        assert_eq!(
            same.refresh_token_encrypted.as_deref(),
            Some("google-refresh")
        );
        assert!(
            matching_previous_connection(
                &previous,
                MailboxProviderKind::Microsoft,
                "mailbox@example.com"
            )
            .is_none()
        );
        assert!(
            matching_previous_connection(
                &previous,
                MailboxProviderKind::Google,
                "other@example.com"
            )
            .is_none()
        );

        assert_eq!(
            google_revocation_token(previous.as_ref()),
            Some("google-refresh")
        );
        previous.as_mut().unwrap().provider = MailboxProviderKind::Microsoft;
        assert_eq!(google_revocation_token(previous.as_ref()), None);
    }

    #[tokio::test]
    async fn start_analysis_requires_a_refresh_token_before_claiming_the_run() {
        let fixture = seeded_app().await;
        let now = Utc::now();
        let mut pending = fixture
            .storage
            .get_analysis_run("run-alice")
            .await
            .unwrap()
            .unwrap();
        let state = AppState::new(test_app_config(), Arc::new(fixture.storage.clone()));
        let bundle = get_or_provision_org_config(&state, "alice@example.com")
            .await
            .unwrap();
        pending.org_id = Some(bundle.org.id.clone());
        pending.mailbox_id = Some(bundle.mailbox.id.clone());
        pending.policy_snapshot = Some(bundle.policy_version.snapshot.clone());
        pending.status = AnalysisStatus::Pending;
        pending.completed_at = None;
        fixture.storage.update_analysis_run(&pending).await.unwrap();
        fixture
            .storage
            .upsert_gmail_connection(&MailboxConnection {
                microsoft_target_email: None,
                imap_config: None,
                imap_password_encrypted: None,
                needs_reauth_at: None,
                owner_email: "alice@example.com".to_string(),
                provider: MailboxProviderKind::Google,
                mailbox_email: "alice@example.com".to_string(),
                access_token_encrypted: "current-access-token".to_string(),
                refresh_token_encrypted: None,
                connected_at: now,
                updated_at: now,
                revoked_at: None,
            })
            .await
            .unwrap();

        let response = fixture
            .app
            .oneshot(request(
                Method::POST,
                "/analysis-runs/run-alice/start",
                Some(&fixture.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let run = fixture
            .storage
            .get_analysis_run("run-alice")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(run.status, AnalysisStatus::Pending);
    }

    #[tokio::test]
    async fn readiness_reports_ok_with_healthy_storage() {
        let test = seeded_app().await;
        let response = test
            .app
            .oneshot(
                Request::builder()
                    .uri("/health/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["ok"], true);
    }

    #[tokio::test]
    async fn responses_get_a_server_generated_request_id() {
        let test = seeded_app().await;
        let response = test
            .app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header("x-request-id", "untrusted-client-value")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .expect("every response includes a request id");

        assert_ne!(request_id, "untrusted-client-value");
        assert!(Uuid::parse_str(request_id).is_ok());
    }

    #[test]
    fn persistence_discards_full_email_bodies() {
        let mut message = message("message-1", "cliente@customer.test");
        message.body_text = Some("contenido completo que no debe persistirse".to_string());

        let persisted = messages_for_persistence(&[message.clone()]);

        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].body_text, None);
        assert_eq!(persisted[0].id, message.id);
        assert_eq!(persisted[0].snippet, message.snippet);
    }

    #[test]
    fn batch_audit_selects_unique_milestones_and_compact_content() {
        let mut thread = thread("thread-1", "run-1");
        thread.first_client_message_id = Some("msg-d".to_string());
        let mut first = message("msg-a", "cliente@customer.test");
        first.body_text = Some("abcdefgh".to_string());
        let mut first_reply = message("msg-b", "agente@example.com");
        first_reply.is_internal = true;
        first_reply.is_external = false;
        first_reply.to_emails = vec![first.from_email.clone()];
        first_reply.date = first.date + chrono::Duration::seconds(1);
        let mut automated = message("msg-c", "robot@example.com");
        automated.is_automated = true;
        automated.date = first.date + chrono::Duration::seconds(2);
        let mut last_client = message("msg-d", "cliente@customer.test");
        last_client.date = first.date + chrono::Duration::seconds(3);
        let mut last_reply = message("msg-e", "agente@example.com");
        last_reply.is_internal = true;
        last_reply.is_external = false;
        last_reply.to_emails = vec![last_client.from_email.clone()];
        last_reply.date = first.date + chrono::Duration::seconds(4);
        let messages = vec![first, first_reply, automated, last_client, last_reply];
        let selected = batch_messages_for_thread(&thread, &messages, 3, 5);
        let full_context = batch_messages_for_thread(&thread, &messages, 5, 5);
        let fingerprint = message_fingerprint(&messages);
        let expected_thread = serde_json::to_value(&thread).unwrap();
        let expected_messages = serde_json::to_value(&selected).unwrap();
        let prepared = PreparedThread::new(thread, messages, vec![], true, false, (3, 5));

        assert_eq!(selected.len(), 3);
        assert_eq!(
            selected
                .iter()
                .map(|message| message.message_id.as_str())
                .collect::<Vec<_>>(),
            vec!["msg-a", "msg-d", "msg-e"]
        );
        assert_eq!(selected[0].content, "abcde\n[truncado]");
        assert_eq!(selected[1].content, "Neces\n[truncado]");
        assert_eq!(selected[2].to_emails, vec!["cliente@customer.test"]);

        assert_eq!(full_context.len(), 5);
        assert_eq!(
            serde_json::to_value(batch_summary(&prepared)).unwrap()["messages"],
            expected_messages
        );
        assert!(
            prepared
                .messages
                .iter()
                .all(|message| message.body_text.is_none())
        );
        assert_eq!(message_fingerprint(&prepared.messages), fingerprint);
        assert_eq!(
            serde_json::to_value(&prepared.thread).unwrap(),
            expected_thread
        );
    }

    #[test]
    fn prepared_threads_release_large_bodies_and_enforce_metadata_budget() {
        let mut thread = thread("large-thread", "run-1");
        thread.first_client_message_id = Some("message-0".to_string());
        let messages = (0..500)
            .map(|index| {
                let mut message = message(&format!("message-{index}"), "cliente@customer.test");
                message.body_text = Some("🦊".repeat(32 * 1024));
                message.date += chrono::Duration::seconds(index);
                message
            })
            .collect::<Vec<_>>();
        let original_body_bytes = messages
            .iter()
            .map(|message| message.body_text.as_ref().unwrap().capacity())
            .sum::<usize>();
        assert_eq!(original_body_bytes, 500 * 128 * 1024);

        let mut prepared = PreparedThread::new(thread, messages, vec![], true, false, (24, 2000));

        assert!(
            prepared
                .messages
                .iter()
                .all(|message| message.body_text.is_none())
        );
        assert_eq!(prepared.batch_messages.len(), 24);
        assert!(
            prepared
                .batch_messages
                .iter()
                .all(|message| { message.content.len() <= 2000 * 4 + "\n[truncado]".len() })
        );
        eprintln!(
            "full_body_bytes={original_body_bytes}; retained_estimate_bytes={}",
            prepared.retained_bytes()
        );
        assert!(prepared.retained_bytes() < 1024 * 1024);
        assert!(prepared.retained_bytes() < original_body_bytes / 60);

        let mut retained = 0;
        reserve_prepared_bytes(&mut retained, &prepared).unwrap();
        let accepted_bytes = retained;
        prepared.messages[0].headers = json!({"x-large": "x".repeat(MAX_PREPARED_RUN_BYTES)});
        let error = reserve_prepared_bytes(&mut retained, &prepared).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("analysis_memory_budget_exceeded")
        );
        assert_eq!(retained, accepted_bytes);
    }

    #[test]
    fn missing_batch_decision_uses_safe_manual_review_reason() {
        let mut failed = thread("thread-1", "run-1");
        mark_ai_audit_unavailable(&mut failed);

        assert!(failed.manual_review_required);
        assert_eq!(failed.classification, Classification::Ambiguous);
        assert!(!failed.is_valid_client_request);
        assert_eq!(calculate_metrics(&[failed.clone()], 0, 0).valid_requests, 0);
        assert!(
            failed
                .reasons
                .iter()
                .any(|reason| reason.contains("no pudo auditar"))
        );
    }

    #[test]
    fn truncated_high_confidence_result_keeps_proposal_out_of_confirmed_metrics() {
        let mut candidate = thread("truncated", "run-truncated");
        candidate.classification = Classification::ValidClientRequest;
        candidate.classification_source = ClassificationSource::Ai;
        candidate.classification_confidence = 0.99;
        candidate.manual_review_required = false;
        candidate.is_answered = true;
        candidate.first_internal_reply_message_id = Some("reply".into());
        candidate.response_time_minutes = Some(12);
        assert_eq!(calculate_metrics(&[candidate.clone()], 0, 0).answered, 1);
        defer_incomplete_conversation(&mut candidate);
        let metrics = calculate_metrics(&[candidate.clone()], 0, 0);
        assert_eq!(metrics.valid_requests, 0);
        assert_eq!(metrics.answered, 0);
        assert_eq!(metrics.avg_first_response_minutes, None);
        assert_eq!(metrics.pending_review, 1);
        let proposal = candidate.ai_suggestion.unwrap();
        assert_eq!(proposal.classification, Classification::ValidClientRequest);
        assert!(proposal.is_answered);
        assert_eq!(
            proposal.first_internal_reply_message_id.as_deref(),
            Some("reply")
        );
        assert_eq!(candidate.first_internal_reply_message_id, None);
    }

    #[test]
    fn ai_disabled_candidates_require_semantic_review_and_do_not_count_as_valid() {
        let mut candidate = thread("candidate", "run");
        candidate.classification = Classification::ValidClientRequest;
        candidate.is_valid_client_request = true;
        candidate.manual_review_required = false;
        mark_semantic_review_pending(&mut candidate);
        let metrics = calculate_metrics(&[candidate], 0, 0);
        assert_eq!(metrics.valid_requests, 0);
        assert_eq!(metrics.pending_review, 1);
        assert_eq!(metrics.ignored, 0);
    }

    #[test]
    fn ai_cannot_confirm_an_answer_to_a_team_only_exchange() {
        let mut candidate = thread("team-only", "run");
        let request = message("msg-a", "cliente@customer.test");
        let mut team_reply = message("msg-b", "agente@example.com");
        team_reply.is_internal = true;
        team_reply.is_external = false;
        team_reply.to_emails = vec!["supervisor@example.com".to_string()];
        team_reply.date = request.date + chrono::Duration::minutes(5);
        let decision = BatchAuditDecision {
            thread_id: candidate.thread_id.clone(),
            classification: Classification::ValidClientRequest,
            is_valid_client_request: true,
            is_answered: true,
            first_client_message_id: Some(request.id.clone()),
            first_internal_reply_message_id: Some(team_reply.id.clone()),
            last_internal_message_id: Some(team_reply.id.clone()),
            confidence: 0.99,
            manual_review_required: false,
            issues: vec![],
        };
        apply_batch_decision(
            &mut candidate,
            &[request, team_reply],
            &decision,
            0.92,
            0.72,
        );
        assert!(candidate.manual_review_required);
        assert!(!candidate.is_answered);
        assert!(!candidate.is_valid_client_request);
        assert!(candidate.ai_suggestion.as_ref().unwrap().is_answered);
        assert_eq!(calculate_metrics(&[candidate], 0, 0).answered, 0);
    }

    #[test]
    fn ai_cannot_confirm_an_automated_message_as_a_human_request() {
        let mut candidate = thread("automated", "run");
        let mut automated = message("msg-a", "system@customer.test");
        automated.is_automated = true;
        let decision = BatchAuditDecision {
            thread_id: candidate.thread_id.clone(),
            classification: Classification::ValidClientRequest,
            is_valid_client_request: true,
            is_answered: false,
            first_client_message_id: Some(automated.id.clone()),
            first_internal_reply_message_id: None,
            last_internal_message_id: None,
            confidence: 0.99,
            manual_review_required: false,
            issues: vec![],
        };
        apply_batch_decision(&mut candidate, &[automated], &decision, 0.92, 0.72);
        assert_eq!(candidate.classification, Classification::Ambiguous);
        assert!(!candidate.is_valid_client_request);
        assert!(candidate.manual_review_required);
        assert_eq!(calculate_metrics(&[candidate], 0, 0).valid_requests, 0);
    }

    struct TestApp {
        app: axum::Router,
        storage: MemoryStorage,
        alice_cookie: String,
        bob_cookie: String,
    }

    async fn seeded_app() -> TestApp {
        seeded_app_with_config(test_app_config()).await
    }

    async fn seeded_app_with_config(config: AppConfig) -> TestApp {
        let storage = MemoryStorage::default();
        storage
            .upsert_user_session(&session("alice-session", "alice@example.com"))
            .await
            .unwrap();
        storage
            .upsert_user_session(&session("bob-session", "bob@example.com"))
            .await
            .unwrap();
        storage
            .upsert_account(&account(
                "workos-alice-session",
                "alice@example.com",
                "alice-org",
            ))
            .await
            .unwrap();
        storage
            .upsert_account(&account("workos-bob-session", "bob@example.com", "bob-org"))
            .await
            .unwrap();
        let alice_run = run("run-alice", "alice@example.com");
        let bob_run = run("run-bob", "bob@example.com");
        storage.create_analysis_run(&alice_run).await.unwrap();
        storage.create_analysis_run(&bob_run).await.unwrap();
        storage
            .upsert_subscription(&subscription("alice-org", "alice@example.com"))
            .await
            .unwrap();
        storage
            .upsert_subscription(&subscription("bob-org", "bob@example.com"))
            .await
            .unwrap();
        storage
            .upsert_thread(
                &thread("thread-alice", "run-alice"),
                &[message("msg-a", "cliente@customer.test")],
            )
            .await
            .unwrap();
        storage
            .upsert_thread(
                &thread("thread-bob", "run-bob"),
                &[message("msg-b", "cliente@customer.test")],
            )
            .await
            .unwrap();

        let alice_cookie = signed_cookie("alice-session", &config.session_secret);
        let bob_cookie = signed_cookie("bob-session", &config.session_secret);
        TestApp {
            app: crate::build_app(config, Arc::new(storage.clone())),
            storage,
            alice_cookie,
            bob_cookie,
        }
    }

    fn signed_cookie(session_id: &str, secret: &str) -> String {
        format!(
            "ghmi_session={}",
            sign_session_id(session_id, secret).unwrap()
        )
    }

    fn signed_workos_webhook(body: &str, secret: &str) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .to_string();
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{timestamp}.{body}").as_bytes());
        format!(
            "t={timestamp},v1={}",
            hex_lower(&mac.finalize().into_bytes())
        )
    }

    fn workos_webhook_request(body: &str, signature: &str) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri("/auth/workos/webhook")
            .header(header::CONTENT_TYPE, "application/json")
            .header("workos-signature", signature)
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    fn session(id: &str, email: &str) -> UserSession {
        let now = Utc::now();
        UserSession {
            id: id.to_string(),
            workos_user_id: Some(format!("workos-{id}")),
            workos_session_id: None,
            google_account_email: email.to_string(),
            gmail_account_email: Some(email.to_string()),
            access_token_encrypted: "access".to_string(),
            refresh_token_encrypted: Some("refresh".to_string()),
            gmail_access_token_encrypted: Some("access".to_string()),
            gmail_refresh_token_encrypted: Some("refresh".to_string()),
            expires_at: Some(session_expires_at(now)),
            revoked_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn workos_access_token_extracts_the_session_id_for_local_mapping() {
        let payload = URL_SAFE_NO_PAD.encode(json!({ "sid": "session_123" }).to_string());
        let token = format!("header.{payload}.signature");

        assert_eq!(
            workos_session_id_from_access_token(Some(&token)),
            Some("session_123".to_string())
        );
        assert_eq!(workos_session_id_from_access_token(Some("invalid")), None);
    }

    fn subscription(org_id: &str, email: &str) -> Subscription {
        let now = Utc::now();
        Subscription {
            id: format!("sub-{email}"),
            org_id: org_id.to_string(),
            plan_id: BillingPlanId::Pro,
            status: SubscriptionStatus::Active,
            provider: "test".to_string(),
            provider_subscription_id: Some(format!("provider-{email}")),
            billing_interval: BillingInterval::Monthly,
            current_period_start: Some(now),
            current_period_end: Some(now + chrono::Duration::days(30)),
            trial_ends_at: None,
            cancel_at_period_end: false,
            created_at: now,
            updated_at: now,
        }
    }

    fn account(workos_user_id: &str, email: &str, org_id: &str) -> Account {
        let now = Utc::now();
        Account {
            workos_user_id: workos_user_id.to_string(),
            email: email.to_string(),
            name: None,
            org_id: org_id.to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    fn run(id: &str, user_email: &str) -> AnalysisRun {
        AnalysisRun {
            id: id.to_string(),
            user_email: user_email.to_string(),
            org_id: None,
            mailbox_id: None,
            trigger_type: None,
            policy_version_id: None,
            policy_hash: None,
            policy_snapshot: None,
            gmail_scope_snapshot: vec![],
            retention_expires_at: None,
            data_minimization_mode: None,
            config: AnalysisConfig {
                responder_emails: vec![],
                request_scope: crate::analysis::RequestScope::External,
                date_from: "2026-06-01".to_string(),
                date_to: "2026-06-02".to_string(),
                time_from: "00:00".to_string(),
                time_to: "23:59".to_string(),
                timezone: "America/Santiago".to_string(),
                internal_domains: vec!["example.com".to_string()],
                ignored_senders: vec![],
                ignored_domains: vec![],
                ignored_keywords: vec![],
                include_labels: vec![],
                exclude_labels: vec![],
            },
            status: AnalysisStatus::Completed,
            progress_message: "done".to_string(),
            processed_threads: 1,
            total_candidate_threads: 1,
            metrics: AnalysisMetrics::default(),
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
            error_message: None,
        }
    }

    fn thread(id: &str, run_id: &str) -> EmailThread {
        let now = chrono::DateTime::parse_from_rfc3339("2026-06-01T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        EmailThread {
            ai_suggestion: None,
            id: id.to_string(),
            analysis_run_id: run_id.to_string(),
            thread_id: format!("gmail-{id}"),
            subject: "Ayuda".to_string(),
            normalized_subject: "ayuda".to_string(),
            classification: Classification::ValidClientRequest,
            classification_source: ClassificationSource::Rules,
            classification_confidence: 0.9,
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
            reasons: vec!["test".to_string()],
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn ai_batch_size_splits_fifty_candidates_into_twenty_twenty_ten() {
        let indexes = (0..50).collect::<Vec<_>>();
        let sizes = indexes
            .chunks(AI_BATCH_SIZE)
            .map(<[usize]>::len)
            .collect::<Vec<_>>();
        assert_eq!(sizes, vec![20, 20, 10]);
    }

    #[test]
    fn batch_applies_high_confidence_and_prefills_manual_suggestions() {
        let mut candidate = thread("batch", "run");
        candidate.manual_review_required = false;
        candidate.is_answered = true;
        candidate.first_internal_reply_message_id = Some("msg-b".to_string());
        candidate.last_internal_message_id = Some("msg-b".to_string());
        let mut first = message("msg-a", "cliente@customer.test");
        let mut reply = message("msg-b", "agente@example.com");
        reply.is_internal = true;
        reply.is_external = false;
        reply.to_emails = vec![first.from_email.clone()];
        reply.date = first.date + chrono::Duration::seconds(1);
        first.date -= chrono::Duration::seconds(1);
        let messages = vec![first, reply];
        let mut decision = BatchAuditDecision {
            thread_id: candidate.thread_id.clone(),
            classification: Classification::ValidClientRequest,
            is_valid_client_request: true,
            is_answered: true,
            first_client_message_id: Some("msg-a".to_string()),
            first_internal_reply_message_id: Some("msg-b".to_string()),
            last_internal_message_id: Some("msg-b".to_string()),
            confidence: 0.95,
            manual_review_required: false,
            issues: vec![],
        };
        apply_batch_decision(&mut candidate, &messages, &decision, 0.92, 0.72);
        assert!(candidate.is_answered);
        assert!(!candidate.manual_review_required);
        assert_eq!(candidate.classification_source, ClassificationSource::Ai);

        decision.confidence = 0.85;
        decision.classification = Classification::Misc;
        decision.is_valid_client_request = false;
        decision.manual_review_required = true;
        apply_batch_decision(&mut candidate, &messages, &decision, 0.92, 0.72);
        assert_eq!(candidate.classification, Classification::Ambiguous);
        assert_eq!(
            candidate.ai_suggestion.as_ref().unwrap().classification,
            Classification::Misc
        );
        assert!(candidate.manual_review_required);
        assert_eq!(candidate.classification_source, ClassificationSource::Ai);

        let mut low_confidence = thread("low", "run");
        decision.confidence = 0.5;
        apply_batch_decision(&mut low_confidence, &messages, &decision, 0.92, 0.72);
        assert_eq!(low_confidence.classification, Classification::Ambiguous);
        assert_eq!(
            low_confidence
                .ai_suggestion
                .as_ref()
                .unwrap()
                .classification,
            Classification::Misc
        );
        assert!(!low_confidence.is_valid_client_request);
        assert_eq!(
            low_confidence.classification_source,
            ClassificationSource::Ai
        );
        assert_eq!(low_confidence.classification_confidence, 0.5);
        assert!(low_confidence.manual_review_required);
        let provisional = calculate_metrics(&[low_confidence], 0, 0);
        assert_eq!(provisional.ignored, 0);
        assert_eq!(provisional.valid_requests, 0);
        assert_eq!(provisional.pending_review, 1);

        let mut invalid_ids = thread("invalid", "run");
        decision.confidence = 0.95;
        decision.first_client_message_id = Some("invented".to_string());
        apply_batch_decision(&mut invalid_ids, &messages, &decision, 0.92, 0.72);
        assert_eq!(invalid_ids.classification, Classification::Ambiguous);
        assert!(!invalid_ids.is_valid_client_request);
        assert_eq!(invalid_ids.classification_source, ClassificationSource::Ai);
        assert!(invalid_ids.manual_review_required);
        assert_eq!(
            invalid_ids.first_client_message_id.as_deref(),
            Some("msg-a")
        );
        assert!(!invalid_ids.is_answered);
        assert!(invalid_ids.ai_suggestion.as_ref().unwrap().is_answered);
    }

    #[tokio::test]
    async fn privileged_account_has_no_billing_limits_without_subscription() {
        let mut config = test_app_config();
        config.internal_full_access_emails = vec!["alice@example.com".to_string()];
        let storage = MemoryStorage::default();
        let state = AppState::new(config, Arc::new(storage.clone()));

        let entitlement = entitlement_snapshot(&state, "alice-org", "alice@example.com")
            .await
            .unwrap();
        assert!(entitlement.allowed);
        assert_eq!(entitlement.plan.unwrap().id, BillingPlanId::Pro);

        storage
            .add_usage("alice-org", &current_period_key(), 10, 400, 100)
            .await
            .unwrap();
        enforce_usage_allows_run(&state, "alice-org", "alice@example.com")
            .await
            .unwrap();
        increment_runs_usage(&state, Some("alice-org"), "alice@example.com")
            .await
            .unwrap();
        assert_eq!(
            storage
                .get_usage_ledger("alice-org", &current_period_key())
                .await
                .unwrap()
                .unwrap()
                .runs_created,
            10
        );
    }

    #[test]
    fn screen_hint_whitelist_only_accepts_known_values() {
        assert_eq!(normalize_screen_hint(Some("sign-up")), Some("sign-up"));
        assert_eq!(normalize_screen_hint(Some("sign-in")), Some("sign-in"));
        assert_eq!(normalize_screen_hint(Some("javascript:alert(1)")), None);
        assert_eq!(normalize_screen_hint(Some("")), None);
        assert_eq!(normalize_screen_hint(None), None);
    }

    fn message(id: &str, from: &str) -> EmailMessage {
        EmailMessage {
            id: id.to_string(),
            message_id: format!("gmail-{id}"),
            from_email: from.to_string(),
            from_name: None,
            to_emails: vec!["help@example.com".to_string()],
            cc_emails: vec![],
            date: chrono::DateTime::parse_from_rfc3339("2026-06-01T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            subject: "Ayuda".to_string(),
            snippet: "Necesito ayuda".to_string(),
            headers: json!({}),
            is_internal: false,
            is_external: true,
            is_automated: false,
            body_text: None,
        }
    }

    async fn response_json<T: serde::de::DeserializeOwned>(
        response: axum::response::Response,
    ) -> T {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn request(
        method: Method,
        uri: &str,
        cookie: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        if body.is_some() {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
        }
        builder
            .body(Body::from(
                body.map(|value| value.to_string()).unwrap_or_default(),
            ))
            .unwrap()
    }

    async fn complete_analysis_setup(app: &Router, cookie: &str) {
        let response = app.clone().oneshot(request(
            Method::PUT, "/me/org/config", Some(cookie),
            Some(json!({"analysis_policy":{"valid_request_criteria":["Solicitudes de soporte"]}})),
        )).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    async fn report_fixture() -> TestApp {
        let test = seeded_app().await;
        let mut bundle = provision_default_config("alice@example.com", Utc::now());
        bundle.org.id = "alice-org".to_string();
        bundle.membership.org_id = bundle.org.id.clone();
        bundle.mailbox.org_id = bundle.org.id.clone();
        bundle.draft.org_id = bundle.org.id.clone();
        bundle.draft.analysis_policy.timezone = "America/Santiago".to_string();
        bundle.policy_version = policy_version_from_draft(
            &bundle.mailbox,
            &bundle.draft,
            1,
            "alice@example.com",
            Utc::now(),
        );
        test.storage.upsert_org_config(&bundle).await.unwrap();
        test
    }

    async fn store_report_snapshot(
        test: &TestApp,
        run_id: &str,
        created_at: chrono::DateTime<Utc>,
        received_at: chrono::DateTime<Utc>,
        updated_at: chrono::DateTime<Utc>,
    ) {
        let mut snapshot = run(run_id, "alice@example.com");
        snapshot.org_id = Some("alice-org".to_string());
        snapshot.mailbox_id = Some("mailbox-alice".to_string());
        snapshot.config.date_from = "2026-07-01".to_string();
        snapshot.config.date_to = "2026-07-07".to_string();
        snapshot.created_at = created_at;
        snapshot.completed_at = Some(created_at);
        snapshot.retention_expires_at = Some(Utc::now() + chrono::Duration::days(90));
        test.storage.create_analysis_run(&snapshot).await.unwrap();
        let mut item = thread(&format!("record-{run_id}"), run_id);
        item.thread_id = "provider-thread-shared".to_string();
        item.first_client_message_id = Some(format!("request-{}", received_at.timestamp()));
        item.first_client_message_at = Some(received_at);
        item.first_message_at = Some(received_at);
        item.manual_review_required = false;
        item.updated_at = updated_at;
        test.storage.upsert_thread(&item, &[]).await.unwrap();
    }

    #[tokio::test]
    async fn consolidated_report_deduplicates_overlapping_runs_before_counting() {
        let test = report_fixture().await;
        let received = "2026-07-03T15:00:00Z".parse().unwrap();
        let first = "2026-07-08T10:00:00Z".parse().unwrap();
        let latest = "2026-07-09T10:00:00Z".parse().unwrap();
        store_report_snapshot(&test, "report-old", first, received, first).await;
        store_report_snapshot(&test, "report-new", latest, received, latest).await;
        for (id, scopes) in [
            (
                "report-old",
                vec!["User.Read", "Mail.Read", "offline_access"],
            ),
            (
                "report-new",
                vec![
                    "Mail.Read.Shared",
                    "offline_access",
                    "User.Read",
                    "Mail.Read",
                ],
            ),
        ] {
            let mut snapshot = test.storage.get_analysis_run(id).await.unwrap().unwrap();
            snapshot.gmail_scope_snapshot = scopes.into_iter().map(str::to_string).collect();
            test.storage.update_analysis_run(&snapshot).await.unwrap();
        }
        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/me/report?date_from=2026-07-03&date_to=2026-07-03",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["run_count"], 2);
        assert_eq!(body["metrics"]["total_threads"], 1);
        assert_eq!(body["metrics"]["valid_requests"], 1);
        assert_eq!(body["threads"][0]["analysis_run_id"], "report-new");
    }

    #[tokio::test]
    async fn consolidated_report_keeps_distinct_mailboxes_after_replacing_a_connection() {
        let test = report_fixture().await;
        let received = "2026-07-03T15:00:00Z".parse().unwrap();
        let first = "2026-07-08T10:00:00Z".parse().unwrap();
        let latest = "2026-07-09T10:00:00Z".parse().unwrap();
        store_report_snapshot(&test, "report-old", first, received, first).await;
        store_report_snapshot(&test, "report-new", latest, received, latest).await;
        for (id, mailbox) in [
            ("report-old", "support-a@company.test"),
            ("report-new", "support-b@company.test"),
        ] {
            let mut snapshot = test.storage.get_analysis_run(id).await.unwrap().unwrap();
            let mut policy = provision_default_config("alice@example.com", Utc::now())
                .policy_version
                .snapshot;
            policy.mailbox.google_account_email = mailbox.to_string();
            snapshot.policy_snapshot = Some(policy);
            snapshot.gmail_scope_snapshot = vec!["Mail.Read".to_string()];
            test.storage.update_analysis_run(&snapshot).await.unwrap();
        }
        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/me/report?date_from=2026-07-03&date_to=2026-07-03",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["metrics"]["total_threads"], 2);
        assert_eq!(body["metrics"]["valid_requests"], 2);
    }

    #[tokio::test]
    async fn consolidated_report_filters_received_dates_at_the_tenant_midnight_boundary() {
        let test = report_fixture().await;
        let created = "2026-07-08T10:00:00Z".parse().unwrap();
        store_report_snapshot(
            &test,
            "report-last-second",
            created,
            "2026-07-04T03:59:59Z".parse().unwrap(),
            created,
        )
        .await;
        store_report_snapshot(
            &test,
            "report-next-day",
            created,
            "2026-07-04T04:00:00Z".parse().unwrap(),
            created,
        )
        .await;
        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/me/report?date_from=2026-07-03&date_to=2026-07-03",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["timezone"], "America/Santiago");
        assert_eq!(body["metrics"]["total_threads"], 1);
        assert_eq!(body["threads"][0]["analysis_run_id"], "report-last-second");
    }

    #[tokio::test]
    async fn consolidated_report_uses_a_later_manual_correction_on_an_older_run() {
        let test = report_fixture().await;
        let received = "2026-07-03T15:00:00Z".parse().unwrap();
        let first = "2026-07-08T10:00:00Z".parse().unwrap();
        let latest = "2026-07-09T10:00:00Z".parse().unwrap();
        store_report_snapshot(&test, "report-old", first, received, first).await;
        store_report_snapshot(&test, "report-new", latest, received, latest).await;
        let mut corrected = test
            .storage
            .get_thread("report-old", "record-report-old")
            .await
            .unwrap()
            .unwrap();
        corrected.classification = Classification::Misc;
        corrected.classification_source = ClassificationSource::Manual;
        corrected.is_valid_client_request = false;
        corrected.manual_override_applied = true;
        corrected.updated_at = "2026-07-10T10:00:00Z".parse().unwrap();
        test.storage.upsert_thread(&corrected, &[]).await.unwrap();
        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/me/report?date_from=2026-07-03&date_to=2026-07-03",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["metrics"]["valid_requests"], 0);
        assert_eq!(body["metrics"]["ignored"], 1);
        assert_eq!(body["metrics"]["manual_overrides"], 1);
        assert_eq!(body["threads"][0]["analysis_run_id"], "report-old");
    }

    #[tokio::test]
    async fn org_config_provisions_default_policy() {
        let test = seeded_app().await;
        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/me/org/config",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: OrgConfigResponse = response_json(response).await;
        assert_eq!(body.membership.user_email, "alice@example.com");
        assert_eq!(
            body.draft.analysis_policy.internal_domains,
            vec!["example.com"]
        );
        assert!(!body.draft.ai_policy.enabled);
        assert!(body.draft.ai_policy.consent_granted_at.is_none());
        assert_eq!(body.draft.retention_policy.retention_days, 30);
        assert_eq!(body.setup_state.missing, vec!["valid_request_criteria"]);
    }

    #[tokio::test]
    async fn org_config_finalize_rejects_incomplete_without_persisting() {
        let test = seeded_app().await;
        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::PUT,
                "/me/org/config",
                Some(&test.alice_cookie),
                Some(json!({
                    "finalize": true,
                    "analysis_policy": { "internal_domains": [] }
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(
            body["error"]["details"]["missing"],
            json!(["responder_emails", "valid_request_criteria"])
        );

        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/me/org/config",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        let body: OrgConfigResponse = response_json(response).await;
        assert_eq!(
            body.draft.analysis_policy.internal_domains,
            vec!["example.com"]
        );
    }

    #[tokio::test]
    async fn org_config_draft_can_remain_incomplete_and_finalize_can_complete_it() {
        let test = seeded_app().await;
        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::PUT,
                "/me/org/config",
                Some(&test.alice_cookie),
                Some(json!({
                    "analysis_policy": { "valid_request_criteria": [] }
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["setup_state"]["ready_for_analysis"], false);

        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::PUT,
                "/me/org/config",
                Some(&test.alice_cookie),
                Some(json!({
                    "finalize": true,
                    "analysis_policy": {
                        "mailbox_aliases": ["soporte@example.com"],
                        "valid_request_criteria": ["Clientes externos solicitan soporte"]
                    }
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["setup_state"]["ready_for_analysis"], true);
        assert_eq!(
            body["policy_version"]["snapshot"]["analysis_policy"]["mailbox_aliases"],
            json!(["soporte@example.com"])
        );

        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/me/org/config",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        let body: OrgConfigResponse = response_json(response).await;
        assert!(body.draft.mailbox_aliases_configured);
    }

    #[tokio::test]
    async fn org_config_accepts_personal_gmail_account() {
        // La beta ahora acepta cualquier cuenta de Google, incluidas las
        // personales @gmail.com (antes devolvía 400 "solo Workspace").
        let config = test_app_config();
        let storage = MemoryStorage::default();
        storage
            .upsert_user_session(&session("gmail-session", "persona@gmail.com"))
            .await
            .unwrap();
        let cookie = signed_cookie("gmail-session", &config.session_secret);
        let app = crate::build_app(config, Arc::new(storage));

        let response = app
            .oneshot(request(
                Method::PUT,
                "/me/org/config",
                Some(&cookie),
                Some(json!({
                    "analysis_policy": {
                        "valid_request_criteria": ["Clientes externos solicitan soporte"]
                    }
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn detect_rejects_a_malformed_email_before_touching_dns() {
        let test = seeded_app().await;
        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/mailbox/detect?email=sin-arroba",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn detect_requires_a_session() {
        let test = seeded_app().await;
        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/mailbox/detect?email=ana@empresa.cl",
                None,
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn public_plans_expose_clp_prices_and_only_pro_trial() {
        let app = crate::build_app(test_app_config(), Arc::new(MemoryStorage::default()));
        let response = app
            .oneshot(request(Method::GET, "/public/plans", None, None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        // Solo dos planes comprables: Mira Free se asigna por defecto, no se vende.
        assert_eq!(body.as_array().unwrap().len(), 2);
        assert_eq!(body[0]["id"], "inicial");
        assert_eq!(body[0]["clp_monthly"], 9990);
        assert_eq!(body[0]["trial_days"], 0);
        assert_eq!(body[1]["id"], "pro");
        assert_eq!(body[1]["clp_monthly"], 29990);
        assert_eq!(body[1]["trial_days"], 30);
    }

    #[tokio::test]
    async fn logout_revokes_the_server_side_session() {
        let fixture = seeded_app().await;
        let mut session = fixture
            .storage
            .get_user_session("alice-session")
            .await
            .unwrap()
            .unwrap();
        session.workos_session_id = Some("session-alice".to_string());
        fixture.storage.upsert_user_session(&session).await.unwrap();
        let response = fixture
            .app
            .clone()
            .oneshot(request(
                Method::POST,
                "/auth/logout",
                Some(&fixture.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(
            body["logout_url"],
            "https://api.workos.com/user_management/sessions/logout?session_id=session-alice&return_to=http%3A%2F%2Flocalhost%3A5173"
        );

        let response = fixture
            .app
            .oneshot(request(
                Method::GET,
                "/auth/me",
                Some(&fixture.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn logout_all_revokes_every_session_for_the_owner_only() {
        let fixture = seeded_app().await;
        fixture
            .storage
            .upsert_user_session(&session("alice-other", "alice@example.com"))
            .await
            .unwrap();
        let alice_other_cookie = signed_cookie("alice-other", &test_app_config().session_secret);

        let response = fixture
            .app
            .clone()
            .oneshot(request(
                Method::POST,
                "/auth/logout-all",
                Some(&fixture.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        for id in ["alice-session", "alice-other"] {
            assert!(
                fixture
                    .storage
                    .get_user_session(id)
                    .await
                    .unwrap()
                    .unwrap()
                    .revoked_at
                    .is_some()
            );
        }
        assert!(
            fixture
                .storage
                .get_user_session("bob-session")
                .await
                .unwrap()
                .unwrap()
                .revoked_at
                .is_none()
        );

        for cookie in [&fixture.alice_cookie, &alice_other_cookie] {
            let response = fixture
                .app
                .clone()
                .oneshot(request(Method::GET, "/auth/me", Some(cookie), None))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        let response = fixture
            .app
            .oneshot(request(
                Method::GET,
                "/auth/me",
                Some(&fixture.bob_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn workos_callback_hides_provider_error_description() {
        let fixture = seeded_app().await;
        let response = fixture
            .app
            .oneshot(request(
                Method::GET,
                "/auth/workos/callback?error=access_denied&error_description=provider-secret%40example.com",
                None,
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["error"]["code"], "BAD_REQUEST");
        assert_eq!(
            body["error"]["message"],
            "No se pudo completar el inicio de sesión. Inténtalo nuevamente."
        );
        assert!(!body.to_string().contains("provider-secret"));
    }

    #[tokio::test]
    async fn mutations_reject_an_untrusted_origin() {
        let fixture = seeded_app().await;
        for (method, uri) in [
            (Method::POST, "/auth/logout"),
            (Method::POST, "/auth/logout-all"),
            (Method::POST, "/gmail/disconnect"),
            (Method::DELETE, "/me/analysis-data"),
            (Method::PUT, "/me/org/config"),
            (Method::POST, "/me/filter-presets"),
            (Method::PUT, "/me/filter-presets/preset-1"),
            (Method::DELETE, "/me/filter-presets/preset-1"),
            (Method::POST, "/analysis-runs"),
            (Method::POST, "/analysis-runs/run-alice/start"),
            (
                Method::PATCH,
                "/analysis-runs/run-alice/threads/thread-alice/manual-review",
            ),
            (Method::PATCH, "/threads/thread-alice/manual-review"),
        ] {
            let response = fixture
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header(header::COOKIE, &fixture.alice_cookie)
                        .header(header::ORIGIN, "https://untrusted.example")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{uri}");
        }
    }

    #[tokio::test]
    async fn expired_sessions_are_rejected_server_side() {
        let config = test_app_config();
        let storage = MemoryStorage::default();
        let mut expired = session("expired-session", "expired@example.com");
        expired.expires_at = Some(Utc::now() - chrono::Duration::seconds(1));
        storage.upsert_user_session(&expired).await.unwrap();
        let cookie = signed_cookie(&expired.id, &config.session_secret);
        let app = crate::build_app(config, Arc::new(storage));

        let response = app
            .oneshot(request(Method::GET, "/auth/me", Some(&cookie), None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn disconnect_gmail_preserves_schedule_preference_for_reconnection() {
        let fixture = seeded_app().await;
        let response = fixture
            .app
            .clone()
            .oneshot(request(
                Method::PUT,
                "/me/org/config",
                Some(&fixture.alice_cookie),
                Some(json!({
                    "schedule_report_policy": {
                        "scheduler_enabled": true,
                        "analysis_time": "14:00",
                        "report_recipients": ["ops@example.com"]
                    }
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let mut current = fixture
            .storage
            .get_user_session("alice-session")
            .await
            .unwrap()
            .unwrap();
        current.access_token_encrypted.clear();
        current.refresh_token_encrypted = None;
        current.gmail_access_token_encrypted = None;
        current.gmail_refresh_token_encrypted = None;
        fixture.storage.upsert_user_session(&current).await.unwrap();

        let mut historical = session("alice-old", "alice@example.com");
        historical.access_token_encrypted.clear();
        historical.refresh_token_encrypted = None;
        historical.gmail_access_token_encrypted = None;
        historical.gmail_refresh_token_encrypted = None;
        fixture
            .storage
            .upsert_user_session(&historical)
            .await
            .unwrap();
        let mut connection = fixture
            .storage
            .get_gmail_connection("alice@example.com")
            .await
            .unwrap()
            .unwrap();
        connection.access_token_encrypted.clear();
        connection.refresh_token_encrypted = None;
        fixture
            .storage
            .upsert_gmail_connection(&connection)
            .await
            .unwrap();

        let response = fixture
            .app
            .oneshot(request(
                Method::POST,
                "/gmail/disconnect",
                Some(&fixture.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        assert!(!mailbox_connection_is_active(
            fixture
                .storage
                .get_gmail_connection("alice@example.com")
                .await
                .unwrap()
                .as_ref()
        ));
        let bundle = fixture
            .storage
            .get_org_config_for_user("alice@example.com")
            .await
            .unwrap()
            .unwrap();
        assert!(bundle.mailbox.revoked_at.is_some());
        assert!(bundle.draft.schedule_report_policy.scheduler_enabled);
        assert_eq!(
            bundle.draft.schedule_report_policy.report_recipients,
            vec!["ops@example.com"]
        );
        assert!(
            fixture
                .storage
                .list_schedule_configs()
                .await
                .unwrap()
                .into_iter()
                .find(|config| config.user_email == "alice@example.com")
                .is_some_and(|config| !config.enabled)
        );

        connection.access_token_encrypted = "new-access".to_string();
        connection.refresh_token_encrypted = Some("new-refresh".to_string());
        connection.revoked_at = None;
        fixture
            .storage
            .upsert_gmail_connection(&connection)
            .await
            .unwrap();
        let mut bundle = bundle;
        bundle.mailbox.revoked_at = None;
        fixture.storage.upsert_org_config(&bundle).await.unwrap();
        let state = AppState::new(test_app_config(), Arc::new(fixture.storage.clone()));
        sync_schedule_config_from_policy(&state, &bundle)
            .await
            .unwrap();
        assert!(
            fixture
                .storage
                .list_schedule_configs()
                .await
                .unwrap()
                .into_iter()
                .find(|config| config.user_email == "alice@example.com")
                .is_some_and(|config| config.enabled && config.recipients == ["ops@example.com"])
        );
    }

    #[test]
    fn embedded_preapproval_uses_card_token_and_native_trial_without_redirect_url() {
        let now = Utc::now();
        let checkout = CheckoutSession {
            id: "checkout-1".to_string(),
            org_id: "org-1".to_string(),
            account_email: "owner@example.com".to_string(),
            plan_id: BillingPlanId::Pro,
            status: CheckoutSessionStatus::Pending,
            provider: "mercadopago".to_string(),
            provider_subscription_id: None,
            billing_interval: BillingInterval::Monthly,
            currency_id: "CLP".to_string(),
            amount_clp: 29_990,
            usd_reference_monthly: 29,
            trial_days: 30,
            created_at: now,
            updated_at: now,
        };

        let body = mercadopago_preapproval_body(
            &checkout,
            "payer@example.com",
            "card-token",
            "https://mira.example",
            "https://api.example",
        );

        assert_eq!(body["status"], "authorized");
        assert_eq!(body["card_token_id"], "card-token");
        assert_eq!(body["auto_recurring"]["frequency"], 1);
        assert_eq!(body["auto_recurring"]["free_trial"]["frequency"], 30);
        assert_eq!(
            body["auto_recurring"]["free_trial"]["frequency_type"],
            "days"
        );
        assert_eq!(body["back_url"], "https://mira.example");
        assert!(body.get("init_point").is_none());
    }

    #[test]
    fn annual_checkout_bills_every_twelve_months() {
        let now = Utc::now();
        let checkout = CheckoutSession {
            id: "checkout-annual".to_string(),
            org_id: "org-1".to_string(),
            account_email: "owner@example.com".to_string(),
            plan_id: BillingPlanId::Pro,
            status: CheckoutSessionStatus::Pending,
            provider: "mercadopago".to_string(),
            provider_subscription_id: None,
            billing_interval: BillingInterval::Annual,
            currency_id: "CLP".to_string(),
            amount_clp: 299_900,
            usd_reference_monthly: 29,
            trial_days: 30,
            created_at: now,
            updated_at: now,
        };

        let body = mercadopago_preapproval_body(
            &checkout,
            "payer@example.com",
            "card-token",
            "https://mira.example",
            "https://api.example",
        );

        assert_eq!(body["auto_recurring"]["frequency"], 12);
        assert_eq!(body["auto_recurring"]["frequency_type"], "months");
        assert_eq!(body["auto_recurring"]["transaction_amount"], 299_900);
    }

    #[tokio::test]
    async fn failed_quota_alert_is_released_for_retry() {
        let storage = MemoryStorage::default();
        let state = AppState::new(test_app_config(), Arc::new(storage));
        let org_id = "org-quota-test";
        let period_key = current_period_key();
        // Mira Free (sin suscripción) da 10 análisis/mes; 8 = 80%.
        state
            .storage
            .add_usage(org_id, &period_key, 8, 0, 0)
            .await
            .unwrap();

        check_quota_alerts(&state, org_id, &period_key, "owner@example.com").await;

        assert!(
            state
                .storage
                .record_quota_alert_if_new(org_id, &period_key, "runs", 80)
                .await
                .unwrap()
        );
        assert!(
            state
                .storage
                .record_quota_alert_if_new(org_id, &period_key, "runs", 100)
                .await
                .unwrap(),
            "al 80% de uso no debió dispararse (ni quedar registrado) el aviso de 100%"
        );

        state
            .storage
            .release_quota_alert(org_id, &period_key, "runs", 80)
            .await
            .unwrap();
        check_quota_alerts(&state, org_id, &period_key, "owner@example.com").await;
        assert!(
            state
                .storage
                .record_quota_alert_if_new(org_id, &period_key, "runs", 80)
                .await
                .unwrap()
        );
    }

    fn billing_invoice(date: &str, status: &str, id: u64) -> MercadoPagoInvoice {
        serde_json::from_value(json!({
            "preapproval_id": "provider-1", "debit_date": date,
            "payment": { "id": id, "status": status }
        }))
        .unwrap()
    }

    #[test]
    fn billing_reconciliation_handles_cancellation_failure_renewal_and_replay() {
        let now = "2026-09-10T00:00:00Z".parse().unwrap();
        let mut sub = subscription("org-1", "owner@example.com");
        sub.trial_ends_at = None;
        let mut provider: MercadoPagoPreapprovalResponse = serde_json::from_value(json!({
            "id": "provider-1", "status": "authorized", "date_created": "2026-09-01T00:00:00Z"
        }))
        .unwrap();
        let mut invoices = vec![billing_invoice("2026-09-01T00:00:00Z", "approved", 100)];
        apply_mercadopago_reconciliation(&mut sub, &provider, &invoices, now);
        let paid_end = Some("2026-10-01T00:00:00Z".parse().unwrap());
        assert_eq!(sub.current_period_end, paid_end);
        assert!(subscription_allows_access(Some(&sub), now));
        provider.status = Some("cancelled".to_string());
        apply_mercadopago_reconciliation(&mut sub, &provider, &invoices, now);
        assert_eq!(sub.status, SubscriptionStatus::Cancelled);
        assert!(subscription_allows_access(Some(&sub), now));
        assert_eq!(sub.current_period_end, paid_end);
        provider.status = Some("authorized".to_string());
        apply_mercadopago_reconciliation(&mut sub, &provider, &invoices, now);
        assert_eq!(
            sub.status,
            SubscriptionStatus::Cancelled,
            "una autorización tardía no deshace la cancelación local"
        );
        sub.cancel_at_period_end = false;
        let renewal_date = "2026-10-02T00:00:00Z".parse().unwrap();
        invoices.push(billing_invoice("2026-10-01T00:00:00Z", "rejected", 101));
        apply_mercadopago_reconciliation(&mut sub, &provider, &invoices, renewal_date);
        assert_eq!(sub.status, SubscriptionStatus::PastDue);
        assert_eq!(sub.current_period_end, paid_end);
        assert!(!subscription_allows_access(Some(&sub), renewal_date));
        invoices[1].payment.as_mut().unwrap().status = "approved".to_string();
        invoices.reverse();
        apply_mercadopago_reconciliation(&mut sub, &provider, &invoices, renewal_date);
        let renewed_end = Some("2026-11-01T00:00:00Z".parse().unwrap());
        assert_eq!(sub.current_period_end, renewed_end);
        assert_eq!(sub.status, SubscriptionStatus::Active);
        apply_mercadopago_reconciliation(
            &mut sub,
            &provider,
            &invoices,
            renewal_date + chrono::Duration::days(1),
        );
        assert_eq!(
            sub.current_period_end, renewed_end,
            "replay no agrega otro período"
        );
        invoices[0].payment.as_mut().unwrap().status = "refunded".to_string();
        apply_mercadopago_reconciliation(&mut sub, &provider, &invoices, renewal_date);
        assert_eq!(sub.current_period_end, paid_end);
        assert!(!subscription_allows_access(Some(&sub), renewal_date));
    }

    #[test]
    fn billing_reconciliation_does_not_turn_authorization_into_payment_or_restart_trial() {
        let now = "2026-10-01T00:00:00Z".parse().unwrap();
        let started = "2026-08-01T00:00:00Z".parse().unwrap();
        let mut sub = active_subscription_for_trial(
            "org-1".to_string(),
            BillingPlanId::Pro,
            Some("provider-1".to_string()),
            BillingInterval::Annual,
            started,
        );
        let trial_end = sub.trial_ends_at;
        let provider: MercadoPagoPreapprovalResponse =
            serde_json::from_value(json!({"id":"provider-1", "status":"authorized"})).unwrap();
        apply_mercadopago_reconciliation(&mut sub, &provider, &[], now);
        assert!(sub.current_period_end.is_none());
        assert_eq!(sub.trial_ends_at, trial_end);
        assert!(!subscription_allows_access(Some(&sub), now));
        let invoices = vec![billing_invoice("2026-09-01T00:00:00Z", "approved", 100)];
        apply_mercadopago_reconciliation(&mut sub, &provider, &invoices, now);
        assert_eq!(
            sub.current_period_end,
            Some("2027-09-01T00:00:00Z".parse().unwrap())
        );
    }

    #[test]
    fn billing_webhook_binds_signed_resource_and_accepts_numeric_notification_id() {
        let payload: MercadoPagoWebhook = serde_json::from_value(json!({
            "id":12345, "type":"payment", "data":{"id":"999"}
        }))
        .unwrap();
        let mut query = HashMap::from([("data.id".to_string(), "999".to_string())]);
        assert_eq!(
            mercadopago_webhook_resource_id(&query, &payload).unwrap(),
            "999"
        );
        query.insert("data.id".to_string(), "998".to_string());
        assert!(mercadopago_webhook_resource_id(&query, &payload).is_err());
        query.clear();
        assert!(mercadopago_webhook_resource_id(&query, &payload).is_err());
        let numeric: MercadoPagoWebhook =
            serde_json::from_value(json!({"data":{"id":999}})).unwrap();
        query.insert("data.id".to_string(), "999".to_string());
        assert_eq!(
            mercadopago_webhook_resource_id(&query, &numeric).unwrap(),
            "999"
        );
    }

    #[tokio::test]
    async fn billing_interrupted_checkout_reuses_the_session_with_a_new_card_token() {
        let mut config = test_app_config();
        config.billing.mercadopago_access_token = None;
        let fixture = seeded_app_with_config(config.clone()).await;
        let state = AppState::new(config, Arc::new(fixture.storage.clone()));
        let bundle = get_or_provision_org_config(&state, "alice@example.com")
            .await
            .unwrap();
        let mut expired = subscription(&bundle.org.id, "alice@example.com");
        expired.status = SubscriptionStatus::Cancelled;
        expired.cancel_at_period_end = true;
        expired.current_period_end = Some(Utc::now() - chrono::Duration::seconds(1));
        fixture.storage.upsert_subscription(&expired).await.unwrap();
        let checkout = CheckoutSession {
            id: checkout_idempotency_key(
                &bundle.org.id,
                "original-card",
                &BillingPlanId::Pro,
                &BillingInterval::Annual,
            ),
            org_id: bundle.org.id.clone(),
            account_email: "alice@example.com".to_string(),
            plan_id: BillingPlanId::Pro,
            status: CheckoutSessionStatus::ProviderCreated,
            provider: "mercadopago".to_string(),
            provider_subscription_id: Some("pending-provider".to_string()),
            billing_interval: BillingInterval::Annual,
            currency_id: "CLP".to_string(),
            amount_clp: 299_900,
            usd_reference_monthly: 29,
            trial_days: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        fixture
            .storage
            .upsert_checkout_session(&checkout)
            .await
            .unwrap();
        let response = fixture
            .app
            .clone()
            .oneshot(request(
                Method::POST,
                "/checkout/subscriptions",
                Some(&fixture.alice_cookie),
                Some(json!({"plan_id":"inicial","card_token_id":"new-card"})),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let response = fixture
            .app
            .clone()
            .oneshot(request(
                Method::POST,
                "/checkout/subscriptions",
                Some(&fixture.alice_cookie),
                Some(
                    json!({"plan_id":"pro","billing_interval":"annual","card_token_id":"new-card"}),
                ),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["error"]["message"], "Mercado Pago no está configurado");
        let different_key = checkout_idempotency_key(
            &bundle.org.id,
            "new-card",
            &BillingPlanId::Pro,
            &BillingInterval::Annual,
        );
        assert!(
            fixture
                .storage
                .get_checkout_session(&different_key)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            fixture
                .storage
                .find_incomplete_checkout_for_org(&bundle.org.id)
                .await
                .unwrap()
                .unwrap()
                .id,
            checkout.id
        );
    }

    #[tokio::test]
    async fn billing_mutations_require_an_org_owner_or_admin() {
        let fixture = seeded_app().await;
        let state = AppState::new(test_app_config(), Arc::new(fixture.storage.clone()));
        let mut bundle = get_or_provision_org_config(&state, "alice@example.com")
            .await
            .unwrap();
        for role in [OrgRole::Analyst, OrgRole::Viewer] {
            bundle.membership.role = role;
            fixture.storage.upsert_org_config(&bundle).await.unwrap();
            for (path, body) in [
                (
                    "/checkout/subscriptions",
                    Some(json!({"plan_id":"pro", "card_token_id":"card"})),
                ),
                ("/me/subscription/cancel", None),
                (
                    "/me/subscription/change-plan",
                    Some(json!({"plan_id":"pro"})),
                ),
                ("/me/subscription/reconcile", None),
            ] {
                let response = fixture
                    .app
                    .clone()
                    .oneshot(request(
                        Method::POST,
                        path,
                        Some(&fixture.alice_cookie),
                        body,
                    ))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
            }
        }
    }

    #[tokio::test]
    async fn billing_cancel_replay_preserves_the_purchased_period() {
        let fixture = seeded_app().await;
        let mut paid = subscription("alice-org", "alice@example.com");
        paid.provider_subscription_id = None;
        let paid_end = paid.current_period_end;
        fixture.storage.upsert_subscription(&paid).await.unwrap();
        for _ in 0..2 {
            let response = fixture
                .app
                .clone()
                .oneshot(request(
                    Method::POST,
                    "/me/subscription/cancel",
                    Some(&fixture.alice_cookie),
                    None,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let snapshot: EntitlementSnapshot = response_json(response).await;
            assert!(snapshot.allowed);
            let current = fixture
                .storage
                .get_subscription_for_org("alice-org")
                .await
                .unwrap()
                .unwrap();
            assert!(current.cancel_at_period_end);
            assert_eq!(current.current_period_end, paid_end);
            assert_eq!(current.status, SubscriptionStatus::Cancelled);
        }
    }

    #[tokio::test]
    async fn billing_checkout_replay_returns_existing_activation_without_a_second_charge() {
        let fixture = seeded_app().await;
        let state = AppState::new(test_app_config(), Arc::new(fixture.storage.clone()));
        let bundle = get_or_provision_org_config(&state, "alice@example.com")
            .await
            .unwrap();
        let checkout = CheckoutSession {
            id: checkout_idempotency_key(
                &bundle.org.id,
                "same-card",
                &BillingPlanId::Pro,
                &BillingInterval::Annual,
            ),
            org_id: bundle.org.id,
            account_email: "alice@example.com".to_string(),
            plan_id: BillingPlanId::Pro,
            status: CheckoutSessionStatus::Activated,
            provider: "mercadopago".to_string(),
            provider_subscription_id: Some("provider-1".to_string()),
            billing_interval: BillingInterval::Annual,
            currency_id: "CLP".to_string(),
            amount_clp: 299_900,
            usd_reference_monthly: 29,
            trial_days: 30,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        fixture
            .storage
            .upsert_checkout_session(&checkout)
            .await
            .unwrap();
        for _ in 0..2 {
            let response = fixture
                .app
                .clone()
                .oneshot(request(
                    Method::POST,
                    "/checkout/subscriptions",
                    Some(&fixture.alice_cookie),
                    Some(json!({
                        "plan_id":"pro", "billing_interval":"annual", "card_token_id":"same-card"
                    })),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let response: serde_json::Value = response_json(response).await;
            assert_eq!(response["session"]["id"], checkout.id);
        }
    }

    #[test]
    fn mercadopago_errors_preserve_provider_boundary() {
        let bad_request = mercadopago_api_error(StatusCode::BAD_REQUEST);
        assert_eq!(bad_request.status, StatusCode::BAD_REQUEST);
        assert!(bad_request.message.contains("revisa el correo"));

        let provider_failure = mercadopago_api_error(StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(provider_failure.status, StatusCode::BAD_GATEWAY);
        assert!(!provider_failure.message.contains("Internal server error"));
    }

    #[test]
    fn mercadopago_signature_uses_hmac_manifest() {
        let secret = "mp-webhook-secret";
        let data_id = "PREAPPROVAL-123";
        let request_id = "2066ca19-c6f1-498a-be75-1923005edd06";
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        for ts in [now.as_millis().to_string(), now.as_secs().to_string()] {
            let manifest = format!("id:preapproval-123;request-id:{request_id};ts:{ts};");
            type HmacSha256 = Hmac<Sha256>;
            let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
            mac.update(manifest.as_bytes());
            let signature = format!("ts={ts},v1={}", hex_lower(&mac.finalize().into_bytes()));
            assert!(
                validate_mercadopago_signature(&signature, request_id, Some(data_id), secret)
                    .is_ok()
            );
            assert!(
                validate_mercadopago_signature(&signature, request_id, Some("other"), secret)
                    .is_err()
            );
        }
        assert!(validate_mercadopago_signature(secret, request_id, Some(data_id), secret).is_err());
    }

    #[test]
    fn webhook_signature_rejects_stale_timestamp() {
        let old_ts = "1742505638683";
        let now = UNIX_EPOCH + Duration::from_millis(1742505638683 + 301_000);

        assert!(validate_webhook_timestamp(old_ts, now).is_err());
    }

    #[test]
    fn mercadopago_webhook_without_secret_is_rejected_in_production_only() {
        let mut config = test_app_config();
        config.billing.mercadopago_webhook_secret = None;
        config.app_env = "production".to_string();
        let state = AppState::new(config.clone(), Arc::new(MemoryStorage::default()));
        assert!(verify_mercadopago_webhook(&state, &HeaderMap::new(), None).is_err());

        config.app_env = "development".to_string();
        let state = AppState::new(config, Arc::new(MemoryStorage::default()));
        assert!(verify_mercadopago_webhook(&state, &HeaderMap::new(), None).is_ok());
    }

    #[tokio::test]
    async fn workos_session_revoked_only_invalidates_the_matching_local_session() {
        let fixture = seeded_app().await;
        let mut alice = fixture
            .storage
            .get_user_session("alice-session")
            .await
            .unwrap()
            .unwrap();
        alice.workos_session_id = Some("session-alice".to_string());
        fixture.storage.upsert_user_session(&alice).await.unwrap();
        let body = r#"{"event":"session.revoked","data":{"id":"session-alice"}}"#;
        let signature = signed_workos_webhook(body, "test-workos-webhook-secret");

        let response = fixture
            .app
            .clone()
            .oneshot(workos_webhook_request(body, &signature))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            fixture
                .storage
                .get_user_session("alice-session")
                .await
                .unwrap()
                .unwrap()
                .revoked_at
                .is_some()
        );
        assert!(
            fixture
                .storage
                .get_user_session("bob-session")
                .await
                .unwrap()
                .unwrap()
                .revoked_at
                .is_none()
        );
    }

    #[tokio::test]
    async fn invalid_workos_signature_cannot_revoke_a_local_session() {
        let fixture = seeded_app().await;
        let mut alice = fixture
            .storage
            .get_user_session("alice-session")
            .await
            .unwrap()
            .unwrap();
        alice.workos_session_id = Some("session-alice".to_string());
        fixture.storage.upsert_user_session(&alice).await.unwrap();
        let body = r#"{"event":"session.revoked","data":{"id":"session-alice"}}"#;

        let response = fixture
            .app
            .oneshot(workos_webhook_request(body, "t=1,v1=deadbeef"))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(
            fixture
                .storage
                .get_user_session("alice-session")
                .await
                .unwrap()
                .unwrap()
                .revoked_at
                .is_none()
        );
    }

    #[tokio::test]
    async fn workos_user_deleted_revokes_access_gmail_and_scheduler() {
        let fixture = seeded_app().await;
        let connection = fixture
            .storage
            .get_gmail_connection("alice@example.com")
            .await
            .unwrap()
            .unwrap();
        assert!(mailbox_connection_is_active(Some(&connection)));
        let mut bundle = crate::policies::provision_default_config("alice@example.com", Utc::now());
        bundle.draft.schedule_report_policy.scheduler_enabled = true;
        fixture.storage.upsert_org_config(&bundle).await.unwrap();
        let body = r#"{"event":"user.deleted","data":{"id":"workos-alice-session"}}"#;
        let signature = signed_workos_webhook(body, "test-workos-webhook-secret");

        let response = fixture
            .app
            .oneshot(workos_webhook_request(body, &signature))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            fixture
                .storage
                .get_user_session("alice-session")
                .await
                .unwrap()
                .unwrap()
                .revoked_at
                .is_some()
        );
        let connection = fixture
            .storage
            .get_gmail_connection("alice@example.com")
            .await
            .unwrap()
            .unwrap();
        assert!(!mailbox_connection_is_active(Some(&connection)));
        let bundle = fixture
            .storage
            .get_org_config_for_user("alice@example.com")
            .await
            .unwrap()
            .unwrap();
        assert!(bundle.mailbox.revoked_at.is_some());
        assert!(!bundle.draft.schedule_report_policy.scheduler_enabled);
        assert!(
            fixture
                .storage
                .list_schedule_configs()
                .await
                .unwrap()
                .iter()
                .any(|config| config.user_email == "alice@example.com" && !config.enabled)
        );
    }

    #[tokio::test]
    async fn stale_policy_sync_cannot_reenable_scheduler_after_gmail_revocation() {
        let fixture = seeded_app().await;
        let mut stale_bundle =
            crate::policies::provision_default_config("alice@example.com", Utc::now());
        stale_bundle.draft.schedule_report_policy.scheduler_enabled = true;

        fixture
            .storage
            .disconnect_gmail("alice@example.com", Utc::now())
            .await
            .unwrap();
        let state = AppState::new(test_app_config(), Arc::new(fixture.storage.clone()));
        sync_schedule_config_from_policy(&state, &stale_bundle)
            .await
            .unwrap();

        assert!(
            fixture
                .storage
                .list_schedule_configs()
                .await
                .unwrap()
                .into_iter()
                .find(|config| config.user_email == "alice@example.com")
                .is_some_and(|config| !config.enabled)
        );
    }

    #[test]
    fn checkout_blocks_repurchase_while_cancelled_access_remains() {
        let now = Utc::now();
        let mut active = subscription("org-1", "owner@example.com");
        active.status = SubscriptionStatus::Active;
        active.cancel_at_period_end = false;

        let mut scheduled_cancel = active.clone();
        scheduled_cancel.cancel_at_period_end = true;

        assert!(checkout_blocked_by_active_subscription(Some(&active), now));
        assert!(checkout_blocked_by_active_subscription(
            Some(&scheduled_cancel),
            now
        ));
        assert!(!checkout_blocked_by_active_subscription(None, now));
        let mut failed_renewal = active.clone();
        failed_renewal.status = SubscriptionStatus::PastDue;
        failed_renewal.current_period_end = Some(now - chrono::Duration::seconds(1));
        assert!(checkout_blocked_by_active_subscription(
            Some(&failed_renewal),
            now
        ));
        failed_renewal.cancel_at_period_end = true;
        failed_renewal.status = SubscriptionStatus::Cancelled;
        assert!(!checkout_blocked_by_active_subscription(
            Some(&failed_renewal),
            now
        ));
    }

    #[tokio::test]
    async fn free_tier_allows_analysis_creation_without_subscription() {
        // Capa gratuita: sin suscripción, una cuenta nueva puede crear análisis de
        // inmediato (antes esto devolvía 402). El siguiente gate ya no es pago.
        let config = test_app_config();
        let storage = MemoryStorage::default();
        storage
            .upsert_user_session(&session("trialless-session", "trialless@example.com"))
            .await
            .unwrap();
        let cookie = signed_cookie("trialless-session", &config.session_secret);
        let app = crate::build_app(config, Arc::new(storage));
        complete_analysis_setup(&app, &cookie).await;

        let response = app
            .oneshot(request(
                Method::POST,
                "/analysis-runs",
                Some(&cookie),
                Some(json!({
                    "date_from": "2026-06-01",
                    "date_to": "2026-06-02"
                })),
            ))
            .await
            .unwrap();
        assert_ne!(
            response.status(),
            StatusCode::PAYMENT_REQUIRED,
            "el plan gratis no debe exigir pago para crear un análisis"
        );
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn free_tier_allows_history_reads_without_subscription() {
        let config = test_app_config();
        let storage = MemoryStorage::default();
        storage
            .upsert_user_session(&session("free-reader-session", "free-reader@example.com"))
            .await
            .unwrap();
        let cookie = signed_cookie("free-reader-session", &config.session_secret);
        let app = crate::build_app(config, Arc::new(storage));

        for uri in ["/analysis-runs", "/threads/whatever"] {
            let response = app
                .clone()
                .oneshot(request(Method::GET, uri, Some(&cookie), None))
                .await
                .unwrap();
            assert_ne!(
                response.status(),
                StatusCode::PAYMENT_REQUIRED,
                "GET {uri} ya no debe exigir suscripción en el plan gratis"
            );
        }
    }

    #[tokio::test]
    async fn free_tier_allows_gmail_connect_without_subscription() {
        // Sin suscripción, conectar Gmail redirige a Google (307) en vez de bloquear.
        let config = test_app_config();
        let storage = MemoryStorage::default();
        storage
            .upsert_user_session(&session("no-plan-session", "noplan@example.com"))
            .await
            .unwrap();
        let cookie = signed_cookie("no-plan-session", &config.session_secret);
        let app = crate::build_app(config, Arc::new(storage));

        let response = app
            .oneshot(request(
                Method::GET,
                "/gmail/connect/login",
                Some(&cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    }

    #[tokio::test]
    async fn list_analysis_runs_legacy_returns_array_without_pagination_query() {
        let test = seeded_app().await;
        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/analysis-runs",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert!(body.as_array().is_some());
    }

    #[tokio::test]
    async fn paginated_analysis_runs_and_threads_return_wrapper() {
        let test = seeded_app().await;
        complete_analysis_setup(&test.app, &test.alice_cookie).await;
        for day in ["2026-06-03", "2026-06-04", "2026-06-05"] {
            let response = test
                .app
                .clone()
                .oneshot(request(
                    Method::POST,
                    "/analysis-runs",
                    Some(&test.alice_cookie),
                    Some(json!({
                        "date_from": day,
                        "date_to": day
                    })),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::GET,
                "/analysis-runs?limit=2",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["items"].as_array().unwrap().len(), 2);
        assert_eq!(body["total_count"], 4);
        let token = body["next_page_token"].as_str().unwrap();

        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::GET,
                &format!("/analysis-runs?limit=2&page_token={token}"),
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["items"].as_array().unwrap().len(), 2);
        assert!(body["next_page_token"].is_null());

        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/analysis-runs/run-alice/threads?limit=1",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["total_count"], 1);
        assert!(body["next_page_token"].is_null());
    }

    #[tokio::test]
    async fn data_summary_is_read_only_and_counts_owned_data() {
        let test = seeded_app().await;
        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/me/data-summary",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["account"]["google_account_email"], "alice@example.com");
        assert_eq!(body["account"]["mailbox_connected"], true);
        assert_eq!(body["privacy"]["ai_enabled"], false);
        assert_eq!(body["privacy"]["retention_days"], 30);
        assert_eq!(body["stored_data"]["analysis_runs_count"], 1);
        assert_eq!(body["stored_data"]["threads_count"], 1);
        assert_eq!(body["stored_data"]["messages_count"], 1);
        assert_eq!(body["actions"]["delete_analysis_data"]["available"], true);
        assert_eq!(body["actions"]["disconnect_gmail"]["available"], true);
        assert_eq!(
            body["actions"]["disconnect_gmail"]["reason"],
            "requires_confirmation"
        );
    }

    #[tokio::test]
    async fn delete_analysis_data_requires_confirmation_and_only_deletes_the_owner_data() {
        let fixture = seeded_app().await;
        let rejected = fixture
            .app
            .clone()
            .oneshot(request(
                Method::DELETE,
                "/me/analysis-data",
                Some(&fixture.alice_cookie),
                Some(json!({ "confirmation": "BORRAR" })),
            ))
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
        assert!(
            fixture
                .storage
                .analysis_data_deletion_audits()
                .await
                .is_empty()
        );

        let response = fixture
            .app
            .oneshot(request(
                Method::DELETE,
                "/me/analysis-data",
                Some(&fixture.alice_cookie),
                Some(json!({ "confirmation": "BORRAR MIS ANALISIS" })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .expect("deletion response includes a request id");
        let audits = fixture.storage.analysis_data_deletion_audits().await;
        assert_eq!(audits.len(), 1);
        assert_eq!(audits[0].id, request_id);
        assert_eq!(audits[0].owner_hash, hash_owner_email("alice@example.com"));
        assert_eq!(audits[0].status, AnalysisDataDeletionStatus::Completed);
        assert!(audits[0].completed_at.is_some());
        assert!(
            fixture
                .storage
                .list_analysis_runs("alice@example.com")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            fixture
                .storage
                .list_messages("run-alice", "thread-alice")
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            fixture
                .storage
                .list_analysis_runs("bob@example.com")
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn operations_history_lists_recent_runs_without_sensitive_details() {
        let test = seeded_app().await;
        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/me/operations/history?limit=5",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["entries"].as_array().unwrap().len(), 1);
        assert_eq!(body["entries"][0]["kind"], "analysis_run");
        assert_eq!(body["entries"][0]["run_id"], "run-alice");
        assert!(body["entries"][0].get("error_redacted").is_some());
        assert!(body["entries"][0].get("processed_threads").is_some());
    }

    #[tokio::test]
    async fn operations_status_is_read_only_and_redacts_errors() {
        let test = seeded_app().await;
        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::PUT,
                "/me/org/config",
                Some(&test.alice_cookie),
                Some(json!({
                    "analysis_policy": {
                        "valid_request_criteria": ["Clientes externos solicitan soporte"]
                    },
                    "schedule_report_policy": {
                        "scheduler_enabled": true,
                        "analysis_time": "14:00",
                        "report_recipients": ["ops@example.com"]
                    }
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/me/operations/status",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["scheduler"]["enabled"], true);
        assert_eq!(body["scheduler"]["recipients_count"], 1);
        assert_eq!(body["scheduler"]["preset"], "weekdays_custom_hour_local");
        assert_eq!(body["scheduler"]["analysis_time"], "14:00");
        assert_eq!(body["policy"]["setup_ready"], true);
    }

    #[tokio::test]
    async fn schedule_analysis_time_rejects_invalid_hours() {
        let test = seeded_app().await;
        for analysis_time in ["24:30", "11:60"] {
            let response = test
                .app
                .clone()
                .oneshot(request(
                    Method::PUT,
                    "/me/org/config",
                    Some(&test.alice_cookie),
                    Some(json!({
                        "schedule_report_policy": {
                            "analysis_time": analysis_time
                        }
                    })),
                ))
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let body: serde_json::Value = response_json(response).await;
            assert_eq!(body["error"]["code"], "BAD_REQUEST");
        }
    }

    #[tokio::test]
    async fn create_analysis_run_is_rate_limited_per_user() {
        let mut config = test_app_config();
        config.rate_limit.analysis_create_per_hour = 1;
        let test = seeded_app_with_config(config).await;
        complete_analysis_setup(&test.app, &test.alice_cookie).await;
        let payload = json!({
            "date_from": "2026-06-01",
            "date_to": "2026-06-02"
        });

        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::POST,
                "/analysis-runs",
                Some(&test.alice_cookie),
                Some(payload.clone()),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = test
            .app
            .oneshot(request(
                Method::POST,
                "/analysis-runs",
                Some(&test.alice_cookie),
                Some(payload),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn privileged_account_bypasses_rate_limit() {
        let mut config = test_app_config();
        config.rate_limit.analysis_create_per_hour = 1;
        config.internal_full_access_emails = vec!["alice@example.com".to_string()];
        let test = seeded_app_with_config(config).await;
        let payload = json!({
            "date_from": "2026-06-01",
            "date_to": "2026-06-02"
        });
        // Tres veces sobre un límite de 1/hora: la cuenta privilegiada nunca 429.
        for _ in 0..3 {
            let response = test
                .app
                .clone()
                .oneshot(request(
                    Method::POST,
                    "/analysis-runs",
                    Some(&test.alice_cookie),
                    Some(payload.clone()),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn privileged_account_enables_ai_without_consent() {
        let mut config = test_app_config();
        config.internal_full_access_emails = vec!["alice@example.com".to_string()];
        let test = seeded_app_with_config(config).await;
        // Las cuentas internas conservan su excepción explícita de consentimiento.
        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::PUT,
                "/me/org/config",
                Some(&test.alice_cookie),
                Some(json!({ "ai_policy": { "enabled": false } })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Re-activar sin consent_confirmed: para la cuenta privilegiada igual queda OK.
        let response = test
            .app
            .oneshot(request(
                Method::PUT,
                "/me/org/config",
                Some(&test.alice_cookie),
                Some(json!({ "ai_policy": { "enabled": true } })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn privileged_account_bypasses_setup_gating() {
        let mut config = test_app_config();
        config.internal_full_access_emails = vec!["alice@example.com".to_string()];
        let test = seeded_app_with_config(config).await;
        // Payload de policy (sin campos legacy) con setup incompleto
        // (valid_request_criteria vacío): no-privilegiado daría 400, privilegiado OK.
        let response = test
            .app
            .oneshot(request(
                Method::POST,
                "/analysis-runs",
                Some(&test.alice_cookie),
                Some(json!({
                    "date_from": "2026-06-01",
                    "date_to": "2026-06-02"
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ai_opt_out_is_free_and_reenabling_requires_consent_and_bumps_version() {
        let test = seeded_app().await;
        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::PUT,
                "/me/org/config",
                Some(&test.alice_cookie),
                Some(json!({"ai_policy":{"enabled":true,"consent_confirmed":true}})),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Desactivar una IA previamente autorizada no exige otra confirmación.
        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::PUT,
                "/me/org/config",
                Some(&test.alice_cookie),
                Some(json!({ "ai_policy": { "enabled": false } })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Re-activar tras un opt-out sí re-confirma consentimiento: sin él, 400.
        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::PUT,
                "/me/org/config",
                Some(&test.alice_cookie),
                Some(json!({ "ai_policy": { "enabled": true } })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Con consentimiento explícito + setup completo: OK, sube versión y queda lista.
        let response = test
            .app
            .oneshot(request(
                Method::PUT,
                "/me/org/config",
                Some(&test.alice_cookie),
                Some(json!({
                    "analysis_policy": {
                        "valid_request_criteria": ["Clientes externos solicitan soporte"]
                    },
                    "ai_policy": {
                        "enabled": true,
                        "consent_confirmed": true
                    },
                    "retention_policy": { "retention_days": 60 }
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        // v1 inicial → v2 (consentimiento) → v3 (opt-out) → v4 (reactivación).
        assert_eq!(body["policy_version"]["version"], 4);
        assert_eq!(body["setup_state"]["ready_for_analysis"], true);
    }

    #[tokio::test]
    async fn policy_run_uses_snapshot_and_rejects_legacy_overrides() {
        let test = seeded_app().await;
        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::PUT,
                "/me/org/config",
                Some(&test.alice_cookie),
                Some(json!({
                    "analysis_policy": {
                        "valid_request_criteria": ["Clientes externos solicitan soporte"]
                    }
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::POST,
                "/analysis-runs",
                Some(&test.alice_cookie),
                Some(json!({
                    "date_from": "2026-06-01",
                    "date_to": "2026-06-02"
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let run: AnalysisRun = response_json(response).await;
        assert!(run.org_id.is_some());
        assert!(run.policy_snapshot.is_some());
        assert_eq!(run.config.internal_domains, vec!["example.com"]);
        assert_eq!(
            run.retention_expires_at,
            run.created_at.checked_add_days(chrono::Days::new(30))
        );

        let response = test
            .app
            .oneshot(request(
                Method::POST,
                "/analysis-runs",
                Some(&test.alice_cookie),
                Some(json!({
                    "date_from": "2026-06-01",
                    "date_to": "2026-06-02",
                    "internal_domains": ["legacy.test"]
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    /// Sin credenciales de Azure AD, Microsoft no debe ofrecerse ni aceptar un
    /// connect: un botón que no puede funcionar es peor que ningún botón.
    #[tokio::test]
    async fn microsoft_connect_is_rejected_until_azure_ad_is_configured() {
        let test = seeded_app().await;
        let response = test
            .app
            .clone()
            .oneshot(request(Method::GET, "/mailbox/providers", None, None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["providers"], json!(["google", "imap"]));

        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/mailbox/connect/microsoft/login",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn owned_run_endpoints_require_session() {
        let test = seeded_app().await;
        let response = test
            .app
            .oneshot(request(Method::GET, "/analysis-runs/run-alice", None, None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn foreign_run_endpoints_return_not_found() {
        let test = seeded_app().await;
        for uri in [
            "/analysis-runs/run-alice",
            "/analysis-runs/run-alice/status",
            "/analysis-runs/run-alice/metrics",
            "/analysis-runs/run-alice/threads",
            "/analysis-runs/run-alice/events",
        ] {
            let response = test
                .app
                .clone()
                .oneshot(request(Method::GET, uri, Some(&test.bob_cookie), None))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        }
    }

    #[tokio::test]
    async fn foreign_thread_read_and_manual_review_return_not_found() {
        let test = seeded_app().await;
        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::GET,
                "/threads/thread-alice",
                Some(&test.bob_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = test
            .app
            .oneshot(request(
                Method::PATCH,
                "/threads/thread-alice/manual-review",
                Some(&test.bob_cookie),
                Some(json!({
                    "reviewer_label": "spoofed-admin@example.com",
                    "new_classification": "misc",
                    "is_valid_client_request": false,
                    "is_answered": false,
                    "first_client_message_id": null,
                    "first_internal_reply_message_id": null,
                    "last_internal_message_id": null,
                    "notes": "should not be applied"
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn ambiguous_manual_reviews_increment_valid_and_decrement_ambiguous() {
        let test = seeded_app().await;
        let mut original = test
            .storage
            .get_thread("run-alice", "thread-alice")
            .await
            .unwrap()
            .expect("seeded thread");
        original.manual_review_required = false;
        test.storage
            .upsert_thread(&original, &[message("msg-a", "cliente@customer.test")])
            .await
            .unwrap();

        let mut template = original;
        template.classification = Classification::Ambiguous;
        template.is_valid_client_request = false;
        template.manual_review_required = true;

        for index in 0..4 {
            let mut thread = template.clone();
            thread.id = format!("ambiguous-{index}");
            thread.thread_id = format!("gmail-ambiguous-{index}");
            thread.first_client_message_id = Some(format!("message-{index}"));
            test.storage
                .upsert_thread(
                    &thread,
                    &[message(
                        &format!("message-{index}"),
                        "cliente@customer.test",
                    )],
                )
                .await
                .unwrap();
        }

        let baseline_threads = test.storage.list_threads("run-alice").await.unwrap();
        let baseline = calculate_metrics(&baseline_threads, 17, 9);
        assert_eq!(baseline.total_threads, 5);
        assert_eq!(baseline.valid_requests, 1);
        assert_eq!(baseline.ambiguous, 4);
        assert_eq!(baseline.pending_review, 4);

        let mut last_run = None;
        for index in 0..4 {
            let response = test
                .app
                .clone()
                .oneshot(request(
                    Method::PATCH,
                    &format!("/threads/ambiguous-{index}/manual-review"),
                    Some(&test.alice_cookie),
                    Some(json!({
                        "new_classification": "valid_client_request",
                        // Compatibilidad con clientes antiguos: este valor
                        // contradictorio debe ignorarse.
                        "is_valid_client_request": false,
                        "is_answered": false,
                        "first_client_message_id": format!("message-{index}"),
                        "first_internal_reply_message_id": null,
                        "last_internal_message_id": null,
                        "notes": format!("ticket válido {index}")
                    })),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let run: serde_json::Value = response_json(response).await;
            assert_eq!(run["metrics"]["valid_requests"], json!(index + 2));
            assert_eq!(run["metrics"]["ambiguous"], json!(3 - index));
            assert_eq!(run["metrics"]["pending_review"], json!(3 - index));
            last_run = Some(run);
        }

        let run = last_run.expect("last updated run");
        assert_eq!(run["metrics"]["total_threads"], json!(5));
        assert_eq!(run["metrics"]["valid_requests"], json!(5));
        assert_eq!(run["metrics"]["ambiguous"], json!(0));
        assert_eq!(run["metrics"]["pending_review"], json!(0));
        assert_eq!(run["metrics"]["ai_input_tokens"], json!(0));
        assert_eq!(run["metrics"]["ai_output_tokens"], json!(0));
    }

    #[tokio::test]
    async fn duplicate_gmail_thread_ids_are_scoped_by_run() {
        let test = seeded_app().await;
        let second_run = run("run-alice-new", "alice@example.com");
        test.storage.create_analysis_run(&second_run).await.unwrap();
        let mut duplicate = thread("thread-alice", "run-alice-new");
        duplicate.classification = Classification::Ambiguous;
        duplicate.is_valid_client_request = false;
        duplicate.manual_review_required = true;
        test.storage
            .upsert_thread(&duplicate, &[message("msg-new", "cliente@customer.test")])
            .await
            .unwrap();

        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::GET,
                "/threads/thread-alice",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::PATCH,
                "/analysis-runs/run-alice-new/threads/thread-alice/manual-review",
                Some(&test.alice_cookie),
                Some(json!({
                    "new_classification": "misc",
                    "is_answered": false,
                    "first_client_message_id": "msg-new",
                    "first_internal_reply_message_id": null,
                    "last_internal_message_id": null,
                    "notes": "revisión del run nuevo"
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let original = test
            .storage
            .get_thread("run-alice", "thread-alice")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(original.classification, Classification::ValidClientRequest);
        assert!(!original.manual_override_applied);

        let reviewed = test
            .storage
            .get_thread("run-alice-new", "thread-alice")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reviewed.classification, Classification::Misc);
        assert!(!reviewed.manual_review_required);
        assert!(reviewed.manual_override_applied);

        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/analysis-runs/run-alice-new/threads/thread-alice",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let detail: serde_json::Value = response_json(response).await;
        assert_eq!(detail["thread"]["analysis_run_id"], json!("run-alice-new"));
        assert_eq!(detail["thread"]["classification"], json!("misc"));
    }

    #[tokio::test]
    async fn owner_can_read_threads_and_apply_manual_review() {
        let test = seeded_app().await;
        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::GET,
                "/threads/thread-alice",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = test
            .app
            .clone()
            .oneshot(request(
                Method::PATCH,
                "/threads/thread-alice/manual-review",
                Some(&test.alice_cookie),
                Some(json!({
                    "reviewer_label": "spoofed-admin@example.com",
                    "new_classification": "misc",
                    // El cliente manda true a propósito: el servidor debe ignorarlo y
                    // derivar la validez de la clasificación ("misc" => no es válida).
                    "is_valid_client_request": true,
                    "is_answered": false,
                    "first_client_message_id": null,
                    "first_internal_reply_message_id": null,
                    "last_internal_message_id": null,
                    "notes": "confirmed ignored"
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // El PATCH devuelve el run recalculado: el hilo, antes válido, ya no cuenta
        // como válido y pasa a Ignorados. Así la métrica de "Válidos" se descuenta
        // al reclasificar (en vez de quedarse igual como antes).
        let run: serde_json::Value = response_json(response).await;
        assert_eq!(run["metrics"]["valid_requests"], json!(0));
        assert_eq!(run["metrics"]["ignored"], json!(1));

        // Al releer el hilo, la nota debe haber persistido y la validez debe
        // haberse derivado de la clasificación (misc => is_valid = false), de modo
        // que un hilo ignorado deja de contar como válido en las métricas.
        let response = test
            .app
            .oneshot(request(
                Method::GET,
                "/threads/thread-alice",
                Some(&test.alice_cookie),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let detail: serde_json::Value = response_json(response).await;
        assert_eq!(detail["thread"]["notes"], json!("confirmed ignored"));
        assert_eq!(detail["thread"]["is_valid_client_request"], json!(false));
        assert_eq!(detail["thread"]["classification"], json!("misc"));
    }

    #[test]
    fn mailbox_oauth_state_rejects_session_org_provider_nonce_and_expiry_changes() {
        let alice = session("alice-session", "alice@example.com");
        let secret = "test-oauth-secret";
        let mut oauth =
            MailboxOAuthState::new(&alice, "alice-org", MailboxProviderKind::Google, None);
        let signed = oauth.signed(secret).unwrap();
        assert!(
            MailboxOAuthState::verify(
                &signed,
                secret,
                Some(&oauth.state),
                &alice,
                "alice-org",
                MailboxProviderKind::Google
            )
            .is_ok()
        );
        let bob = session("bob-session", "bob@example.com");
        let other_alice_session = session("alice-other-session", "alice@example.com");
        let reassigned_owner = session("alice-session", "bob@example.com");
        for changed in [&bob, &other_alice_session, &reassigned_owner] {
            assert!(
                MailboxOAuthState::verify(
                    &signed,
                    secret,
                    Some(&oauth.state),
                    changed,
                    "alice-org",
                    MailboxProviderKind::Google
                )
                .is_err()
            );
        }
        assert!(
            MailboxOAuthState::verify(
                &signed,
                secret,
                Some(&oauth.state),
                &alice,
                "bob-org",
                MailboxProviderKind::Google
            )
            .is_err()
        );
        assert!(
            MailboxOAuthState::verify(
                &signed,
                secret,
                Some(&oauth.state),
                &alice,
                "alice-org",
                MailboxProviderKind::Microsoft
            )
            .is_err()
        );
        assert!(
            MailboxOAuthState::verify(
                &signed,
                secret,
                Some("another-nonce"),
                &alice,
                "alice-org",
                MailboxProviderKind::Google
            )
            .is_err()
        );
        assert!(
            MailboxOAuthState::verify(
                &signed,
                secret,
                None,
                &alice,
                "alice-org",
                MailboxProviderKind::Google
            )
            .is_err()
        );
        assert!(
            MailboxOAuthState::verify(
                &signed,
                "wrong-secret",
                Some(&oauth.state),
                &alice,
                "alice-org",
                MailboxProviderKind::Google
            )
            .is_err()
        );
        oauth.expires_at = Utc::now().timestamp() - 1;
        assert!(
            MailboxOAuthState::verify(
                &oauth.signed(secret).unwrap(),
                secret,
                Some(&oauth.state),
                &alice,
                "alice-org",
                MailboxProviderKind::Google
            )
            .is_err()
        );
        let legacy = sign_session_id("nonce:old-verifier", secret).unwrap();
        assert!(
            MailboxOAuthState::verify(
                &legacy,
                secret,
                Some("nonce"),
                &alice,
                "alice-org",
                MailboxProviderKind::Google
            )
            .is_err()
        );
    }

    #[test]
    fn microsoft_mailbox_oauth_preserves_dotted_shared_target_and_conditional_scope() {
        let alice = session("alice-session", "alice@example.com");
        let oauth = MailboxOAuthState::new(
            &alice,
            "alice-org",
            MailboxProviderKind::Microsoft,
            Some("helpdesk@contoso.example.com"),
        );
        let signed = oauth.signed("secret").unwrap();
        assert_eq!(signed.matches('.').count(), 1);
        let verified = MailboxOAuthState::verify(
            &signed,
            "secret",
            Some(&oauth.state),
            &alice,
            "alice-org",
            MailboxProviderKind::Microsoft,
        )
        .unwrap();
        assert_eq!(
            verified.target_mailbox.as_deref(),
            Some("helpdesk@contoso.example.com")
        );
        assert!(
            microsoft_mailbox_scope(verified.target_mailbox.as_deref())
                .contains("Mail.Read.Shared")
        );
        assert!(!microsoft_mailbox_scope(None).contains("Mail.Read.Shared"));
    }

    #[test]
    fn microsoft_mailbox_oauth_requires_refresh_from_current_consent() {
        for refresh_token in [None, Some(String::new()), Some("  ".to_string())] {
            let token = GoogleTokenResponse {
                access_token: "new-delegate-access".to_string(),
                refresh_token,
            };
            assert!(required_microsoft_refresh_token(&token).is_err());
        }
        let token = GoogleTokenResponse {
            access_token: "new-delegate-access".to_string(),
            refresh_token: Some("new-delegate-refresh".to_string()),
        };
        assert_eq!(
            required_microsoft_refresh_token(&token).unwrap(),
            "new-delegate-refresh"
        );
    }

    #[tokio::test]
    async fn mailbox_oauth_callbacks_reject_another_session_before_token_exchange() {
        let mut config = test_app_config();
        config.microsoft = Some(MicrosoftConfig {
            client_id: "test-client".to_string(),
            client_secret: "test-secret".to_string(),
            redirect_url: "https://mira.example.com/mailbox/connect/microsoft/callback".to_string(),
            tenant: "common".to_string(),
        });
        let test = seeded_app_with_config(config.clone()).await;
        for (login, callback, provider) in [
            (
                "/gmail/connect/login",
                "/gmail/connect/callback",
                MailboxProviderKind::Google,
            ),
            (
                "/mailbox/connect/microsoft/login?target_mailbox=helpdesk%40contoso.example.com",
                "/mailbox/connect/microsoft/callback",
                MailboxProviderKind::Microsoft,
            ),
        ] {
            let response = test
                .app
                .clone()
                .oneshot(request(Method::GET, login, Some(&test.alice_cookie), None))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
            let location = url::Url::parse(
                response
                    .headers()
                    .get(header::LOCATION)
                    .unwrap()
                    .to_str()
                    .unwrap(),
            )
            .unwrap();
            let parameters = location.query_pairs().collect::<HashMap<_, _>>();
            let nonce = parameters.get("state").unwrap();
            let oauth_cookie = extract_named_cookie(
                response
                    .headers()
                    .get(header::SET_COOKIE)
                    .unwrap()
                    .to_str()
                    .unwrap(),
                "ghmi_oauth",
            )
            .unwrap();
            let encoded = verify_session_cookie(&oauth_cookie, &config.session_secret).unwrap();
            let payload: MailboxOAuthState =
                serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded).unwrap()).unwrap();
            assert_eq!(payload.session_id, "alice-session");
            assert_eq!(payload.owner_email, "alice@example.com");
            assert_eq!(payload.org_id, "alice-org");
            assert_eq!(payload.provider, provider);
            assert_eq!(
                parameters.get("code_challenge").unwrap().as_ref(),
                URL_SAFE_NO_PAD.encode(Sha256::digest(payload.code_verifier.as_bytes()))
            );
            if provider == MailboxProviderKind::Microsoft {
                assert_eq!(
                    payload.target_mailbox.as_deref(),
                    Some("helpdesk@contoso.example.com")
                );
                assert!(
                    parameters
                        .get("scope")
                        .unwrap()
                        .contains("Mail.Read.Shared")
                );
            }
            let swapped_cookie = format!("{}; ghmi_oauth={oauth_cookie}", test.bob_cookie);
            let response = test
                .app
                .clone()
                .oneshot(request(
                    Method::GET,
                    &format!("{callback}?code=not-exchanged&state={nonce}"),
                    Some(&swapped_cookie),
                    None,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let body: serde_json::Value = response_json(response).await;
            assert!(
                body["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("sesión o la organización cambió")
            );
            let connection = test
                .storage
                .get_gmail_connection("bob@example.com")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(connection.provider, MailboxProviderKind::Google);
            assert_eq!(connection.mailbox_email, "bob@example.com");
            assert_eq!(connection.access_token_encrypted, "access");
            assert!(connection.microsoft_target_email.is_none());
        }
    }
}
