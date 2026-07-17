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
        AiAuditResult, AnalysisConfig, AnalysisFunnel, AnalysisMetrics, AnalysisRun,
        AnalysisStatus, Classification, ClassificationSource, DroppedThreadInfo, EmailMessage,
        EmailThread, ManualReview, ManualReviewOverride, ThreadDisposition, TriggerType,
        ai_message_ids_known, apply_manual_review_override, calculate_metrics, classify_thread,
        message_fingerprint, message_is_inside_analysis_window,
        refine_classification_with_gmail_labels, refine_classification_with_policy_hints,
        rescue_classification_with_valid_signals, should_auto_apply_ai,
    },
    auth::{
        GoogleTokenResponse, UserSession, clear_oauth_cookie, clear_session_cookie, decrypt_token,
        encrypt_token, oauth_cookie, session_cookie, session_expires_at, session_is_active,
        sign_session_id, verify_session_cookie,
    },
    billing::{
        Account, BillingPlan, BillingPlanId, CheckoutSession, CheckoutSessionStatus,
        EntitlementSnapshot, Subscription, SubscriptionStatus, UNLIMITED_ANALYZED_PER_RUN,
        UsageLedger, active_subscription_for_trial, free_plan, plan_by_id, public_plans,
        subscription_allows_access,
    },
    config::AppConfig,
    gmail::GmailClient,
    mailbox::{FilterPreset, GmailConnection, gmail_connection_is_active},
    policies::{
        AiPolicy, AnalysisPolicy, MailboxPurpose, OrgConfigBundle, OrgConfigResponse,
        PolicyVersion, ScheduleReportPolicy, apply_ai_defaults_migration, hash_owner_email,
        normalize_domains, normalize_list, policy_version_from_draft, provision_default_config,
        retention_expires_at, setup_state, validate_timezone,
    },
    scheduler::{
        model::{ScheduleConfig, ScheduleState},
        window::next_fire_time_label,
    },
    storage::{AnalysisDataDeletionAudit, AnalysisDataDeletionStatus, StorageRepository},
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
    pub gmail: GmailClient,
    rate_limiter: RateLimiter,
}

#[derive(Clone, Default)]
struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, VecDeque<chrono::DateTime<Utc>>>>>,
}

impl AppState {
    pub fn new(config: AppConfig, storage: Arc<dyn StorageRepository>) -> Self {
        Self {
            config,
            storage,
            http: Client::builder()
                .timeout(Duration::from_secs(30))
                .connect_timeout(Duration::from_secs(10))
                .build()
                .expect("failed to build http client"),
            gmail: GmailClient::default(),
            rate_limiter: RateLimiter::default(),
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
        .route("/auth/workos/login", get(auth_workos_login))
        .route("/auth/workos/callback", get(auth_workos_callback))
        .route("/auth/workos/webhook", post(workos_webhook))
        .route("/auth/google/login", get(gmail_connect_login))
        .route("/auth/google/callback", get(gmail_connect_callback))
        .route("/gmail/connect/login", get(gmail_connect_login))
        .route("/gmail/connect/callback", get(gmail_connect_callback))
        .route("/gmail/disconnect", post(gmail_disconnect))
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
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true }))
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

async fn gmail_connect_login(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let session = require_session(&state, &headers).await?;
    require_active_entitlement(&state, &session).await?;
    let scope = "https://www.googleapis.com/auth/gmail.readonly";
    let oauth_state = random_urlsafe(24);
    let code_verifier = random_urlsafe(48);
    let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
    let signed_oauth = sign_session_id(
        &format!("{oauth_state}:{code_verifier}"),
        &state.config.session_secret,
    )?;
    let url = url::Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/v2/auth",
        &[
            ("client_id", state.config.google.client_id.as_str()),
            ("redirect_uri", state.config.google.redirect_url.as_str()),
            ("response_type", "code"),
            ("scope", scope),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("state", oauth_state.as_str()),
            ("code_challenge", code_challenge.as_str()),
            ("code_challenge_method", "S256"),
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
    require_active_entitlement(&state, &existing_session).await?;
    let verifier_payload = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| extract_named_cookie(cookies, "ghmi_oauth"))
        .and_then(|cookie| verify_session_cookie(&cookie, &state.config.session_secret))
        .ok_or_else(|| ApiError::bad_request("missing or invalid OAuth PKCE cookie"))?;
    let (expected_state, code_verifier) = verifier_payload
        .split_once(':')
        .ok_or_else(|| ApiError::bad_request("invalid OAuth PKCE cookie payload"))?;
    if query.state.as_deref() != Some(expected_state) {
        return Err(ApiError::bad_request("OAuth state mismatch"));
    }

    let token: GoogleTokenResponse = state
        .http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", query.code.as_str()),
            ("client_id", state.config.google.client_id.as_str()),
            ("client_secret", state.config.google.client_secret.as_str()),
            ("redirect_uri", state.config.google.redirect_url.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", code_verifier),
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
    let connection = GmailConnection {
        owner_email: existing_session.google_account_email.clone(),
        gmail_account_email: profile.email_address.clone(),
        access_token_encrypted: encrypt_token(&token.access_token, &state.config.encryption_key)?,
        refresh_token_encrypted: token
            .refresh_token
            .as_deref()
            .map(|refresh| encrypt_token(refresh, &state.config.encryption_key))
            .transpose()?
            .or_else(|| {
                previous
                    .as_ref()
                    .and_then(|connection| connection.refresh_token_encrypted.clone())
            }),
        connected_at: previous
            .as_ref()
            .map_or(now, |connection| connection.connected_at),
        updated_at: now,
        revoked_at: None,
    };
    state.storage.upsert_gmail_connection(&connection).await?;
    let mut bundle =
        get_or_provision_org_config(&state, &existing_session.google_account_email).await?;
    bundle.mailbox.google_account_email = profile.email_address.clone();
    bundle.mailbox.display_name = profile.email_address.clone();
    bundle.mailbox.authorized_by_user_email = existing_session.google_account_email.clone();
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
        let gmail = state.gmail.clone();
        let storage = state.storage.clone();
        let access_token = token.access_token.clone();
        let owner = existing_session.google_account_email.clone();
        tokio::spawn(async move {
            let metadata = gmail.fetch_mailbox_metadata(&access_token, now).await;
            if storage
                .upsert_mailbox_metadata(&owner, &metadata)
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

async fn gmail_disconnect(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let session = require_session(&state, &headers).await?;
    let connection = state
        .storage
        .get_gmail_connection(&session.google_account_email)
        .await?;
    let encrypted_token = connection.as_ref().and_then(|connection| {
        connection
            .refresh_token_encrypted
            .as_deref()
            .or((!connection.access_token_encrypted.trim().is_empty())
                .then_some(connection.access_token_encrypted.as_str()))
    });

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
    bundle.draft.schedule_report_policy.scheduler_enabled = false;
    state.storage.upsert_org_config(&bundle).await?;
    sync_schedule_config_from_policy(&state, &bundle).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn auth_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let mut session = require_session(&state, &headers).await?;
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
    Ok((StatusCode::NO_CONTENT, headers))
}

async fn auth_logout_all(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let session = require_session(&state, &headers).await?;
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
    Ok((StatusCode::NO_CONTENT, headers))
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
        "gmail_connected": gmail_connection_is_active(connection.as_ref()),
        "gmail_account_email": connection
            .filter(|connection| gmail_connection_is_active(Some(connection)))
            .map(|connection| connection.gmail_account_email),
    })))
}

#[derive(Debug, Serialize)]
struct AccountStatusResponse {
    account_email: String,
    workos_user_id: Option<String>,
    org_id: String,
    gmail_connected: bool,
    gmail_account_email: Option<String>,
    entitlement: EntitlementSnapshot,
}

