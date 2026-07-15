use anyhow::{Context, anyhow};
use base64::{
    Engine,
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
};
use chrono::{DateTime, Days, NaiveDate, Utc};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::analysis::{AnalysisConfig, EmailMessage, is_automated_sender, is_internal_email};
use crate::mailbox::{GmailLabel, GmailProfile, GmailSendAs, MailboxMetadata};

const PRIMARY_INBOX_LABELS: [&str; 2] = ["INBOX", "CATEGORY_PERSONAL"];
const EXCLUDED_MESSAGE_LABELS: [&str; 2] = ["SPAM", "TRASH"];

#[derive(Clone)]
pub struct GmailClient {
    client: Client,
}

#[derive(Debug, Clone)]
pub struct GmailThreadData {
    pub id: String,
    pub is_primary_inbox: bool,
    /// Unión de las etiquetas de Gmail del hilo (INBOX, CATEGORY_*, etiquetas de
    /// usuario…). Señal para refinar la clasificación.
    pub label_ids: Vec<String>,
    pub messages: Vec<EmailMessage>,
}

#[derive(Debug, Deserialize)]
struct ThreadListResponse {
    #[serde(default)]
    threads: Vec<ThreadRef>,
    /// Presente cuando hay más resultados de los que cupieron en `maxResults`:
    /// señal de que la recuperación quedó truncada (hay más hilos en el rango).
    #[serde(rename = "nextPageToken", default)]
    next_page_token: Option<String>,
    /// Estimación de Gmail del total de resultados (aproximada).
    #[serde(rename = "resultSizeEstimate", default)]
    result_size_estimate: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ThreadRef {
    id: String,
}

/// Página de IDs de hilos recuperados de Gmail, con señales de truncación para
/// poder informar "recuperamos N, pero hay más" sin una segunda llamada.
#[derive(Debug, Clone)]
pub struct ThreadListPage {
    pub ids: Vec<String>,
    pub next_page_token: Option<String>,
    pub result_size_estimate: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GmailThreadResponse {
    id: String,
    #[serde(default)]
    messages: Vec<GmailMessageResponse>,
}

#[derive(Debug, Deserialize)]
struct GmailMessageResponse {
    id: String,
    #[serde(default)]
    snippet: String,
    #[serde(rename = "labelIds", default)]
    label_ids: Vec<String>,
    payload: GmailPayload,
    #[serde(rename = "internalDate")]
    internal_date: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct GmailPayload {
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
    #[serde(default)]
    filename: String,
    #[serde(default)]
    headers: Vec<GmailHeader>,
    body: Option<GmailBody>,
    #[serde(default)]
    parts: Vec<GmailPayload>,
}

#[derive(Debug, Deserialize)]
struct GmailHeader {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct GmailBody {
    data: Option<String>,
    #[serde(rename = "attachmentId")]
    attachment_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LabelsListResponse {
    #[serde(default)]
    labels: Vec<LabelResource>,
}

#[derive(Debug, Deserialize)]
struct LabelResource {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(rename = "type", default)]
    label_type: String,
}

#[derive(Debug, Deserialize)]
struct ProfileResponse {
    #[serde(rename = "emailAddress", default)]
    email_address: String,
    #[serde(rename = "messagesTotal", default)]
    messages_total: u64,
    #[serde(rename = "threadsTotal", default)]
    threads_total: u64,
}

#[derive(Debug, Deserialize)]
struct SendAsListResponse {
    #[serde(rename = "sendAs", default)]
    send_as: Vec<SendAsResource>,
}

#[derive(Debug, Deserialize)]
struct SendAsResource {
    #[serde(rename = "sendAsEmail", default)]
    send_as_email: String,
    #[serde(rename = "displayName", default)]
    display_name: String,
    #[serde(rename = "isPrimary", default)]
    is_primary: bool,
    #[serde(rename = "isDefault", default)]
    is_default: bool,
    #[serde(rename = "treatAsAlias", default)]
    treat_as_alias: bool,
}

#[derive(Debug, Deserialize)]
struct FiltersListResponse {
    #[serde(default)]
    filter: Vec<serde_json::Value>,
}

impl Default for GmailClient {
    fn default() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("failed to build gmail http client"),
        }
    }
}

impl GmailClient {
    pub async fn list_thread_ids(
        &self,
        access_token: &str,
        config: &AnalysisConfig,
        max_threads: u32,
    ) -> anyhow::Result<ThreadListPage> {
        let url = build_thread_list_url(config, max_threads);
        let response: ThreadListResponse = self
            .client
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await?
            .error_for_status()
            .context("failed to list Gmail threads")?
            .json()
            .await?;
        Ok(ThreadListPage {
            ids: response
                .threads
                .into_iter()
                .map(|thread| thread.id)
                .collect(),
            next_page_token: response.next_page_token.filter(|token| !token.is_empty()),
            result_size_estimate: response.result_size_estimate,
        })
    }

