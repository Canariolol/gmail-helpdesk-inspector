mod internal;

use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{
        IntoResponse, Redirect, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, patch, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use futures_util::{StreamExt, stream};
use rand::RngCore;
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    analysis::{
        AiAuditResult, AnalysisConfig, AnalysisMetrics, AnalysisRun, AnalysisStatus,
        ClassificationSource, EmailMessage, EmailThread, ManualReview, TriggerType,
        calculate_metrics, classify_thread, message_is_inside_analysis_window,
        should_auto_apply_ai,
    },
    auth::{
        GoogleTokenResponse, UserSession, clear_oauth_cookie, clear_session_cookie, decrypt_token,
        encrypt_token, oauth_cookie, session_cookie, sign_session_id, verify_session_cookie,
    },
    config::AppConfig,
    gmail::GmailClient,
    policies::{
        AiPolicy, AnalysisPolicy, MailboxPurpose, OrgConfigBundle, OrgConfigResponse,
        PolicyVersion, ScheduleReportPolicy, email_domain, is_consumer_gmail_domain,
        normalize_domains, normalize_list, policy_version_from_draft, provision_default_config,
        retention_expires_at, setup_state, validate_timezone,
    },
    storage::StorageRepository,
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
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/auth/google/login", get(auth_google_login))
        .route("/auth/google/callback", get(auth_google_callback))
        .route("/auth/logout", post(auth_logout))
        .route("/auth/me", get(auth_me))
        .route("/me/org/config", get(get_org_config).put(update_org_config))
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
        .route("/threads/{thread_id}", get(get_thread))
        .route("/threads/{thread_id}/manual-review", patch(manual_review))
        .route(
            "/internal/scheduled-analysis",
            post(internal::scheduled_analysis),
        )
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true }))
}

async fn auth_google_login(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
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

async fn auth_google_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OAuthCallback>,
) -> Result<impl IntoResponse, ApiError> {
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
    let session = UserSession {
        id: Uuid::new_v4().to_string(),
        google_account_email: profile.email_address,
        access_token_encrypted: encrypt_token(&token.access_token, &state.config.encryption_key)?,
        refresh_token_encrypted: token
            .refresh_token
            .as_deref()
            .map(|refresh| encrypt_token(refresh, &state.config.encryption_key))
            .transpose()?,
        created_at: now,
        updated_at: now,
    };
    state.storage.upsert_user_session(&session).await?;

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

async fn auth_logout(State(state): State<AppState>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&clear_session_cookie(
            &state.config.cookie_same_site,
            state.config.cookie_secure,
        ))
        .unwrap(),
    );
    (StatusCode::NO_CONTENT, headers)
}

async fn auth_me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = require_session(&state, &headers).await?;
    Ok(Json(json!({
        "email": session.google_account_email,
    })))
}

async fn get_org_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OrgConfigResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let bundle = get_or_provision_org_config(&state, &session.google_account_email).await?;
    Ok(Json(bundle.response()))
}

#[derive(Debug, Deserialize, Default)]
struct OrgConfigUpdateRequest {
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
    apply_org_config_update(&mut bundle, request, &session.google_account_email)?;
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
    Ok(Json(UpdateOrgConfigResponse {
        policy_version: bundle.policy_version,
        setup_state: setup_state(&bundle.draft),
    }))
}

