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
use chrono::Utc;
use futures_util::stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    analysis::{
        AiAuditResult, AnalysisConfig, AnalysisMetrics, AnalysisRun, AnalysisStatus, ClassificationSource,
        EmailMessage, EmailThread, ManualReview, calculate_metrics, classify_thread, should_auto_apply_ai,
    },
    auth::{
        GoogleTokenResponse, GoogleUserInfo, UserSession, clear_session_cookie, decrypt_token, encrypt_token,
        session_cookie, sign_session_id, verify_session_cookie,
    },
    config::AppConfig,
    gmail::GmailClient,
    storage::StorageRepository,
};

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
            http: Client::new(),
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
        .route("/analysis-runs", post(create_analysis_run).get(list_analysis_runs))
        .route("/analysis-runs/{id}", get(get_analysis_run))
        .route("/analysis-runs/{id}/start", post(start_analysis_run))
        .route("/analysis-runs/{id}/status", get(get_analysis_run))
        .route("/analysis-runs/{id}/events", get(analysis_events))
        .route("/analysis-runs/{id}/metrics", get(get_metrics))
        .route("/analysis-runs/{id}/threads", get(list_threads))
        .route("/threads/{thread_id}", get(get_thread))
        .route("/threads/{thread_id}/manual-review", patch(manual_review))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true }))
}

async fn auth_google_login(State(state): State<AppState>) -> impl IntoResponse {
    let scope = "https://www.googleapis.com/auth/gmail.readonly";
    let url = url::Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/v2/auth",
        &[
            ("client_id", state.config.google.client_id.as_str()),
            ("redirect_uri", state.config.google.redirect_url.as_str()),
            ("response_type", "code"),
            ("scope", scope),
            ("access_type", "offline"),
            ("prompt", "consent"),
        ],
    )
    .expect("valid oauth url");
    Redirect::temporary(url.as_str())
}

#[derive(Debug, Deserialize)]
struct OAuthCallback {
    code: String,
}

