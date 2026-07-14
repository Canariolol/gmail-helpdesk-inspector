mod analysis;
mod auth;
mod billing;
mod config;
mod firestore;
mod gmail;
mod http;
mod mailbox;
mod policies;
mod report;
mod scheduler;
mod storage;

use std::sync::Arc;

use anyhow::Context;
use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderName, HeaderValue, Method, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use config::AppConfig;
use firestore::FirestoreStorage;
use http::AppState;
use storage::{MemoryStorage, StorageRepository};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info,html5ever::tree_builder=error".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = AppConfig::from_env()?;
    let storage: Arc<dyn StorageRepository> = if config.app_storage == "memory" {
        Arc::new(MemoryStorage::default())
    } else {
        Arc::new(FirestoreStorage::new(config.firestore.clone())?)
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
        return (
            axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": "Origen no permitido" })),
        )
            .into_response();
    }
    next.run(request).await
}
