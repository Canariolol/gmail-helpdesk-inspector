//! Correo Microsoft Graph para casillas propias, compartidas y delegadas.

use std::collections::{HashSet, VecDeque};

use anyhow::{Context, anyhow, ensure};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::analysis::{AnalysisConfig, EmailMessage, is_automated_sender, is_responder_email};
use crate::mailbox::{
    GmailLabel, GmailProfile, MailboxMetadata, MailboxProvider, MailboxProviderKind,
    ProviderThread, ThreadListPage, analysis_window_utc, read_mail_json, send_mail_request,
};

const GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";
const INBOX_FOLDER: &str = "inbox";
const MAX_PAGE_SIZE: u32 = 1000;
const MAX_LIST_PAGES: usize = 50;
const MAX_CONVERSATION_MESSAGES: usize = 2_000;
const MAX_FOLDERS: usize = 1_000;
const MAX_FOLDER_REQUESTS: usize = 1_000;
const MAX_FOLDER_DEPTH: usize = 20;

#[derive(Clone)]
pub struct GraphClient {
    client: Client,
    mailbox: Option<String>,
    base_url: String,
}

impl Default for GraphClient {
    fn default() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .connect_timeout(std::time::Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("failed to build graph http client"),
            mailbox: None,
            base_url: GRAPH_BASE.to_string(),
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
        tokio::time::timeout(
            std::time::Duration::from_secs(180),
            self.list(access_token, config, max_threads),
        )
        .await
        .context("Graph conversation listing timed out")?
    }

    async fn fetch_thread(
        &self,
        access_token: &str,
        thread_id: &str,
        config: &AnalysisConfig,
    ) -> anyhow::Result<ProviderThread> {
        tokio::time::timeout(
            std::time::Duration::from_secs(180),
            self.thread(access_token, thread_id, config),
        )
        .await
        .context("Graph conversation retrieval timed out")?
    }

    async fn fetch_mailbox_metadata(
        &self,
        access_token: &str,
        now: DateTime<Utc>,
    ) -> MailboxMetadata {
        let profile = match &self.mailbox {
            Some(mailbox) => Some(GmailProfile {
                email_address: mailbox.clone(),
                messages_total: 0,
                threads_total: 0,
            }),
            None => self.get_profile(access_token).await.ok(),
        };
        let (labels, folders_truncated) = match tokio::time::timeout(
            std::time::Duration::from_secs(120),
            self.folder_catalog(access_token),
        )
        .await
        {
            Ok(Ok(catalog)) => catalog,
            _ => (Vec::new(), true),
        };
        MailboxMetadata {
            profile,
            labels,
            send_as: Vec::new(),
            filters_count: 0,
            folders_truncated,
            synced_at: now,
        }
    }
}

impl GraphClient {
    async fn list(
        &self,
        access_token: &str,
        config: &AnalysisConfig,
        max_threads: u32,
    ) -> anyhow::Result<ThreadListPage> {
        ensure!(max_threads > 0, "max_threads must be positive");
        let page_size = max_threads.saturating_mul(4).clamp(1, MAX_PAGE_SIZE);
        let request = self
            .client
            .get(self.message_list_url(config))
            .bearer_auth(access_token)
            .query(&[
                ("$filter", message_list_filter(config)?),
                ("$select", "id,conversationId".to_string()),
                ("$orderby", "receivedDateTime desc".to_string()),
                ("$top", page_size.to_string()),
            ]);
        let mut response: MessageListResponse =
            read_mail_json(send_mail_request(&self.client, request).await?).await?;
        let mut ids = Vec::new();
        let mut seen = HashSet::new();
        let mut pages = 0;
        loop {
            pages += 1;
            let mut remaining_in_page = false;
            for message in response.value {
                if let Some(id) = message.conversation_id
                    && seen.insert(id.clone())
                {
                    if ids.len() == max_threads as usize {
                        remaining_in_page = true;
                        break;
                    }
                    ids.push(id);
                }
            }
            if remaining_in_page || ids.len() == max_threads as usize || pages == MAX_LIST_PAGES {
                return Ok(ThreadListPage {
                    ids,
                    next_page_token: response.next_link.or_else(|| {
                        remaining_in_page.then(|| "truncated-by-max-threads".to_string())
                    }),
                    result_size_estimate: None,
                });
            }
            let Some(next) = response.next_link else {
                return Ok(ThreadListPage {
                    ids,
                    next_page_token: None,
                    result_size_estimate: None,
                });
            };
            response = self.get_page(access_token, &next).await?;
        }
    }

