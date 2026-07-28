mod analysis;
mod auth;
mod billing;
mod config;
mod gmail;
mod graph;
mod http;
mod mailbox;
mod policies;
mod postgres;
mod provider_detect;
mod report;
mod scheduler;
mod storage;

use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderName, HeaderValue, Method, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use config::AppConfig;
use http::AppState;
use postgres::PostgresStorage;
use storage::{MemoryStorage, StorageRepository};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};
use tracing::Instrument;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

tokio::task_local! {
    pub(crate) static REQUEST_ID: String;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info,html5ever::tree_builder=error".into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true),
        )
        .init();

    let config = AppConfig::from_env()?;
    let storage: Arc<dyn StorageRepository> = match config.app_storage.as_str() {
        "memory" => Arc::new(MemoryStorage::default()),
        "postgres" => Arc::new(
            PostgresStorage::connect(
                config
                    .postgres_database_url
                    .as_deref()
                    .expect("validated POSTGRES_DATABASE_URL"),
            )
            .await?,
        ),
        storage => anyhow::bail!("unsupported APP_STORAGE={storage}"),
    };
    let migration = storage.reconcile_manual_review_metrics_v1().await?;
    if migration.already_applied {
        tracing::info!("manual review metrics migration already applied");
    } else {
        tracing::info!(
            scanned_runs = migration.scanned_runs,
            updated_runs = migration.updated_runs,
            updated_threads = migration.updated_threads,
            "manual review metrics migration completed"
        );
    }
    let inheritance = storage.reconcile_manual_review_inheritance_v2().await?;
    if inheritance.already_applied {
        tracing::info!("manual review inheritance migration already applied");
    } else {
        tracing::info!(
            scanned_runs = inheritance.scanned_runs,
            created_overrides = inheritance.created_overrides,
            updated_runs = inheritance.updated_runs,
            updated_threads = inheritance.updated_threads,
            "manual review inheritance migration completed"
        );
    }

    let state = AppState::new(config.clone(), storage);
    if config.scheduler.enabled {
        scheduler::spawn(state.clone());
    }
    let app = build_app_from_state(state);
    let addr = format!("0.0.0.0:{}", config.api_port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind API on {addr}"))?;

    tracing::info!("API listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn build_app(config: AppConfig, storage: Arc<dyn StorageRepository>) -> Router {
    build_app_from_state(AppState::new(config, storage))
}

pub fn build_app_from_state(state: AppState) -> Router {
    let request_state = state.clone();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::exact(
            HeaderValue::from_str(&state.config.web_base_url).expect("valid WEB_BASE_URL"),
        ))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::ACCEPT,
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("x-requested-with"),
        ])
        .allow_credentials(true);

    http::router(state.clone())
        .layer(cors)
        .layer(axum::middleware::from_fn_with_state(
            state,
            reject_cross_origin_mutation,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn_with_state(
            request_state,
            attach_request_id,
        ))
}

async fn attach_request_id(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let started_at = Instant::now();
    let span = tracing::info_span!(
        "http_request",
        service = "api",
        environment = %state.config.app_env,
        %request_id,
        %method,
        %path
    );
    let mut response = REQUEST_ID
        .scope(
            request_id.clone(),
            next.run(request).instrument(span.clone()),
        )
        .await;
    tracing::info!(
        parent: &span,
        operation = "http_request",
        status = %response.status(),
        duration_ms = started_at.elapsed().as_millis(),
        "request completed"
    );
    response.headers_mut().insert(
        HeaderName::from_static("x-request-id"),
        HeaderValue::from_str(&request_id).expect("UUID is a valid header value"),
    );
    response
}

async fn reject_cross_origin_mutation(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let unsafe_method = matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    let origin_is_trusted = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_none_or(|origin| origin == state.config.web_base_url);
    if unsafe_method && !origin_is_trusted {
        return http::ApiError::forbidden("Origen no permitido").into_response();
    }
    next.run(request).await
}
