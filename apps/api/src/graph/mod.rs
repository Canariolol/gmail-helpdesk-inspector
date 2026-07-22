//! Adaptador de Microsoft Graph: Outlook y Microsoft 365.
//!
//! Es el gemelo estructural del adaptador de Gmail: OAuth, hilos nativos y
//! filtrado del lado del servidor. Las tres diferencias que importan:
//!
//! 1. El hilo es `conversationId` en cada mensaje, no un recurso propio. Se
//!    listan mensajes de la ventana y se agrupan por conversación.
//! 2. No existen las pestañas de Gmail (`CATEGORY_PROMOTIONS`/`SOCIAL`/…), así
//!    que `folder_ids` trae IDs de carpeta y el refinamiento por pestaña queda
//!    en no-op. Ver §6 de `docs/plan-conexion-multiproveedor.md`.
//! 3. El refresh token **rota** en cada uso: quien refresque debe persistir el
//!    nuevo valor o la conexión muere sola.

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Days, NaiveDate, TimeZone, Utc};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::analysis::{AnalysisConfig, EmailMessage, is_automated_sender, is_internal_email};
use crate::mailbox::{
    GmailLabel, GmailProfile, MailboxMetadata, MailboxProvider, MailboxProviderKind,
    ProviderThread, ThreadListPage,
};

const GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";
/// Carpeta bien conocida de Graph. El equivalente funcional del `INBOX` +
/// pestaña Principal de Gmail es simplemente la bandeja de entrada: Outlook no
/// tiene pestañas.
const INBOX_FOLDER: &str = "inbox";
/// Graph pagina de a 1000 como máximo; se pide una sola página por análisis,
/// igual que el adaptador de Gmail.
const MAX_PAGE_SIZE: u32 = 1000;

#[derive(Clone)]
pub struct GraphClient {
    client: Client,
}

impl Default for GraphClient {
    fn default() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("failed to build graph http client"),
        }
    }
}

#[async_trait]
impl MailboxProvider for GraphClient {
    fn kind(&self) -> MailboxProviderKind {
        MailboxProviderKind::Microsoft
    }

    async fn list_thread_ids(
        &self,
        access_token: &str,
        config: &AnalysisConfig,
        max_threads: u32,
    ) -> anyhow::Result<ThreadListPage> {
        // Se piden más mensajes que hilos porque varios mensajes colapsan en una
        // misma conversación; el tope duro lo pone Graph.
        let page_size = max_threads.saturating_mul(4).clamp(1, MAX_PAGE_SIZE);
        let response: MessageListResponse = self
            .client
            .get(message_list_url(config))
            .bearer_auth(access_token)
            .query(&[
                ("$filter", message_list_filter(config)?),
                ("$select", "id,conversationId".to_string()),
                ("$orderby", "receivedDateTime desc".to_string()),
                ("$top", page_size.to_string()),
            ])
            .send()
            .await?
            .error_for_status()
            .context("failed to list Graph messages")?
            .json()
            .await?;

        let mut ids: Vec<String> = Vec::new();
        for message in response.value {
            let Some(conversation_id) = message.conversation_id else {
                continue;
            };
            if !ids.iter().any(|existing| existing == &conversation_id) {
                ids.push(conversation_id);
            }
        }
        // `@odata.nextLink` es la señal de truncación equivalente al
        // `nextPageToken` de Gmail: hay más mensajes en la ventana.
        let next_page_token = response.next_link;
        let truncated_by_cap = ids.len() > max_threads as usize;
        ids.truncate(max_threads as usize);

        Ok(ThreadListPage {
            ids,
            next_page_token: next_page_token
                .or_else(|| truncated_by_cap.then(|| "truncated-by-max-threads".to_string())),
            // Graph no entrega un estimado de total; se omite en vez de inventarlo.
            result_size_estimate: None,
        })
    }

