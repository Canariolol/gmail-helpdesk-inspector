mod analysis;
mod auth;
mod config;
mod firestore;
mod gmail;
mod http;
mod storage;

use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use config::AppConfig;
use firestore::FirestoreStorage;
use http::AppState;
use storage::{MemoryStorage, StorageRepository};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = AppConfig::from_env()?;
    let storage: Arc<dyn StorageRepository> = if config.app_storage == "memory" {
        Arc::new(MemoryStorage::default())
    } else {
        Arc::new(FirestoreStorage::new(config.firestore.clone())?)
    };

    let app = build_app(config.clone(), storage);
    let addr = format!("0.0.0.0:{}", config.api_port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind API on {addr}"))?;

    tracing::info!("API listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn build_app(config: AppConfig, storage: Arc<dyn StorageRepository>) -> Router {
    let state = AppState::new(config, storage);

    http::router(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

