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
use futures_util::stream;
use rand::RngCore;
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    analysis::{
        AiAuditResult, AnalysisConfig, AnalysisMetrics, AnalysisRun, AnalysisStatus,
        ClassificationSource, EmailMessage, EmailThread, ManualReview, calculate_metrics,
        classify_thread, message_is_inside_analysis_window, should_auto_apply_ai,
    },
    auth::{
        GoogleTokenResponse, UserSession, clear_oauth_cookie, clear_session_cookie, decrypt_token,
        encrypt_token, oauth_cookie, session_cookie, sign_session_id, verify_session_cookie,
    },
    config::AppConfig,
    gmail::GmailClient,
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

#[derive(Debug, Deserialize)]
struct CreateAnalysisRunRequest {
    date_from: String,
    date_to: String,
    time_from: Option<String>,
    time_to: Option<String>,
    timezone: Option<String>,
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
    Ok(Json(
        state
            .storage
            .list_analysis_runs(&session.google_account_email)
            .await?,
    ))
}

async fn get_analysis_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AnalysisRun>, ApiError> {
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

async fn get_metrics(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AnalysisMetrics>, ApiError> {
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
    Path(thread_id): Path<String>,
) -> Result<Json<ThreadDetailResponse>, ApiError> {
    let mut thread = state
        .storage
        .get_thread(&thread_id)
        .await?
        .ok_or(ApiError::not_found("thread not found"))?;
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
    reviewer_label: String,
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
    Path(thread_id): Path<String>,
    Json(request): Json<ManualReviewRequest>,
) -> Result<Json<AnalysisRun>, ApiError> {
    let thread = state
        .storage
        .get_thread(&thread_id)
        .await?
        .ok_or(ApiError::not_found("thread not found"))?;
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
        reviewer_label: request.reviewer_label,
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

async fn execute_analysis(
    state: AppState,
    run_id: String,
    access_token: String,
) -> anyhow::Result<()> {
    let mut run = state
        .storage
        .get_analysis_run(&run_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("analysis run not found"))?;
    let thread_ids = state
        .gmail
        .list_thread_ids(
            &access_token,
            &run.config,
            state.config.google.gmail_max_threads,
        )
        .await?;
    run.total_candidate_threads = thread_ids.len() as u64;
    run.progress_message = format!("{} hilos encontrados en Gmail", thread_ids.len());
    state.storage.update_analysis_run(&run).await?;

    let mut ai_input_tokens = 0;
    let mut ai_output_tokens = 0;
    let excluded_sample = excluded_sample_ids(&thread_ids);

    for thread_id in thread_ids {
        let data = state
            .gmail
            .fetch_thread(&access_token, &thread_id, &run.config)
            .await?;
        if !data.is_primary_inbox {
            run.processed_threads += 1;
            run.progress_message = format!(
                "Procesados {}/{} hilos · IA entrada {} · salida {} tokens",
                run.processed_threads,
                run.total_candidate_threads,
                ai_input_tokens,
                ai_output_tokens
            );
            state.storage.update_analysis_run(&run).await?;
            continue;
        }
        if let Some(first_message) = data.messages.iter().min_by_key(|message| message.date) {
            if !message_is_inside_analysis_window(first_message, &run.config) {
                run.processed_threads += 1;
                run.progress_message = format!(
                    "Procesados {}/{} hilos · IA entrada {} · salida {} tokens",
                    run.processed_threads,
                    run.total_candidate_threads,
                    ai_input_tokens,
                    ai_output_tokens
                );
                state.storage.update_analysis_run(&run).await?;
                continue;
            }
        }
        let mut thread = classify_thread(&run.id, &data.id, &data.messages, &run.config);
        let received_human_thread = thread.first_client_message_id.is_some();
        let should_audit = received_human_thread
            && (thread.is_valid_client_request
                || thread.manual_review_required
                || excluded_sample.contains(&thread.gmail_thread_id));

        if should_audit {
            let audit_messages = audit_messages_for_thread(&thread, &data.messages);
            match audit_thread(&state, &thread, &audit_messages).await {
                Ok(audit) => {
                    ai_input_tokens += audit.input_tokens;
                    ai_output_tokens += audit.output_tokens;
                    state
                        .storage
                        .add_ai_audit(&run.id, &thread.id, &audit)
                        .await?;
                    let known_ids = data
                        .messages
                        .iter()
                        .map(|m| m.id.clone())
                        .collect::<Vec<_>>();
                    if should_auto_apply_ai(&audit, &known_ids)
                        && audit.confidence >= state.config.ai.apply_confidence_threshold
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
                            "La auditoría IA se aplicó automáticamente por alta confianza."
                                .to_string(),
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

fn audit_messages_for_thread(thread: &EmailThread, messages: &[EmailMessage]) -> Vec<EmailMessage> {
    const MAX_AUDIT_TEXT_CHARS: usize = 1600;

    let mut ids = Vec::new();
    if let Some(id) = &thread.first_client_message_id {
        ids.push(id.clone());
    }
    if let Some(id) = &thread.first_internal_reply_message_id {
        ids.push(id.clone());
    }
    if let Some(id) = &thread.last_internal_message_id {
        ids.push(id.clone());
    }

    if ids.is_empty() {
        ids.extend(messages.iter().take(3).map(|message| message.id.clone()));
    }

    let mut selected = messages
        .iter()
        .filter(|message| ids.contains(&message.id))
        .cloned()
        .map(|mut message| {
            if let Some(text) = &message.body_text {
                message.body_text = Some(truncate_chars(text, MAX_AUDIT_TEXT_CHARS));
            }
            message
        })
        .collect::<Vec<_>>();
    selected.sort_by_key(|message| message.date);
    selected.dedup_by(|left, right| left.id == right.id);
    selected
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

    fn forbidden() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: "Acceso denegado".to_string(),
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
