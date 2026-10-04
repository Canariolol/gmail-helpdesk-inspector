//! IMAP de solo lectura sobre TLS. Nunca usa SELECT, STORE ni BODY sin PEEK.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;

use anyhow::{Context, anyhow, ensure};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mailparse::{MailAddr, MailHeaderMap, ParsedMail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_rustls::{TlsConnector, client::TlsStream, rustls};

use crate::analysis::{
    AnalysisConfig, EmailMessage, is_automated_sender, is_responder_email,
    message_is_inside_analysis_window,
};
use crate::mailbox::{
    GmailLabel, GmailProfile, MailboxMetadata, MailboxProvider, MailboxProviderKind,
    ProviderThread, ThreadListPage, analysis_window_utc,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_FOLDERS: usize = 200;
const MAX_ANALYSIS_FOLDERS: usize = 20;
const MAX_HEADER_MESSAGES: usize = 5_000;
const MAX_CONVERSATION_MESSAGES: usize = 200;
const MAX_MESSAGE_BYTES: usize = 128 * 1024;
const MAX_SESSION_BYTES: usize = 32 * 1024 * 1024;
const HEADER_QUERY: &str = "(UID INTERNALDATE BODY.PEEK[HEADER])";

#[derive(Debug, thiserror::Error)]
#[error("IMAP authentication rejected; check the application password")]
pub struct ImapAuthenticationRejected;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapConfig {
    pub host: String,
    #[serde(default = "default_imap_port")]
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub sent_folder: Option<String>,
}

fn default_imap_port() -> u16 {
    993
}

impl ImapConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(self.port == 993, "IMAP requires implicit TLS on port 993");
        ensure!(
            self.host.len() <= 253
                && self.host.contains('.')
                && self.host.parse::<IpAddr>().is_err()
                && self.host.split('.').all(|part| !part.is_empty()
                    && part.len() <= 63
                    && !part.starts_with('-')
                    && !part.ends_with('-')
                    && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')),
            "IMAP host must be a public DNS hostname"
        );
        ensure!(
            !self.username.trim().is_empty()
                && self.username.len() <= 320
                && !self.username.chars().any(char::is_control),
            "invalid IMAP username"
        );
        if let Some(folder) = &self.sent_folder {
            validate_folder(folder)?;
        }
        Ok(())
    }
}

struct ImapIndex {
    threads: HashMap<String, Vec<IndexedMessage>>,
    truncated: bool,
}

#[derive(Clone)]
struct IndexedMessage {
    folder: String,
    uid: u32,
    uid_validity: u32,
    date: DateTime<Utc>,
    message_id: String,
    parent_id: Option<String>,
    selected: bool,
}

pub struct ImapClient {
    config: ImapConfig,
    index: RwLock<ImapIndex>,
    #[cfg(test)]
    test_address: Option<std::net::SocketAddr>,
}

type Session = async_imap::Session<LimitedStream>;

impl ImapClient {
    pub fn new(mut config: ImapConfig) -> anyhow::Result<Self> {
        config.host = config.host.trim().to_ascii_lowercase();
        config.username = config.username.trim().to_string();
        config.validate()?;
        Ok(Self {
            config,
            index: RwLock::new(ImapIndex {
                threads: HashMap::new(),
                truncated: false,
            }),
            #[cfg(test)]
            test_address: None,
        })
    }

    pub async fn validate_connection(&self, password: &str) -> anyhow::Result<GmailProfile> {
        tokio::time::timeout(OPERATION_TIMEOUT, async {
            let mut session = self.connect(password).await?;
            session
                .examine("INBOX")
                .await
                .map_err(|_| anyhow!("IMAP inbox access rejected"))?;
            if let Some(sent) = &self.config.sent_folder {
                session
                    .examine(sent)
                    .await
                    .map_err(|_| anyhow!("IMAP sent folder access rejected"))?;
            }
            let _ = session.logout().await;
            Ok(self.profile())
        })
        .await
        .context("IMAP validation timed out")?
    }

    fn profile(&self) -> GmailProfile {
        GmailProfile {
            email_address: self.config.username.clone(),
            messages_total: 0,
            threads_total: 0,
        }
    }

    async fn connect(&self, password: &str) -> anyhow::Result<Session> {
        ensure!(
            !password.is_empty()
                && password.len() <= 4096
                && !password.chars().any(char::is_control),
            "invalid IMAP application password"
        );
        #[cfg(test)]
        if let Some(address) = self.test_address {
            let stream = TcpStream::connect(address).await?;
            return self.login(MailStream::Plain(stream), password).await;
        }
        let resolved = tokio::time::timeout(
            CONNECT_TIMEOUT,
            tokio::net::lookup_host((self.config.host.as_str(), self.config.port)),
        )
        .await
        .context("IMAP DNS lookup timed out")?
        .map_err(|_| anyhow!("IMAP DNS lookup failed"))?
        .collect::<Vec<_>>();
        ensure!(
            !resolved.is_empty()
                && resolved.len() <= 32
                && resolved.iter().all(|address| is_public_ip(address.ip())),
            "IMAP DNS must resolve only to public Internet addresses"
        );
        // Conecta a las IP ya verificadas; TLS conserva hostname para SNI y cert.
        let mut connected = None;
        for address in resolved {
            if let Ok(Ok(stream)) =
                tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(address)).await
            {
                connected = Some(stream);
                break;
            }
        }
        let stream = connected.context("IMAP connection failed")?;
        let roots =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls_config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let name = rustls::pki_types::ServerName::try_from(self.config.host.clone())
            .context("invalid IMAP TLS server name")?;
        let tls = tokio::time::timeout(
            CONNECT_TIMEOUT,
            TlsConnector::from(Arc::new(tls_config)).connect(name, stream),
        )
        .await
        .context("IMAP TLS handshake timed out")?
        .map_err(|_| anyhow!("IMAP TLS certificate verification failed"))?;
        self.login(MailStream::Tls(Box::new(tls)), password).await
    }

    async fn login(&self, stream: MailStream, password: &str) -> anyhow::Result<Session> {
        let mut client = async_imap::Client::new(LimitedStream {
            inner: stream,
            remaining: MAX_SESSION_BYTES,
        });
        ensure!(
            client
                .read_response()
                .await
                .map_err(|_| anyhow!("invalid IMAP greeting"))?
                .is_some(),
            "IMAP server closed before greeting"
        );
        client
            .login(&self.config.username, password)
            .await
            .map_err(|_| ImapAuthenticationRejected.into())
    }

    async fn folders(
        &self,
        session: &mut Session,
    ) -> anyhow::Result<(Vec<GmailLabel>, Option<String>, bool)> {
        use async_imap::imap_proto::NameAttribute;
        let mut stream = session
            .list(Some(""), Some("*"))
            .await
            .map_err(|_| anyhow!("IMAP folder listing failed"))?;
        let mut folders = Vec::new();
        let mut sent = self.config.sent_folder.clone();
        let mut truncated = false;
        while let Some(folder) = stream
            .try_next()
            .await
            .map_err(|_| anyhow!("IMAP folder listing failed"))?
        {
            if folder.attributes().contains(&NameAttribute::NoSelect) {
                continue;
            }
            validate_folder(folder.name())?;
            if folders.len() == MAX_FOLDERS {
                truncated = true;
                continue;
            }
            if sent.is_none() && folder.attributes().contains(&NameAttribute::Sent) {
                sent = Some(folder.name().to_string());
            }
            let kind = if folder.name().eq_ignore_ascii_case("INBOX") {
                "inbox"
            } else if folder.attributes().contains(&NameAttribute::Sent) {
                "sent"
            } else if folder.attributes().contains(&NameAttribute::Drafts) {
                "drafts"
            } else if folder.attributes().contains(&NameAttribute::Junk) {
                "junk"
            } else if folder.attributes().contains(&NameAttribute::Trash) {
                "trash"
            } else {
                "folder"
            };
            folders.push(GmailLabel {
                id: folder.name().to_string(),
                name: folder.name().to_string(),
                label_type: kind.to_string(),
            });
        }
        // Servidores sin SPECIAL-USE: sólo nombres frecuentes, nunca carpetas
        // inventadas. Se puede seleccionar la carpeta exacta en configuración.
        if sent.is_none() {
            sent = folders
                .iter()
                .find(|folder| {
                    ["sent", "sent items", "sent messages", "inbox.sent"]
                        .iter()
                        .any(|name| folder.id.eq_ignore_ascii_case(name))
                })
                .map(|folder| folder.id.clone());
        }
        if let Some(sent) = &sent {
            for folder in &mut folders {
                if folder.id == *sent && folder.label_type == "folder" {
                    folder.label_type = "sent".to_string();
                }
            }
        }
        Ok((folders, sent, truncated))
    }

    async fn list(
        &self,
        password: &str,
        config: &AnalysisConfig,
        max_threads: u32,
    ) -> anyhow::Result<ThreadListPage> {
        let _ = analysis_window_utc(config)?;
        ensure!(max_threads > 0, "max_threads must be positive");
        let mut session = self.connect(password).await?;
        let (catalog, sent, catalog_truncated) = self.folders(&mut session).await?;
        let selected = if config.include_labels.is_empty() {
            if config.exclude_labels.is_empty() {
                vec!["INBOX".to_string()]
            } else {
                catalog
                    .iter()
                    .filter(|folder| {
                        !matches!(folder.label_type.as_str(), "drafts" | "junk" | "trash")
                    })
                    .map(|folder| folder.id.clone())
                    .collect()
            }
        } else {
            config.include_labels.clone()
        };
        let selected = selected
            .into_iter()
            .filter(|folder| {
                !config
                    .exclude_labels
                    .iter()
                    .any(|excluded| excluded == folder)
            })
            .collect::<Vec<_>>();
        ensure!(
            selected.len() <= MAX_ANALYSIS_FOLDERS,
            "select at most 20 IMAP folders per analysis"
        );
        for folder in &selected {
            validate_folder(folder)?;
            ensure!(
                catalog.iter().any(|item| item.id == *folder
                    || (item.id.eq_ignore_ascii_case("INBOX")
                        && folder.eq_ignore_ascii_case("INBOX"))),
                "selected IMAP folder does not exist"
            );
        }
        let mut sources = selected.clone();
        let sent_available = sent.is_some();
        if let Some(sent) = sent
            && !sources.contains(&sent)
            && !config
                .exclude_labels
                .iter()
                .any(|excluded| excluded == &sent)
        {
            sources.push(sent);
        }
        let mut indexed = Vec::new();
        let mut candidate_messages = HashSet::new();
        let mut truncated = catalog_truncated || !sent_available;
        for folder in sources {
            let info = session
                .examine(&folder)
                .await
                .map_err(|_| anyhow!("IMAP folder access rejected"))?;
            let validity = info
                .uid_validity
                .context("IMAP server omitted UIDVALIDITY")?;
            if info.exists == 0 {
                continue;
            }
            let budget = MAX_HEADER_MESSAGES.saturating_sub(indexed.len());
            if budget == 0 {
                truncated = true;
                break;
            }
            let count = (info.exists as usize).min(budget);
            truncated |= info.exists as usize > count;
            let start = info.exists.saturating_sub(count as u32) + 1;
            let mut stream = session
                .fetch(format!("{start}:{}", info.exists), HEADER_QUERY)
                .await
                .map_err(|_| anyhow!("IMAP header retrieval failed"))?;
            while let Some(fetch) = stream
                .try_next()
                .await
                .map_err(|_| anyhow!("IMAP header retrieval failed"))?
            {
                if indexed.len() == MAX_HEADER_MESSAGES {
                    truncated = true;
                    continue;
                }
                let uid = fetch.uid.context("IMAP server omitted UID")?;
                let raw = fetch
                    .header()
                    .context("IMAP server omitted message headers")?;
                let received = fetch.internal_date().map(|date| date.with_timezone(&Utc));
                let message =
                    normalize_imap_message(raw, &folder, validity, uid, received, config)?;
                let mail = mailparse::parse_mail(raw).context("invalid IMAP headers")?;
                let message_id = mail
                    .headers
                    .get_first_value("Message-ID")
                    .and_then(|value| message_ids(&value).into_iter().next())
                    .unwrap_or_else(|| message.id.clone());
                let parent_id = mail
                    .headers
                    .get_first_value("References")
                    .and_then(|value| message_ids(&value).into_iter().next())
                    .or_else(|| {
                        mail.headers
                            .get_first_value("In-Reply-To")
                            .and_then(|value| message_ids(&value).into_iter().next())
                    });
                let in_selected = selected.iter().any(|item| {
                    item == &folder
                        || (item.eq_ignore_ascii_case("INBOX")
                            && folder.eq_ignore_ascii_case("INBOX"))
                });
                if in_selected && message_is_inside_analysis_window(&message, config) {
                    candidate_messages.insert(message_id.clone());
                }
                indexed.push(IndexedMessage {
                    folder: folder.clone(),
                    uid,
                    uid_validity: validity,
                    date: message.date,
                    message_id,
                    parent_id,
                    selected: in_selected,
                });
            }
        }
        let parents = indexed
            .iter()
            .filter_map(|message| {
                message
                    .parent_id
                    .as_ref()
                    .map(|parent| (message.message_id.clone(), parent.clone()))
            })
            .collect::<HashMap<_, _>>();
        let mut threads: HashMap<String, Vec<IndexedMessage>> = HashMap::new();
        let mut candidates: HashMap<String, DateTime<Utc>> = HashMap::new();
        for message in indexed {
            let root = conversation_root(&message.message_id, &parents);
            let thread_id = format!("imap:{}", hex_digest(root.as_bytes()));
            if message.selected && candidate_messages.contains(&message.message_id) {
                candidates
                    .entry(thread_id.clone())
                    .and_modify(|date| *date = (*date).max(message.date))
                    .or_insert(message.date);
            }
            let messages = threads.entry(thread_id).or_default();
            if !messages
                .iter()
                .any(|existing| existing.message_id == message.message_id)
            {
                messages.push(message);
            }
        }
        let mut candidates = candidates.into_iter().collect::<Vec<_>>();
        candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        truncated |= candidates.len() > max_threads as usize;
        candidates.truncate(max_threads as usize);
        let ids = candidates.into_iter().map(|(id, _)| id).collect::<Vec<_>>();
        threads.retain(|id, _| ids.contains(id));
        *self.index.write().await = ImapIndex { threads, truncated };
        let _ = session.logout().await;
        Ok(ThreadListPage {
            ids,
            next_page_token: truncated.then(|| "imap-retrieval-limit".to_string()),
            result_size_estimate: None,
        })
    }

    async fn thread(
        &self,
        password: &str,
        thread_id: &str,
        config: &AnalysisConfig,
    ) -> anyhow::Result<ProviderThread> {
        let index = self.index.read().await;
        let locations = index
            .threads
            .get(thread_id)
            .context("IMAP conversation is not in this analysis index")?
            .clone();
        let mut truncated = index.truncated || locations.len() > MAX_CONVERSATION_MESSAGES;
        drop(index);
        let mut session = self.connect(password).await?;
        let mut messages = Vec::new();
        let mut folders = Vec::new();
        let mut current = None;
        let query = format!("(UID RFC822.SIZE INTERNALDATE BODY.PEEK[]<0.{MAX_MESSAGE_BYTES}>)");
        for location in locations.into_iter().take(MAX_CONVERSATION_MESSAGES) {
            if current.as_deref() != Some(location.folder.as_str()) {
                let mailbox = session
                    .examine(&location.folder)
                    .await
                    .map_err(|_| anyhow!("IMAP folder access rejected"))?;
                ensure!(
                    mailbox.uid_validity == Some(location.uid_validity),
                    "IMAP folder changed UIDVALIDITY during analysis"
                );
                current = Some(location.folder.clone());
                if !folders.contains(&location.folder) {
                    folders.push(location.folder.clone());
                }
            }
            let mut stream = session
                .uid_fetch(location.uid.to_string(), &query)
                .await
                .map_err(|_| anyhow!("IMAP message retrieval failed"))?;
            let fetch = stream
                .try_next()
                .await
                .map_err(|_| anyhow!("IMAP message retrieval failed"))?
                .context("IMAP message disappeared during analysis")?;
            ensure!(
                fetch.uid == Some(location.uid),
                "IMAP server returned an unexpected UID"
            );
            let body = fetch.body().context("IMAP server omitted message body")?;
            truncated |= fetch.size.is_none_or(|size| size as usize > body.len());
            messages.push(normalize_imap_message(
                body,
                &location.folder,
                location.uid_validity,
                location.uid,
                fetch.internal_date().map(|date| date.with_timezone(&Utc)),
                config,
            )?);
            // Drena el tagged OK antes de enviar el próximo comando.
            while stream
                .try_next()
                .await
                .map_err(|_| anyhow!("IMAP message retrieval failed"))?
                .is_some()
            {}
        }
        let _ = session.logout().await;
        messages.sort_by_key(|message| message.date);
        Ok(ProviderThread {
            id: thread_id.to_string(),
            is_primary_inbox: true,
            folder_ids: folders,
            messages,
            truncated,
        })
    }
}

