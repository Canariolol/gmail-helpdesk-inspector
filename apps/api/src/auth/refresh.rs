use anyhow::anyhow;

use super::GoogleTokenResponse;
use crate::config::{GoogleConfig, MicrosoftConfig};
use crate::mailbox::MailboxProviderKind;

#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    #[error(
        "el proveedor rechazó el refresh token (invalid_grant); se requiere reconectar la casilla"
    )]
    InvalidGrant,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Intercambia el refresh token por un access token nuevo. Lo usa el análisis
/// programado, donde el access token guardado en la sesión ya expiró.
pub async fn refresh_google_access_token(
    http: &reqwest::Client,
    google: &GoogleConfig,
    refresh_token: &str,
) -> Result<GoogleTokenResponse, RefreshError> {
    post_refresh(
        http,
        "https://oauth2.googleapis.com/token",
        &[
            ("client_id", google.client_id.as_str()),
            ("client_secret", google.client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ],
    )
    .await
}

/// Microsoft **rota** el refresh token en cada uso: la respuesta trae uno nuevo
/// y el anterior deja de servir. Quien llame debe persistir el rotado.
pub async fn refresh_microsoft_access_token(
    http: &reqwest::Client,
    microsoft: &MicrosoftConfig,
    refresh_token: &str,
) -> Result<GoogleTokenResponse, RefreshError> {
    let url = format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
        microsoft.tenant
    );
    post_refresh(
        http,
        &url,
        &[
            ("client_id", microsoft.client_id.as_str()),
            ("client_secret", microsoft.client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
            ("scope", super::MICROSOFT_MAIL_SCOPE),
        ],
    )
    .await
}

/// Refresca según el proveedor de la conexión. Devuelve `InvalidGrant` cuando el
/// usuario debe reconectar, para que el llamador revoque en vez de reintentar.
pub async fn refresh_access_token_for(
    http: &reqwest::Client,
    provider: MailboxProviderKind,
    google: &GoogleConfig,
    microsoft: Option<&MicrosoftConfig>,
    refresh_token: &str,
) -> Result<GoogleTokenResponse, RefreshError> {
    match provider {
        MailboxProviderKind::Google => {
            refresh_google_access_token(http, google, refresh_token).await
        }
        MailboxProviderKind::Microsoft => {
            let microsoft = microsoft.ok_or_else(|| {
                RefreshError::Other(anyhow!("Microsoft provider is not configured"))
            })?;
            refresh_microsoft_access_token(http, microsoft, refresh_token).await
        }
    }
}

async fn post_refresh(
    http: &reqwest::Client,
    url: &str,
    form: &[(&str, &str)],
) -> Result<GoogleTokenResponse, RefreshError> {
    let response = http
        .post(url)
        .form(form)
        .send()
        .await
        .map_err(|_| anyhow!("token refresh request failed"))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    classify_refresh_response(status, &body)
}

pub(crate) fn classify_refresh_response(
    status: u16,
    body: &str,
) -> Result<GoogleTokenResponse, RefreshError> {
    if (200..300).contains(&status) {
        return serde_json::from_str(body).map_err(|error| {
            RefreshError::Other(anyhow!("token refresh returned invalid JSON: {error}"))
        });
    }
    let error_code = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("error")?.as_str().map(ToOwned::to_owned));
    if error_code.as_deref() == Some("invalid_grant") {
        return Err(RefreshError::InvalidGrant);
    }
    Err(RefreshError::Other(anyhow!(
        "token refresh failed with {status}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_successful_refresh_without_rotated_token() {
        let token = classify_refresh_response(
            200,
            r#"{"access_token":"ya29.new","expires_in":3599,"token_type":"Bearer"}"#,
        )
        .unwrap();
        assert_eq!(token.access_token, "ya29.new");
        assert!(token.refresh_token.is_none());
    }

    #[test]
    fn parses_rotated_refresh_token_when_present() {
        let token = classify_refresh_response(
            200,
            r#"{"access_token":"ya29.new","refresh_token":"1//rotated"}"#,
        )
        .unwrap();
        assert_eq!(token.refresh_token.as_deref(), Some("1//rotated"));
    }

    #[test]
    fn maps_invalid_grant_to_dedicated_error() {
        let error = classify_refresh_response(
            400,
            r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#,
        )
        .unwrap_err();
        assert!(matches!(error, RefreshError::InvalidGrant));
    }

    #[test]
    fn other_failures_keep_status_without_provider_body() {
        let error = classify_refresh_response(500, "provider body with secret-token").unwrap_err();
        match error {
            RefreshError::Other(inner) => {
                let message = inner.to_string();
                assert!(message.contains("500"));
                assert!(!message.contains("secret-token"));
            }
            RefreshError::InvalidGrant => panic!("expected Other"),
        }
    }

    #[test]
    fn success_with_invalid_json_is_other_error() {
        assert!(matches!(
            classify_refresh_response(200, "not-json"),
            Err(RefreshError::Other(_))
        ));
    }
}