async fn get_account_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AccountStatusResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    let entitlement = entitlement_snapshot(&state, &bundle.org.id).await?;
    let connection = state
        .storage
        .get_gmail_connection(&session.google_account_email)
        .await?;
    Ok(Json(AccountStatusResponse {
        account_email: session.google_account_email.clone(),
        workos_user_id: session.workos_user_id.clone(),
        org_id: bundle.org.id,
        gmail_connected: gmail_connection_is_active(connection.as_ref()),
        gmail_account_email: connection
            .filter(|connection| gmail_connection_is_active(Some(connection)))
            .map(|connection| connection.gmail_account_email),
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
}

#[derive(Debug, Serialize)]
struct CheckoutSessionResponse {
    session: CheckoutSession,
}

async fn create_checkout_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateCheckoutSubscriptionRequest>,
) -> Result<Json<CheckoutSessionResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    let plan_id = BillingPlanId::parse(&request.plan_id)
        .ok_or_else(|| ApiError::bad_request("plan_id inválido"))?;
    let card_token_id = request
        .card_token_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("debes completar los datos de tu tarjeta"))?;
    let existing_subscription = state
        .storage
        .get_subscription_for_org(&bundle.org.id)
        .await?;
    if checkout_blocked_by_active_subscription(existing_subscription.as_ref(), Utc::now()) {
        return Err(ApiError::conflict(
            "ya tienes una suscripción activa; cambia de plan desde tu cuenta",
        ));
    }
    let plan = plan_by_id(&plan_id);
    let now = Utc::now();
    let mut checkout = CheckoutSession {
        id: Uuid::new_v4().to_string(),
        org_id: bundle.org.id.clone(),
        account_email: session.google_account_email.clone(),
        plan_id: plan_id.clone(),
        status: CheckoutSessionStatus::Pending,
        provider: "mercadopago".to_string(),
        provider_subscription_id: None,
        currency_id: "CLP".to_string(),
        amount_clp: plan.clp_monthly,
        usd_reference_monthly: plan.usd_reference_monthly,
        trial_days: plan.trial_days,
        created_at: now,
        updated_at: now,
    };

    let access_token = state
        .config
        .billing
        .mercadopago_access_token
        .as_deref()
        .ok_or_else(|| ApiError::service_unavailable("Mercado Pago no está configurado"))?;
    let payer_email = request
        .payer_email
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&checkout.account_email);
    let mp =
        create_mercadopago_preapproval(&state, access_token, &checkout, card_token_id, payer_email)
            .await?;
    checkout.provider_subscription_id = Some(mp.id.clone());
    checkout.updated_at = Utc::now();

    if !matches!(
        map_mercadopago_status(mp.status.as_deref()),
        SubscriptionStatus::Active
    ) {
        checkout.status = CheckoutSessionStatus::Failed;
        state.storage.upsert_checkout_session(&checkout).await?;
        return Err(ApiError::bad_request(
            "Mercado Pago no pudo autorizar la tarjeta; revisa los datos e inténtalo otra vez",
        ));
    }

    // La tarjeta ya quedó autorizada en el checkout embebido. El webhook mantiene
    // el estado sincronizado, pero no hace falta redirigir ni bloquear a la persona.
    checkout.status = CheckoutSessionStatus::Activated;
    let now = Utc::now();
    let mut subscription = active_subscription_for_trial(
        checkout.org_id.clone(),
        checkout.plan_id.clone(),
        Some(mp.id),
        now,
    );
    if !matches!(subscription.status, SubscriptionStatus::Trialing) {
        subscription.status = SubscriptionStatus::Active;
    }
    state.storage.upsert_subscription(&subscription).await?;

    state.storage.upsert_checkout_session(&checkout).await?;
    Ok(Json(CheckoutSessionResponse { session: checkout }))
}

fn checkout_blocked_by_active_subscription(
    subscription: Option<&Subscription>,
    now: chrono::DateTime<Utc>,
) -> bool {
    subscription.is_some_and(|subscription| {
        subscription_allows_access(Some(subscription), now) && !subscription.cancel_at_period_end
    })
}

async fn cancel_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<EntitlementSnapshot>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    let mut subscription = state
        .storage
        .get_subscription_for_org(&bundle.org.id)
        .await?
        .ok_or_else(|| ApiError::conflict("no hay una suscripción para cancelar"))?;

    // Corta cobros futuros en Mercado Pago; el acceso se mantiene hasta el fin del período.
    if let Some(provider_id) = subscription.provider_subscription_id.clone() {
        let access_token = state
            .config
            .billing
            .mercadopago_access_token
            .as_deref()
            .ok_or_else(|| ApiError::service_unavailable("Mercado Pago no está configurado"))?;
        update_mercadopago_preapproval(
            &state,
            access_token,
            &provider_id,
            json!({ "status": "cancelled" }),
        )
        .await?;
    }

    subscription.cancel_at_period_end = true;
    subscription.updated_at = Utc::now();
    state.storage.upsert_subscription(&subscription).await?;

    let entitlement = entitlement_snapshot(&state, &bundle.org.id).await?;
    Ok(Json(entitlement))
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
    let plan_id = BillingPlanId::parse(&request.plan_id)
        .ok_or_else(|| ApiError::bad_request("plan_id inválido"))?;
    let plan = plan_by_id(&plan_id);

    let mut subscription = state
        .storage
        .get_subscription_for_org(&bundle.org.id)
        .await?
        .ok_or_else(|| ApiError::payment_required("no tienes una suscripción activa"))?;

    if !subscription_allows_access(Some(&subscription), Utc::now()) {
        return Err(ApiError::conflict(
            "reactiva tu suscripción antes de cambiar de plan",
        ));
    }

    let provider_id = subscription
        .provider_subscription_id
        .clone()
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
        &provider_id,
        json!({
            "auto_recurring": {
                "transaction_amount": plan.clp_monthly,
                "currency_id": "CLP"
            },
            "reason": format!("{} - Helpdesk Inspector", plan.name)
        }),
    )
    .await?;

    subscription.plan_id = plan_id;
    subscription.cancel_at_period_end = false;
    subscription.updated_at = Utc::now();
    state.storage.upsert_subscription(&subscription).await?;

    let entitlement = entitlement_snapshot(&state, &bundle.org.id).await?;
    Ok(Json(entitlement))
}

#[derive(Debug, Deserialize)]
struct MercadoPagoWebhook {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    #[serde(rename = "type")]
    event_type: Option<String>,
    #[serde(default)]
    #[serde(rename = "action")]
    _action: Option<String>,
    #[serde(default)]
    data: Option<MercadoPagoWebhookData>,
}

#[derive(Debug, Deserialize)]
struct MercadoPagoWebhookData {
    id: String,
}