    async fn fetch_thread(
        &self,
        access_token: &str,
        thread_id: &str,
        config: &AnalysisConfig,
    ) -> anyhow::Result<ProviderThread> {
        let response: MessageListResponse = self
            .client
            .get(format!("{GRAPH_BASE}/me/messages"))
            .bearer_auth(access_token)
            // Pide el cuerpo en texto plano en vez de HTML: evita arrastrar el
            // markup al motor de análisis y a la auditoría IA.
            .header("Prefer", "outlook.body-content-type=\"text\"")
            .query(&[
                (
                    "$filter",
                    format!("conversationId eq '{}'", escape_odata(thread_id)),
                ),
                (
                    "$select",
                    "id,conversationId,parentFolderId,subject,from,toRecipients,ccRecipients,\
                     receivedDateTime,bodyPreview,body,internetMessageHeaders"
                        .to_string(),
                ),
                ("$top", MAX_PAGE_SIZE.to_string()),
            ])
            .send()
            .await?
            .error_for_status()
            .with_context(|| format!("failed to fetch Graph conversation {thread_id}"))?
            .json()
            .await?;

        let folder_ids = collect_folder_ids(&response.value);
        let mut messages = response
            .value
            .into_iter()
            .map(|message| normalize_message(message, config))
            .collect::<anyhow::Result<Vec<_>>>()?;
        // Graph rechaza combinar `conversationId` en `$filter` con
        // `$orderby=receivedDateTime` (`InefficientFilter`), así que el orden
        // cronológico se aplica localmente.
        messages.sort_by_key(|message| message.date);

        Ok(ProviderThread {
            id: thread_id.to_string(),
            // La consulta ya vino acotada a la bandeja de entrada al listar; un
            // hilo alcanzado por este camino está en la bandeja principal.
            is_primary_inbox: true,
            folder_ids,
            messages,
        })
    }

    async fn fetch_mailbox_metadata(
        &self,
        access_token: &str,
        now: DateTime<Utc>,
    ) -> MailboxMetadata {
        let profile = self.get_profile(access_token).await.ok();
        let labels = self.list_folders(access_token).await.unwrap_or_default();
        MailboxMetadata {
            profile,
            labels,
            // Graph expone reglas y alias por otras rutas con scopes adicionales;
            // no se piden permisos que Mira no usa.
            send_as: Vec::new(),
            filters_count: 0,
            synced_at: now,
        }
    }
}

impl GraphClient {
    /// Identidad de la casilla. A diferencia de Gmail, sale de `/me` y no de un
    /// endpoint de perfil de correo.
    pub async fn get_profile(&self, access_token: &str) -> anyhow::Result<GmailProfile> {
        let response: GraphUser = self
            .client
            .get(format!("{GRAPH_BASE}/me"))
            .bearer_auth(access_token)
            .query(&[("$select", "mail,userPrincipalName")])
            .send()
            .await?
            .error_for_status()
            .context("failed to read Graph profile")?
            .json()
            .await?;
        let email = response
            .mail
            .or(response.user_principal_name)
            .ok_or_else(|| anyhow!("Graph profile without an email address"))?;
        Ok(GmailProfile {
            email_address: email,
            messages_total: 0,
            threads_total: 0,
        })
    }

    /// Carpetas de la casilla, mapeadas a la misma forma que el catálogo de
    /// etiquetas de Gmail para que la UI de filtros no cambie todavía.
    pub async fn list_folders(&self, access_token: &str) -> anyhow::Result<Vec<GmailLabel>> {
        let response: FolderListResponse = self
            .client
            .get(format!("{GRAPH_BASE}/me/mailFolders"))
            .bearer_auth(access_token)
            .query(&[("$select", "id,displayName"), ("$top", "100")])
            .send()
            .await?
            .error_for_status()
            .context("failed to list Graph mail folders")?
            .json()
            .await?;
        Ok(response
            .value
            .into_iter()
            .map(|folder| GmailLabel {
                id: folder.id,
                name: folder.display_name,
                label_type: "folder".to_string(),
            })
            .collect())
    }
}

/// Ventana de fechas como filtro OData. `date_to` es inclusivo para el usuario,
/// así que el filtro usa el día siguiente como cota exclusiva — mismo criterio
/// que `gmail_end_exclusive`.
fn received_window_filter(config: &AnalysisConfig) -> anyhow::Result<String> {
    let from = parse_day_start(&config.date_from)
        .with_context(|| format!("invalid date_from: {}", config.date_from))?;
    let to = parse_day_start(&config.date_to)
        .and_then(|date| date.checked_add_days(Days::new(1)))
        .with_context(|| format!("invalid date_to: {}", config.date_to))?;
    Ok(format!(
        "receivedDateTime ge {} and receivedDateTime lt {}",
        from.to_rfc3339(),
        to.to_rfc3339()
    ))
}

fn message_list_url(config: &AnalysisConfig) -> String {
    if config.include_labels.is_empty() && config.exclude_labels.is_empty() {
        format!("{GRAPH_BASE}/me/mailFolders/{INBOX_FOLDER}/messages")
    } else {
        format!("{GRAPH_BASE}/me/messages")
    }
}

