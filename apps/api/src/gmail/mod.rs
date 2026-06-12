use anyhow::{Context, anyhow};
use base64::{Engine, engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD}};
use chrono::{DateTime, Utc};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::analysis::{AnalysisConfig, EmailMessage, is_automated_sender, is_internal_email};

#[derive(Clone)]
pub struct GmailClient {
    client: Client,
}

#[derive(Debug, Clone)]
pub struct GmailThreadData {
    pub id: String,
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
            client: Client::new(),
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
        let query = format!(
            "after:{} before:{}",
            config.date_from.replace('-', "/"),
            config.date_to.replace('-', "/")
        );
        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/threads?q={}&maxResults={}",
            utf8_percent_encode(&query, NON_ALPHANUMERIC),
            max_threads
        );
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
        Ok(response.threads.into_iter().map(|thread| thread.id).collect())
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

        let messages = response
            .messages
            .into_iter()
            .map(|message| normalize_message(message, config))
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(GmailThreadData {
            id: response.id,
            messages,
        })
    }
}

fn normalize_message(message: GmailMessageResponse, config: &AnalysisConfig) -> anyhow::Result<EmailMessage> {
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
    headers.get(&name.to_lowercase()).and_then(|v| v.as_str()).map(ToOwned::to_owned)
}

fn parse_date(headers: &Map<String, Value>, internal_date: Option<&str>) -> anyhow::Result<DateTime<Utc>> {
    if let Some(date) = header(headers, "date") {
        if let Ok(parsed) = DateTime::parse_from_rfc2822(&date) {
            return Ok(parsed.with_timezone(&Utc));
        }
    }
    if let Some(ms) = internal_date.and_then(|v| v.parse::<i64>().ok()) {
        return DateTime::from_timestamp_millis(ms).ok_or_else(|| anyhow!("invalid Gmail internalDate"));
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
    if let Some(body) = &payload.body {
        if let Some(data) = &body.data {
            if payload.mime_type.as_deref().unwrap_or("").starts_with("text/") {
                if let Ok(bytes) = URL_SAFE_NO_PAD.decode(data.as_bytes()).or_else(|_| URL_SAFE.decode(data.as_bytes())) {
                    if let Ok(text) = String::from_utf8(bytes) {
                        if payload.mime_type.as_deref() == Some("text/html") {
                            chunks.push(html_to_text(&text));
                        } else {
                            chunks.push(text);
                        }
                    }
                }
            }
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
