use std::env;

use anyhow::{Context, anyhow};
use serde::Deserialize;

const DEFAULT_ENCRYPTION_KEY: &str = "development-only-change-me-32-bytes";
const DEFAULT_SESSION_SECRET: &str = "development-only-session-secret";

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
    pub workos: WorkosConfig,
    pub billing: BillingConfig,
    pub firestore: FirestoreConfig,
    pub ai: AiConfig,
    pub scheduler: SchedulerConfig,
    pub report: ReportConfig,
    pub rate_limit: RateLimitConfig,
    /// Cuentas internas privilegiadas (correos en minúsculas) con acceso total:
    /// sin rate limits, sin gating de setup y sin futuros límites de plan.
    pub internal_full_access_emails: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GoogleConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
    pub gmail_max_threads: u32,
}

#[derive(Debug, Clone)]
pub struct WorkosConfig {
    pub client_id: String,
    pub api_key: String,
    pub redirect_uri: String,
    pub cookie_secret: String,
}

#[derive(Debug, Clone)]
pub struct BillingConfig {
    pub mercadopago_access_token: Option<String>,
    pub mercadopago_webhook_secret: Option<String>,
    pub enforcement_enabled: bool,
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

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub analysis_create_per_hour: usize,
    pub analysis_start_per_hour: usize,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let app_env = env_or("APP_ENV", "development");
        let production = is_production_env(&app_env);
        let app_storage = env_or("APP_STORAGE", "firestore");
        let google = GoogleConfig {
            client_id: env_or("GOOGLE_CLIENT_ID", ""),
            client_secret: env_or("GOOGLE_CLIENT_SECRET", ""),
            redirect_url: env_or(
                "GOOGLE_REDIRECT_URL",
                "http://localhost:8080/gmail/connect/callback",
            ),
            gmail_max_threads: env_or("GMAIL_MAX_THREADS", "50")
                .parse()
                .context("invalid GMAIL_MAX_THREADS")?,
        };
        let workos = WorkosConfig {
            client_id: env_or("WORKOS_CLIENT_ID", ""),
            api_key: secret_or_empty("WORKOS_API_KEY", production)?,
            redirect_uri: env_or(
                "WORKOS_REDIRECT_URI",
                "http://localhost:8080/auth/workos/callback",
            ),
            cookie_secret: secret_or_dev_default(
                "WORKOS_COOKIE_SECRET",
                "development-only-workos-cookie-secret",
                production,
            )?,
        };
        if production && workos.client_id.trim().is_empty() {
            return Err(anyhow!(
                "WORKOS_CLIENT_ID is required when APP_ENV=production"
            ));
        }
        let billing = BillingConfig {
            mercadopago_access_token: empty_to_none(env::var("MERCADOPAGO_ACCESS_TOKEN").ok()),
            mercadopago_webhook_secret: empty_to_none(env::var("MERCADOPAGO_WEBHOOK_SECRET").ok()),
            enforcement_enabled: env::var("BILLING_ENFORCEMENT_ENABLED")
                .ok()
                .map(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
                .unwrap_or(production),
        };
        validate_billing_config(&billing, production)?;

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

        let rate_limit = RateLimitConfig {
            analysis_create_per_hour: env_or("RATE_LIMIT_ANALYSIS_CREATE_PER_HOUR", "12")
                .parse()
                .context("invalid RATE_LIMIT_ANALYSIS_CREATE_PER_HOUR")?,
            analysis_start_per_hour: env_or("RATE_LIMIT_ANALYSIS_START_PER_HOUR", "12")
                .parse()
                .context("invalid RATE_LIMIT_ANALYSIS_START_PER_HOUR")?,
        };

        let internal_full_access_emails = env_list("INTERNAL_FULL_ACCESS_EMAILS")
            .into_iter()
            .map(|email| email.to_lowercase())
            .collect();

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
            encryption_key: secret_or_dev_default(
                "APP_ENCRYPTION_KEY",
                DEFAULT_ENCRYPTION_KEY,
                production,
            )?,
            session_secret: secret_or_dev_default(
                "APP_SESSION_SECRET",
                DEFAULT_SESSION_SECRET,
                production,
            )?,
            google,
            workos,
            billing,
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
            rate_limit,
            internal_full_access_emails,
        })
    }

    /// True si el correo pertenece a una cuenta interna privilegiada (acceso
    /// total). El bypass aplica a cuotas/gating/consentimiento, nunca al
    /// aislamiento de sesión ni de propiedad de datos.
    pub fn is_privileged_account(&self, email: &str) -> bool {
        let normalized = email.trim().to_lowercase();
        !normalized.is_empty()
            && self
                .internal_full_access_emails
                .iter()
                .any(|allowed| allowed == &normalized)
    }
}