    pub async fn fetch_thread(
        &self,
        access_token: &str,
        thread_id: &str,
        config: &AnalysisConfig,
    ) -> anyhow::Result<GmailThreadData> {
        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/threads/{thread_id}?format=full"
        );
        let response: GmailThreadResponse = self
            .client
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await?
            .error_for_status()
            .with_context(|| format!("failed to fetch Gmail thread {thread_id}"))?
            .json()
            .await?;

        let is_primary_inbox = thread_has_all_labels(&response.messages, &PRIMARY_INBOX_LABELS);
        let label_ids = collect_thread_labels(&response.messages);
        let messages = normalize_visible_messages(response.messages, config)?;
        Ok(GmailThreadData {
            id: response.id,
            is_primary_inbox,
            label_ids,
            messages,
        })
    }

    /// Catálogo de etiquetas (system + usuario). Bajo `gmail.readonly`.
    pub async fn list_labels(&self, access_token: &str) -> anyhow::Result<Vec<GmailLabel>> {
        let response: LabelsListResponse = self
            .client
            .get("https://gmail.googleapis.com/gmail/v1/users/me/labels")
            .bearer_auth(access_token)
            .send()
            .await?
            .error_for_status()
            .context("failed to list Gmail labels")?
            .json()
            .await?;
        Ok(response
            .labels
            .into_iter()
            .map(|label| GmailLabel {
                id: label.id,
                name: label.name,
                label_type: label.label_type,
            })
            .collect())
    }

    pub async fn get_profile(&self, access_token: &str) -> anyhow::Result<GmailProfile> {
        let response: ProfileResponse = self
            .client
            .get("https://gmail.googleapis.com/gmail/v1/users/me/profile")
            .bearer_auth(access_token)
            .send()
            .await?
            .error_for_status()
            .context("failed to fetch Gmail profile")?
            .json()
            .await?;
        Ok(GmailProfile {
            email_address: response.email_address,
            messages_total: response.messages_total,
            threads_total: response.threads_total,
        })
    }

    /// Alias "enviar como" / direcciones de envío. Bajo `gmail.readonly`.
    pub async fn list_send_as(&self, access_token: &str) -> anyhow::Result<Vec<GmailSendAs>> {
        let response: SendAsListResponse = self
            .client
            .get("https://gmail.googleapis.com/gmail/v1/users/me/settings/sendAs")
            .bearer_auth(access_token)
            .send()
            .await?
            .error_for_status()
            .context("failed to list Gmail send-as aliases")?
            .json()
            .await?;
        Ok(response
            .send_as
            .into_iter()
            .map(|item| GmailSendAs {
                email: item.send_as_email,
                display_name: item.display_name,
                is_primary: item.is_primary,
                is_default: item.is_default,
                treat_as_alias: item.treat_as_alias,
            })
            .collect())
    }

    pub async fn count_filters(&self, access_token: &str) -> anyhow::Result<u32> {
        let response: FiltersListResponse = self
            .client
            .get("https://gmail.googleapis.com/gmail/v1/users/me/settings/filters")
            .bearer_auth(access_token)
            .send()
            .await?
            .error_for_status()
            .context("failed to list Gmail filters")?
            .json()
            .await?;
        Ok(response.filter.len() as u32)
    }

