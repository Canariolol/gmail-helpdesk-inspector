use axum::{Json, body::Bytes, extract::State, http::HeaderMap};
use chrono::{NaiveDate, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::{ApiError, AppState};
use crate::{
    report::ResendMailer,
    scheduler::{self, ScheduledOutcome, window},
};

const CRON_SECRET_HEADER: &str = "x-cron-secret";

#[derive(Debug, Default, Deserialize)]
pub struct ScheduledAnalysisRequest {
    /// Fecha local (America/Santiago) del tick; permite backfill manual.
    pub as_of_date: Option<String>,
}

/// Disparo externo del análisis programado (Cloud Scheduler, crontab, curl).
/// No usa cookie de sesión: se autentica solo con el secreto compartido.
pub async fn scheduled_analysis(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Vec<ScheduledOutcome>>, ApiError> {
    let Some(expected_secret) = &state.config.scheduler.cron_secret else {
        // Sin CRON_SECRET el disparo externo está apagado; no revelar la ruta.
        return Err(ApiError::not_found("recurso no encontrado"));
    };
    let provided_secret = headers
        .get(CRON_SECRET_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !cron_secret_matches(provided_secret, expected_secret) {
        return Err(ApiError::unauthorized());
    }

    let request: ScheduledAnalysisRequest = if body.is_empty() {
        ScheduledAnalysisRequest::default()
    } else {
        serde_json::from_slice(&body)
            .map_err(|_| ApiError::bad_request("cuerpo inválido: se espera JSON"))?
    };
    let as_of_date = match request.as_of_date {
        Some(raw) => NaiveDate::parse_from_str(&raw, "%Y-%m-%d")
            .map_err(|_| ApiError::bad_request("as_of_date inválida; usa el formato YYYY-MM-DD"))?,
        None => Utc::now().with_timezone(&window::SCL).date_naive(),
    };

    let mailer = ResendMailer::from_config(&state.config.report)?;
    let outcomes = scheduler::run_scheduled_analysis(&state, &mailer, as_of_date).await?;
    Ok(Json(outcomes))
}

/// Comparar digests hace que el tiempo no dependa del punto donde difieren
/// los secretos (comparación timing-safe sin dependencias nuevas).
fn cron_secret_matches(provided: &str, expected: &str) -> bool {
    Sha256::digest(provided.as_bytes()) == Sha256::digest(expected.as_bytes())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::{config::test_app_config, storage::MemoryStorage};

    fn app(cron_secret: Option<&str>) -> axum::Router {
        let mut config = test_app_config();
        config.scheduler.cron_secret = cron_secret.map(ToOwned::to_owned);
        crate::build_app(config, Arc::new(MemoryStorage::default()))
    }

    fn request(secret: Option<&str>, body: &str) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/internal/scheduled-analysis")
            .header("content-type", "application/json");
        if let Some(secret) = secret {
            builder = builder.header(CRON_SECRET_HEADER, secret);
        }
        builder.body(Body::from(body.to_string())).unwrap()
    }

    async fn body_text(response: axum::response::Response) -> String {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn endpoint_is_hidden_without_configured_secret() {
        let response = app(None)
            .oneshot(request(Some("dev-secret"), ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn missing_or_wrong_secret_is_unauthorized() {
        let response = app(Some("dev-secret"))
            .oneshot(request(None, ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = app(Some("dev-secret"))
            .oneshot(request(Some("otro"), ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn saturday_returns_weekend_skip() {
        let response = app(Some("dev-secret"))
            .oneshot(request(
                Some("dev-secret"),
                r#"{"as_of_date":"2026-06-13"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("fin de semana"));
    }

    #[tokio::test]
    async fn weekday_without_config_returns_skip() {
        let response = app(Some("dev-secret"))
            .oneshot(request(
                Some("dev-secret"),
                r#"{"as_of_date":"2026-06-12"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("sin configuración"));
    }

    #[tokio::test]
    async fn malformed_date_is_bad_request() {
        let response = app(Some("dev-secret"))
            .oneshot(request(
                Some("dev-secret"),
                r#"{"as_of_date":"12-06-2026"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn secret_comparison_accepts_exact_match_only() {
        assert!(cron_secret_matches("abc", "abc"));
        assert!(!cron_secret_matches("abc", "abd"));
        assert!(!cron_secret_matches("", "abc"));
    }
}