async fn mercadopago_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Json(payload): Json<MercadoPagoWebhook>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let signed_data_id = query.get("data.id").or_else(|| query.get("data_id"));
    verify_mercadopago_webhook(&state, &headers, signed_data_id.map(String::as_str))?;
    let provider_id = payload
        .data
        .map(|data| data.id)
        .or(payload.id)
        .ok_or_else(|| ApiError::bad_request("missing Mercado Pago data id"))?;
    let event_type = payload.event_type.unwrap_or_default();
    if !event_type.is_empty()
        && event_type != "subscription_preapproval"
        && event_type != "preapproval"
    {
        return Ok(Json(json!({ "ok": true, "ignored": true })));
    }
    let checkout = state
        .storage
        .find_checkout_session_by_provider_id(&provider_id)
        .await?
        .ok_or(ApiError::not_found("checkout session not found"))?;
    let access_token = state
        .config
        .billing
        .mercadopago_access_token
        .as_deref()
        .ok_or_else(|| ApiError::service_unavailable("Mercado Pago no está configurado"))?;
    let provider = get_mercadopago_preapproval(&state, access_token, &provider_id).await?;
    let status = map_mercadopago_status(provider.status.as_deref());
    let now = Utc::now();
    let mut subscription = state
        .storage
        .find_subscription_by_provider_id(&provider_id)
        .await?
        .unwrap_or_else(|| {
            active_subscription_for_trial(
                checkout.org_id.clone(),
                checkout.plan_id.clone(),
                Some(provider_id.clone()),
                now,
            )
        });
    subscription.status = status;
    subscription.updated_at = now;
    subscription.provider_subscription_id = Some(provider_id);
    state.storage.upsert_subscription(&subscription).await?;
    Ok(Json(json!({ "ok": true })))
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
    // Plan vigente (pagado o Mira Free por defecto): siempre hay límites que mostrar.
    let limits = Some(effective_plan_for_org(&state, &bundle.org.id).await?.limits);
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
            // ponytail: Firestore cannot atomically delete every nested document and update this audit.
            // A pending record is safer than claiming deletion completed when this write fails.
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

    Ok(Json(DataSummaryResponse {
        account: DataSummaryAccount {
            google_account_email: session.google_account_email,
            gmail_scope_snapshot: bundle.mailbox.gmail_scope_snapshot.clone(),
            mailbox_connected: bundle.mailbox.revoked_at.is_none(),
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
                available: false,
                reason: "available_in_account",
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
            preset: "weekdays_08_local".to_string(),
            recipients_count,
            next_run_estimate: if enabled {
                next_fire_time_label(Utc::now(), &timezone)
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
    scheduler_enabled: Option<bool>,
    timezone: Option<String>,
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
        schedule.scheduler_enabled && gmail_connection_is_active(gmail_connection.as_ref());
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
    if let Some(timezone) = update.timezone {
        validate_policy_timezone(&timezone)?;
        current.timezone = timezone;
    }
    if let Some(values) = update.internal_domains {
        current.internal_domains = normalize_domains(values);
    }
    if let Some(values) = update.responder_emails {
        current.responder_emails = normalize_list(values);
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
    if let Some(timezone) = update.timezone {
        validate_policy_timezone(&timezone)?;
        current.timezone = timezone;
    }
    if let Some(values) = update.report_recipients {
        current.report_recipients = normalize_list(values);
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
        // Migración perezosa al modelo opt-out: enciende una sola vez la IA de orgs
        // previas que estaban apagadas por el viejo default, persistiendo el cambio.
        if apply_ai_defaults_migration(&mut bundle.draft.ai_policy, Utc::now()) {
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
    enforce_usage_allows_run(&state, &bundle.org.id).await?;
    require_gmail_connected(
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
    let uses_legacy_config = request.policy_version_id.is_none()
        && (!request.internal_domains.is_empty()
            || !request.ignored_senders.is_empty()
            || !request.ignored_domains.is_empty()
            || !request.ignored_keywords.is_empty()
            || request.timezone.is_some());
    let run = if uses_legacy_config {
        build_legacy_run(session.google_account_email, request, now)
    } else {
        build_policy_run(&state, &session.google_account_email, request, now).await?
    };
    state.storage.create_analysis_run(&run).await?;
    tracing::info!(
        operation = "analysis_run_created",
        run_id = %run.id,
        "analysis run created"
    );
    increment_runs_usage(&state, run.org_id.as_deref()).await?;
    Ok(Json(run))
}

fn build_legacy_run(
    user_email: String,
    request: CreateAnalysisRunRequest,
    now: chrono::DateTime<Utc>,
) -> AnalysisRun {
    AnalysisRun {
        id: Uuid::new_v4().to_string(),
        user_email,
        org_id: None,
        mailbox_id: None,
        trigger_type: Some(TriggerType::Manual),
        policy_version_id: None,
        policy_hash: None,
        policy_snapshot: None,
        gmail_scope_snapshot: vec![],
        retention_expires_at: None,
        data_minimization_mode: Some("metadata_snippets_excerpts_only".to_string()),
        config: AnalysisConfig {
            date_from: request.date_from,
            date_to: request.date_to,
            time_from: request.time_from.unwrap_or_else(|| "00:00".to_string()),
            time_to: request.time_to.unwrap_or_else(|| "23:59".to_string()),
            timezone: request
                .timezone
                .unwrap_or_else(|| "America/Santiago".to_string()),
            internal_domains: request.internal_domains,
            ignored_senders: request.ignored_senders,
            ignored_domains: request.ignored_domains,
            ignored_keywords: request.ignored_keywords,
            include_labels: request.include_labels,
            exclude_labels: request.exclude_labels,
        },
        status: AnalysisStatus::Pending,
        progress_message: "Listo para analizar".to_string(),
        processed_threads: 0,
        total_candidate_threads: 0,
        metrics: AnalysisMetrics::default(),
        created_at: now,
        completed_at: None,
        error_message: None,
    }
}

async fn build_policy_run(
    state: &AppState,
    user_email: &str,
    request: CreateAnalysisRunRequest,
    now: chrono::DateTime<Utc>,
) -> Result<AnalysisRun, ApiError> {
    let bundle = get_or_provision_org_config(state, user_email).await?;
    let policy_version = if let Some(policy_version_id) = request.policy_version_id.as_deref() {
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
            time_from: request
                .time_from
                .unwrap_or_else(|| analysis.default_time_from.clone()),
            time_to: request
                .time_to
                .unwrap_or_else(|| analysis.default_time_to.clone()),
            timezone: analysis.timezone.clone(),
            internal_domains: analysis.internal_domains.clone(),
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
        .await?;
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
    require_active_entitlement(&state, &session).await?;
    let connection = state
        .storage
        .get_gmail_connection(&session.google_account_email)
        .await?;
    require_gmail_connected(connection.as_ref())?;
    let run = require_owned_run(&state, &id, &session).await?;
    if run.status != AnalysisStatus::Pending {
        return Err(ApiError::conflict(
            "el análisis solo puede iniciarse cuando está pendiente",
        ));
    }
    enforce_rate_limit(
        &state,
        &session.google_account_email,
        "analysis_start",
        state.config.rate_limit.analysis_start_per_hour,
    )
    .await?;
    let run = state
        .storage
        .claim_pending_analysis_run(&run.id)
        .await?
        .ok_or_else(|| {
            ApiError::conflict("el análisis solo puede iniciarse cuando está pendiente")
        })?;

    let access_token = decrypt_token(
        &gmail_access_token(connection.as_ref())?,
        &state.config.encryption_key,
    )?;
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
    let messages = state
        .storage
        .list_messages(&thread.analysis_run_id, &thread.id)
        .await?;
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
            gmail_thread_id: thread.gmail_thread_id.clone(),
            source_run_id: thread.analysis_run_id.clone(),
            message_fingerprint: message_fingerprint(&messages),
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
        failed.error_message = Some("analysis_failed".to_string());
        failed.progress_message = "El análisis falló".to_string();
        let _ = state.storage.update_analysis_run(&failed).await;
    }
}

pub(crate) async fn execute_analysis(
    state: AppState,
    run_id: String,
    access_token: String,
) -> anyhow::Result<()> {
    let mut run = state
        .storage
        .get_analysis_run(&run_id)
        .await
        .context(AnalysisFailureStage("run_lookup"))?
        .ok_or_else(|| anyhow::anyhow!("analysis run not found"))
        .context(AnalysisFailureStage("run_lookup"))?;
    // Plan vigente para este run (pagado o Mira Free por defecto). El tope del plan
    // se aplica sobre los hilos ANALIZADOS (los que sobreviven el embudo), no sobre
    // los recuperados de Gmail.
    let plan = plan_for_run(&state, run.org_id.as_deref())
        .await
        .context(AnalysisFailureStage("plan_lookup"))?;
    let used_analyzed = match run.org_id.as_deref() {
        Some(org_id) => state
            .storage
            .get_usage_ledger(org_id, &current_period_key())
            .await
            .context(AnalysisFailureStage("usage_lookup"))?
            .map(|usage| usage.analyzed_threads)
            .unwrap_or(0),
        None => 0,
    };
    let policy_max = run
        .policy_snapshot
        .as_ref()
        .map(|snapshot| snapshot.analysis_policy.max_threads_per_run)
        .unwrap_or(state.config.google.gmail_max_threads);
    // En dev (`enforcement_enabled=false`) no hay tope: análisis ilimitado como antes.
    let enforce = state.config.billing.enforcement_enabled;
    let analyzed_cap = if enforce {
        analyzed_run_cap(
            plan.limits.analyzed_threads_per_run,
            plan.limits.analyzed_threads_per_month,
            used_analyzed,
        )
    } else {
        u32::MAX
    };
    let has_finite_run_cap =
        enforce && plan.limits.analyzed_threads_per_run != UNLIMITED_ANALYZED_PER_RUN;
    // Recuperamos con holgura cuando hay tope finito (Free), para que el tope de
    // analizados pueda llenarse pese a los descartes del embudo.
    let retrieval_max = retrieval_max_for(has_finite_run_cap, analyzed_cap, policy_max).max(1);

    let page = state
        .gmail
        .list_thread_ids(&access_token, &run.config, retrieval_max)
        .await
        .context(AnalysisFailureStage("gmail_list_threads"))?;
    let more_beyond_retrieved = page.next_page_token.is_some();
    let thread_ids = page.ids;
    run.total_candidate_threads = thread_ids.len() as u64;
    run.progress_message = format!("{} hilos encontrados en Gmail", thread_ids.len());
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

    // Primera fase: Gmail + filtros + heurística local. No se llama a IA todavía,
    // para poder agrupar los candidatos humanos y reutilizar la política por lote.
    let config = run.config.clone();
    let policy_snapshot = run.policy_snapshot.clone();
    let policy_version_id = run.policy_version_id.clone();
    let owner_email = run.user_email.clone();
    let run_id = run.id.clone();
    let total = run.total_candidate_threads;
    let mut stored = 0u64;
    let mut skipped_by_plan_cap = 0u64;
    let mut prepared_threads = Vec::new();
    {
        let state_ref = &state;
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
                access_ref,
                run_id_ref,
                config_ref,
                ThreadProcessingContext {
                    policy_snapshot: policy_ref,
                    owner_email: owner_email_ref,
                    analyzed_slot: analyzed_slot_ref,
                    analyzed_cap,
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
                        prepared_threads.push(prepared);
                    }
                    if let Some(dropped) = outcome.dropped
                        && funnel.dropped_samples.len() < FUNNEL_DROPPED_SAMPLE_CAP
                    {
                        funnel.dropped_samples.push(dropped);
                    }
                }
                Err(_) => {
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

    let ai_enabled = policy_snapshot
        .as_ref()
        .map(|snapshot| snapshot.ai_policy.enabled)
        .unwrap_or(true);
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

    // Segunda fase: clasificador compacto por lotes. Los errores se reintentan una
    // vez dividiendo el lote; únicamente desacuerdos/ambigüedades escalan al auditor
    // detallado por hilo.
    let mut detailed_indexes = HashSet::new();
    for index_chunk in batch_indexes.chunks(AI_BATCH_SIZE) {
        for index in index_chunk {
            ai_unique_thread_ids.insert(prepared_threads[*index].thread.gmail_thread_id.clone());
        }
        let batch = audit_batch_with_split(
            &state,
            &prepared_threads,
            index_chunk,
            policy_snapshot.as_ref(),
        )
        .await;
        ai_input_tokens += batch.input_tokens;
        ai_output_tokens += batch.output_tokens;
        funnel.ai_calls += batch.calls;
        funnel.ai_batch_classified += batch.decisions.len() as u64;

        for index in index_chunk {
            let prepared = &mut prepared_threads[*index];
            let decision = batch.decisions.get(&prepared.thread.gmail_thread_id);
            let should_escalate = match decision {
                Some(decision)
                    if !batch_decision_requires_detailed(
                        &prepared.thread,
                        decision,
                        auto_apply_threshold,
                        prepared.force_detailed,
                    ) =>
                {
                    apply_batch_decision(&mut prepared.thread, decision);
                    false
                }
                _ => true,
            };
            if should_escalate {
                detailed_indexes.insert(*index);
            }
        }
    }

    for index in detailed_indexes {
        let prepared = &mut prepared_threads[index];
        ai_unique_thread_ids.insert(prepared.thread.gmail_thread_id.clone());
        funnel.ai_calls += 1;
        funnel.ai_detailed_audited += 1;
        let usage = audit_prepared_thread(
            &state,
            &run_id,
            prepared,
            policy_snapshot.as_ref(),
            policy_version_id.as_deref(),
            auto_apply_threshold,
            manual_review_threshold,
        )
        .await;
        ai_input_tokens += usage.input_tokens;
        ai_output_tokens += usage.output_tokens;
    }

    // Los hilos que no necesitaban IA y los resultados ya resueltos se persisten al
    // final; los cuerpos se descartan como antes.
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
    funnel.would_be_analyzed = Some(stored + skipped_by_plan_cap);
    funnel.plan_analyzed_cap = (skipped_by_plan_cap > 0).then_some(analyzed_cap);
    funnel.ai_unique_threads = ai_unique_thread_ids.len() as u64;

    let threads = state.storage.list_threads(&run.id).await?;
    run.metrics = calculate_metrics(&threads, ai_input_tokens, ai_output_tokens);
    // Los descartados no están en `threads` (nunca se guardan), así que el embudo
    // se adjunta aparte, después de recomputar las métricas de los almacenados.
    run.metrics.funnel = funnel;
    run.status = AnalysisStatus::Completed;
    run.progress_message = "Análisis completado".to_string();
    run.completed_at = Some(Utc::now());
    state.storage.update_analysis_run(&run).await?;
    // El cupo mensual se cobra por hilos ANALIZADOS (guardados), no por recuperados.
    add_analysis_usage(
        &state,
        run.org_id.as_deref(),
        stored as u32,
        ai_unique_thread_ids.len() as u32,
    )
    .await?;
    Ok(())
}

/// Plan vigente para un run en background (versión `anyhow` de `effective_plan_for_org`).
async fn plan_for_run(state: &AppState, org_id: Option<&str>) -> anyhow::Result<BillingPlan> {
    if !state.config.billing.enforcement_enabled {
        return Ok(plan_by_id(&BillingPlanId::Pro));
    }
    let Some(org_id) = org_id else {
        return Ok(plan_by_id(&BillingPlanId::Pro));
    };
    let subscription = state.storage.get_subscription_for_org(org_id).await?;
    if subscription_allows_access(subscription.as_ref(), Utc::now()) {
        Ok(plan_by_id(&subscription.expect("checked above").plan_id))
    } else {
        Ok(free_plan())
    }
}

/// Tope de hilos analizados para un run: el menor entre el tope por análisis del
/// plan y lo que reste del cupo mensual.
fn analyzed_run_cap(per_run: u32, per_month: u32, used_this_month: u32) -> u32 {
    let monthly_remaining = per_month.saturating_sub(used_this_month);
    per_run.min(monthly_remaining)
}

/// Cuántos candidatos recuperar de Gmail. Con tope finito (Free) recuperamos con
/// holgura para que el tope de analizados pueda llenarse pese a los descartes del
/// embudo; sin tope (planes de pago) respetamos el `max_threads_per_run` de policy.
fn retrieval_max_for(has_finite_run_cap: bool, analyzed_cap: u32, policy_max: u32) -> u32 {
    if has_finite_run_cap {
        analyzed_cap
            .saturating_mul(FREE_RETRIEVAL_FACTOR)
            .min(FREE_RETRIEVAL_HARD_MAX)
            .max(analyzed_cap.min(FREE_RETRIEVAL_HARD_MAX))
    } else {
        policy_max
    }
}

/// Holgura de recuperación para planes con tope finito de analizados.
const FREE_RETRIEVAL_FACTOR: u32 = 3;
/// Techo duro de recuperación para no disparar las llamadas a Gmail en Free.
const FREE_RETRIEVAL_HARD_MAX: u32 = 300;

/// Maximum number of threads processed concurrently in a single analysis run.
const ANALYSIS_CONCURRENCY: usize = 5;
/// Write run progress to storage every N processed threads (instead of every one).
const PROGRESS_UPDATE_EVERY: u64 = 5;
/// Máximo de hilos descartados que guardamos como muestra en el embudo, para
/// acotar el tamaño del documento del run sin perder utilidad de diagnóstico.
const FUNNEL_DROPPED_SAMPLE_CAP: usize = 100;
const AI_BATCH_SIZE: usize = 20;
const AI_BATCH_EXCERPT_CHARS: usize = 200;

#[derive(Default)]
struct ThreadOutcome {
    /// Dónde terminó el hilo: almacenado o descartado (y por qué). Alimenta el
    /// embudo de diagnóstico para que los descartes dejen de ser invisibles.
    disposition: ThreadDisposition,
    /// Metadatos del descartado (solo cuando `disposition` es un descarte).
    dropped: Option<DroppedThreadInfo>,
    /// Hilo ya filtrado y clasificado localmente, pendiente de batch IA/persistencia.
    prepared: Option<PreparedThread>,
}

struct PreparedThread {
    thread: EmailThread,
    messages: Vec<EmailMessage>,
    label_ids: Vec<String>,
    should_batch: bool,
    force_detailed: bool,
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
        gmail_thread_id: thread_id.to_string(),
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
    analyzed_cap: u32,
}

async fn prepare_one_thread(
    state: &AppState,
    access_token: &str,
    run_id: &str,
    config: &AnalysisConfig,
    processing: ThreadProcessingContext<'_>,
    thread_id: String,
) -> anyhow::Result<ThreadOutcome> {
    let data = state
        .gmail
        .fetch_thread(access_token, &thread_id, config)
        .await?;
    if !data.is_primary_inbox {
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
    let has_external_in_window = data
        .messages
        .iter()
        .any(|message| message.is_external && message_is_inside_analysis_window(message, config));
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
    if slot >= processing.analyzed_cap {
        return Ok(ThreadOutcome {
            disposition: ThreadDisposition::SkippedByPlanCap,
            ..Default::default()
        });
    }

    let mut thread = classify_thread(run_id, &data.id, &data.messages, config);
    // Señal extra: refina el veredicto heurístico con las pestañas/categorías de Gmail.
    refine_classification_with_gmail_labels(&mut thread, &data.label_ids);
    let mut force_detailed = thread.manual_review_required;
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
        // Señal extra: rescata a válido los hilos cuyo asunto/cuerpo menciona una
        // palabra de "señal de ticket" del tenant; la IA confirma o degrada después.
        let was_valid = thread.is_valid_client_request;
        rescue_classification_with_valid_signals(
            &mut thread,
            &data.messages,
            &snapshot.analysis_policy.valid_signal_keywords,
        );
        force_detailed |=
            thread.manual_review_required || (!was_valid && thread.is_valid_client_request);
    }
    if let Some(review) = state
        .storage
        .get_manual_review_override(processing.owner_email, &thread.gmail_thread_id)
        .await?
        && review.message_fingerprint == message_fingerprint(&data.messages)
    {
        apply_manual_review_override(&mut thread, &data.messages, &review);
        let sanitized = messages_for_persistence(&data.messages);
        state.storage.upsert_thread(&thread, &sanitized).await?;
        return Ok(ThreadOutcome {
            disposition: ThreadDisposition::Stored,
            ..Default::default()
        });
    }
    let should_batch = thread.first_client_message_id.is_some()
        && matches!(
            thread.classification,
            Classification::ValidClientRequest | Classification::Ambiguous
        );
    Ok(ThreadOutcome {
        disposition: ThreadDisposition::Stored,
        prepared: Some(PreparedThread {
            thread,
            messages: data.messages,
            label_ids: data.label_ids,
            should_batch,
            force_detailed,
        }),
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
    mailbox_aliases: &'a [String],
    valid_request_criteria: &'a [String],
    non_responsibility_rules: &'a [String],
    ignored_senders: &'a [String],
    ignored_domains: &'a [String],
    ignored_keywords: &'a [String],
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
        mailbox_aliases: &snapshot.analysis_policy.mailbox_aliases,
        valid_request_criteria: &snapshot.analysis_policy.valid_request_criteria,
        non_responsibility_rules: &snapshot.analysis_policy.non_responsibility_rules,
        ignored_senders: &snapshot.analysis_policy.ignored_senders,
        ignored_domains: &snapshot.analysis_policy.ignored_domains,
        ignored_keywords: &snapshot.analysis_policy.ignored_keywords,
        prompt_version: &snapshot.ai_policy.prompt_version,
        allowed_fields: &snapshot.ai_policy.allowed_fields,
    }
}

#[derive(Serialize)]
struct BatchMessageSummary {
    message_id: String,
    from_email: String,
    date: chrono::DateTime<Utc>,
    is_internal: bool,
    is_external: bool,
    is_automated: bool,
    excerpt: String,
}

#[derive(Serialize)]
struct BatchThreadSummary {
    thread_id: String,
    subject: String,
    gmail_labels: Vec<String>,
    automatic_classification: Classification,
    automatic_confidence: f64,
    automatic_is_valid: bool,
    automatic_is_answered: bool,
    automatic_manual_review_required: bool,
    messages: Vec<BatchMessageSummary>,
}

#[derive(Serialize)]
struct BatchAuditRequest<'a> {
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
}

#[derive(Default)]
struct BatchExecution {
    decisions: HashMap<String, BatchAuditDecision>,
    input_tokens: u64,
    output_tokens: u64,
    calls: u64,
}

#[derive(Default)]
struct TokenUsage {
    input_tokens: u64,
    output_tokens: u64,
}

fn batch_messages_for_thread(prepared: &PreparedThread) -> Vec<BatchMessageSummary> {
    let mut ordered = prepared.messages.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|message| message.date);
    let mut selected_ids: Vec<&str> = Vec::new();
    for id in [
        prepared.thread.first_client_message_id.as_ref(),
        prepared.thread.first_internal_reply_message_id.as_ref(),
        ordered.last().map(|message| &message.id),
    ]
    .into_iter()
    .flatten()
    {
        if !selected_ids.contains(&id.as_str()) {
            selected_ids.push(id.as_str());
        }
    }
    selected_ids
        .into_iter()
        .filter_map(|id| prepared.messages.iter().find(|message| message.id == id))
        .map(|message| {
            let source = message
                .body_text
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(&message.snippet);
            BatchMessageSummary {
                message_id: message.id.clone(),
                from_email: message.from_email.clone(),
                date: message.date,
                is_internal: message.is_internal,
                is_external: message.is_external,
                is_automated: message.is_automated,
                excerpt: truncate_chars(source, AI_BATCH_EXCERPT_CHARS),
            }
        })
        .collect()
}

fn batch_summary(prepared: &PreparedThread) -> BatchThreadSummary {
    BatchThreadSummary {
        thread_id: prepared.thread.gmail_thread_id.clone(),
        subject: prepared.thread.subject.clone(),
        gmail_labels: prepared.label_ids.clone(),
        automatic_classification: prepared.thread.classification.clone(),
        automatic_confidence: prepared.thread.classification_confidence,
        automatic_is_valid: prepared.thread.is_valid_client_request,
        automatic_is_answered: prepared.thread.is_answered,
        automatic_manual_review_required: prepared.thread.manual_review_required,
        messages: batch_messages_for_thread(prepared),
    }
}

async fn audit_batch_once(
    state: &AppState,
    prepared: &[PreparedThread],
    indexes: &[usize],
    policy_snapshot: Option<&crate::policies::PolicySnapshot>,
) -> anyhow::Result<BatchAuditResponse> {
    let policy_context = policy_snapshot.map(ai_worker_policy_context);
    let threads = indexes
        .iter()
        .map(|index| batch_summary(&prepared[*index]))
        .collect();
    let mut request = state
        .http
        .post(format!("{}/audit/batch", state.config.ai.worker_url))
        .json(&BatchAuditRequest {
            policy_context,
            threads,
        });
    if let Some(request_id) = worker_request_id() {
        request = request.header("x-request-id", request_id);
    }
    if let Some(audience) = &state.config.ai.worker_audience {
        request = request.bearer_auth(fetch_cloud_run_identity_token(&state.http, audience).await?);
    }
    Ok(request
        .send()
        .await?
        .error_for_status()?
        .json::<BatchAuditResponse>()
        .await?)
}

async fn audit_batch_with_split(
    state: &AppState,
    prepared: &[PreparedThread],
    indexes: &[usize],
    policy_snapshot: Option<&crate::policies::PolicySnapshot>,
) -> BatchExecution {
    let mut execution = BatchExecution {
        calls: 1,
        ..Default::default()
    };
    match audit_batch_once(state, prepared, indexes, policy_snapshot).await {
        Ok(response) => {
            merge_batch_response(&mut execution, response, prepared, indexes);
            return execution;
        }
        Err(_) => {
            tracing::warn!(
                operation = "ai_batch",
                batch_size = indexes.len(),
                "falló auditoría IA batch"
            );
        }
    }
    if indexes.len() <= 1 {
        return execution;
    }
    let middle = indexes.len() / 2;
    for half in [&indexes[..middle], &indexes[middle..]] {
        if half.is_empty() {
            continue;
        }
        execution.calls += 1;
        match audit_batch_once(state, prepared, half, policy_snapshot).await {
            Ok(response) => merge_batch_response(&mut execution, response, prepared, half),
            Err(_) => {
                tracing::warn!(
                    operation = "ai_batch_retry",
                    batch_size = half.len(),
                    "falló reintento IA batch"
                );
            }
        }
    }
    execution
}

fn merge_batch_response(
    execution: &mut BatchExecution,
    response: BatchAuditResponse,
    prepared: &[PreparedThread],
    indexes: &[usize],
) {
    let expected = indexes
        .iter()
        .map(|index| prepared[*index].thread.gmail_thread_id.as_str())
        .collect::<HashSet<_>>();
    execution.input_tokens += response.input_tokens;
    execution.output_tokens += response.output_tokens;
    for decision in response.decisions {
        if expected.contains(decision.thread_id.as_str()) {
            execution
                .decisions
                .insert(decision.thread_id.clone(), decision);
        }
    }
}

fn batch_agrees_with_heuristic(thread: &EmailThread, decision: &BatchAuditDecision) -> bool {
    !decision.manual_review_required
        && decision.classification != Classification::Ambiguous
        && decision.classification == thread.classification
        && decision.is_valid_client_request == thread.is_valid_client_request
        && decision.is_answered == thread.is_answered
}

fn batch_decision_requires_detailed(
    thread: &EmailThread,
    decision: &BatchAuditDecision,
    auto_apply_threshold: f64,
    force_detailed: bool,
) -> bool {
    force_detailed
        || decision.confidence < auto_apply_threshold
        || !batch_agrees_with_heuristic(thread, decision)
}

fn apply_batch_decision(thread: &mut EmailThread, decision: &BatchAuditDecision) {
    thread.classification = decision.classification.clone();
    thread.classification_source = ClassificationSource::Ai;
    thread.classification_confidence = decision.confidence;
    thread.is_valid_client_request = decision.is_valid_client_request;
    thread.is_answered = decision.is_answered;
    thread.manual_review_required = false;
    thread
        .reasons
        .push("La clasificación IA por lote confirmó la heurística.".to_string());
    thread.reasons.extend(decision.issues.iter().cloned());
}

async fn audit_prepared_thread(
    state: &AppState,
    run_id: &str,
    prepared: &mut PreparedThread,
    policy_snapshot: Option<&crate::policies::PolicySnapshot>,
    policy_version_id: Option<&str>,
    auto_apply_threshold: f64,
    manual_review_threshold: f64,
) -> TokenUsage {
    let (max_messages, max_body_chars) = policy_snapshot
        .map(|snapshot| {
            (
                snapshot.ai_policy.max_audit_messages as usize,
                snapshot.ai_policy.max_body_chars_per_message as usize,
            )
        })
        .unwrap_or((14, 280));
    let audit_messages = audit_messages_for_thread(
        &prepared.thread,
        &prepared.messages,
        max_messages,
        max_body_chars,
    );
    let result = audit_thread(
        state,
        &prepared.thread,
        &audit_messages,
        policy_snapshot,
        &prepared.label_ids,
    )
    .await;
    let mut audit = match result {
        Ok(audit) => audit,
        Err(error) => {
            mark_detailed_audit_failed(&mut prepared.thread, &error);
            return TokenUsage::default();
        }
    };
    audit.policy_version_id = policy_version_id.map(ToOwned::to_owned);
    if let Some(snapshot) = policy_snapshot {
        audit.prompt_version = Some(snapshot.ai_policy.prompt_version.clone());
        audit.model_id = Some(snapshot.ai_policy.model_id.clone());
        audit.auto_apply_threshold = Some(auto_apply_threshold);
        audit.max_audit_messages = Some(max_messages as u32);
        audit.max_body_chars_per_message = Some(max_body_chars as u32);
    }
    let usage = TokenUsage {
        input_tokens: audit.input_tokens,
        output_tokens: audit.output_tokens,
    };
    if state
        .storage
        .add_ai_audit(run_id, &prepared.thread.id, &audit)
        .await
        .is_err()
    {
        tracing::warn!(
            operation = "ai_audit_persist",
            "no se pudo persistir auditoría IA detallada"
        );
    }

    let known_ids = prepared
        .messages
        .iter()
        .map(|message| message.id.clone())
        .collect::<Vec<_>>();
    if should_auto_apply_ai(&audit, &known_ids) && audit.confidence >= auto_apply_threshold {
        prepared.thread.classification = audit.classification.clone();
        prepared.thread.classification_source = ClassificationSource::Ai;
        prepared.thread.classification_confidence = audit.confidence;
        prepared.thread.is_valid_client_request = audit.is_valid_client_request;
        prepared.thread.is_answered = audit.is_answered;
        prepared.thread.first_client_message_id = audit.first_client_message_id.clone();
        prepared.thread.first_internal_reply_message_id =
            audit.first_internal_reply_message_id.clone();
        prepared.thread.last_internal_message_id = audit.last_internal_message_id.clone();
        apply_trace_dates(&mut prepared.thread, &prepared.messages);
        prepared.thread.manual_review_required = false;
        prepared
            .thread
            .reasons
            .push("La auditoría IA detallada se aplicó por alta confianza.".to_string());
    } else if ai_message_ids_known(&audit, &known_ids)
        && audit.confidence >= manual_review_threshold
    {
        prepared.thread.classification = audit.classification.clone();
        prepared.thread.classification_source = ClassificationSource::Ai;
        prepared.thread.classification_confidence = audit.confidence;
        prepared.thread.is_valid_client_request = audit.is_valid_client_request;
        prepared.thread.is_answered = audit.is_answered;
        prepared.thread.first_client_message_id = audit.first_client_message_id.clone();
        prepared.thread.first_internal_reply_message_id =
            audit.first_internal_reply_message_id.clone();
        prepared.thread.last_internal_message_id = audit.last_internal_message_id.clone();
        apply_trace_dates(&mut prepared.thread, &prepared.messages);
        prepared.thread.manual_review_required = true;
        prepared.thread.reasons.push(
            "La auditoría IA detallada sugiere un cambio que requiere confirmación manual."
                .to_string(),
        );
    } else {
        prepared.thread.manual_review_required = true;
        prepared
            .thread
            .reasons
            .push("La auditoría IA detallada requiere confirmación manual.".to_string());
    }
    usage
}

fn mark_detailed_audit_failed(thread: &mut EmailThread, _error: &anyhow::Error) {
    tracing::warn!(
        operation = "ai_detailed_audit",
        error_code = "ai_detailed_audit_failed",
        "la auditoría IA detallada falló"
    );
    thread.manual_review_required = true;
    thread.reasons.push(
        "La auditoría IA detallada no estuvo disponible; revisa este hilo manualmente.".to_string(),
    );
}

fn audit_messages_for_thread(
    thread: &EmailThread,
    messages: &[EmailMessage],
    max_audit_messages: usize,
    max_audit_body_chars: usize,
) -> Vec<EmailMessage> {
    // For the AI arbiter, breadth beats depth: send a compact view of as many messages
    // as possible (short body excerpts) so it can see who is involved and the gist of
    // each, instead of only a few full-body "milestones" the heuristic may have mis-picked.
    let max_audit_messages = max_audit_messages.max(1);
    let mut ordered = messages.to_vec();
    ordered.sort_by_key(|message| message.date);

    if ordered.len() > max_audit_messages {
        // Long thread: keep the messages that matter for validity/answered — all
        // external (client) messages, the heuristic milestones, and the latest few —
        // and drop internal back-and-forth from the middle.
        let len = ordered.len();
        let milestones: Vec<String> = [
            thread.first_client_message_id.clone(),
            thread.first_internal_reply_message_id.clone(),
            thread.last_internal_message_id.clone(),
        ]
        .into_iter()
        .flatten()
        .collect();
        let mut kept: Vec<EmailMessage> = ordered
            .iter()
            .enumerate()
            .filter(|(idx, message)| {
                message.is_external
                    || milestones.iter().any(|id| id == &message.id)
                    || *idx >= len.saturating_sub(4)
            })
            .map(|(_, message)| message.clone())
            .collect();
        kept.truncate(max_audit_messages);
        ordered = kept;
    }

    ordered
        .into_iter()
        .map(|mut message| {
            if let Some(text) = &message.body_text {
                message.body_text = Some(truncate_chars(text, max_audit_body_chars));
            }
            message
        })
        .collect()
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

async fn audit_thread(
    state: &AppState,
    thread: &EmailThread,
    messages: &[EmailMessage],
    policy_snapshot: Option<&crate::policies::PolicySnapshot>,
    gmail_labels: &[String],
) -> anyhow::Result<AiAuditResult> {
    #[derive(Serialize)]
    struct AuditRequest<'a> {
        thread: &'a EmailThread,
        messages: &'a [EmailMessage],
        gmail_labels: &'a [String],
        #[serde(skip_serializing_if = "Option::is_none")]
        policy_context: Option<AiWorkerPolicyContext<'a>>,
    }

    let policy_context = policy_snapshot.map(ai_worker_policy_context);

    let mut request = state
        .http
        .post(format!("{}/audit/thread", state.config.ai.worker_url))
        .json(&AuditRequest {
            thread,
            messages,
            gmail_labels,
            policy_context,
        });
    if let Some(request_id) = worker_request_id() {
        request = request.header("x-request-id", request_id);
    }
    if let Some(audience) = &state.config.ai.worker_audience {
        request = request.bearer_auth(fetch_cloud_run_identity_token(&state.http, audience).await?);
    }
    let response = request.send().await?.error_for_status()?.json().await?;
    Ok(response)
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

fn gmail_access_token(connection: Option<&GmailConnection>) -> Result<String, ApiError> {
    connection
        .filter(|connection| gmail_connection_is_active(Some(connection)))
        .map(|connection| connection.access_token_encrypted.clone())
        .ok_or_else(|| ApiError::forbidden("conecta Gmail antes de analizar"))
}

fn require_gmail_connected(connection: Option<&GmailConnection>) -> Result<(), ApiError> {
    if gmail_connection_is_active(connection) {
        Ok(())
    } else {
        Err(ApiError::forbidden("conecta Gmail antes de analizar"))
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
) -> Result<EntitlementSnapshot, ApiError> {
    if !state.config.billing.enforcement_enabled {
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
    let entitlement = entitlement_snapshot(state, &bundle.org.id).await?;
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
        analyzed_threads: 0,
        ai_audited_threads: 0,
        updated_at: Utc::now(),
    }
}

async fn enforce_usage_allows_run(state: &AppState, org_id: &str) -> Result<(), ApiError> {
    if !state.config.billing.enforcement_enabled {
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
    if usage.analyzed_threads >= plan.limits.analyzed_threads_per_month {
        return Err(ApiError::payment_required(
            "agotaste tu cupo mensual de hilos analizados — sube de plan para seguir",
        ));
    }
    Ok(())
}

async fn increment_runs_usage(state: &AppState, org_id: Option<&str>) -> Result<(), ApiError> {
    let Some(org_id) = org_id else {
        return Ok(());
    };
    let period_key = current_period_key();
    state
        .storage
        .add_usage(org_id, &period_key, 1, 0, 0)
        .await?;
    Ok(())
}

async fn add_analysis_usage(
    state: &AppState,
    org_id: Option<&str>,
    analyzed_threads: u32,
    ai_audited_threads: u32,
) -> anyhow::Result<()> {
    let Some(org_id) = org_id else {
        return Ok(());
    };
    let period_key = current_period_key();
    state
        .storage
        .add_usage(org_id, &period_key, 0, analyzed_threads, ai_audited_threads)
        .await
}

#[derive(Debug, Deserialize)]
struct MercadoPagoPreapprovalResponse {
    id: String,
    #[serde(default)]
    status: Option<String>,
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
    let mut auto_recurring = json!({
        "frequency": 1,
        "frequency_type": "months",
        "start_date": Utc::now().to_rfc3339(),
        "transaction_amount": checkout.amount_clp,
        "currency_id": "CLP"
    });
    if plan.trial_days > 0 {
        auto_recurring["free_trial"] = json!({
            "frequency": plan.trial_days,
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
    mercadopago_json(response, "get_preapproval").await
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

fn map_mercadopago_status(status: Option<&str>) -> SubscriptionStatus {
    match status.unwrap_or_default() {
        "authorized" => SubscriptionStatus::Active,
        "paused" => SubscriptionStatus::PastDue,
        "canceled" | "cancelled" => SubscriptionStatus::Cancelled,
        _ => SubscriptionStatus::Pending,
    }
}

fn verify_mercadopago_webhook(
    state: &AppState,
    headers: &HeaderMap,
    signed_data_id: Option<&str>,
) -> Result<(), ApiError> {
    let Some(expected) = &state.config.billing.mercadopago_webhook_secret else {
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
    validate_webhook_timestamp(timestamp, SystemTime::now())?;
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
    fn analyzed_run_cap_takes_min_of_per_run_and_monthly_remaining() {
        // Free: tope por run 40, cupo mensual 120 sin uso => 40.
        assert_eq!(analyzed_run_cap(40, 120, 0), 40);
        // Queda poco cupo mensual: el run se topa por el remanente.
        assert_eq!(analyzed_run_cap(40, 120, 100), 20);
        // Cupo agotado => 0.
        assert_eq!(analyzed_run_cap(40, 120, 120), 0);
        assert_eq!(analyzed_run_cap(40, 120, 999), 0);
        // Plan de pago (sin tope por run): manda el remanente mensual.
        assert_eq!(
            analyzed_run_cap(UNLIMITED_ANALYZED_PER_RUN, 7_500, 100),
            7_400
        );
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
    }

    #[test]
    fn analysis_failure_stage_is_safe_and_specific() {
        let staged = anyhow::anyhow!("provider body contains secret-token")
            .context(AnalysisFailureStage("gmail_list_threads"));

        assert_eq!(analysis_failure_stage(&staged), "gmail_list_threads");
        assert_eq!(
            analysis_failure_stage(&anyhow::anyhow!("provider body contains secret-token")),
            "unknown"
        );
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
        let mut message = message("message-1", "cliente@example.com");
        message.body_text = Some("contenido completo que no debe persistirse".to_string());

        let persisted = messages_for_persistence(&[message.clone()]);

        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].body_text, None);
        assert_eq!(persisted[0].id, message.id);
        assert_eq!(persisted[0].snippet, message.snippet);
    }

    #[test]
    fn detailed_ai_audit_limits_messages_and_body_length() {
        let thread = thread("thread-1", "run-1");
        let mut first = message("msg-a", "cliente@example.com");
        first.body_text = Some("abcdefgh".to_string());
        let mut second = message("msg-b", "cliente@example.com");
        second.body_text = Some("otro cuerpo completo".to_string());
        second.date = first.date + chrono::Duration::seconds(1);

        let selected = audit_messages_for_thread(&thread, &[first, second], 1, 5);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "msg-a");
        assert_eq!(selected[0].body_text.as_deref(), Some("abcde\n[truncado]"));
    }

    #[test]
    fn detailed_ai_failure_does_not_persist_provider_detail() {
        let mut failed = thread("thread-1", "run-1");
        let provider_error = anyhow::anyhow!("Bedrock response included secret-token");

        mark_detailed_audit_failed(&mut failed, &provider_error);

        assert!(failed.manual_review_required);
        assert!(
            failed
                .reasons
                .iter()
                .any(|reason| reason.contains("no estuvo disponible"))
        );
        assert!(
            failed
                .reasons
                .iter()
                .all(|reason| !reason.contains("secret-token"))
        );
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
                &[message("msg-a", "cliente@example.com")],
            )
            .await
            .unwrap();
        storage
            .upsert_thread(
                &thread("thread-bob", "run-bob"),
                &[message("msg-b", "cliente@example.com")],
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
        let now = Utc::now();
        EmailThread {
            id: id.to_string(),
            analysis_run_id: run_id.to_string(),
            gmail_thread_id: format!("gmail-{id}"),
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
    fn batch_only_escalates_disagreements_ambiguity_or_forced_threads() {
        let mut candidate = thread("batch", "run");
        candidate.manual_review_required = false;
        let mut decision = BatchAuditDecision {
            thread_id: candidate.gmail_thread_id.clone(),
            classification: Classification::ValidClientRequest,
            is_valid_client_request: true,
            is_answered: false,
            confidence: 0.95,
            manual_review_required: false,
            issues: vec![],
        };
        assert!(!batch_decision_requires_detailed(
            &candidate, &decision, 0.92, false
        ));

        decision.classification = Classification::Misc;
        decision.is_valid_client_request = false;
        assert!(batch_decision_requires_detailed(
            &candidate, &decision, 0.92, false
        ));

        decision.classification = Classification::Ambiguous;
        decision.manual_review_required = true;
        assert!(batch_decision_requires_detailed(
            &candidate, &decision, 0.92, false
        ));

        decision.classification = Classification::ValidClientRequest;
        decision.is_valid_client_request = true;
        decision.manual_review_required = false;
        assert!(batch_decision_requires_detailed(
            &candidate, &decision, 0.92, true
        ));
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
            gmail_message_id: format!("gmail-{id}"),
            from_email: from.to_string(),
            from_name: None,
            to_emails: vec!["help@example.com".to_string()],
            cc_emails: vec![],
            date: Utc::now(),
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
        // Modelo opt-out: la IA nace activa con el consentimiento sellado al provisionar.
        assert!(body.draft.ai_policy.enabled);
        assert!(body.draft.ai_policy.consent_granted_at.is_some());
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
            json!(["internal_domains", "valid_request_criteria"])
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
            .oneshot(request(
                Method::PUT,
                "/me/org/config",
                Some(&test.alice_cookie),
                Some(json!({
                    "finalize": true,
                    "analysis_policy": {
                        "valid_request_criteria": ["Clientes externos solicitan soporte"]
                    }
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["setup_state"]["ready_for_analysis"], true);
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
    async fn public_plans_expose_clp_prices_and_only_pro_trial() {
        let app = crate::build_app(test_app_config(), Arc::new(MemoryStorage::default()));
        let response = app
            .oneshot(request(Method::GET, "/public/plans", None, None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body.as_array().unwrap().len(), 3);
        assert_eq!(body[0]["id"], "inicial");
        assert_eq!(body[0]["clp_monthly"], 9990);
        assert_eq!(body[0]["trial_days"], 0);
        assert_eq!(body[1]["id"], "pro");
        assert_eq!(body[1]["clp_monthly"], 29990);
        assert_eq!(body[1]["trial_days"], 30);
        assert_eq!(body[2]["id"], "equipo");
        assert_eq!(body[2]["clp_monthly"], 99990);
        assert_eq!(body[2]["trial_days"], 0);
    }

    #[tokio::test]
    async fn logout_revokes_the_server_side_session() {
        let fixture = seeded_app().await;
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
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

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
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

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
    async fn disconnect_gmail_removes_tokens_from_all_owner_sessions() {
        let fixture = seeded_app().await;
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

        assert!(!gmail_connection_is_active(
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
        assert_eq!(body["auto_recurring"]["free_trial"]["frequency"], 30);
        assert_eq!(
            body["auto_recurring"]["free_trial"]["frequency_type"],
            "days"
        );
        assert_eq!(body["back_url"], "https://mira.example");
        assert!(body.get("init_point").is_none());
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
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .to_string();
        let manifest =
            format!("id:preapproval-123;request-id:2066ca19-c6f1-498a-be75-1923005edd06;ts:{ts};");
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(manifest.as_bytes());
        let signature = format!("ts={ts},v1={}", hex_lower(&mac.finalize().into_bytes()));

        assert!(
            validate_mercadopago_signature(&signature, request_id, Some(data_id), secret).is_ok()
        );
        assert!(validate_mercadopago_signature(secret, request_id, Some(data_id), secret).is_err());
    }

    #[test]
    fn webhook_signature_rejects_stale_timestamp() {
        let old_ts = "1742505638683";
        let now = UNIX_EPOCH + Duration::from_millis(1742505638683 + 301_000);

        assert!(validate_webhook_timestamp(old_ts, now).is_err());
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
        assert!(gmail_connection_is_active(Some(&connection)));
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
        assert!(!gmail_connection_is_active(Some(&connection)));
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
    fn checkout_blocks_active_subscription_but_allows_scheduled_cancel() {
        let now = Utc::now();
        let mut active = subscription("org-1", "owner@example.com");
        active.status = SubscriptionStatus::Active;
        active.cancel_at_period_end = false;

        let mut scheduled_cancel = active.clone();
        scheduled_cancel.cancel_at_period_end = true;

        assert!(checkout_blocked_by_active_subscription(Some(&active), now));
        assert!(!checkout_blocked_by_active_subscription(
            Some(&scheduled_cancel),
            now
        ));
        assert!(!checkout_blocked_by_active_subscription(None, now));
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

        let response = app
            .oneshot(request(
                Method::POST,
                "/analysis-runs",
                Some(&cookie),
                Some(json!({
                    "date_from": "2026-06-01",
                    "date_to": "2026-06-02",
                    "internal_domains": ["example.com"]
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
                        "date_to": day,
                        "internal_domains": ["example.com"]
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
        // Modelo opt-out: la org provisionada nace con IA activa por defecto.
        assert_eq!(body["privacy"]["ai_enabled"], true);
        assert_eq!(body["privacy"]["retention_days"], 30);
        assert_eq!(body["stored_data"]["analysis_runs_count"], 1);
        assert_eq!(body["stored_data"]["threads_count"], 1);
        assert_eq!(body["stored_data"]["messages_count"], 1);
        assert_eq!(body["actions"]["delete_analysis_data"]["available"], true);
        assert_eq!(body["actions"]["disconnect_gmail"]["available"], false);
        assert_eq!(
            body["actions"]["disconnect_gmail"]["reason"],
            "available_in_account"
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
        assert_eq!(body["scheduler"]["preset"], "weekdays_08_local");
        assert_eq!(body["policy"]["setup_ready"], true);
    }

    #[tokio::test]
    async fn create_analysis_run_is_rate_limited_per_user() {
        let mut config = test_app_config();
        config.rate_limit.analysis_create_per_hour = 1;
        let test = seeded_app_with_config(config).await;
        let payload = json!({
            "date_from": "2026-06-01",
            "date_to": "2026-06-02",
            "internal_domains": ["example.com"]
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
            "date_to": "2026-06-02",
            "internal_domains": ["example.com"]
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
        // La org nace con IA activa; primero la desactivamos para forzar una transición
        // off→on, que es la que dispara la compuerta de consentimiento.
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

        // Modelo opt-out: desactivar la IA proactivamente siempre se permite.
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
        // v1 inicial → v2 (opt-out) → v3 (re-activación + setup).
        assert_eq!(body["policy_version"]["version"], 3);
        assert_eq!(body["setup_state"]["ready_for_analysis"], true);
    }

    #[tokio::test]
    async fn policy_run_uses_snapshot_and_keeps_legacy_compatibility() {
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
        assert_eq!(response.status(), StatusCode::OK);
        let legacy: AnalysisRun = response_json(response).await;
        assert!(legacy.org_id.is_none());
        assert!(legacy.policy_snapshot.is_none());
        assert_eq!(legacy.config.internal_domains, vec!["legacy.test"]);
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
            .upsert_thread(&original, &[message("msg-a", "cliente@example.com")])
            .await
            .unwrap();

        let mut template = original;
        template.classification = Classification::Ambiguous;
        template.is_valid_client_request = false;
        template.manual_review_required = true;

        for index in 0..4 {
            let mut thread = template.clone();
            thread.id = format!("ambiguous-{index}");
            thread.gmail_thread_id = format!("gmail-ambiguous-{index}");
            thread.first_client_message_id = Some(format!("message-{index}"));
            test.storage
                .upsert_thread(
                    &thread,
                    &[message(&format!("message-{index}"), "cliente@example.com")],
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
            .upsert_thread(&duplicate, &[message("msg-new", "cliente@example.com")])
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
}