    /// Lectura best-effort de toda la metadata al conectar. Cada parte tolera su
    /// propio error para no abortar el login si una llamada de Gmail falla.
    pub async fn fetch_mailbox_metadata(
        &self,
        access_token: &str,
        now: DateTime<Utc>,
    ) -> MailboxMetadata {
        let profile = self.get_profile(access_token).await.ok();
        let labels = self.list_labels(access_token).await.unwrap_or_default();
        let send_as = self.list_send_as(access_token).await.unwrap_or_default();
        let filters_count = self.count_filters(access_token).await.unwrap_or(0);
        MailboxMetadata {
            profile,
            labels,
            send_as,
            filters_count,
            synced_at: now,
        }
    }
}

fn build_thread_list_url(config: &AnalysisConfig, max_threads: u32) -> String {
    let query = gmail_thread_search_query(config);
    let mut url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/threads?q={}&maxResults={}&includeSpamTrash=false",
        utf8_percent_encode(&query, NON_ALPHANUMERIC),
        max_threads
    );
    // Sin selección de etiquetas se mantiene el comportamiento histórico: bandeja
    // principal (INBOX + pestaña Principal). Con selección, el filtrado vive en
    // el parámetro `q` (los operadores label:/category: permiten OR y exclusión,
    // que labelIds no soporta).
    if config.include_labels.is_empty() && config.exclude_labels.is_empty() {
        url.push_str("&labelIds=INBOX&labelIds=CATEGORY_PERSONAL");
    }
    url
}

fn gmail_thread_search_query(config: &AnalysisConfig) -> String {
    let mut parts = vec![format!(
        "after:{} before:{}",
        config.date_from.replace('-', "/"),
        gmail_end_exclusive(&config.date_to)
    )];

    let includes: Vec<String> = config
        .include_labels
        .iter()
        .filter_map(|label| label_query_token(label))
        .collect();
    match includes.len() {
        0 => {}
        1 => parts.push(includes.into_iter().next().expect("len == 1")),
        _ => parts.push(format!("({})", includes.join(" OR "))),
    }

    for label in &config.exclude_labels {
        if let Some(token) = label_query_token(label) {
            parts.push(format!("-{token}"));
        }
    }

    parts.join(" ")
}

/// Convierte una etiqueta seleccionada en un operador de búsqueda de Gmail.
/// Las categorías (`CATEGORY_*`) usan `category:`; las etiquetas de sistema
/// conocidas usan `in:`/`is:`; el resto usa `label:"<nombre>"` (comillas para
/// soportar espacios/acentos en etiquetas de usuario).
fn label_query_token(label: &str) -> Option<String> {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return None;
    }
    let upper = trimmed.to_ascii_uppercase();
    if let Some(category) = upper.strip_prefix("CATEGORY_") {
        return Some(format!("category:{}", category.to_ascii_lowercase()));
    }
    let token = match upper.as_str() {
        "INBOX" => "in:inbox".to_string(),
        "SENT" => "in:sent".to_string(),
        "SPAM" => "in:spam".to_string(),
        "TRASH" => "in:trash".to_string(),
        "IMPORTANT" => "is:important".to_string(),
        "STARRED" => "is:starred".to_string(),
        "UNREAD" => "is:unread".to_string(),
        _ => format!("label:\"{}\"", trimmed.replace('"', "")),
    };
    Some(token)
}

fn gmail_end_exclusive(date_to: &str) -> String {
    NaiveDate::parse_from_str(date_to, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.checked_add_days(Days::new(1)))
        .map(|date| date.format("%Y/%m/%d").to_string())
        .unwrap_or_else(|| date_to.replace('-', "/"))
}

fn message_has_label(message: &GmailMessageResponse, label: &str) -> bool {
    message
        .label_ids
        .iter()
        .any(|message_label| message_label.eq_ignore_ascii_case(label))
}

