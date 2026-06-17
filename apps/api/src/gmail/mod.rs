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
    pub messages: Vec<EmailMessage>,
}

#[derive(Debug, Deserialize)]
struct ThreadListResponse {
    #[serde(default)]
    threads: Vec<ThreadRef>,
}

#[derive(Debug, Deserialize)]
struct ThreadRef {
    id: String,
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
    ) -> anyhow::Result<Vec<String>> {
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
        Ok(response
            .threads
            .into_iter()
            .map(|thread| thread.id)
            .collect())
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
        let messages = normalize_visible_messages(response.messages, config)?;
        Ok(GmailThreadData {
            id: response.id,
            is_primary_inbox,
            messages,
        })
    }
}

fn build_thread_list_url(config: &AnalysisConfig, max_threads: u32) -> String {
    let query = gmail_thread_search_query(config);
    format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/threads?q={}&maxResults={}&includeSpamTrash=false&labelIds=INBOX&labelIds=CATEGORY_PERSONAL",
        utf8_percent_encode(&query, NON_ALPHANUMERIC),
        max_threads
    )
}

fn gmail_thread_search_query(config: &AnalysisConfig) -> String {
    format!(
        "after:{} before:{}",
        config.date_from.replace('-', "/"),
        gmail_end_exclusive(&config.date_to)
    )
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
    let subject = header(&headers, "subject").unwrap_or_else(|| "(sin asunto)".to_string());
    let from = header(&headers, "from").unwrap_or_default();
    let (from_name, from_email) = parse_mailbox(&from);
    let to_emails = parse_address_list(header(&headers, "to").unwrap_or_default().as_str());
    let cc_emails = parse_address_list(header(&headers, "cc").unwrap_or_default().as_str());
    let date = parse_date(&headers, message.internal_date.as_deref())?;
    let is_internal = is_internal_email(&from_email, &config.internal_domains);
    let is_automated = is_automated_sender(&from_email, &Value::Object(headers.clone()));

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
        headers: Value::Object(headers),
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
        }
    }

    fn gmail_message(id: &str, labels: Vec<&str>) -> GmailMessageResponse {
        GmailMessageResponse {
            id: id.to_string(),
            snippet: "hola".to_string(),
            label_ids: labels.into_iter().map(str::to_string).collect(),
            payload: GmailPayload {
                mime_type: None,
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
