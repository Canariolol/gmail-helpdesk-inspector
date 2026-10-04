use anyhow::{Context, anyhow, ensure};
use async_trait::async_trait;
use chrono::{DateTime, Days, LocalResult, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::analysis::AnalysisConfig;

/// Proveedor de correo de una casilla conectada. El discriminante vive en la
/// conexión y no en la sesión web: el scheduler analiza sin sesión, así que la
/// sesión no puede ser la fuente de verdad del proveedor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailboxProviderKind {
    /// Gmail y dominios en Google Workspace.
    #[default]
    Google,
    /// Outlook y dominios en Microsoft 365, vía Microsoft Graph.
    Microsoft,
    /// Proveedores externos mediante IMAP sobre TLS y contraseña de aplicación.
    Imap,
}

impl MailboxProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Microsoft => "microsoft",
            Self::Imap => "imap",
        }
    }

    /// Scope de solo lectura que Mira consiente en este proveedor. Se estampa en
    /// la casilla AL CONECTAR: la vista de Privacidad lo muestra como la
    /// declaración de qué permiso tiene Mira, así que no puede quedar con el
    /// valor de otro proveedor.
    pub fn read_scope(self) -> &'static str {
        match self {
            Self::Google => "https://www.googleapis.com/auth/gmail.readonly",
            Self::Microsoft => "Mail.Read",
            Self::Imap => "IMAP EXAMINE / BODY.PEEK",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "google" | "gmail" => Some(Self::Google),
            "microsoft" | "outlook" => Some(Self::Microsoft),
            "imap" => Some(Self::Imap),
            _ => None,
        }
    }
}

/// Página de IDs de hilos recuperados del proveedor, con señales de truncación
/// para poder informar "recuperamos N, pero hay más" sin una segunda llamada.
#[derive(Debug, Clone)]
pub struct ThreadListPage {
    pub ids: Vec<String>,
    /// Presente cuando el proveedor indicó que hay más resultados de los que
    /// cupieron en el máximo pedido.
    pub next_page_token: Option<String>,
    /// Estimación del total de resultados, cuando el proveedor la entrega.
    pub result_size_estimate: Option<u64>,
}

/// Hilo tal como lo entrega un proveedor, ya normalizado a mensajes neutrales.
#[derive(Debug, Clone)]
pub struct ProviderThread {
    pub id: String,
    /// Cumple la selección de carpetas. Sin filtros explícitos se usa la bandeja
    /// principal de Gmail o Inbox en otros proveedores.
    pub is_primary_inbox: bool,
    /// Carpetas o etiquetas del hilo, con valores opacos definidos por cada
    /// adaptador. Gmail entrega sus label IDs; Microsoft, IDs de carpeta.
    #[allow(clippy::struct_field_names)]
    pub folder_ids: Vec<String>,
    pub messages: Vec<crate::analysis::EmailMessage>,
    /// El límite de recuperación impidió obtener la conversación completa.
    pub truncated: bool,
}

/// Lo que Mira necesita de una casilla, sea cual sea el proveedor. El motor de
/// análisis vive por debajo de esta frontera y no conoce ningún proveedor.
#[async_trait]
pub trait MailboxProvider: Send + Sync {
    fn kind(&self) -> MailboxProviderKind;

    async fn list_thread_ids(
        &self,
        access_token: &str,
        config: &AnalysisConfig,
        max_threads: u32,
    ) -> anyhow::Result<ThreadListPage>;

    async fn fetch_thread(
        &self,
        access_token: &str,
        thread_id: &str,
        config: &AnalysisConfig,
    ) -> anyhow::Result<ProviderThread>;

    /// Lectura best-effort al conectar. Cada parte tolera su propio error para
    /// no abortar el login si una llamada del proveedor falla.
    async fn fetch_mailbox_metadata(
        &self,
        access_token: &str,
        now: DateTime<Utc>,
    ) -> MailboxMetadata;
}

