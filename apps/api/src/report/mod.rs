pub mod template;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::config::ReportConfig;

const RESEND_ENDPOINT: &str = "https://api.resend.com/emails";

#[async_trait]
pub trait ReportMailer: Send + Sync {
    /// Envía un correo HTML y devuelve el id asignado por el proveedor.
    async fn send(&self, to: &[String], subject: &str, html: &str) -> anyhow::Result<String>;
}

pub struct ResendMailer {
    http: reqwest::Client,
    api_key: String,
    from: String,
}

#[derive(Debug, Deserialize)]
struct ResendResponse {
    id: String,
}

impl ResendMailer {
    pub fn from_config(config: &ReportConfig) -> anyhow::Result<Self> {
        let api_key = config
            .resend_api_key
            .clone()
            .ok_or_else(|| anyhow!("RESEND_API_KEY no está configurada"))?;
        let from = config
            .from_email
            .clone()
            .ok_or_else(|| anyhow!("REPORT_FROM_EMAIL no está configurado"))?;
        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
            from,
        })
    }
}

#[async_trait]
impl ReportMailer for ResendMailer {
    async fn send(&self, to: &[String], subject: &str, html: &str) -> anyhow::Result<String> {
        if to.is_empty() {
            return Err(anyhow!("no hay destinatarios para el reporte"));
        }
        let response = self
            .http
            .post(RESEND_ENDPOINT)
            .bearer_auth(&self.api_key)
            .json(&resend_payload(&self.from, to, subject, html))
            .send()
            .await
            .context("no se pudo contactar a Resend")?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("Resend rechazó el envío ({status})"));
        }
        let parsed: ResendResponse = serde_json::from_str(&body)
            .map_err(|error| anyhow!("Resend devolvió JSON inválido: {error}"))?;
        Ok(parsed.id)
    }
}

pub(crate) fn resend_payload(from: &str, to: &[String], subject: &str, html: &str) -> Value {
    json!({
        "from": from,
        "to": to,
        "subject": subject,
        "html": html,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resend_payload_has_expected_shape() {
        let payload = resend_payload(
            "Helpdesk <reportes@x.cl>",
            &["admin@x.cl".to_string(), "jefa@x.cl".to_string()],
            "Reporte",
            "<p>hola</p>",
        );
        assert_eq!(payload["from"], "Helpdesk <reportes@x.cl>");
        assert_eq!(payload["to"].as_array().unwrap().len(), 2);
        assert_eq!(payload["subject"], "Reporte");
        assert_eq!(payload["html"], "<p>hola</p>");
    }

    #[test]
    fn from_config_requires_api_key_and_sender() {
        let missing_key = ReportConfig {
            resend_api_key: None,
            from_email: Some("a@x.cl".to_string()),
            to_emails: vec![],
        };
        assert!(ResendMailer::from_config(&missing_key).is_err());

        let missing_from = ReportConfig {
            resend_api_key: Some("re_123".to_string()),
            from_email: None,
            to_emails: vec![],
        };
        assert!(ResendMailer::from_config(&missing_from).is_err());

        let valid = ReportConfig {
            resend_api_key: Some("re_123".to_string()),
            from_email: Some("a@x.cl".to_string()),
            to_emails: vec![],
        };
        assert!(ResendMailer::from_config(&valid).is_ok());
    }
}