fn apply_org_config_update(
    bundle: &mut OrgConfigBundle,
    request: OrgConfigUpdateRequest,
    user_email: &str,
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
        apply_ai_policy_update(&mut bundle.draft.ai_policy, policy)?;
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
    let Some(domain) = email_domain(user_email) else {
        return Err(ApiError::bad_request("email de cuenta Google inválido"));
    };
    if is_consumer_gmail_domain(&domain) {
        return Err(ApiError::bad_request(
            "la beta privada acepta solo cuentas Google Workspace",
        ));
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
        if enabled && !current.enabled && update.consent_confirmed != Some(true) {
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
    if let Some(bundle) = state.storage.get_org_config_for_user(user_email).await? {
        return Ok(bundle);
    }
    let bundle = provision_default_config(user_email, Utc::now());
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
    policy_version_id: Option<String>,
}

async fn create_analysis_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateAnalysisRunRequest>,
) -> Result<Json<AnalysisRun>, ApiError> {
    let session = require_session(&state, &headers).await?;
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
    if !current_setup.ready_for_analysis {
        return Err(ApiError::bad_request(
            "completa la configuración antes de crear análisis desde policy",
        ));
    }
    let snapshot = policy_version.snapshot.clone();
    let analysis = &snapshot.analysis_policy;
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
) -> Result<Json<Vec<AnalysisRun>>, ApiError> {
    let session = require_session(&state, &headers).await?;
    Ok(Json(
        state
            .storage
            .list_analysis_runs(&session.google_account_email)
            .await?,
    ))
}

async fn get_analysis_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<AnalysisRun>, ApiError> {
    let session = require_session(&state, &headers).await?;
    Ok(Json(require_owned_run(&state, &id, &session).await?))
}

async fn start_analysis_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<AnalysisRun>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let mut run = require_owned_run(&state, &id, &session).await?;
    run.status = AnalysisStatus::Running;
    run.progress_message = "Iniciando lectura de Gmail".to_string();
    state.storage.update_analysis_run(&run).await?;

    let access_token = decrypt_token(
        &session.access_token_encrypted,
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
    require_owned_run(&state, &id, &session).await?;
    let stream = stream::unfold((), move |_| {
        let state = state.clone();
        let id = id.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let payload = match state.storage.get_analysis_run(&id).await {
                Ok(Some(run)) => serde_json::to_string(&run).unwrap_or_else(|_| "{}".to_string()),
                Ok(None) => json!({"error": "not_found"}).to_string(),
                Err(error) => json!({"error": error.to_string()}).to_string(),
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
    let run = require_owned_run(&state, &id, &session).await?;
    Ok(Json(run.metrics))
}

async fn list_threads(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<ThreadQuery>,
) -> Result<Json<Vec<EmailThread>>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let run = require_owned_run(&state, &id, &session).await?;
    let mut threads = state.storage.list_threads(&run.id).await?;
    if let Some(classification) = query.classification {
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
    Ok(Json(threads))
}

#[derive(Debug, Deserialize)]
struct ThreadQuery {
    classification: Option<String>,
    answered: Option<bool>,
    manual_review_required: Option<bool>,
}

async fn get_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<String>,
) -> Result<Json<ThreadDetailResponse>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let mut thread = require_owned_thread(&state, &thread_id, &session).await?;
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
    is_valid_client_request: bool,
    is_answered: bool,
    first_client_message_id: Option<String>,
    first_internal_reply_message_id: Option<String>,
    last_internal_message_id: Option<String>,
    notes: Option<String>,
}

async fn manual_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<String>,
    Json(request): Json<ManualReviewRequest>,
) -> Result<Json<AnalysisRun>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let thread = require_owned_thread(&state, &thread_id, &session).await?;
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
    let review = ManualReview {
        id: Uuid::new_v4().to_string(),
        email_thread_id: thread.id.clone(),
        reviewer_label: session.google_account_email,
        new_classification: request.new_classification,
        is_valid_client_request: request.is_valid_client_request,
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
    recalculate_run_metrics(&state, &thread.analysis_run_id).await?;
    let run = state
        .storage
        .get_analysis_run(&thread.analysis_run_id)
        .await?
        .ok_or(ApiError::not_found("analysis run not found"))?;
    Ok(Json(run))
}

pub(crate) async fn mark_run_failed(state: &AppState, run_id: &str, error: &anyhow::Error) {
    tracing::error!(?error, run_id, "analysis failed");
    if let Ok(Some(mut failed)) = state.storage.get_analysis_run(run_id).await {
        failed.status = AnalysisStatus::Failed;
        failed.error_message = Some(error.to_string());
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
        .await?
        .ok_or_else(|| anyhow::anyhow!("analysis run not found"))?;
    let max_threads = run
        .policy_snapshot
        .as_ref()
        .map(|snapshot| snapshot.analysis_policy.max_threads_per_run)
        .unwrap_or(state.config.google.gmail_max_threads);
    let thread_ids = state
        .gmail
        .list_thread_ids(&access_token, &run.config, max_threads)
        .await?;
    run.total_candidate_threads = thread_ids.len() as u64;
    run.progress_message = format!("{} hilos encontrados en Gmail", thread_ids.len());
    state.storage.update_analysis_run(&run).await?;

    let mut ai_input_tokens = 0u64;
    let mut ai_output_tokens = 0u64;
    let excluded_sample = excluded_sample_ids(&thread_ids);

    // Process threads with bounded concurrency: each thread's Gmail fetch, local
    // classification, AI audit and storage writes are independent, so we run up to
    // ANALYSIS_CONCURRENCY of them at once instead of strictly one-at-a-time.
    let config = run.config.clone();
    let policy_snapshot = run.policy_snapshot.clone();
    let policy_version_id = run.policy_version_id.clone();
    let run_id = run.id.clone();
    let total = run.total_candidate_threads;
    {
        let state_ref = &state;
        let access_ref = access_token.as_str();
        let config_ref = &config;
        let policy_ref = policy_snapshot.as_ref();
        let policy_version_id_ref = policy_version_id.as_deref();
        let excluded_ref = &excluded_sample;
        let run_id_ref = run_id.as_str();
        let mut task_stream = stream::iter(thread_ids.into_iter().map(|thread_id| async move {
            process_one_thread(
                state_ref,
                access_ref,
                run_id_ref,
                config_ref,
                ThreadProcessingContext {
                    excluded_sample: excluded_ref,
                    policy_snapshot: policy_ref,
                    policy_version_id: policy_version_id_ref,
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
                    ai_input_tokens += outcome.input_tokens;
                    ai_output_tokens += outcome.output_tokens;
                }
                Err(error) => {
                    tracing::warn!(?error, "el procesamiento de un hilo falló; se omite");
                }
            }
            if processed.is_multiple_of(PROGRESS_UPDATE_EVERY) {
                run.processed_threads = processed;
                run.metrics.ai_input_tokens = ai_input_tokens;
                run.metrics.ai_output_tokens = ai_output_tokens;
                run.progress_message = format!(
                    "Procesados {}/{} hilos · IA entrada {} · salida {} tokens",
                    processed, total, ai_input_tokens, ai_output_tokens
                );
                state.storage.update_analysis_run(&run).await?;
            }
        }
        run.processed_threads = processed;
    }

    let threads = state.storage.list_threads(&run.id).await?;
    run.metrics = calculate_metrics(&threads, ai_input_tokens, ai_output_tokens);
    run.status = AnalysisStatus::Completed;
    run.progress_message = "Análisis completado".to_string();
    run.completed_at = Some(Utc::now());
    state.storage.update_analysis_run(&run).await?;
    Ok(())
}

/// Maximum number of threads processed concurrently in a single analysis run.
const ANALYSIS_CONCURRENCY: usize = 5;
/// Write run progress to storage every N processed threads (instead of every one).
const PROGRESS_UPDATE_EVERY: u64 = 5;

#[derive(Default)]
struct ThreadOutcome {
    input_tokens: u64,
    output_tokens: u64,
}

struct ThreadProcessingContext<'a> {
    excluded_sample: &'a [String],
    policy_snapshot: Option<&'a crate::policies::PolicySnapshot>,
    policy_version_id: Option<&'a str>,
}

async fn process_one_thread(
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
        return Ok(ThreadOutcome::default());
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
        return Ok(ThreadOutcome::default());
    }

    let mut thread = classify_thread(run_id, &data.id, &data.messages, config);
    let received_human_thread = thread.first_client_message_id.is_some();
    let ai_enabled = processing
        .policy_snapshot
        .map(|snapshot| snapshot.ai_policy.enabled)
        .unwrap_or(true);
    let should_audit = ai_enabled
        && received_human_thread
        && (thread.is_valid_client_request
            || thread.manual_review_required
            || processing.excluded_sample.contains(&thread.gmail_thread_id));

    let mut outcome = ThreadOutcome::default();
    if should_audit {
        let (max_messages, max_body_chars, auto_apply_threshold) = processing
            .policy_snapshot
            .map(|snapshot| {
                (
                    snapshot.ai_policy.max_audit_messages as usize,
                    snapshot.ai_policy.max_body_chars_per_message as usize,
                    snapshot.ai_policy.auto_apply_threshold,
                )
            })
            .unwrap_or((14, 280, state.config.ai.apply_confidence_threshold));
        let audit_messages =
            audit_messages_for_thread(&thread, &data.messages, max_messages, max_body_chars);
        match audit_thread(state, &thread, &audit_messages).await {
            Ok(mut audit) => {
                audit.policy_version_id = processing.policy_version_id.map(ToOwned::to_owned);
                if let Some(snapshot) = processing.policy_snapshot {
                    audit.prompt_version = Some(snapshot.ai_policy.prompt_version.clone());
                    audit.model_id = Some(snapshot.ai_policy.model_id.clone());
                    audit.auto_apply_threshold = Some(auto_apply_threshold);
                    audit.max_audit_messages = Some(max_messages as u32);
                    audit.max_body_chars_per_message = Some(max_body_chars as u32);
                }
                outcome.input_tokens = audit.input_tokens;
                outcome.output_tokens = audit.output_tokens;
                state
                    .storage
                    .add_ai_audit(run_id, &thread.id, &audit)
                    .await?;
                let known_ids = data
                    .messages
                    .iter()
                    .map(|m| m.id.clone())
                    .collect::<Vec<_>>();
                if should_auto_apply_ai(&audit, &known_ids)
                    && audit.confidence >= auto_apply_threshold
                {
                    thread.classification = audit.classification.clone();
                    thread.classification_source = ClassificationSource::Ai;
                    thread.classification_confidence = audit.confidence;
                    thread.is_valid_client_request = audit.is_valid_client_request;
                    thread.is_answered = audit.is_answered;
                    thread.first_client_message_id = audit.first_client_message_id.clone();
                    thread.first_internal_reply_message_id =
                        audit.first_internal_reply_message_id.clone();
                    thread.last_internal_message_id = audit.last_internal_message_id.clone();
                    apply_trace_dates(&mut thread, &data.messages);
                    thread.manual_review_required = false;
                    thread.reasons.push(
                        "La auditoría IA se aplicó automáticamente por alta confianza.".to_string(),
                    );
                } else {
                    thread.manual_review_required = true;
                    thread
                        .reasons
                        .push("La auditoría IA requiere confirmación manual.".to_string());
                }
            }
            Err(error) => {
                thread.manual_review_required = true;
                thread
                    .reasons
                    .push(format!("La auditoría IA falló: {error}"));
            }
        }
    }

    let sanitized = data
        .messages
        .iter()
        .cloned()
        .map(|mut message| {
            message.body_text = None;
            message
        })
        .collect::<Vec<_>>();
    state.storage.upsert_thread(&thread, &sanitized).await?;
    Ok(outcome)
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
) -> anyhow::Result<AiAuditResult> {
    #[derive(Serialize)]
    struct AuditRequest<'a> {
        thread: &'a EmailThread,
        messages: &'a [EmailMessage],
    }

    let mut request = state
        .http
        .post(format!("{}/audit/thread", state.config.ai.worker_url))
        .json(&AuditRequest { thread, messages });
    if let Some(audience) = &state.config.ai.worker_audience {
        request = request.bearer_auth(fetch_cloud_run_identity_token(&state.http, audience).await?);
    }
    let response = request.send().await?.error_for_status()?.json().await?;
    Ok(response)
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
    run.metrics = calculate_metrics(
        &threads,
        run.metrics.ai_input_tokens,
        run.metrics.ai_output_tokens,
    );
    state.storage.update_analysis_run(&run).await
}

fn excluded_sample_ids(ids: &[String]) -> Vec<String> {
    ids.iter()
        .enumerate()
        .filter(|(index, _)| index % 10 == 0)
        .map(|(_, id)| id.clone())
        .collect()
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
    thread_id: &str,
    session: &UserSession,
) -> Result<EmailThread, ApiError> {
    let thread = state
        .storage
        .get_thread(thread_id)
        .await?
        .ok_or(ApiError::not_found("thread not found"))?;
    require_owned_run(state, &thread.analysis_run_id, session)
        .await
        .map_err(|_| ApiError::not_found("thread not found"))?;
    Ok(thread)
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
        tracing::warn!(session_id, "auth rejected: session not found in storage");
    }
    session.ok_or(ApiError::unauthorized())
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
}