/// Conserva `receivedDateTime` primero porque Graph lo exige al combinar
/// `$filter` y `$orderby`. Los IDs vienen de Graph y se escapan como OData.
fn message_list_filter(config: &AnalysisConfig) -> anyhow::Result<String> {
    let mut filter = received_window_filter(config)?;
    if !config.include_labels.is_empty() {
        let folders = config
            .include_labels
            .iter()
            .filter(|folder| !folder.trim().is_empty())
            .map(|folder| format!("parentFolderId eq '{}'", escape_odata(folder.trim())))
            .collect::<Vec<_>>();
        if !folders.is_empty() {
            filter.push_str(&format!(" and ({})", folders.join(" or ")));
        }
    }
    for folder in config
        .exclude_labels
        .iter()
        .filter(|folder| !folder.trim().is_empty())
    {
        filter.push_str(&format!(
            " and parentFolderId ne '{}'",
            escape_odata(folder.trim())
        ));
    }
    Ok(filter)
}

fn parse_day_start(value: &str) -> Option<DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    Utc.from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
        .single()
}

/// OData escapa la comilla simple duplicándola. Sin esto, un `conversationId`
/// con comilla rompería el filtro.
fn escape_odata(value: &str) -> String {
    value.replace('\'', "''")
}

fn collect_folder_ids(messages: &[GraphMessage]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for message in messages {
        if let Some(folder) = &message.parent_folder_id
            && !ids.iter().any(|existing| existing == folder)
        {
            ids.push(folder.clone());
        }
    }
    ids
}

fn normalize_message(
    message: GraphMessage,
    config: &AnalysisConfig,
) -> anyhow::Result<EmailMessage> {
    let persisted_headers = persisted_headers(&message.internet_message_headers);
    let (from_name, from_email) = message
        .from
        .as_ref()
        .map(|from| {
            (
                from.email_address.name.clone().filter(|n| !n.is_empty()),
                from.email_address.address.clone().unwrap_or_default(),
            )
        })
        .unwrap_or((None, String::new()));
    let is_internal = is_internal_email(&from_email, &config.internal_domains);
    let is_automated = is_automated_sender(&from_email, &Value::Object(persisted_headers.clone()));
    let date = message
        .received_date_time
        .ok_or_else(|| anyhow!("Graph message without receivedDateTime"))?;

    Ok(EmailMessage {
        id: message.id.clone(),
        message_id: message.id,
        from_email,
        from_name,
        to_emails: addresses(&message.to_recipients),
        cc_emails: addresses(&message.cc_recipients),
        date,
        subject: message
            .subject
            .filter(|subject| !subject.trim().is_empty())
            .unwrap_or_else(|| "(sin asunto)".to_string()),
        snippet: message.body_preview.unwrap_or_default(),
        headers: Value::Object(persisted_headers),
        is_internal,
        is_external: !is_internal,
        is_automated,
        body_text: Some(message.body.map(|body| body.content).unwrap_or_default()),
    })
}

fn addresses(recipients: &[GraphRecipient]) -> Vec<String> {
    recipients
        .iter()
        .filter_map(|recipient| recipient.email_address.address.clone())
        .filter(|address| !address.is_empty())
        .collect()
}

/// Solo se persiste la cabecera de automatización, igual que en Gmail: el resto
/// de las cabeceras son datos personales que Mira no necesita guardar.
fn persisted_headers(headers: &[GraphHeader]) -> Map<String, Value> {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("auto-submitted"))
        .map(|header| {
            Map::from_iter([(
                "auto-submitted".to_string(),
                Value::String(header.value.clone()),
            )])
        })
        .unwrap_or_default()
}

#[derive(Debug, Deserialize)]
struct MessageListResponse {
    #[serde(default)]
    value: Vec<GraphMessage>,
    #[serde(rename = "@odata.nextLink", default)]
    next_link: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphMessage {
    id: String,
    #[serde(rename = "conversationId", default)]
    conversation_id: Option<String>,
    #[serde(rename = "parentFolderId", default)]
    parent_folder_id: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    from: Option<GraphRecipient>,
    #[serde(rename = "toRecipients", default)]
    to_recipients: Vec<GraphRecipient>,
    #[serde(rename = "ccRecipients", default)]
    cc_recipients: Vec<GraphRecipient>,
    #[serde(rename = "receivedDateTime", default)]
    received_date_time: Option<DateTime<Utc>>,
    #[serde(rename = "bodyPreview", default)]
    body_preview: Option<String>,
    #[serde(default)]
    body: Option<GraphBody>,
    #[serde(rename = "internetMessageHeaders", default)]
    internet_message_headers: Vec<GraphHeader>,
}

#[derive(Debug, Deserialize)]
struct GraphRecipient {
    #[serde(rename = "emailAddress", default)]
    email_address: GraphEmailAddress,
}

#[derive(Debug, Default, Deserialize)]
struct GraphEmailAddress {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    address: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphBody {
    #[serde(default)]
    content: String,
}

#[derive(Debug, Deserialize)]
struct GraphHeader {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct FolderListResponse {
    #[serde(default)]
    value: Vec<GraphFolder>,
}

#[derive(Debug, Deserialize)]
struct GraphFolder {
    id: String,
    #[serde(rename = "displayName", default)]
    display_name: String,
}

#[derive(Debug, Deserialize)]
struct GraphUser {
    #[serde(default)]
    mail: Option<String>,
    #[serde(rename = "userPrincipalName", default)]
    user_principal_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AnalysisConfig {
        AnalysisConfig {
            date_from: "2026-03-01".to_string(),
            date_to: "2026-03-31".to_string(),
            time_from: "00:00".to_string(),
            time_to: "23:59".to_string(),
            timezone: "America/Santiago".to_string(),
            internal_domains: vec!["west-ingenieria.cl".to_string()],
            ignored_senders: vec![],
            ignored_domains: vec![],
            ignored_keywords: vec![],
            include_labels: vec![],
            exclude_labels: vec![],
        }
    }

