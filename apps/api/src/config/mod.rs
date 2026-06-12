use std::env;

use anyhow::{Context, anyhow};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub api_port: u16,
    pub web_base_url: String,
    pub api_base_url: String,
    pub cookie_secure: bool,
    pub cookie_same_site: String,
    pub app_storage: String,
    pub encryption_key: String,
    pub session_secret: String,
    pub google: GoogleConfig,
    pub firestore: FirestoreConfig,
    pub ai: AiConfig,
}

#[derive(Debug, Clone)]
pub struct GoogleConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
    pub gmail_max_threads: u32,
}

#[derive(Debug, Clone)]
pub struct FirestoreConfig {
    pub project_id: String,
    pub database_id: String,
    pub bearer_token: Option<String>,
    pub service_account_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AiConfig {
    pub worker_url: String,
    pub worker_audience: Option<String>,
    pub apply_confidence_threshold: f64,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let app_storage = env_or("APP_STORAGE", "firestore");
        let google = GoogleConfig {
            client_id: env_or("GOOGLE_CLIENT_ID", ""),
            client_secret: env_or("GOOGLE_CLIENT_SECRET", ""),
            redirect_url: env_or(
                "GOOGLE_REDIRECT_URL",
                "http://localhost:8080/auth/google/callback",
            ),
            gmail_max_threads: env_or("GMAIL_MAX_THREADS", "50")
                .parse()
                .context("invalid GMAIL_MAX_THREADS")?,
        };

        if app_storage != "memory" {
            require("GCP_PROJECT_ID")?;
        }

        let api_base_url = env_or("API_BASE_URL", "http://localhost:8080");
        let cookie_secure = env::var("APP_COOKIE_SECURE")
            .ok()
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or_else(|| api_base_url.starts_with("https://"));
        let cookie_same_site = env_or(
            "APP_COOKIE_SAMESITE",
            if cookie_secure { "None" } else { "Lax" },
        );

        Ok(Self {
            api_port: env::var("PORT")
                .unwrap_or_else(|_| env_or("API_PORT", "8080"))
                .parse()
                .context("invalid API_PORT")?,
            web_base_url: env_or("WEB_BASE_URL", "http://localhost:5173"),
            api_base_url,
            cookie_secure,
            cookie_same_site,
            app_storage,
            encryption_key: require("APP_ENCRYPTION_KEY")
                .unwrap_or_else(|_| "development-only-change-me-32-bytes".to_string()),
            session_secret: require("APP_SESSION_SECRET")
                .unwrap_or_else(|_| "development-only-session-secret".to_string()),
            google,
            firestore: FirestoreConfig {
                project_id: env_or("GCP_PROJECT_ID", ""),
                database_id: env_or("FIRESTORE_DATABASE_ID", "(default)"),
                bearer_token: empty_to_none(env::var("FIRESTORE_BEARER_TOKEN").ok()),
                service_account_path: empty_to_none(
                    env::var("GOOGLE_APPLICATION_CREDENTIALS").ok(),
                ),
            },
            ai: AiConfig {
                worker_url: env_or("AI_WORKER_URL", "http://localhost:8090"),
                worker_audience: empty_to_none(env::var("AI_WORKER_AUDIENCE").ok()),
                apply_confidence_threshold: env_or("AI_APPLY_CONFIDENCE_THRESHOLD", "0.92")
                    .parse()
                    .context("invalid AI_APPLY_CONFIDENCE_THRESHOLD")?,
            },
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct ServiceAccountKey {
    #[allow(dead_code)]
    pub project_id: Option<String>,
    pub private_key: String,
    pub client_email: String,
    pub token_uri: Option<String>,
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn require(key: &str) -> anyhow::Result<String> {
    let value = env::var(key).map_err(|_| anyhow!("{key} is required"))?;
    if value.trim().is_empty() {
        return Err(anyhow!("{key} cannot be empty"));
    }
    Ok(value)
}

fn empty_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}