async fn auth_google_callback(
    State(state): State<AppState>,
    Query(query): Query<OAuthCallback>,
) -> Result<impl IntoResponse, ApiError> {
    let token: GoogleTokenResponse = state
        .http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", query.code.as_str()),
            ("client_id", state.config.google.client_id.as_str()),
            ("client_secret", state.config.google.client_secret.as_str()),
            ("redirect_uri", state.config.google.redirect_url.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let user: GoogleUserInfo = state
        .http
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(&token.access_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let now = Utc::now();
    let session = UserSession {
        id: Uuid::new_v4().to_string(),
        google_account_email: user.email,
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
    headers.insert(header::SET_COOKIE, HeaderValue::from_str(&session_cookie(&signed)).unwrap());
    headers.insert(header::LOCATION, HeaderValue::from_str(&state.config.web_base_url).unwrap());
    Ok((StatusCode::FOUND, headers))
}

async fn auth_logout() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, HeaderValue::from_str(&clear_session_cookie()).unwrap());
    (StatusCode::NO_CONTENT, headers)
}

async fn auth_me(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<serde_json::Value>, ApiError> {
    let session = require_session(&state, &headers).await?;
    Ok(Json(json!({
        "email": session.google_account_email,
    })))
}

#[derive(Debug, Deserialize)]
struct CreateAnalysisRunRequest {
    date_from: String,
    date_to: String,
    internal_domains: Vec<String>,
    ignored_senders: Vec<String>,
    ignored_domains: Vec<String>,
    ignored_keywords: Vec<String>,
}

async fn create_analysis_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateAnalysisRunRequest>,
) -> Result<Json<AnalysisRun>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let run = AnalysisRun {
        id: Uuid::new_v4().to_string(),
        user_email: session.google_account_email,
        config: AnalysisConfig {
            date_from: request.date_from,
            date_to: request.date_to,
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
        created_at: Utc::now(),
        completed_at: None,
        error_message: None,
    };
    state.storage.create_analysis_run(&run).await?;
    Ok(Json(run))
}

async fn list_analysis_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AnalysisRun>>, ApiError> {
    let session = require_session(&state, &headers).await?;
    Ok(Json(state.storage.list_analysis_runs(&session.google_account_email).await?))
}

async fn get_analysis_run(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<AnalysisRun>, ApiError> {
    Ok(Json(
        state
            .storage
            .get_analysis_run(&id)
            .await?
            .ok_or(ApiError::not_found("analysis run not found"))?,
    ))
}

async fn start_analysis_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<AnalysisRun>, ApiError> {
    let session = require_session(&state, &headers).await?;
    let mut run = state
        .storage
        .get_analysis_run(&id)
        .await?
        .ok_or(ApiError::not_found("analysis run not found"))?;
    if run.user_email != session.google_account_email {
        return Err(ApiError::forbidden());
    }
    run.status = AnalysisStatus::Running;
    run.progress_message = "Iniciando lectura de Gmail".to_string();
    state.storage.update_analysis_run(&run).await?;

    let access_token = decrypt_token(&session.access_token_encrypted, &state.config.encryption_key)?;
    let worker_state = state.clone();
    let run_id = run.id.clone();
    tokio::spawn(async move {
        if let Err(error) = execute_analysis(worker_state.clone(), run_id.clone(), access_token).await {
            tracing::error!(?error, "analysis failed");
            if let Ok(Some(mut failed)) = worker_state.storage.get_analysis_run(&run_id).await {
                failed.status = AnalysisStatus::Failed;
                failed.error_message = Some(error.to_string());
                failed.progress_message = "El análisis falló".to_string();
                let _ = worker_state.storage.update_analysis_run(&failed).await;
            }
        }
    });

    Ok(Json(run))
}

async fn analysis_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
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
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn get_metrics(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<AnalysisMetrics>, ApiError> {
    let run = state
        .storage
        .get_analysis_run(&id)
        .await?
        .ok_or(ApiError::not_found("analysis run not found"))?;
    Ok(Json(run.metrics))
}

async fn list_threads(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ThreadQuery>,
) -> Result<Json<Vec<EmailThread>>, ApiError> {
    let mut threads = state.storage.list_threads(&id).await?;
    if let Some(classification) = query.classification {
        threads.retain(|thread| serde_json::to_value(&thread.classification).ok() == Some(json!(classification)));
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
    Path(thread_id): Path<String>,
) -> Result<Json<ThreadDetailResponse>, ApiError> {
    let thread = state
        .storage
        .get_thread(&thread_id)
        .await?
        .ok_or(ApiError::not_found("thread not found"))?;
    let messages = state.storage.list_messages(&thread.analysis_run_id, &thread.id).await?;
    Ok(Json(ThreadDetailResponse { thread, messages }))
}

#[derive(Debug, Serialize)]
struct ThreadDetailResponse {
    thread: EmailThread,
    messages: Vec<EmailMessage>,
}

#[derive(Debug, Deserialize)]
struct ManualReviewRequest {
    reviewer_label: String,
    new_classification: crate::analysis::Classification,
    is_valid_client_request: bool,
    is_answered: bool,
    first_client_message_id: Option<String>,
    first_internal_reply_message_id: Option<String>,
    notes: Option<String>,
}

async fn manual_review(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
    Json(request): Json<ManualReviewRequest>,
) -> Result<Json<AnalysisRun>, ApiError> {
    let thread = state
        .storage
        .get_thread(&thread_id)
        .await?
        .ok_or(ApiError::not_found("thread not found"))?;
    let review = ManualReview {
        id: Uuid::new_v4().to_string(),
        email_thread_id: thread.id.clone(),
        reviewer_label: request.reviewer_label,
        new_classification: request.new_classification,
        is_valid_client_request: request.is_valid_client_request,
        is_answered: request.is_answered,
        first_client_message_id: request.first_client_message_id,
        first_internal_reply_message_id: request.first_internal_reply_message_id,
        notes: request.notes,
        created_at: Utc::now(),
    };
    state.storage.add_manual_review(&thread.analysis_run_id, &review).await?;
    recalculate_run_metrics(&state, &thread.analysis_run_id).await?;
    let run = state
        .storage
        .get_analysis_run(&thread.analysis_run_id)
        .await?
        .ok_or(ApiError::not_found("analysis run not found"))?;
    Ok(Json(run))
}

async fn execute_analysis(state: AppState, run_id: String, access_token: String) -> anyhow::Result<()> {
    let mut run = state
        .storage
        .get_analysis_run(&run_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("analysis run not found"))?;
    let thread_ids = state
        .gmail
        .list_thread_ids(&access_token, &run.config, state.config.google.gmail_max_threads)
        .await?;
    run.total_candidate_threads = thread_ids.len() as u64;
    run.progress_message = format!("{} hilos encontrados en Gmail", thread_ids.len());
    state.storage.update_analysis_run(&run).await?;

    let mut ai_input_tokens = 0;
    let mut ai_output_tokens = 0;
    let excluded_sample = excluded_sample_ids(&thread_ids);

    for thread_id in thread_ids {
        let data = state.gmail.fetch_thread(&access_token, &thread_id, &run.config).await?;
        let mut thread = classify_thread(&run.id, &data.id, &data.messages, &run.config);
        let should_audit = thread.is_valid_client_request
            || thread.manual_review_required
            || excluded_sample.contains(&thread.gmail_thread_id);

        if should_audit {
            match audit_thread(&state, &thread, &data.messages).await {
                Ok(audit) => {
                    ai_input_tokens += audit.input_tokens;
                    ai_output_tokens += audit.output_tokens;
                    state.storage.add_ai_audit(&run.id, &thread.id, &audit).await?;
                    let known_ids = data.messages.iter().map(|m| m.id.clone()).collect::<Vec<_>>();
                    if should_auto_apply_ai(&audit, &known_ids)
                        && audit.confidence >= state.config.ai.apply_confidence_threshold
                    {
                        thread.classification = audit.classification.clone();
                        thread.classification_source = ClassificationSource::Ai;
                        thread.classification_confidence = audit.confidence;
                        thread.is_valid_client_request = audit.is_valid_client_request;
                        thread.is_answered = audit.is_answered;
                        thread.first_client_message_id = audit.first_client_message_id.clone();
                        thread.first_internal_reply_message_id = audit.first_internal_reply_message_id.clone();
                        thread.manual_review_required = false;
                        thread.reasons.push("AI audit auto-applied above strict threshold".to_string());
                    } else {
                        thread.manual_review_required = true;
                        thread.reasons.push("AI audit requires manual confirmation".to_string());
                    }
                }
                Err(error) => {
                    thread.manual_review_required = true;
                    thread.reasons.push(format!("AI audit failed: {error}"));
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

        run.processed_threads += 1;
        run.metrics.ai_input_tokens = ai_input_tokens;
        run.metrics.ai_output_tokens = ai_output_tokens;
        run.progress_message = format!(
            "Procesados {}/{} hilos · IA entrada {} · salida {} tokens",
            run.processed_threads, run.total_candidate_threads, ai_input_tokens, ai_output_tokens
        );
        state.storage.update_analysis_run(&run).await?;
    }

    let threads = state.storage.list_threads(&run.id).await?;
    run.metrics = calculate_metrics(&threads, ai_input_tokens, ai_output_tokens);
    run.status = AnalysisStatus::Completed;
    run.progress_message = "Análisis completado".to_string();
    run.completed_at = Some(Utc::now());
    state.storage.update_analysis_run(&run).await?;
    Ok(())
}

async fn audit_thread(state: &AppState, thread: &EmailThread, messages: &[EmailMessage]) -> anyhow::Result<AiAuditResult> {
    #[derive(Serialize)]
    struct AuditRequest<'a> {
        thread: &'a EmailThread,
        messages: &'a [EmailMessage],
    }

    let response = state
        .http
        .post(format!("{}/audit/thread", state.config.ai.worker_url))
        .json(&AuditRequest { thread, messages })
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response)
}

async fn recalculate_run_metrics(state: &AppState, run_id: &str) -> anyhow::Result<()> {
    let mut run = state
        .storage
        .get_analysis_run(run_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("analysis run not found"))?;
    let threads = state.storage.list_threads(run_id).await?;
    run.metrics = calculate_metrics(&threads, run.metrics.ai_input_tokens, run.metrics.ai_output_tokens);
    state.storage.update_analysis_run(&run).await
}

fn excluded_sample_ids(ids: &[String]) -> Vec<String> {
    ids.iter()
        .enumerate()
        .filter(|(index, _)| index % 10 == 0)
        .map(|(_, id)| id.clone())
        .collect()
}

async fn require_session(state: &AppState, headers: &HeaderMap) -> Result<UserSession, ApiError> {
    let cookie = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(extract_session_cookie)
        .ok_or(ApiError::unauthorized())?;
    let session_id = verify_session_cookie(&cookie, &state.config.session_secret).ok_or(ApiError::unauthorized())?;
    state
        .storage
        .get_user_session(&session_id)
        .await?
        .ok_or(ApiError::unauthorized())
}

fn extract_session_cookie(cookies: &str) -> Option<String> {
    cookies.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == "ghmi_session").then(|| value.to_string())
    })
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
            message: "authentication required".to_string(),
        }
    }

    fn forbidden() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: "forbidden".to_string(),
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