    #[test]
    fn window_filter_uses_an_exclusive_upper_bound() {
        let filter = received_window_filter(&config()).unwrap();
        assert!(filter.contains("receivedDateTime ge 2026-03-01T00:00:00+00:00"));
        // 2026-03-31 es inclusivo para el usuario: la cota es el 1 de abril.
        assert!(filter.contains("receivedDateTime lt 2026-04-01T00:00:00+00:00"));
    }

    #[test]
    fn folder_filters_use_all_messages_and_keep_date_filter_first() {
        let mut config = config();
        config.include_labels = vec!["inbox-id".to_string(), "team's-folder".to_string()];
        config.exclude_labels = vec!["deleted-id".to_string()];

        assert_eq!(
            message_list_url(&config),
            format!("{GRAPH_BASE}/me/messages")
        );
        assert_eq!(
            message_list_filter(&config).unwrap(),
            "receivedDateTime ge 2026-03-01T00:00:00+00:00 and receivedDateTime lt 2026-04-01T00:00:00+00:00 and (parentFolderId eq 'inbox-id' or parentFolderId eq 'team''s-folder') and parentFolderId ne 'deleted-id'"
        );
    }

    #[test]
    fn odata_filter_escapes_single_quotes_in_conversation_ids() {
        assert_eq!(escape_odata("AAQk'AGI"), "AAQk''AGI");
    }

    #[test]
    fn normalize_message_keeps_only_the_automation_header() {
        let message = GraphMessage {
            id: "m1".to_string(),
            conversation_id: Some("c1".to_string()),
            parent_folder_id: Some("inbox".to_string()),
            subject: Some("Consulta".to_string()),
            from: Some(GraphRecipient {
                email_address: GraphEmailAddress {
                    name: Some("Cliente".to_string()),
                    address: Some("cliente@externo.cl".to_string()),
                },
            }),
            to_recipients: vec![GraphRecipient {
                email_address: GraphEmailAddress {
                    name: None,
                    address: Some("soporte@west-ingenieria.cl".to_string()),
                },
            }],
            cc_recipients: Vec::new(),
            received_date_time: Some(Utc.with_ymd_and_hms(2026, 3, 2, 10, 0, 0).unwrap()),
            body_preview: Some("hola".to_string()),
            body: Some(GraphBody {
                content: "cuerpo".to_string(),
            }),
            internet_message_headers: vec![
                GraphHeader {
                    name: "Auto-Submitted".to_string(),
                    value: "auto-generated".to_string(),
                },
                GraphHeader {
                    name: "X-Interno".to_string(),
                    value: "valor-privado".to_string(),
                },
            ],
        };

        let normalized = normalize_message(message, &config()).unwrap();

        assert_eq!(
            normalized.headers,
            serde_json::json!({ "auto-submitted": "auto-generated" })
        );
        assert!(normalized.is_automated);
        assert!(normalized.is_external);
        assert_eq!(normalized.from_email, "cliente@externo.cl");
        assert_eq!(normalized.to_emails, vec!["soporte@west-ingenieria.cl"]);
        assert_eq!(normalized.body_text.as_deref(), Some("cuerpo"));
    }

    #[test]
    fn thread_ids_are_deduplicated_conversation_ids_in_order() {
        let response: MessageListResponse = serde_json::from_value(serde_json::json!({
            "value": [
                { "id": "m1", "conversationId": "c1" },
                { "id": "m2", "conversationId": "c2" },
                { "id": "m3", "conversationId": "c1" },
                { "id": "m4" },
            ]
        }))
        .unwrap();

        let mut ids: Vec<String> = Vec::new();
        for message in response.value {
            if let Some(conversation_id) = message.conversation_id
                && !ids.contains(&conversation_id)
            {
                ids.push(conversation_id);
            }
        }
        assert_eq!(ids, vec!["c1".to_string(), "c2".to_string()]);
    }
}