/// Credencial de la casilla, independiente de cualquier sesión web. Una misma
/// conexión permite análisis programados aunque la persona cierre sus sesiones.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxConnection {
    pub owner_email: String,
    /// Ausente en los registros anteriores al multiproveedor: por defecto Google.
    #[serde(default)]
    pub provider: MailboxProviderKind,
    /// Casilla Microsoft compartida/delegada. None consulta la casilla propia.
    #[serde(default)]
    pub microsoft_target_email: Option<String>,
    #[serde(default)]
    pub imap_config: Option<crate::imap::ImapConfig>,
    #[serde(default)]
    pub imap_password_encrypted: Option<String>,
    #[serde(default)]
    pub needs_reauth_at: Option<DateTime<Utc>>,
    #[serde(alias = "gmail_account_email")]
    pub mailbox_email: String,
    pub access_token_encrypted: String,
    #[serde(default)]
    pub refresh_token_encrypted: Option<String>,
    pub connected_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub revoked_at: Option<DateTime<Utc>>,
}

pub fn mailbox_connection_is_active(connection: Option<&MailboxConnection>) -> bool {
    connection.is_some_and(|connection| {
        connection.revoked_at.is_none()
            && connection.needs_reauth_at.is_none()
            && match connection.provider {
                MailboxProviderKind::Imap => {
                    connection.imap_config.is_some()
                        && connection
                            .imap_password_encrypted
                            .as_deref()
                            .is_some_and(|value| !value.trim().is_empty())
                }
                _ => !connection.access_token_encrypted.trim().is_empty(),
            }
    })
}