impl ApiError {
    fn not_found(message: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.to_string(),
        }
    }

    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: "Autenticación requerida".to_string(),
        }
    }

    fn bad_request(message: &str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(error: reqwest::Error) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: error.to_string(),
        }
    }
}

trait GoogleResponseExt {
    async fn json_or_google_error<T: DeserializeOwned>(self, label: &str) -> anyhow::Result<T>;
}

impl GoogleResponseExt for reqwest::Response {
    async fn json_or_google_error<T: DeserializeOwned>(self, label: &str) -> anyhow::Result<T> {
        let status = self.status();
        let text = self.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow::anyhow!("{label} failed with {status}: {text}"));
        }
        serde_json::from_str(&text).map_err(|error| {
            anyhow::anyhow!("{label} returned invalid JSON: {error}; body: {text}")
        })
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
        config::test_app_config,
        policies::OrgConfigResponse,
        storage::MemoryStorage,
    };

    struct TestApp {
        app: axum::Router,
        alice_cookie: String,
        bob_cookie: String,
    }

    async fn seeded_app() -> TestApp {
        let config = test_app_config();
        let storage = MemoryStorage::default();
        storage
            .upsert_user_session(&session("alice-session", "alice@example.com"))
            .await
            .unwrap();
        storage
            .upsert_user_session(&session("bob-session", "bob@example.com"))
            .await
            .unwrap();
        let alice_run = run("run-alice", "alice@example.com");
        let bob_run = run("run-bob", "bob@example.com");
        storage.create_analysis_run(&alice_run).await.unwrap();
        storage.create_analysis_run(&bob_run).await.unwrap();
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
            app: crate::build_app(config, Arc::new(storage)),
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

    fn session(id: &str, email: &str) -> UserSession {
        let now = Utc::now();
        UserSession {
            id: id.to_string(),
            google_account_email: email.to_string(),
            access_token_encrypted: "access".to_string(),
            refresh_token_encrypted: Some("refresh".to_string()),
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
            created_at: now,
            updated_at: now,
        }
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
        assert!(!body.draft.ai_policy.enabled);
        assert_eq!(body.draft.retention_policy.retention_days, 30);
        assert_eq!(body.setup_state.missing, vec!["valid_request_criteria"]);
    }

    #[tokio::test]
    async fn org_config_requires_ai_consent_and_updates_policy_version() {
        let test = seeded_app().await;
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
        assert_eq!(body["policy_version"]["version"], 2);
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
            .oneshot(request(
                Method::PATCH,
                "/threads/thread-alice/manual-review",
                Some(&test.alice_cookie),
                Some(json!({
                    "reviewer_label": "spoofed-admin@example.com",
                    "new_classification": "misc",
                    "is_valid_client_request": false,
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
    }
}
