mod analysis;
mod auth;
mod config;
mod firestore;
mod gmail;
mod http;
mod report;
mod scheduler;
mod storage;

use std::sync::Arc;

use anyhow::Context;
use axum::{
    Router,
    http::{HeaderName, HeaderValue, Method, header},
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
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::OPTIONS])
        .allow_headers([
            header::ACCEPT,
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("x-requested-with"),
        ])
        .allow_credentials(true);

    http::router(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}