fn secret_or_empty(key: &str, production: bool) -> anyhow::Result<String> {
    match require(key) {
        Ok(value) => Ok(value),
        Err(error) if production => Err(anyhow!(
            "{key} is required when APP_ENV=production: {error}"
        )),
        Err(_) => Ok(String::new()),
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

fn secret_or_dev_default(key: &str, default: &str, production: bool) -> anyhow::Result<String> {
    match require(key) {
        Ok(value) => validate_secret_not_default(key, &value, default, production),
        Err(error) if production => Err(anyhow!(
            "{key} is required when APP_ENV=production: {error}"
        )),
        Err(_) => Ok(default.to_string()),
    }
}

fn validate_secret_not_default(
    key: &str,
    value: &str,
    default: &str,
    production: bool,
) -> anyhow::Result<String> {
    if production && (value == default || value.contains("development-only")) {
        return Err(anyhow!(
            "{key} must not use a development default when APP_ENV=production"
        ));
    }
    Ok(value.to_string())
}

fn validate_billing_config(billing: &BillingConfig, production: bool) -> anyhow::Result<()> {
    if production && billing.mercadopago_access_token.is_none() {
        return Err(anyhow!(
            "MERCADOPAGO_ACCESS_TOKEN is required when APP_ENV=production"
        ));
    }
    if production && billing.mercadopago_webhook_secret.is_none() {
        return Err(anyhow!(
            "MERCADOPAGO_WEBHOOK_SECRET is required when APP_ENV=production"
        ));
    }
    Ok(())
}

fn is_production_env(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "prod" | "production"
    )
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
        workos: WorkosConfig {
            client_id: "client_test".to_string(),
            api_key: "sk_test".to_string(),
            redirect_uri: "http://localhost:8080/auth/workos/callback".to_string(),
            cookie_secret: "test-workos-cookie-secret".to_string(),
        },
        billing: BillingConfig {
            mercadopago_access_token: Some("TEST-access-token".to_string()),
            mercadopago_webhook_secret: Some("test-webhook-secret".to_string()),
            enforcement_enabled: true,
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
        rate_limit: RateLimitConfig {
            analysis_create_per_hour: 12,
            analysis_start_per_hour: 12,
        },
        internal_full_access_emails: vec![],
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

    #[test]
    fn production_env_aliases_are_detected() {
        assert!(is_production_env("production"));
        assert!(is_production_env("prod"));
        assert!(is_production_env(" PROD "));
        assert!(!is_production_env("development"));
        assert!(!is_production_env("staging"));
    }

    #[test]
    fn production_rejects_development_secret_defaults() {
        let err = validate_secret_not_default(
            "APP_SESSION_SECRET",
            DEFAULT_SESSION_SECRET,
            DEFAULT_SESSION_SECRET,
            true,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("must not use a development default"));

        let err = validate_secret_not_default(
            "APP_SESSION_SECRET",
            "development-only-custom",
            DEFAULT_SESSION_SECRET,
            true,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("must not use a development default"));
    }

    #[test]
    fn production_requires_mercadopago_webhook_secret() {
        let billing = BillingConfig {
            mercadopago_access_token: Some("TEST-access-token".to_string()),
            mercadopago_webhook_secret: None,
            enforcement_enabled: true,
        };

        let err = validate_billing_config(&billing, true)
            .unwrap_err()
            .to_string();
        assert!(err.contains("MERCADOPAGO_WEBHOOK_SECRET"));
        assert!(validate_billing_config(&billing, false).is_ok());
    }

    #[test]
    fn development_allows_defaults_for_local_runs() {
        assert_eq!(
            validate_secret_not_default(
                "APP_SESSION_SECRET",
                DEFAULT_SESSION_SECRET,
                DEFAULT_SESSION_SECRET,
                false,
            )
            .unwrap(),
            DEFAULT_SESSION_SECRET
        );
    }

    #[test]
    fn privileged_account_matches_case_insensitively_and_ignores_empty() {
        let mut config = test_app_config();
        config.internal_full_access_emails =
            vec!["catherine.trivino@west-ingenieria.cl".to_string()];
        assert!(config.is_privileged_account("Catherine.Trivino@West-Ingenieria.CL"));
        assert!(config.is_privileged_account("  catherine.trivino@west-ingenieria.cl  "));
        assert!(!config.is_privileged_account("someone@else.com"));
        assert!(!config.is_privileged_account(""));
    }
}