#[async_trait]
impl MailboxProvider for ImapClient {
    fn kind(&self) -> MailboxProviderKind {
        MailboxProviderKind::Imap
    }

    async fn list_thread_ids(
        &self,
        access_token: &str,
        config: &AnalysisConfig,
        max_threads: u32,
    ) -> anyhow::Result<ThreadListPage> {
        tokio::time::timeout(
            Duration::from_secs(180),
            self.list(access_token, config, max_threads),
        )
        .await
        .context("IMAP conversation listing timed out")?
    }

    async fn fetch_thread(
        &self,
        access_token: &str,
        thread_id: &str,
        config: &AnalysisConfig,
    ) -> anyhow::Result<ProviderThread> {
        tokio::time::timeout(
            Duration::from_secs(180),
            self.thread(access_token, thread_id, config),
        )
        .await
        .context("IMAP conversation retrieval timed out")?
    }

    async fn fetch_mailbox_metadata(
        &self,
        access_token: &str,
        now: DateTime<Utc>,
    ) -> MailboxMetadata {
        let folders = tokio::time::timeout(OPERATION_TIMEOUT, async {
            let mut session = self.connect(access_token).await?;
            let catalog = self.folders(&mut session).await?;
            let _ = session.logout().await;
            Ok::<_, anyhow::Error>(catalog)
        })
        .await;
        let (labels, folders_truncated) = match folders {
            Ok(Ok((labels, _, truncated))) => (labels, truncated),
            _ => (Vec::new(), true),
        };
        MailboxMetadata {
            profile: Some(self.profile()),
            labels,
            send_as: Vec::new(),
            filters_count: 0,
            folders_truncated,
            synced_at: now,
        }
    }
}