    async fn thread(
        &self,
        access_token: &str,
        thread_id: &str,
        config: &AnalysisConfig,
    ) -> anyhow::Result<ProviderThread> {
        let request = self.client.get(format!("{}/messages", self.mailbox_url()))
            .bearer_auth(access_token)
            .header("Prefer", "outlook.body-content-type=\"text\", IdType=\"ImmutableId\"")
            .query(&[
                ("$filter", format!("conversationId eq '{}'", escape_odata(thread_id))),
                ("$select", "id,conversationId,parentFolderId,subject,from,toRecipients,ccRecipients,receivedDateTime,bodyPreview,body,internetMessageHeaders".to_string()),
                ("$top", "200".to_string()),
            ]);
        let mut response: MessageListResponse =
            read_mail_json(send_mail_request(&self.client, request).await?).await?;
        let mut raw = Vec::new();
        let mut seen = HashSet::new();
        let mut truncated = false;
        let mut pages = 0;
        loop {
            pages += 1;
            for message in response.value {
                if seen.insert(message.id.clone()) {
                    if raw.len() == MAX_CONVERSATION_MESSAGES {
                        truncated = true;
                        break;
                    }
                    raw.push(message);
                }
            }
            if truncated || raw.len() == MAX_CONVERSATION_MESSAGES || pages == MAX_LIST_PAGES {
                truncated |= response.next_link.is_some();
                break;
            }
            let Some(next) = response.next_link else {
                break;
            };
            response = self.get_page(access_token, &next).await?;
        }
        let folder_ids = collect_folder_ids(&raw);
        let mut messages = raw
            .into_iter()
            .map(|message| normalize_message(message, config))
            .collect::<anyhow::Result<Vec<_>>>()?;
        messages.sort_by_key(|message| message.date);
        Ok(ProviderThread {
            id: thread_id.to_string(),
            is_primary_inbox: true,
            folder_ids,
            messages,
            truncated,
        })
    }

    pub fn for_mailbox(email: &str) -> anyhow::Result<Self> {
        Self::default().with_mailbox(email)
    }

    pub fn with_mailbox(&self, email: &str) -> anyhow::Result<Self> {
        let email = email.trim().to_ascii_lowercase();
        ensure!(
            crate::provider_detect::domain_of(&email).is_some()
                && !email.chars().any(|c| c.is_control() || c.is_whitespace())
                && email.matches('@').count() == 1,
            "invalid Microsoft mailbox email"
        );
        let mut client = self.clone();
        client.mailbox = Some(email);
        Ok(client)
    }

    fn mailbox_url(&self) -> String {
        match &self.mailbox {
            Some(email) => format!(
                "{}/users/{}",
                self.base_url,
                utf8_percent_encode(email, NON_ALPHANUMERIC)
            ),
            None => format!("{}/me", self.base_url),
        }
    }

    fn message_list_url(&self, config: &AnalysisConfig) -> String {
        if config.include_labels.is_empty() && config.exclude_labels.is_empty() {
            format!("{}/mailFolders/{INBOX_FOLDER}/messages", self.mailbox_url())
        } else {
            format!("{}/messages", self.mailbox_url())
        }
    }

    /// Probe de lectura antes de guardar una conexión o cambiar al buzón delegado.
    pub async fn validate_mailbox_access(&self, access_token: &str) -> anyhow::Result<()> {
        send_mail_request(
            &self.client,
            self.client
                .get(format!("{}/mailFolders/inbox/messages", self.mailbox_url()))
                .bearer_auth(access_token)
                .query(&[("$top", "1"), ("$select", "id")]),
        )
        .await?;
        Ok(())
    }

    /// El token identifica al usuario que dio consentimiento, incluso si el
    /// destino de correo es una casilla compartida.
    pub async fn get_profile(&self, access_token: &str) -> anyhow::Result<GmailProfile> {
        let response: GraphUser = read_mail_json(
            send_mail_request(
                &self.client,
                self.client
                    .get(format!("{}/me", self.base_url))
                    .bearer_auth(access_token)
                    .query(&[("$select", "mail,userPrincipalName")]),
            )
            .await?,
        )
        .await?;
        let email = response
            .mail
            .filter(|email| !email.trim().is_empty())
            .or(response
                .user_principal_name
                .filter(|email| !email.trim().is_empty()))
            .context("Graph profile without an email address")?;
        Ok(GmailProfile {
            email_address: email,
            messages_total: 0,
            threads_total: 0,
        })
    }

    pub async fn list_folders(&self, access_token: &str) -> anyhow::Result<Vec<GmailLabel>> {
        Ok(self.folder_catalog(access_token).await?.0)
    }

