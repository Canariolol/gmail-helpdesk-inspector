use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Credencial de la casilla, independiente de cualquier sesión web. Una misma
/// conexión permite análisis programados aunque la persona cierre sus sesiones.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmailConnection {
    pub owner_email: String,
    pub gmail_account_email: String,
    pub access_token_encrypted: String,
    #[serde(default)]
    pub refresh_token_encrypted: Option<String>,
    pub connected_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub revoked_at: Option<DateTime<Utc>>,
}

pub fn gmail_connection_is_active(connection: Option<&GmailConnection>) -> bool {
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
