use async_trait::async_trait;
use chrono::{DateTime, Utc};
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
}

impl MailboxProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Microsoft => "microsoft",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "google" | "gmail" => Some(Self::Google),
            "microsoft" | "outlook" => Some(Self::Microsoft),
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
    /// Está en la bandeja principal *según el proveedor*: Gmail exige
    /// `INBOX`+`CATEGORY_PERSONAL`; Microsoft, la carpeta `Inbox`.
    pub is_primary_inbox: bool,
    /// Carpetas o etiquetas del hilo, con valores opacos definidos por cada
    /// adaptador. Gmail entrega sus label IDs; Microsoft, IDs de carpeta.
    #[allow(clippy::struct_field_names)]
    pub folder_ids: Vec<String>,
    pub messages: Vec<crate::analysis::EmailMessage>,
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
        connection.revoked_at.is_none() && !connection.access_token_encrypted.trim().is_empty()
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
        }
    }

    /// Proveedores que el frontend puede ofrecer hoy. Un botón que no puede
    /// funcionar no debe mostrarse.
    pub fn available(&self) -> Vec<MailboxProviderKind> {
        let mut kinds = vec![MailboxProviderKind::Google];
        if self.microsoft.is_some() {
            kinds.push(MailboxProviderKind::Microsoft);
        }
        kinds
    }

    /// Acceso al cliente de Gmail para las lecturas que aún no pasan por el
    /// trait (catálogo de etiquetas al conectar).
    pub fn gmail(&self) -> &crate::gmail::GmailClient {
        &self.gmail
    }
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
        assert_eq!(providers.available(), vec![MailboxProviderKind::Google]);
    }
}