    async fn folder_catalog(&self, access_token: &str) -> anyhow::Result<(Vec<GmailLabel>, bool)> {
        let inbox: GraphFolder = read_mail_json(
            send_mail_request(
                &self.client,
                self.client
                    .get(format!("{}/mailFolders/inbox", self.mailbox_url()))
                    .bearer_auth(access_token)
                    .query(&[("$select", "id,displayName,childFolderCount")]),
            )
            .await?,
        )
        .await?;
        let mut queue = VecDeque::from([(
            format!(
                "{}/mailFolders?$select=id,displayName,childFolderCount&$top=100",
                self.mailbox_url()
            ),
            String::new(),
            0usize,
        )]);
        let mut labels = Vec::new();
        let mut seen = HashSet::new();
        let mut visited_pages = HashSet::new();
        let mut requests = 0;
        let mut truncated = false;
        while let Some((url, parent, depth)) = queue.pop_front() {
            if requests >= MAX_FOLDER_REQUESTS {
                truncated = true;
                break;
            }
            ensure!(
                visited_pages.insert(url.clone()),
                "Graph returned a repeated folder page"
            );
            requests += 1;
            let response: FolderListResponse = self.get_page(access_token, &url).await?;
            for folder in response.value {
                if !seen.insert(folder.id.clone()) {
                    continue;
                }
                if labels.len() == MAX_FOLDERS {
                    truncated = true;
                    break;
                }
                let name = if parent.is_empty() {
                    folder.display_name
                } else {
                    format!("{parent}/{}", folder.display_name)
                };
                if folder.child_folder_count > 0 {
                    if depth < MAX_FOLDER_DEPTH {
                        queue.push_back((format!("{}/mailFolders/{}/childFolders?$select=id,displayName,childFolderCount&$top=100", self.mailbox_url(), utf8_percent_encode(&folder.id, NON_ALPHANUMERIC)), name.clone(), depth + 1));
                    } else {
                        truncated = true;
                    }
                }
                labels.push(GmailLabel {
                    label_type: if folder.id == inbox.id {
                        "inbox"
                    } else {
                        "folder"
                    }
                    .to_string(),
                    id: folder.id,
                    name,
                });
            }
            if labels.len() == MAX_FOLDERS {
                truncated |= response.next_link.is_some() || !queue.is_empty();
                break;
            }
            if let Some(next) = response.next_link {
                queue.push_front((next, parent, depth));
            }
        }
        Ok((labels, truncated))
    }

    async fn get_page<T: serde::de::DeserializeOwned>(
        &self,
        access_token: &str,
        next: &str,
    ) -> anyhow::Result<T> {
        let url = url::Url::parse(next).context("invalid Graph pagination URL")?;
        let base = url::Url::parse(&self.base_url)?;
        ensure!(
            url.scheme() == base.scheme()
                && url.host_str() == base.host_str()
                && url.port_or_known_default() == base.port_or_known_default()
                && url.username().is_empty()
                && url.password().is_none()
                && url
                    .path()
                    .starts_with(&format!("{}/", base.path().trim_end_matches('/'))),
            "Graph pagination URL left the trusted origin"
        );
        read_mail_json(
            send_mail_request(
                &self.client,
                self.client.get(url).bearer_auth(access_token).header(
                    "Prefer",
                    "outlook.body-content-type=\"text\", IdType=\"ImmutableId\"",
                ),
            )
            .await?,
        )
        .await
    }
}

fn received_window_filter(config: &AnalysisConfig) -> anyhow::Result<String> {
    let (from, to) = analysis_window_utc(config)?;
    Ok(format!(
        "receivedDateTime ge {} and receivedDateTime lt {}",
        from.to_rfc3339(),
        to.to_rfc3339()
    ))
}