fn message_has_any_label(message: &GmailMessageResponse, labels: &[&str]) -> bool {
    labels.iter().any(|label| message_has_label(message, label))
}

fn message_has_all_labels(message: &GmailMessageResponse, labels: &[&str]) -> bool {
    labels.iter().all(|label| message_has_label(message, label))
}

fn thread_has_all_labels(messages: &[GmailMessageResponse], labels: &[&str]) -> bool {
    messages
        .iter()
        .any(|message| message_has_all_labels(message, labels))
}

/// Unión deduplicada de las etiquetas de todos los mensajes del hilo.
fn collect_thread_labels(messages: &[GmailMessageResponse]) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    for message in messages {
        for label in &message.label_ids {
            if !labels.iter().any(|existing| existing == label) {
                labels.push(label.clone());
            }
        }
    }
    labels
}

fn normalize_visible_messages(
    messages: Vec<GmailMessageResponse>,
    config: &AnalysisConfig,
) -> anyhow::Result<Vec<EmailMessage>> {
    messages
        .into_iter()
        .filter(|message| !message_has_any_label(message, &EXCLUDED_MESSAGE_LABELS))
        .map(|message| normalize_message(message, config))
        .collect()
}

fn normalize_message(
    message: GmailMessageResponse,
    config: &AnalysisConfig,
) -> anyhow::Result<EmailMessage> {
    let headers = header_map(&message.payload.headers);
    let persisted_headers = persisted_headers(&headers);
    let subject = header(&headers, "subject").unwrap_or_else(|| "(sin asunto)".to_string());
    let from = header(&headers, "from").unwrap_or_default();
    let (from_name, from_email) = parse_mailbox(&from);
    let to_emails = parse_address_list(header(&headers, "to").unwrap_or_default().as_str());
    let cc_emails = parse_address_list(header(&headers, "cc").unwrap_or_default().as_str());
    let date = parse_date(&headers, message.internal_date.as_deref())?;
    let is_internal = is_internal_email(&from_email, &config.internal_domains);
    let is_automated = is_automated_sender(&from_email, &Value::Object(persisted_headers.clone()));

    Ok(EmailMessage {
        id: message.id.clone(),
        gmail_message_id: message.id,
        from_email,
        from_name,
        to_emails,
        cc_emails,
        date,
        subject,
        snippet: message.snippet,
        headers: Value::Object(persisted_headers),
        is_internal,
        is_external: !is_internal,
        is_automated,
        body_text: Some(extract_text(&message.payload)),
    })
}

fn header_map(headers: &[GmailHeader]) -> Map<String, Value> {
    headers
        .iter()
        .map(|header| (header.name.to_lowercase(), json!(header.value)))
        .collect()
}

fn persisted_headers(headers: &Map<String, Value>) -> Map<String, Value> {
    headers
        .get("auto-submitted")
        .cloned()
        .map(|value| Map::from_iter([("auto-submitted".to_string(), value)]))
        .unwrap_or_default()
}

fn header(headers: &Map<String, Value>, name: &str) -> Option<String> {
    headers
        .get(&name.to_lowercase())
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
}

fn parse_date(
    headers: &Map<String, Value>,
    internal_date: Option<&str>,
) -> anyhow::Result<DateTime<Utc>> {
    if let Some(date) = header(headers, "date")
        && let Ok(parsed) = DateTime::parse_from_rfc2822(&date)
    {
        return Ok(parsed.with_timezone(&Utc));
    }
    if let Some(ms) = internal_date.and_then(|v| v.parse::<i64>().ok()) {
        return DateTime::from_timestamp_millis(ms)
            .ok_or_else(|| anyhow!("invalid Gmail internalDate"));
    }
    Ok(Utc::now())
}

fn parse_mailbox(raw: &str) -> (Option<String>, String) {
    let trimmed = raw.trim();
    if let (Some(start), Some(end)) = (trimmed.rfind('<'), trimmed.rfind('>')) {
        let name = trimmed[..start].trim().trim_matches('"').to_string();
        let email = trimmed[start + 1..end].trim().to_lowercase();
        return ((if name.is_empty() { None } else { Some(name) }), email);
    }
    (None, trimmed.trim_matches('"').to_lowercase())
}