/// Metadata de la cuenta de Gmail leída AL CONECTAR (no al analizar): catálogo de
/// etiquetas, alias "enviar como", recuento de filtros y perfil. Todo se obtiene
/// bajo el scope `gmail.readonly` ya consentido.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxMetadata {
    #[serde(default)]
    pub profile: Option<GmailProfile>,
    #[serde(default)]
    pub labels: Vec<GmailLabel>,
    #[serde(default)]
    pub send_as: Vec<GmailSendAs>,
    #[serde(default)]
    pub filters_count: u32,
    #[serde(default)]
    pub folders_truncated: bool,
    pub synced_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmailLabel {
    pub id: String,
    pub name: String,
    /// "system" o "user" (según la API de Gmail).
    #[serde(default)]
    pub label_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmailSendAs {
    pub email: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub is_primary: bool,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub treat_as_alias: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmailProfile {
    pub email_address: String,
    #[serde(default)]
    pub messages_total: u64,
    #[serde(default)]
    pub threads_total: u64,
}

/// Preset de filtros guardado por el usuario, para no reingresar la selección
/// cada vez y para alternar entre distintos tipos de hilos. Persistido por cuenta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterPreset {
    pub id: String,
    pub owner_email: String,
    pub name: String,
    #[serde(default)]
    pub include_labels: Vec<String>,
    #[serde(default)]
    pub exclude_labels: Vec<String>,
    #[serde(default)]
    pub ignored_senders: Vec<String>,
    #[serde(default)]
    pub ignored_domains: Vec<String>,
    #[serde(default)]
    pub ignored_keywords: Vec<String>,
    #[serde(default)]
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Adaptadores disponibles en este despliegue. El despacho es **por conexión**,
/// no por arranque: cada organización elige su proveedor, así que la instancia a
/// usar se resuelve en cada request desde `MailboxConnection.provider`.
pub struct MailboxProviders {
    gmail: crate::gmail::GmailClient,
    microsoft: Option<crate::graph::GraphClient>,
}

/// El proveedor pedido no está habilitado en este despliegue (típicamente
/// Microsoft sin credenciales de Azure AD configuradas).
#[derive(Debug, thiserror::Error)]
#[error("proveedor de correo no disponible: {0}")]
pub struct ProviderUnavailable(pub &'static str);

impl MailboxProviders {
    pub fn new(
        gmail: crate::gmail::GmailClient,
        microsoft: Option<crate::graph::GraphClient>,
    ) -> Self {
        Self { gmail, microsoft }
    }

    pub fn get(
        &self,
        kind: MailboxProviderKind,
    ) -> Result<&dyn MailboxProvider, ProviderUnavailable> {
        match kind {
            MailboxProviderKind::Google => Ok(&self.gmail),
            MailboxProviderKind::Microsoft => self
                .microsoft
                .as_ref()
                .map(|client| client as &dyn MailboxProvider)
                .ok_or(ProviderUnavailable("microsoft")),
            MailboxProviderKind::Imap => Err(ProviderUnavailable("imap requires a connection")),
        }
    }

    /// Una instancia por análisis mantiene el destino de la conexión y, para
    /// IMAP, el índice de conversaciones recuperado al listar.
    pub fn for_connection(
        &self,
        connection: &MailboxConnection,
    ) -> anyhow::Result<Box<dyn MailboxProvider>> {
        match connection.provider {
            MailboxProviderKind::Google => Ok(Box::new(self.gmail.clone())),
            MailboxProviderKind::Microsoft => {
                let graph = self
                    .microsoft
                    .as_ref()
                    .ok_or(ProviderUnavailable("microsoft"))?;
                Ok(Box::new(match &connection.microsoft_target_email {
                    Some(email) => graph.with_mailbox(email)?,
                    None => graph.clone(),
                }))
            }
            MailboxProviderKind::Imap => Ok(Box::new(crate::imap::ImapClient::new(
                connection
                    .imap_config
                    .clone()
                    .context("IMAP connection without settings")?,
            )?)),
        }
    }

    /// Proveedores que el frontend puede ofrecer hoy. Un botón que no puede
    /// funcionar no debe mostrarse.
    pub fn available(&self) -> Vec<MailboxProviderKind> {
        let mut kinds = vec![MailboxProviderKind::Google];
        if self.microsoft.is_some() {
            kinds.push(MailboxProviderKind::Microsoft);
        }
        kinds.push(MailboxProviderKind::Imap);
        kinds
    }

    /// Acceso al cliente de Gmail para las lecturas que aún no pasan por el
    /// trait (catálogo de etiquetas al conectar).
    pub fn gmail(&self) -> &crate::gmail::GmailClient {
        &self.gmail
    }
}

/// Límites de calendario del tenant, inclusivo al inicio y exclusivo al final.
/// En cambios DST a medianoche se usa el primer instante válido del día.
pub(crate) fn analysis_window_utc(
    config: &AnalysisConfig,
) -> anyhow::Result<(DateTime<Utc>, DateTime<Utc>)> {
    let timezone: Tz = config
        .timezone
        .parse()
        .context("invalid mailbox timezone")?;
    let from =
        NaiveDate::parse_from_str(&config.date_from, "%Y-%m-%d").context("invalid date_from")?;
    let to = NaiveDate::parse_from_str(&config.date_to, "%Y-%m-%d")
        .context("invalid date_to")?
        .checked_add_days(Days::new(1))
        .context("date_to overflow")?;
    ensure!(from < to, "invalid analysis date range");
    let start = local_day_start(from, timezone)?;
    let end = local_day_start(to, timezone)?;
    ensure!(start < end, "analysis range has no local instants");
    Ok((start, end))
}

fn local_day_start(date: NaiveDate, timezone: Tz) -> anyhow::Result<DateTime<Utc>> {
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .context("invalid local midnight")?;
    for minute in 0..=1440 {
        let local = midnight
            .checked_add_signed(chrono::Duration::minutes(minute))
            .context("local date overflow")?;
        match timezone.from_local_datetime(&local) {
            LocalResult::Single(value) => return Ok(value.with_timezone(&Utc)),
            LocalResult::Ambiguous(first, second) => {
                return Ok(first.min(second).with_timezone(&Utc));
            }
            LocalResult::None => {}
        }
    }
    Err(anyhow!("local day has no valid instant"))
}

/// Reintentos limitados para lecturas idempotentes de correo. Si Retry-After
/// supera el presupuesto no se reintenta antes de lo pedido por el proveedor.
pub(crate) async fn send_mail_request(
    client: &reqwest::Client,
    request: reqwest::RequestBuilder,
) -> anyhow::Result<reqwest::Response> {
    let request = request.build().context("invalid mail request")?;
    for attempt in 0..=3u32 {
        let result = client
            .execute(
                request
                    .try_clone()
                    .context("mail request cannot be retried")?,
            )
            .await;
        match result {
            Ok(response) => {
                let status = response.status();
                if (status.as_u16() != 429 && !status.is_server_error()) || attempt == 3 {
                    return response
                        .error_for_status()
                        .map_err(|_| anyhow!("mail provider returned HTTP {}", status.as_u16()));
                }
                let wait = retry_delay(
                    response.headers().get(reqwest::header::RETRY_AFTER),
                    attempt,
                );
                let Some(wait) = wait else {
                    return Err(anyhow!("mail provider rate limit exceeds retry budget"));
                };
                tokio::time::sleep(wait).await;
            }
            Err(error) => {
                if attempt == 3
                    || (!error.is_timeout() && !error.is_connect() && !error.is_request())
                {
                    return Err(anyhow!("mail provider request failed"));
                }
                tokio::time::sleep(std::time::Duration::from_millis(250 * (1u64 << attempt))).await;
            }
        }
    }
    unreachable!()
}

/// Tope aplicado antes de deserializar datos de correo controlados por remitentes.
const MAX_MAIL_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

pub(crate) async fn read_mail_json<T: serde::de::DeserializeOwned>(
    mut response: reqwest::Response,
) -> anyhow::Result<T> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MAIL_RESPONSE_BYTES as u64)
    {
        return Err(anyhow!("mail provider response exceeds 16 MiB limit"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow!("mail provider response read failed"))?
    {
        ensure!(
            chunk.len() <= MAX_MAIL_RESPONSE_BYTES.saturating_sub(bytes.len()),
            "mail provider response exceeds 16 MiB limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    // serde_json conserva su límite de recursión; el error no expone contenido.
    serde_json::from_slice(&bytes).map_err(|_| anyhow!("invalid mail provider JSON response"))
}

fn retry_delay(
    value: Option<&reqwest::header::HeaderValue>,
    attempt: u32,
) -> Option<std::time::Duration> {
    let fallback = std::time::Duration::from_millis(250 * (1u64 << attempt));
    let delay = value
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .parse::<u64>()
                .ok()
                .map(std::time::Duration::from_secs)
                .or_else(|| {
                    DateTime::parse_from_rfc2822(value).ok().map(|date| {
                        std::time::Duration::from_secs(
                            date.with_timezone(&Utc)
                                .signed_duration_since(Utc::now())
                                .num_seconds()
                                .max(0) as u64,
                        )
                    })
                })
        })
        .unwrap_or(fallback);
    (delay <= std::time::Duration::from_secs(10)).then_some(delay)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_connections_without_provider_default_to_google() {
        let connection: MailboxConnection = serde_json::from_value(serde_json::json!({
            "owner_email": "o@x.cl",
            "gmail_account_email": "buzon@x.cl",
            "access_token_encrypted": "enc",
            "connected_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
        }))
        .expect("conexión legacy");
        assert_eq!(connection.provider, MailboxProviderKind::Google);
        assert_eq!(connection.mailbox_email, "buzon@x.cl");
    }

    #[test]
    fn microsoft_is_unavailable_until_azure_ad_is_configured() {
        let providers = MailboxProviders::new(crate::gmail::GmailClient::default(), None);
        assert!(providers.get(MailboxProviderKind::Google).is_ok());
        assert!(providers.get(MailboxProviderKind::Microsoft).is_err());
        assert_eq!(
            providers.available(),
            vec![MailboxProviderKind::Google, MailboxProviderKind::Imap]
        );
    }

    #[test]
    fn calendar_window_respects_tenant_zone_and_dst_midnight() {
        let mut config: AnalysisConfig = serde_json::from_value(serde_json::json!({
            "date_from": "2026-10-04", "date_to": "2026-10-04", "ignored_senders": [], "ignored_domains": [], "ignored_keywords": [], "timezone": "America/Santiago", "internal_domains": []
        })).unwrap();
        let (from, to) = analysis_window_utc(&config).unwrap();
        assert_eq!(from.to_rfc3339(), "2026-10-04T03:00:00+00:00");
        assert_eq!(to.to_rfc3339(), "2026-10-05T03:00:00+00:00");
        let late_message = DateTime::parse_from_rfc3339("2026-10-04T22:00:00-03:00")
            .unwrap()
            .with_timezone(&Utc);
        assert!(from <= late_message && late_message < to);
        config.timezone = "Asia/Tokyo".to_string();
        assert_eq!(
            analysis_window_utc(&config).unwrap().0.to_rfc3339(),
            "2026-10-03T15:00:00+00:00"
        );
        config.timezone = "America/Santiago".to_string();
        config.date_from = "2026-09-06".to_string();
        config.date_to = "2026-09-06".to_string();
        let (from, to) = analysis_window_utc(&config).unwrap();
        assert_eq!(to.signed_duration_since(from).num_hours(), 23);
    }

    #[test]
    fn provider_retry_after_respects_a_bounded_wait() {
        use reqwest::header::HeaderValue;
        assert_eq!(
            retry_delay(Some(&HeaderValue::from_static("3")), 0),
            Some(std::time::Duration::from_secs(3))
        );
        assert_eq!(
            retry_delay(Some(&HeaderValue::from_static("3600")), 0),
            None
        );
        assert_eq!(
            retry_delay(None, 1),
            Some(std::time::Duration::from_millis(500))
        );
    }

    #[tokio::test]
    async fn provider_reads_retry_429_and_never_retry_auth_failures() {
        use axum::{Router, routing::get};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let attempts = Arc::new(AtomicUsize::new(0));
        let reads = attempts.clone();
        let app = Router::new()
            .route(
                "/mail",
                get(move || {
                    let reads = reads.clone();
                    async move {
                        if reads.fetch_add(1, Ordering::SeqCst) == 0 {
                            (
                                axum::http::StatusCode::TOO_MANY_REQUESTS,
                                [("Retry-After", "0")],
                                "limited",
                            )
                        } else {
                            (axum::http::StatusCode::OK, [("Retry-After", "0")], "ok")
                        }
                    }
                }),
            )
            .route(
                "/auth",
                get(|| async { axum::http::StatusCode::UNAUTHORIZED }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = reqwest::Client::new();
        assert!(
            send_mail_request(&client, client.get(format!("http://{address}/mail")))
                .await
                .is_ok()
        );
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert!(
            send_mail_request(&client, client.get(format!("http://{address}/auth")))
                .await
                .is_err()
        );
        server.abort();
    }

    #[tokio::test]
    async fn mail_json_enforces_declared_and_streamed_body_limits_and_json_depth() {
        use axum::{Router, body::Body, routing::get};
        let declared = reqwest::Response::from(
            axum::http::Response::builder()
                .header("Content-Length", (MAX_MAIL_RESPONSE_BYTES + 1).to_string())
                .body(reqwest::Body::from(vec![b' '; MAX_MAIL_RESPONSE_BYTES + 1]))
                .unwrap(),
        );
        assert!(
            read_mail_json::<serde_json::Value>(declared)
                .await
                .unwrap_err()
                .to_string()
                .contains("16 MiB")
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/oversized",
            get(|| async {
                Body::from_stream(futures_util::stream::iter(
                    (0..257).map(|_| Ok::<_, std::io::Error>("x".repeat(64 * 1024))),
                ))
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let response = reqwest::Client::new()
            .get(format!("http://{address}/oversized"))
            .send()
            .await
            .unwrap();
        assert!(response.content_length().is_none());
        assert!(
            read_mail_json::<serde_json::Value>(response)
                .await
                .unwrap_err()
                .to_string()
                .contains("16 MiB")
        );
        server.abort();
        let nested = format!("{}0{}", "[".repeat(150), "]".repeat(150));
        let response =
            reqwest::Response::from(axum::http::Response::new(reqwest::Body::from(nested)));
        assert!(read_mail_json::<serde_json::Value>(response).await.is_err());
        let response = reqwest::Response::from(axum::http::Response::new(reqwest::Body::from(
            "{\"ok\":true}",
        )));
        assert_eq!(
            read_mail_json::<serde_json::Value>(response).await.unwrap(),
            serde_json::json!({"ok":true})
        );
    }
}