fn validate_folder(folder: &str) -> anyhow::Result<()> {
    ensure!(
        !folder.is_empty() && folder.len() <= 1024 && !folder.chars().any(char::is_control),
        "invalid IMAP folder name"
    );
    Ok(())
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && (b == 168 || (b == 0 && (c == 0 || c == 2)) || (b == 88 && c == 99)))
        || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
        || (a == 203 && b == 0 && c == 113))
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => {
            let words = ip.segments();
            // Global unicast únicamente; excluye transición, documentación y
            // direcciones especiales que podrían encaminar a redes internas.
            (words[0] & 0xe000) == 0x2000
                && !(words[0] == 0x2001 && (words[1] <= 0x01ff || words[1] == 0x0db8))
                && words[0] != 0x2002
                && words[0] != 0x3ffe
                && !(words[0] == 0x3fff && (words[1] & 0xf000) == 0)
        }
    }
}

fn message_ids(value: &str) -> Vec<String> {
    mailparse::msgidparse(value)
        .map(|ids| ids.iter().cloned().collect())
        .unwrap_or_default()
}

fn conversation_root(message_id: &str, parents: &HashMap<String, String>) -> String {
    let mut current = message_id.to_string();
    let mut seen = HashSet::new();
    for _ in 0..100 {
        if !seen.insert(current.clone()) {
            return seen.into_iter().min().unwrap_or(current);
        }
        match parents.get(&current) {
            Some(parent) => current = parent.clone(),
            None => return current,
        }
    }
    seen.into_iter().min().unwrap_or(current)
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn addresses(mail: &ParsedMail<'_>, name: &str) -> Vec<(Option<String>, String)> {
    let Some(header) = mail
        .headers
        .iter()
        .find(|header| header.get_key_ref().eq_ignore_ascii_case(name))
    else {
        return Vec::new();
    };
    let Ok(addresses) = mailparse::addrparse_header(header) else {
        return Vec::new();
    };
    addresses
        .iter()
        .flat_map(|address| match address {
            MailAddr::Single(address) => vec![(
                address.display_name.clone(),
                address.addr.to_ascii_lowercase(),
            )],
            MailAddr::Group(group) => group
                .addrs
                .iter()
                .map(|address| {
                    (
                        address.display_name.clone(),
                        address.addr.to_ascii_lowercase(),
                    )
                })
                .collect(),
        })
        .collect()
}

fn normalize_imap_message(
    raw: &[u8],
    folder: &str,
    validity: u32,
    uid: u32,
    received: Option<DateTime<Utc>>,
    config: &AnalysisConfig,
) -> anyhow::Result<EmailMessage> {
    let mail = mailparse::parse_mail(raw).context("invalid IMAP message MIME")?;
    let (from_name, from_email) = addresses(&mail, "From")
        .into_iter()
        .next()
        .unwrap_or_default();
    let date = received
        .or_else(|| {
            mail.headers
                .get_first_value("Date")
                .and_then(|date| mailparse::dateparse(&date).ok())
                .and_then(|timestamp| DateTime::from_timestamp(timestamp, 0))
        })
        .context("IMAP message has no valid date")?;
    let headers = mail
        .headers
        .get_first_value("Auto-Submitted")
        .map(|value| Map::from_iter([("auto-submitted".to_string(), Value::String(value))]))
        .unwrap_or_default();
    let body = body_text(&mail, 0)?;
    let is_internal = is_responder_email(&from_email, config);
    let is_automated = is_automated_sender(&from_email, &Value::Object(headers.clone()));
    let id = format!("imap:{}:{validity}:{uid}", hex_digest(folder.as_bytes()));
    Ok(EmailMessage {
        id: id.clone(),
        message_id: id,
        from_email,
        from_name,
        to_emails: addresses(&mail, "To")
            .into_iter()
            .map(|(_, address)| address)
            .collect(),
        cc_emails: addresses(&mail, "Cc")
            .into_iter()
            .map(|(_, address)| address)
            .collect(),
        date,
        subject: mail
            .headers
            .get_first_value("Subject")
            .filter(|subject| !subject.trim().is_empty())
            .unwrap_or_else(|| "(sin asunto)".to_string()),
        snippet: body.chars().take(500).collect(),
        headers: Value::Object(headers),
        is_internal,
        is_external: !is_internal,
        is_automated,
        body_text: Some(body),
    })
}

fn body_text(mail: &ParsedMail<'_>, depth: usize) -> anyhow::Result<String> {
    ensure!(depth <= 20, "IMAP MIME nesting limit exceeded");
    let disposition = mail.get_content_disposition();
    if disposition.disposition == mailparse::DispositionType::Attachment
        || disposition.params.contains_key("filename")
        || mail.ctype.params.contains_key("name")
    {
        return Ok(String::new());
    }
    if !mail.subparts.is_empty() {
        let plain = mail
            .subparts
            .iter()
            .filter(|part| part.ctype.mimetype == "text/plain")
            .collect::<Vec<_>>();
        let parts = if mail.ctype.mimetype == "multipart/alternative" && !plain.is_empty() {
            plain
        } else {
            mail.subparts.iter().collect()
        };
        return Ok(parts
            .into_iter()
            .map(|part| body_text(part, depth + 1))
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"));
    }
    Ok(match mail.ctype.mimetype.as_str() {
        "text/plain" => mail.get_body().context("IMAP text decoding failed")?,
        "text/html" => {
            scraper::Html::parse_fragment(&mail.get_body().context("IMAP HTML decoding failed")?)
                .root_element()
                .text()
                .collect::<Vec<_>>()
                .join(" ")
        }
        _ => String::new(),
    })
}

/// Límite de bytes antes del parser IMAP, incluso si un servidor envía un
/// literal desproporcionado en contra del tamaño BODY.PEEK solicitado.
#[derive(Debug)]
enum MailStream {
    Tls(Box<TlsStream<TcpStream>>),
    #[cfg(test)]
    Plain(TcpStream),
}

impl AsyncRead for MailStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        target: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_read(cx, target),
            #[cfg(test)]
            Self::Plain(stream) => Pin::new(stream).poll_read(cx, target),
        }
    }
}