fn parse_address_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|part| parse_mailbox(part).1)
        .filter(|email| !email.is_empty())
        .collect()
}

fn extract_text(payload: &GmailPayload) -> String {
    let mut chunks = Vec::new();
    collect_text(payload, &mut chunks);
    chunks.join("\n\n")
}

fn collect_text(payload: &GmailPayload, chunks: &mut Vec<String>) {
    if let Some(body) = &payload.body
        && let Some(data) = &body.data
        && payload.filename.is_empty()
        && body.attachment_id.is_none()
        && payload
            .mime_type
            .as_deref()
            .unwrap_or("")
            .starts_with("text/")
        && let Ok(bytes) = URL_SAFE_NO_PAD
            .decode(data.as_bytes())
            .or_else(|_| URL_SAFE.decode(data.as_bytes()))
        && let Ok(text) = String::from_utf8(bytes)
    {
        if payload.mime_type.as_deref() == Some("text/html") {
            chunks.push(html_to_text(&text));
        } else {
            chunks.push(text);
        }
    }
    for part in &payload.parts {
        collect_text(part, chunks);
    }
}

fn html_to_text(html: &str) -> String {
    let fragment = scraper::Html::parse_fragment(html);
    fragment.root_element().text().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_list_response_reads_truncation_signals() {
        let json = r#"{
            "threads": [{"id": "a"}, {"id": "b"}],
            "nextPageToken": "tok-123",
            "resultSizeEstimate": 137
        }"#;
        let parsed: ThreadListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.threads.len(), 2);
        assert_eq!(parsed.next_page_token.as_deref(), Some("tok-123"));
        assert_eq!(parsed.result_size_estimate, Some(137));
    }

    #[test]
    fn thread_list_response_without_truncation_signals() {
        let json = r#"{"threads": [{"id": "a"}]}"#;
        let parsed: ThreadListResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.next_page_token.is_none());
        assert!(parsed.result_size_estimate.is_none());
    }

    fn config() -> AnalysisConfig {
        AnalysisConfig {
            date_from: "2026-06-01".to_string(),
            date_to: "2026-06-12".to_string(),
            time_from: "00:00".to_string(),
            time_to: "23:59".to_string(),
            timezone: "America/Santiago".to_string(),
            internal_domains: vec!["company.test".to_string()],
            ignored_senders: vec![],
            ignored_domains: vec![],
            ignored_keywords: vec![],
            include_labels: vec![],
            exclude_labels: vec![],
        }
    }

    #[test]
    fn query_without_labels_keeps_legacy_inbox_behavior() {
        let url = build_thread_list_url(&config(), 50);
        assert!(url.contains("labelIds=INBOX&labelIds=CATEGORY_PERSONAL"));
    }

    #[test]
    fn query_includes_and_excludes_selected_labels() {
        let mut cfg = config();
        cfg.include_labels = vec!["CATEGORY_PROMOTIONS".to_string(), "Soporte".to_string()];
        cfg.exclude_labels = vec!["CATEGORY_SOCIAL".to_string()];
        let query = gmail_thread_search_query(&cfg);
        assert!(query.contains("category:promotions"));
        assert!(query.contains("label:\"Soporte\""));
        assert!(query.contains(" OR "));
        assert!(query.contains("-category:social"));

        // Con selección de etiquetas no se anexan los labelIds fijos.
        let url = build_thread_list_url(&cfg, 50);
        assert!(!url.contains("labelIds=INBOX"));
    }

    fn gmail_message(id: &str, labels: Vec<&str>) -> GmailMessageResponse {
        GmailMessageResponse {
            id: id.to_string(),
            snippet: "hola".to_string(),
            label_ids: labels.into_iter().map(str::to_string).collect(),
            payload: GmailPayload {
                mime_type: None,
                filename: String::new(),
                headers: vec![
                    GmailHeader {
                        name: "From".to_string(),
                        value: "Cliente <client@example.com>".to_string(),
                    },
                    GmailHeader {
                        name: "Subject".to_string(),
                        value: "Ayuda".to_string(),
                    },
                ],
                body: None,
                parts: vec![],
            },
            internal_date: Some("1780358400000".to_string()),
        }
    }

    #[test]
    fn thread_list_url_requests_primary_inbox_and_excludes_spam_and_trash() {
        let url = url::Url::parse(&build_thread_list_url(&config(), 25)).unwrap();
        let query = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        let label_ids = url
            .query_pairs()
            .filter_map(|(key, value)| (key == "labelIds").then(|| value.to_string()))
            .collect::<Vec<_>>();

        assert_eq!(
            query.get("q").map(|value| value.as_ref()),
            Some("after:2026/06/01 before:2026/06/13")
        );
        assert_eq!(
            query.get("maxResults").map(|value| value.as_ref()),
            Some("25")
        );
        assert_eq!(
            query.get("includeSpamTrash").map(|value| value.as_ref()),
            Some("false")
        );
        assert_eq!(label_ids, vec!["INBOX", "CATEGORY_PERSONAL"]);
    }

    #[test]
    fn normalize_visible_messages_omits_messages_labeled_as_spam_or_trash() {
        let messages = normalize_visible_messages(
            vec![
                gmail_message("keep", vec!["INBOX"]),
                gmail_message("skip", vec!["SPAM"]),
                gmail_message("trash", vec!["TRASH"]),
            ],
            &config(),
        )
        .unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, "keep");
    }

    #[test]
    fn extract_text_excludes_parts_marked_as_attachments() {
        let payload = GmailPayload {
            mime_type: Some("multipart/mixed".to_string()),
            filename: String::new(),
            headers: vec![],
            body: None,
            parts: vec![
                GmailPayload {
                    mime_type: Some("text/plain".to_string()),
                    filename: String::new(),
                    headers: vec![],
                    body: Some(GmailBody {
                        data: Some(URL_SAFE_NO_PAD.encode("cuerpo del correo")),
                        attachment_id: None,
                    }),
                    parts: vec![],
                },
                GmailPayload {
                    mime_type: Some("text/plain".to_string()),
                    filename: "secreto.txt".to_string(),
                    headers: vec![],
                    body: Some(GmailBody {
                        data: Some(URL_SAFE_NO_PAD.encode("contenido adjunto")),
                        attachment_id: Some("attachment-id".to_string()),
                    }),
                    parts: vec![],
                },
            ],
        };

        assert_eq!(extract_text(&payload), "cuerpo del correo");
    }

    #[test]
    fn normalize_message_persists_only_the_automation_header() {
        let mut message = gmail_message("keep", vec!["INBOX"]);
        message.payload.headers.push(GmailHeader {
            name: "Auto-Submitted".to_string(),
            value: "auto-generated".to_string(),
        });
        message.payload.headers.push(GmailHeader {
            name: "X-Internal-Trace".to_string(),
            value: "private-value".to_string(),
        });

        let normalized = normalize_message(message, &config()).unwrap();

        assert_eq!(
            normalized.headers,
            json!({ "auto-submitted": "auto-generated" })
        );
        assert!(normalized.is_automated);
    }

    #[test]
    fn thread_primary_inbox_requires_inbox_and_personal_labels_on_one_message() {
        assert!(thread_has_all_labels(
            &[
                gmail_message("primary", vec!["INBOX", "CATEGORY_PERSONAL"]),
                gmail_message("sent", vec!["SENT"]),
            ],
            &PRIMARY_INBOX_LABELS,
        ));
        assert!(!thread_has_all_labels(
            &[
                gmail_message("promo", vec!["INBOX", "CATEGORY_PROMOTIONS"]),
                gmail_message("sent", vec!["SENT"]),
            ],
            &PRIMARY_INBOX_LABELS,
        ));
    }
}
