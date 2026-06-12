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
    pub scheduler: SchedulerConfig,
    pub report: ReportConfig,
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

#[derive(Debug, Clone, Default)]
pub struct SchedulerConfig {
    pub enabled: bool,
    pub cron_secret: Option<String>,
    pub seed_user_email: Option<String>,
    pub seed_internal_domains: Vec<String>,
    pub seed_gmail_max_threads: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct ReportConfig {
    pub resend_api_key: Option<String>,
    pub from_email: Option<String>,
    pub to_emails: Vec<String>,
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

        let scheduler = SchedulerConfig {
            enabled: env_flag("SCHEDULER_ENABLED"),
            cron_secret: empty_to_none(env::var("CRON_SECRET").ok()),
            seed_user_email: empty_to_none(env::var("SCHEDULE_USER_EMAIL").ok()),
            seed_internal_domains: env_list("SCHEDULE_INTERNAL_DOMAINS"),
            seed_gmail_max_threads: empty_to_none(env::var("SCHEDULE_GMAIL_MAX_THREADS").ok())
                .map(|value| value.parse().context("invalid SCHEDULE_GMAIL_MAX_THREADS"))
                .transpose()?,
        };
        let report = ReportConfig {
            resend_api_key: empty_to_none(env::var("RESEND_API_KEY").ok()),
            from_email: empty_to_none(env::var("REPORT_FROM_EMAIL").ok()),
            to_emails: env_list("REPORT_TO_EMAIL"),
        };
        if scheduler.enabled {
            if report.resend_api_key.is_none() {
                return Err(anyhow!(
                    "RESEND_API_KEY is required when SCHEDULER_ENABLED=true"
                ));
            }
            if report.from_email.is_none() {
                return Err(anyhow!(
                    "REPORT_FROM_EMAIL is required when SCHEDULER_ENABLED=true"
                ));
            }
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
            scheduler,
            report,
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

fn env_flag(key: &str) -> bool {
    env::var(key)
        .map(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn env_list(key: &str) -> Vec<String> {
    env::var(key)
        .map(|value| split_list(&value))
        .unwrap_or_default()
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Config base para tests unitarios; cada test ajusta los campos que necesita.
#[cfg(test)]
pub(crate) fn test_app_config() -> AppConfig {
    AppConfig {
        api_port: 0,
        web_base_url: "http://localhost:5173".to_string(),
        api_base_url: "http://localhost:8080".to_string(),
        cookie_secure: false,
        cookie_same_site: "Lax".to_string(),
        app_storage: "memory".to_string(),
        encryption_key: "test-encryption-key-32-bytes-long!!".to_string(),
        session_secret: "test-session-secret".to_string(),
        google: GoogleConfig {
            client_id: String::new(),
            client_secret: String::new(),
            redirect_url: String::new(),
            gmail_max_threads: 50,
        },
        firestore: FirestoreConfig {
            project_id: String::new(),
            database_id: String::new(),
            bearer_token: None,
            service_account_path: None,
        },
        ai: AiConfig {
            worker_url: String::new(),
            worker_audience: None,
            apply_confidence_threshold: 0.92,
        },
        scheduler: SchedulerConfig {
            enabled: false,
            cron_secret: Some("dev-secret".to_string()),
            seed_user_email: None,
            seed_internal_domains: vec!["x.cl".to_string()],
            seed_gmail_max_threads: None,
        },
        report: ReportConfig {
            resend_api_key: Some("re_test".to_string()),
            from_email: Some("Helpdesk <r@x.cl>".to_string()),
            to_emails: vec!["admin@x.cl".to_string()],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_list_trims_and_skips_empty_items() {
        assert_eq!(
            split_list(" a@b.cl, ,c.cl ,, d "),
            vec!["a@b.cl", "c.cl", "d"]
        );
        assert!(split_list("").is_empty());
        assert!(split_list(" , ").is_empty());
    }
}