fn message_list_filter(config: &AnalysisConfig) -> anyhow::Result<String> {
    let mut filter = received_window_filter(config)?;
    let folders = config
        .include_labels
        .iter()
        .filter(|folder| !folder.trim().is_empty())
        .map(|folder| format!("parentFolderId eq '{}'", escape_odata(folder.trim())))
        .collect::<Vec<_>>();
    if !folders.is_empty() {
        filter.push_str(&format!(" and ({})", folders.join(" or ")));
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
    let is_internal = is_responder_email(&from_email, config);
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
    #[serde(rename = "@odata.nextLink", default)]
    next_link: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphFolder {
    id: String,
    #[serde(rename = "displayName", default)]
    display_name: String,
    #[serde(rename = "childFolderCount", default)]
    child_folder_count: u32,
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
    use chrono::TimeZone;

    fn config() -> AnalysisConfig {
        AnalysisConfig {
            date_from: "2026-03-01".to_string(),
            date_to: "2026-03-31".to_string(),
            time_from: "00:00".to_string(),
            time_to: "23:59".to_string(),
            timezone: "America/Santiago".to_string(),
            internal_domains: vec!["west-ingenieria.cl".to_string()],
            responder_emails: vec![],
            request_scope: Default::default(),
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
        assert!(filter.contains("receivedDateTime ge 2026-03-01T03:00:00+00:00"));
        // 2026-03-31 es inclusivo para el usuario: la cota es el 1 de abril.
        assert!(filter.contains("receivedDateTime lt 2026-04-01T03:00:00+00:00"));
    }

    #[test]
    fn folder_filters_use_all_messages_and_keep_date_filter_first() {
        let mut config = config();
        config.include_labels = vec!["inbox-id".to_string(), "team's-folder".to_string()];
        config.exclude_labels = vec!["deleted-id".to_string()];

        assert_eq!(
            GraphClient::default().message_list_url(&config),
            format!("{GRAPH_BASE}/me/messages")
        );
        assert_eq!(
            message_list_filter(&config).unwrap(),
            "receivedDateTime ge 2026-03-01T03:00:00+00:00 and receivedDateTime lt 2026-04-01T03:00:00+00:00 and (parentFolderId eq 'inbox-id' or parentFolderId eq 'team''s-folder') and parentFolderId ne 'deleted-id'"
        );
    }

    #[test]
    fn odata_filter_escapes_single_quotes_in_conversation_ids() {
        assert_eq!(escape_odata("AAQk'AGI"), "AAQk''AGI");
    }

    #[tokio::test]
    async fn graph_follows_message_conversation_and_nested_folder_pages() {
        use axum::{Json, Router, routing::get};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1.0", listener.local_addr().unwrap());
        let next = format!("{base}/me/messages-next");
        let next_copy = next.clone();
        let conversation_next = format!("{base}/me/conversation-next");
        let app = Router::new()
            .route("/v1.0/me/mailFolders/inbox/messages", get(move || {
                let next = next_copy.clone();
                async move { Json(serde_json::json!({ "value": [{"id":"m1","conversationId":"c1"},{"id":"m2","conversationId":"c1"}], "@odata.nextLink": next })) }
            }))
            .route("/v1.0/me/messages-next", get(|| async { Json(serde_json::json!({"value":[{"id":"m3","conversationId":"c2"}]})) }))
            .route("/v1.0/me/messages", get(move || {
                let next = conversation_next.clone();
                async move { Json(serde_json::json!({ "value": [{"id":"m1","receivedDateTime":"2026-03-02T10:00:00Z"}], "@odata.nextLink": next })) }
            }))
            .route("/v1.0/me/conversation-next", get(|| async { Json(serde_json::json!({"value":[{"id":"m2","receivedDateTime":"2026-03-03T10:00:00Z"}]})) }))
            .route("/v1.0/me/mailFolders/inbox", get(|| async { Json(serde_json::json!({"id":"inbox-id","displayName":"Inbox","childFolderCount":1})) }))
            .route("/v1.0/me/mailFolders", get(|| async { Json(serde_json::json!({"value":[{"id":"inbox-id","displayName":"Inbox","childFolderCount":1}]})) }))
            .route("/v1.0/me/mailFolders/inbox%2Did/childFolders", get(|| async { Json(serde_json::json!({"value":[{"id":"support-id","displayName":"Support","childFolderCount":0}]})) }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = GraphClient {
            base_url: base,
            ..GraphClient::default()
        };
        let page = client.list_thread_ids("test", &config(), 2).await.unwrap();
        assert_eq!(page.ids, ["c1", "c2"]);
        assert!(page.next_page_token.is_none());
        let thread = client.fetch_thread("test", "c1", &config()).await.unwrap();
        assert_eq!(thread.messages.len(), 2);
        assert!(!thread.truncated);
        let (folders, truncated) = client.folder_catalog("test").await.unwrap();
        assert_eq!(
            folders
                .iter()
                .map(|folder| folder.name.as_str())
                .collect::<Vec<_>>(),
            ["Inbox", "Inbox/Support"]
        );
        assert_eq!(folders[0].label_type, "inbox");
        assert!(!truncated);
        assert!(
            client
                .get_page::<MessageListResponse>("test", "http://example.com/v1.0/messages")
                .await
                .is_err()
        );
        server.abort();
    }

    #[test]
    fn shared_mailbox_uses_users_routes_without_accepting_invalid_targets() {
        let graph = GraphClient::for_mailbox("support@contoso.com").unwrap();
        assert_eq!(
            graph.mailbox_url(),
            format!("{GRAPH_BASE}/users/support%40contoso%2Ecom")
        );
        assert!(GraphClient::for_mailbox("support@contoso.com/path").is_err());
        assert!(GraphClient::for_mailbox("support @contoso.com").is_err());
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