impl AsyncWrite for MailStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_write(cx, bytes),
            #[cfg(test)]
            Self::Plain(stream) => Pin::new(stream).poll_write(cx, bytes),
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
            #[cfg(test)]
            Self::Plain(stream) => Pin::new(stream).poll_flush(cx),
        }
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
            #[cfg(test)]
            Self::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

#[derive(Debug)]
struct LimitedStream {
    inner: MailStream,
    remaining: usize,
}

impl AsyncRead for LimitedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        target: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if target.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if self.remaining == 0 {
            return Poll::Ready(Err(std::io::Error::other(
                "IMAP session byte limit exceeded",
            )));
        }
        let mut bytes = [0u8; 8192];
        let size = bytes.len().min(target.remaining()).min(self.remaining);
        let mut buffer = ReadBuf::new(&mut bytes[..size]);
        match Pin::new(&mut self.inner).poll_read(cx, &mut buffer) {
            Poll::Ready(Ok(())) => {
                self.remaining -= buffer.filled().len();
                target.put_slice(buffer.filled());
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

impl AsyncWrite for LimitedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, bytes)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imap_rejects_private_reserved_mapped_and_transition_addresses() {
        for address in [
            "127.0.0.1",
            "10.0.0.4",
            "169.254.169.254",
            "100.64.1.1",
            "192.0.2.1",
            "198.18.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:8.8.8.8",
            "2001:db8::1",
            "2002:7f00:1::",
            "2001::1",
            "3fff::1",
            "3ffe::1",
        ] {
            assert!(!is_public_ip(address.parse().unwrap()), "{address}");
        }
        for address in [
            "8.8.8.8",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "2001:4860:4860::8888",
        ] {
            assert!(is_public_ip(address.parse().unwrap()), "{address}");
        }
    }

    #[test]
    fn imap_configuration_requires_dns_implicit_tls_and_valid_fields() {
        let config = ImapConfig {
            host: "imap.example.com".to_string(),
            port: 993,
            username: "support@example.com".to_string(),
            sent_folder: None,
        };
        assert!(config.validate().is_ok());
        for host in [
            "localhost",
            "127.0.0.1",
            "https://imap.example.com",
            "imap.example.com/path",
            "foo..example.com",
            "-foo.example.com",
        ] {
            assert!(
                ImapConfig {
                    host: host.to_string(),
                    ..config.clone()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            ImapConfig {
                port: 143,
                ..config
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn thread_roots_join_replies_by_message_id_without_subject_merging() {
        let parents = HashMap::from([
            ("reply".to_string(), "root".to_string()),
            ("followup".to_string(), "reply".to_string()),
        ]);
        assert_eq!(conversation_root("followup", &parents), "root");
        assert_eq!(conversation_root("unrelated", &parents), "unrelated");
        assert_eq!(
            message_ids("<root@example.com> <reply@example.com>"),
            ["root@example.com", "reply@example.com"]
        );
    }

    #[test]
    fn mime_normalization_decodes_addresses_and_omits_attachments() {
        let config: AnalysisConfig = serde_json::from_value(serde_json::json!({
            "date_from": "2026-10-04", "date_to": "2026-10-04", "ignored_senders": [], "ignored_domains": [], "ignored_keywords": [], "internal_domains": ["example.com"]
        }))
        .unwrap();
        let raw = b"From: \"Cliente, Uno\" <client@outside.com>\r\nTo: support@example.com\r\nDate: Sun, 4 Oct 2026 12:00:00 -0300\r\nSubject: =?UTF-8?Q?Solicitud_t=C3=A9cnica?=\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nAyuda por favor\r\n--x\r\nContent-Type: text/plain\r\nContent-Disposition: attachment; filename=private.txt\r\n\r\nsecret attachment\r\n--x--\r\n";
        let message = normalize_imap_message(raw, "INBOX", 1, 3, None, &config).unwrap();
        assert_eq!(message.from_name.as_deref(), Some("Cliente, Uno"));
        assert_eq!(message.subject, "Solicitud técnica");
        assert!(
            message
                .body_text
                .as_ref()
                .unwrap()
                .contains("Ayuda por favor")
        );
        assert!(
            !message
                .body_text
                .as_ref()
                .unwrap()
                .contains("secret attachment")
        );
        assert_eq!(message.headers, serde_json::json!({}));
    }

    #[tokio::test]
    async fn imap_protocol_indexes_inbox_and_sent_without_marking_read() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut socket = BufReader::new(socket);
                    socket
                        .get_mut()
                        .write_all(b"* OK mock IMAP\r\n")
                        .await
                        .unwrap();
                    let mut sent = false;
                    loop {
                        let mut line = String::new();
                        if socket.read_line(&mut line).await.unwrap() == 0 {
                            break;
                        }
                        let tag = line.split_whitespace().next().unwrap();
                        let command = line.split_whitespace().nth(1).unwrap();
                        assert!(!matches!(
                            command,
                            "SELECT" | "STORE" | "APPEND" | "EXPUNGE"
                        ));
                        let response = match command {
                            "LOGIN" => format!("{tag} OK logged in\r\n"),
                            "LIST" => format!(
                                "* LIST () \"/\" \"INBOX\"\r\n* LIST (\\Sent) \"/\" \"Sent\"\r\n{tag} OK listed\r\n"
                            ),
                            "EXAMINE" => {
                                sent = line.contains("Sent");
                                format!(
                                    "* 1 EXISTS\r\n* OK [UIDVALIDITY 42]\r\n{tag} OK [READ-ONLY] examined\r\n"
                                )
                            }
                            "FETCH" | "UID" => {
                                assert!(line.contains("BODY.PEEK"), "{line}");
                                let (uid, from, id, refs, date, text) = if sent {
                                    (
                                        20,
                                        "Support <support@example.com>",
                                        "reply@example.com",
                                        "References: <request@example.net>\r\n",
                                        "Sun, 4 Oct 2026 13:00:00 -0300",
                                        "Respuesta del equipo",
                                    )
                                } else {
                                    (
                                        10,
                                        "Client <client@example.net>",
                                        "request@example.net",
                                        "",
                                        "Sun, 4 Oct 2026 12:00:00 -0300",
                                        "Solicitud de soporte",
                                    )
                                };
                                let headers = format!(
                                    "From: {from}\r\nTo: support@example.com\r\nMessage-ID: <{id}>\r\n{refs}Date: {date}\r\nSubject: Soporte\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n"
                                );
                                let full = format!("{headers}{text}\r\n");
                                let (section, bytes) = if command == "FETCH" {
                                    ("BODY[HEADER]", headers)
                                } else {
                                    ("BODY[]<0>", full.clone())
                                };
                                format!(
                                    "* 1 FETCH (UID {uid} INTERNALDATE \"04-Oct-2026 {}:00:00 +0000\" RFC822.SIZE {} {section} {{{}}}\r\n{bytes})\r\n{tag} OK fetched\r\n",
                                    if sent { 16 } else { 15 },
                                    full.len(),
                                    bytes.len()
                                )
                            }
                            "LOGOUT" => {
                                socket
                                    .get_mut()
                                    .write_all(
                                        format!("* BYE logged out\r\n{tag} OK logout\r\n")
                                            .as_bytes(),
                                    )
                                    .await
                                    .unwrap();
                                break;
                            }
                            other => panic!("unexpected IMAP command {other}"),
                        };
                        socket
                            .get_mut()
                            .write_all(response.as_bytes())
                            .await
                            .unwrap();
                    }
                });
            }
        });
        let mut client = ImapClient::new(ImapConfig {
            host: "imap.example.com".to_string(),
            port: 993,
            username: "support@example.com".to_string(),
            sent_folder: None,
        })
        .unwrap();
        // Sólo este test sustituye TLS/DNS por un servidor de protocolo local.
        // Las pruebas de SSRF anteriores verifican las restricciones de conexión.
        client.test_address = Some(address);
        let config: AnalysisConfig = serde_json::from_value(serde_json::json!({
            "date_from":"2026-10-04", "date_to":"2026-10-04", "ignored_senders":[], "ignored_domains":[], "ignored_keywords":[], "internal_domains":["example.com"], "responder_emails":["support@example.com"]
        })).unwrap();
        let page = client
            .list_thread_ids("test-app-password", &config, 20)
            .await
            .unwrap();
        assert_eq!(page.ids.len(), 1);
        assert!(page.next_page_token.is_none());
        let thread = client
            .fetch_thread("test-app-password", &page.ids[0], &config)
            .await
            .unwrap();
        assert_eq!(thread.messages.len(), 2);
        assert!(thread.messages[0].is_external);
        assert!(thread.messages[1].is_internal);
        assert!(
            thread.messages[1]
                .body_text
                .as_ref()
                .unwrap()
                .contains("Respuesta del equipo")
        );
        assert!(!thread.truncated);
        let metadata = client
            .fetch_mailbox_metadata("test-app-password", Utc::now())
            .await;
        assert_eq!(metadata.labels.len(), 2);
        assert!(!metadata.folders_truncated);
        server.abort();
    }

    #[test]
    fn deeply_nested_mime_is_rejected_without_unbounded_recursion() {
        fn nested_mail(levels: usize) -> Vec<u8> {
            let mut raw = "Content-Type: text/plain\r\n\r\nleaf".to_string();
            for level in 0..levels {
                raw = format!(
                    "Content-Type: multipart/mixed; boundary=b{level}\r\n\r\n--b{level}\r\n{raw}\r\n--b{level}--\r\n"
                );
            }
            raw.into_bytes()
        }
        let deep = nested_mail(150);
        assert!(deep.len() < MAX_MESSAGE_BYTES);
        assert!(
            mailparse::parse_mail(&deep)
                .unwrap_err()
                .to_string()
                .contains("Recursion limit")
        );
        let bounded = nested_mail(25);
        let parsed = mailparse::parse_mail(&bounded).unwrap();
        assert!(
            body_text(&parsed, 0)
                .unwrap_err()
                .to_string()
                .contains("nesting limit")
        );
    }
}
