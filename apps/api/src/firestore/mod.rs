use std::{
    collections::HashMap,
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use futures_util::{StreamExt, stream};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    analysis::{
        AiAuditResult, AnalysisRun, AnalysisStatus, EmailMessage, EmailThread, ManualReview,
        ManualReviewOverride, apply_manual_review_override, calculate_metrics, message_fingerprint,
        reconcile_legacy_thread_classification,
    },
    auth::UserSession,
    billing::{Account, CheckoutSession, Subscription, UsageLedger},
    config::{FirestoreConfig, ServiceAccountKey},
    mailbox::{FilterPreset, MailboxConnection, MailboxMetadata, mailbox_connection_is_active},
    policies::{OrgConfigBundle, PolicyVersion, hash_owner_email},
    postgres::PostgresStorage,
    scheduler::model::{ScheduleConfig, ScheduleState},
    storage::{
        AnalysisDataDeletionAudit, MailboxConnectionRefresh,
        ManualReviewInheritanceMigrationResult, ManualReviewMetricsMigrationResult,
        ScheduleWindowClaim, StorageRepository, clear_gmail_connection,
        existing_schedule_window_claim, gmail_connection_from_legacy,
    },
};

const MANUAL_REVIEW_METRICS_MIGRATION_PATH: &str = "systemMigrations/manual-review-metrics-v1";
const MANUAL_REVIEW_INHERITANCE_MIGRATION_PATH: &str =
    "systemMigrations/manual-review-inheritance-v2";
const SCHEDULE_CLAIM_MAX_ATTEMPTS: usize = 3;
const USAGE_UPDATE_MAX_ATTEMPTS: usize = 3;
const ANALYSIS_START_CLAIM_MAX_ATTEMPTS: usize = 3;

pub struct FirestoreStorage {
    client: Client,
    config: FirestoreConfig,
    token_cache: Mutex<Option<CachedToken>>,
}

#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    expires_at_epoch: u64,
}

#[derive(Debug, Deserialize)]
struct FirestoreDocument {
    name: String,
    #[serde(default)]
    fields: Map<String, Value>,
    #[serde(default, rename = "updateTime")]
    update_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FirestoreListResponse {
    #[serde(default)]
    documents: Vec<FirestoreDocument>,
    #[serde(default, rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationMarker {
    version: String,
    completed_at: chrono::DateTime<Utc>,
    scanned_runs: u64,
    updated_runs: u64,
    updated_threads: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct AccountEmailIndex {
    workos_user_id: String,
}

fn account_email_index_path(email: &str) -> String {
    format!("accountEmailIndexes/{}", hash_owner_email(email))
}

impl FirestoreStorage {
    pub fn new(config: FirestoreConfig) -> anyhow::Result<Self> {
        if config.project_id.trim().is_empty() {
            return Err(anyhow!("GCP_PROJECT_ID is required for Firestore storage"));
        }
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()?,
            config,
            token_cache: Mutex::new(None),
        })
    }

    fn root(&self) -> String {
        format!(
            "https://firestore.googleapis.com/v1/projects/{}/databases/{}/documents",
            self.config.project_id, self.config.database_id
        )
    }

    async fn token(&self) -> anyhow::Result<String> {
        if let Some(token) = &self.config.bearer_token {
            return Ok(token.clone());
        }

        let now = epoch();
        if let Some(cached) = self.token_cache.lock().await.clone()
            && cached.expires_at_epoch > now + 60
        {
            return Ok(cached.token);
        }

        let token = if let Some(path) = &self.config.service_account_path {
            self.service_account_token(path).await?
        } else {
            self.metadata_server_token().await?
        };

        *self.token_cache.lock().await = Some(token.clone());
        Ok(token.token)
    }

    async fn service_account_token(&self, path: &str) -> anyhow::Result<CachedToken> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read service account at {path}"))?;
        let key: ServiceAccountKey =
            serde_json::from_str(&raw).context("invalid service account json")?;
        let now = epoch();
        let exp = now + 3600;
        let claims = json!({
            "iss": key.client_email,
            "scope": "https://www.googleapis.com/auth/datastore",
            "aud": key.token_uri.unwrap_or_else(|| "https://oauth2.googleapis.com/token".to_string()),
            "iat": now,
            "exp": exp
        });
        let jwt = jsonwebtoken::encode(
            &Header::new(Algorithm::RS256),
            &claims,
            &EncodingKey::from_rsa_pem(key.private_key.as_bytes())
                .context("invalid service account private key")?,
        )?;
        let response = self
            .client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", jwt.as_str()),
            ])
            .send()
            .await?;
        let response: TokenResponse =
            json_or_google_error(response, "Firestore service-account token exchange").await?;
        Ok(CachedToken {
            token: response.access_token,
            expires_at_epoch: now + response.expires_in,
        })
    }

    async fn metadata_server_token(&self) -> anyhow::Result<CachedToken> {
        let now = epoch();
        let response = self
            .client
            .get("http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token")
            .header("Metadata-Flavor", "Google")
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .context("failed to fetch Cloud Run metadata token")?
            ;
        let response: TokenResponse =
            json_or_google_error(response, "Cloud Run metadata token request").await?;
        Ok(CachedToken {
            token: response.access_token,
            expires_at_epoch: now + response.expires_in,
        })
    }

    async fn put<T: Serialize + ?Sized>(&self, path: &str, value: &T) -> anyhow::Result<()> {
        let token = self.token().await?;
        let data = serde_json::to_value(value)?;
        let body = json!({ "fields": json_to_firestore_fields(&data)? });
        self.client
            .patch(format!("{}/{}", self.root(), path))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await?
            .error_for_status()
            .with_context(|| format!("failed to write Firestore document {path}"))?;
        Ok(())
    }

    /// Escribe sólo si el documento sigue en la versión recién leída, o si aún
    /// no existe. El `false` es una carrera esperada: otro proceso ganó el
    /// claim y hay que releerlo antes de decidir.
    async fn put_if_current<T: Serialize + ?Sized>(
        &self,
        path: &str,
        value: &T,
        update_time: Option<&str>,
    ) -> anyhow::Result<bool> {
        let token = self.token().await?;
        let data = serde_json::to_value(value)?;
        let body = json!({ "fields": json_to_firestore_fields(&data)? });
        let mut request = self
            .client
            .patch(format!("{}/{}", self.root(), path))
            .bearer_auth(token)
            .json(&body);
        request = match update_time {
            Some(update_time) => request.query(&[("currentDocument.updateTime", update_time)]),
            None => request.query(&[("currentDocument.exists", "false")]),
        };
        let response = request.send().await?;
        if response.status().is_success() {
            return Ok(true);
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if is_precondition_conflict(status, &body) {
            return Ok(false);
        }
        Err(anyhow!(
            "failed to conditionally write Firestore document {path} with {status}"
        ))
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<Option<T>> {
        let Some(doc) = self.get_document(path).await? else {
            return Ok(None);
        };
        let value = firestore_fields_to_json(doc.fields)?;
        Ok(Some(serde_json::from_value(value)?))
    }

    async fn get_with_update_time<T: DeserializeOwned>(
        &self,
        path: &str,
    ) -> anyhow::Result<Option<(T, String)>> {
        let Some(doc) = self.get_document(path).await? else {
            return Ok(None);
        };
        let update_time = doc
            .update_time
            .ok_or_else(|| anyhow!("Firestore document {path} did not include updateTime"))?;
        let value = firestore_fields_to_json(doc.fields)?;
        Ok(Some((serde_json::from_value(value)?, update_time)))
    }

    async fn get_document(&self, path: &str) -> anyhow::Result<Option<FirestoreDocument>> {
        let token = self.token().await?;
        let response = self
            .client
            .get(format!("{}/{}", self.root(), path))
            .bearer_auth(token)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(response.error_for_status()?.json().await?))
    }

    async fn list<T: DeserializeOwned>(
        &self,
        parent: &str,
        collection: &str,
    ) -> anyhow::Result<Vec<T>> {
        let token = self.token().await?;
        let url = if parent.is_empty() {
            format!("{}/{}", self.root(), collection)
        } else {
            format!("{}/{}/{}", self.root(), parent, collection)
        };
        let mut values = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut request = self
                .client
                .get(&url)
                .bearer_auth(&token)
                .query(&[("pageSize", "300")]);
            if let Some(page_token) = page_token.as_deref() {
                request = request.query(&[("pageToken", page_token)]);
            }
            let response: FirestoreListResponse =
                request.send().await?.error_for_status()?.json().await?;
            for doc in response.documents {
                let _ = &doc.name;
                let value = firestore_fields_to_json(doc.fields)?;
                values.push(serde_json::from_value(value)?);
            }
            page_token = response.next_page_token.filter(|token| !token.is_empty());
            if page_token.is_none() {
                break;
            }
        }
        Ok(values)
    }

    async fn list_with_ids<T: DeserializeOwned>(
        &self,
        parent: &str,
        collection: &str,
    ) -> anyhow::Result<Vec<(String, T)>> {
        let token = self.token().await?;
        let url = if parent.is_empty() {
            format!("{}/{}", self.root(), collection)
        } else {
            format!("{}/{}/{}", self.root(), parent, collection)
        };
        let mut values = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut request = self
                .client
                .get(&url)
                .bearer_auth(&token)
                .query(&[("pageSize", "300")]);
            if let Some(page_token) = page_token.as_deref() {
                request = request.query(&[("pageToken", page_token)]);
            }
            let response: FirestoreListResponse =
                request.send().await?.error_for_status()?.json().await?;
            for doc in response.documents {
                let id = doc
                    .name
                    .rsplit('/')
                    .next()
                    .ok_or_else(|| anyhow!("Firestore document name is missing an ID"))?
                    .to_string();
                values.push((
                    id,
                    serde_json::from_value(firestore_fields_to_json(doc.fields)?)?,
                ));
            }
            page_token = response.next_page_token.filter(|token| !token.is_empty());
            if page_token.is_none() {
                break;
            }
        }
        Ok(values)
    }

    async fn list_document_ids(
        &self,
        parent: &str,
        collection: &str,
    ) -> anyhow::Result<Vec<String>> {
        let token = self.token().await?;
        let url = if parent.is_empty() {
            format!("{}/{}", self.root(), collection)
        } else {
            format!("{}/{}/{}", self.root(), parent, collection)
        };
        let mut ids = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut request = self
                .client
                .get(&url)
                .bearer_auth(&token)
                .query(&[("pageSize", "300")]);
            if let Some(page_token) = page_token.as_deref() {
                request = request.query(&[("pageToken", page_token)]);
            }
            let response: FirestoreListResponse =
                request.send().await?.error_for_status()?.json().await?;
            ids.extend(
                response
                    .documents
                    .into_iter()
                    .filter_map(|doc| doc.name.rsplit('/').next().map(ToOwned::to_owned)),
            );
            page_token = response.next_page_token.filter(|token| !token.is_empty());
            if page_token.is_none() {
                break;
            }
        }
        Ok(ids)
    }

    async fn delete(&self, path: &str) -> anyhow::Result<()> {
        let token = self.token().await?;
        let response = self
            .client
            .delete(format!("{}/{}", self.root(), path))
            .bearer_auth(token)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        response
            .error_for_status()
            .with_context(|| format!("failed to delete Firestore document {path}"))?;
        Ok(())
    }
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct FirestoreCopyCounts {
    pub accounts: u64,
    pub sessions: u64,
    pub gmail_connections: u64,
    pub mailbox_metadata: u64,
    pub schedule_configs: u64,
    pub schedule_states: u64,
    pub org_configs: u64,
    pub subscriptions: u64,
    pub checkouts: u64,
    pub usage_ledgers: u64,
    pub runs: u64,
    pub threads: u64,
    pub messages: u64,
    pub ai_audits: u64,
    pub manual_reviews: u64,
    pub manual_overrides: u64,
    pub filter_presets: u64,
    pub analysis_deletion_audits: u64,
    pub records: u64,
}

impl FirestoreCopyCounts {
    fn expected_records(&self) -> u64 {
        self.accounts
            + self.sessions
            + self.gmail_connections
            + self.mailbox_metadata
            + self.schedule_configs
            + self.schedule_states
            + self.org_configs
            + self.subscriptions
            + self.checkouts
            + self.usage_ledgers
            + self.runs
            + self.threads
            + self.messages
            + self.ai_audits
            + self.manual_reviews
            + self.manual_overrides
            + self.filter_presets
            + self.analysis_deletion_audits
    }

    fn add_run_copy(&mut self, copy: Self) {
        self.runs += copy.runs;
        self.threads += copy.threads;
        self.messages += copy.messages;
        self.ai_audits += copy.ai_audits;
        self.manual_reviews += copy.manual_reviews;
    }
}

impl FirestoreStorage {
    /// Copia una instantánea de Firestore sin modificar el origen. El destino
    /// se reemplaza completo y se verifica contra el conteo esperado.
    pub(crate) async fn copy_to_postgres(
        &self,
        target: &PostgresStorage,
    ) -> anyhow::Result<FirestoreCopyCounts> {
        if self
            .get::<MigrationMarker>(MANUAL_REVIEW_METRICS_MIGRATION_PATH)
            .await?
            .is_none()
            || self
                .get::<MigrationMarker>(MANUAL_REVIEW_INHERITANCE_MIGRATION_PATH)
                .await?
                .is_none()
        {
            return Err(anyhow!(
                "Firestore manual-review migrations v1 and v2 must complete before PostgreSQL import"
            ));
        }

        target.clear_for_firestore_import().await?;
        let mut counts = FirestoreCopyCounts::default();
        for account in self.list::<Account>("", "accounts").await? {
            target.upsert_account(&account).await?;
            counts.accounts += 1;
        }
        for session in self.list::<UserSession>("", "users").await? {
            target.upsert_user_session(&session).await?;
            counts.sessions += 1;
        }
        for connection in self
            .list::<MailboxConnection>("", "gmailConnections")
            .await?
        {
            target.upsert_gmail_connection(&connection).await?;
            if let Some(metadata) = self.get_mailbox_metadata(&connection.owner_email).await? {
                target
                    .upsert_mailbox_metadata(&connection.owner_email, &metadata)
                    .await?;
                counts.mailbox_metadata += 1;
            }
            counts.gmail_connections += 1;
        }
        for config in self.list::<ScheduleConfig>("", "scheduleConfigs").await? {
            target.upsert_schedule_config(&config).await?;
            counts.schedule_configs += 1;
        }
        for state in self.list::<ScheduleState>("", "scheduleStates").await? {
            target.upsert_schedule_state(&state).await?;
            counts.schedule_states += 1;
        }
        for owner_profile in self.list_document_ids("", "ownerProfiles").await? {
            if let Some(bundle) = self
                .get::<OrgConfigBundle>(&format!("ownerProfiles/{owner_profile}/config/current"))
                .await?
            {
                target.upsert_org_config(&bundle).await?;
                counts.org_configs += 1;
            }
            for (_, preset) in self
                .list_with_ids::<FilterPreset>(
                    &format!("ownerProfiles/{owner_profile}"),
                    "filterPresets",
                )
                .await?
            {
                target.upsert_filter_preset(&preset).await?;
                counts.filter_presets += 1;
            }
            for (_, review) in self
                .list_with_ids::<ManualReviewOverride>(
                    &format!("ownerProfiles/{owner_profile}"),
                    "manualReviewOverrides",
                )
                .await?
            {
                target.upsert_manual_review_override(&review).await?;
                counts.manual_overrides += 1;
            }
        }
        for subscription in self.list::<Subscription>("", "subscriptions").await? {
            target.upsert_subscription(&subscription).await?;
            counts.subscriptions += 1;
        }
        for checkout in self.list::<CheckoutSession>("", "checkoutSessions").await? {
            target.upsert_checkout_session(&checkout).await?;
            counts.checkouts += 1;
        }
        for usage in self.list::<UsageLedger>("", "usageLedgers").await? {
            target.replace_usage_ledger_for_import(&usage).await?;
            counts.usage_ledgers += 1;
        }
        for audit in self
            .list::<AnalysisDataDeletionAudit>("", "auditLogs")
            .await?
        {
            target.record_analysis_data_deletion(&audit).await?;
            counts.analysis_deletion_audits += 1;
        }
        let runs = self.list::<AnalysisRun>("", "analysisRuns").await?;
        let mut copies = stream::iter(
            runs.into_iter()
                .map(|run| async move { self.copy_analysis_run_to_postgres(target, run).await }),
        )
        .buffer_unordered(5);
        while let Some(copy) = copies.next().await {
            counts.add_run_copy(copy?);
        }
        counts.records = target.record_count().await?;
        let expected_records = counts.expected_records();
        if counts.records != expected_records {
            return Err(anyhow!(
                "PostgreSQL snapshot count mismatch: expected {expected_records}, got {}",
                counts.records
            ));
        }
        Ok(counts)
    }

    async fn copy_analysis_run_to_postgres(
        &self,
        target: &PostgresStorage,
        run: AnalysisRun,
    ) -> anyhow::Result<FirestoreCopyCounts> {
        let mut counts = FirestoreCopyCounts::default();
        target.create_analysis_run(&run).await?;
        counts.runs += 1;
        for thread in self.list_threads(&run.id).await? {
            let messages = self.list_messages(&run.id, &thread.id).await?;
            target.upsert_thread(&thread, &messages).await?;
            counts.threads += 1;
            counts.messages += messages.len() as u64;
            for (id, audit) in self
                .list_with_ids::<AiAuditResult>(
                    &format!("analysisRuns/{}/threads/{}", run.id, thread.id),
                    "aiAudits",
                )
                .await?
            {
                target
                    .import_ai_audit(&id, &run.id, &thread.id, &audit)
                    .await?;
                counts.ai_audits += 1;
            }
        }
        for (_, review) in self
            .list_with_ids::<ManualReview>(&format!("analysisRuns/{}", run.id), "manualReviews")
            .await?
        {
            target.add_manual_review(&run.id, &review).await?;
            counts.manual_reviews += 1;
        }
        Ok(counts)
    }
}

#[async_trait]
impl StorageRepository for FirestoreStorage {
    async fn upsert_account(&self, account: &Account) -> anyhow::Result<()> {
        self.put(&format!("accounts/{}", account.workos_user_id), account)
            .await?;
        self.put(
            &account_email_index_path(&account.email),
            &AccountEmailIndex {
                workos_user_id: account.workos_user_id.clone(),
            },
        )
        .await
    }

    async fn get_account_by_workos_user_id(
        &self,
        workos_user_id: &str,
    ) -> anyhow::Result<Option<Account>> {
        self.get(&format!("accounts/{workos_user_id}")).await
    }

    async fn get_account_by_email(&self, email: &str) -> anyhow::Result<Option<Account>> {
        let index_path = account_email_index_path(email);
        if let Some(index) = self.get::<AccountEmailIndex>(&index_path).await? {
            if let Some(account) = self
                .get_account_by_workos_user_id(&index.workos_user_id)
                .await?
                .filter(|account| account.email.eq_ignore_ascii_case(email))
            {
                return Ok(Some(account));
            }
            // ponytail: un correo cambiado deja un índice viejo hasta que se
            // consulte; se limpia entonces sin añadir una lectura a cada login.
            self.delete(&index_path).await?;
        }

        let accounts: Vec<Account> = self.list("", "accounts").await?;
        let account = accounts
            .into_iter()
            .find(|account| account.email.eq_ignore_ascii_case(email));
        if let Some(account) = &account {
            self.upsert_account(account).await?;
        }
        Ok(account)
    }

    async fn rebind_account_workos_user_id(
        &self,
        previous: &Account,
        account: &Account,
        now: chrono::DateTime<Utc>,
    ) -> anyhow::Result<()> {
        if previous.workos_user_id == account.workos_user_id
            || !previous.email.eq_ignore_ascii_case(&account.email)
        {
            anyhow::bail!("invalid WorkOS account rebind");
        }
        self.upsert_account(account).await?;
        self.revoke_user_sessions(&account.email, now).await?;
        self.delete(&format!("accounts/{}", previous.workos_user_id))
            .await
    }

    async fn upsert_user_session(&self, session: &UserSession) -> anyhow::Result<()> {
        self.put(&format!("users/{}", session.id), session).await
    }

    async fn get_user_session(&self, id: &str) -> anyhow::Result<Option<UserSession>> {
        self.get(&format!("users/{id}")).await
    }

    async fn upsert_gmail_connection(&self, connection: &MailboxConnection) -> anyhow::Result<()> {
        self.put(
            &format!(
                "gmailConnections/{}",
                hash_owner_email(&connection.owner_email)
            ),
            connection,
        )
        .await
    }

    async fn get_gmail_connection(
        &self,
        owner_email: &str,
    ) -> anyhow::Result<Option<MailboxConnection>> {
        let path = format!("gmailConnections/{}", hash_owner_email(owner_email));
        if let Some(connection) = self.get(&path).await? {
            return Ok(Some(connection));
        }

        // ponytail: escanea `users` durante la migración legacy; usar una consulta
        // indexada por propietario si el volumen hace costoso este camino transitorio.
        let legacy = self
            .list::<UserSession>("", "users")
            .await?
            .into_iter()
            .filter(|session| {
                session
                    .google_account_email
                    .eq_ignore_ascii_case(owner_email)
            })
            .filter_map(|session| gmail_connection_from_legacy(&session))
            .max_by_key(|connection| {
                (
                    connection.refresh_token_encrypted.is_some(),
                    connection.updated_at,
                )
            });
        let Some(connection) = legacy else {
            return Ok(None);
        };

        self.put(&path, &connection).await?;
        let sessions: Vec<UserSession> = self.list("", "users").await?;
        for mut session in sessions {
            if session
                .google_account_email
                .eq_ignore_ascii_case(owner_email)
            {
                clear_gmail_connection(&mut session, connection.updated_at);
                self.put(&format!("users/{}", session.id), &session).await?;
            }
        }
        Ok(Some(connection))
    }

    async fn refresh_gmail_connection(
        &self,
        previous: &MailboxConnection,
        updated: &MailboxConnection,
    ) -> anyhow::Result<MailboxConnectionRefresh> {
        let path = format!(
            "gmailConnections/{}",
            hash_owner_email(&previous.owner_email)
        );
        let Some((current, update_time)) = self
            .get_with_update_time::<MailboxConnection>(&path)
            .await?
        else {
            return Ok(MailboxConnectionRefresh::ConnectionChanged);
        };
        if current.updated_at != previous.updated_at
            || !mailbox_connection_is_active(Some(&current))
        {
            return Ok(MailboxConnectionRefresh::ConnectionChanged);
        }
        if self
            .put_if_current(&path, updated, Some(&update_time))
            .await?
        {
            Ok(MailboxConnectionRefresh::Updated)
        } else {
            Ok(MailboxConnectionRefresh::ConnectionChanged)
        }
    }

    async fn revoke_user_sessions(
        &self,
        owner_email: &str,
        now: chrono::DateTime<Utc>,
    ) -> anyhow::Result<()> {
        // `list` recorre páginas; una consulta indexada por propietario evita
        // escanear toda la colección si el volumen crece.
        let sessions: Vec<UserSession> = self.list("", "users").await?;
        for mut session in sessions {
            if session
                .google_account_email
                .eq_ignore_ascii_case(owner_email)
            {
                session.revoked_at = Some(now);
                session.updated_at = now;
                self.put(&format!("users/{}", session.id), &session).await?;
            }
        }
        Ok(())
    }

    async fn revoke_user_session_by_workos_session_id(
        &self,
        workos_session_id: &str,
        now: chrono::DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let sessions: Vec<UserSession> = self.list("", "users").await?;
        for mut session in sessions {
            if session.workos_session_id.as_deref() == Some(workos_session_id) {
                session.revoked_at = Some(now);
                session.updated_at = now;
                self.put(&format!("users/{}", session.id), &session).await?;
            }
        }
        Ok(())
    }

    async fn disconnect_gmail(
        &self,
        owner_email: &str,
        now: chrono::DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let connection_path = format!("gmailConnections/{}", hash_owner_email(owner_email));
        if let Some(mut connection) = self.get::<MailboxConnection>(&connection_path).await? {
            connection.access_token_encrypted.clear();
            connection.refresh_token_encrypted = None;
            connection.revoked_at = Some(now);
            connection.updated_at = now;
            self.put(&connection_path, &connection).await?;
        }
        // `list` recorre páginas; una consulta indexada por propietario evita
        // escanear toda la colección si el volumen crece.
        let sessions: Vec<UserSession> = self.list("", "users").await?;
        for mut session in sessions {
            if session
                .google_account_email
                .eq_ignore_ascii_case(owner_email)
            {
                clear_gmail_connection(&mut session, now);
                self.put(&format!("users/{}", session.id), &session).await?;
            }
        }
        Ok(())
    }

    async fn list_schedule_configs(&self) -> anyhow::Result<Vec<ScheduleConfig>> {
        self.list("", "scheduleConfigs").await
    }

    async fn upsert_schedule_config(&self, config: &ScheduleConfig) -> anyhow::Result<()> {
        self.put(&format!("scheduleConfigs/{}", config.user_email), config)
            .await
    }

    async fn get_schedule_state(&self, user_email: &str) -> anyhow::Result<Option<ScheduleState>> {
        self.get(&format!("scheduleStates/{user_email}")).await
    }

    async fn upsert_schedule_state(&self, state: &ScheduleState) -> anyhow::Result<()> {
        self.put(&format!("scheduleStates/{}", state.user_email), state)
            .await
    }

    async fn claim_schedule_window(
        &self,
        state: &ScheduleState,
        stale_before: chrono::DateTime<Utc>,
    ) -> anyhow::Result<ScheduleWindowClaim> {
        let path = format!("scheduleStates/{}", state.user_email);
        for _ in 0..SCHEDULE_CLAIM_MAX_ATTEMPTS {
            match self.get_with_update_time::<ScheduleState>(&path).await? {
                Some((existing, update_time)) => {
                    if let Some(result) =
                        existing_schedule_window_claim(&existing, state, stale_before)
                    {
                        return Ok(result);
                    }
                    if self
                        .put_if_current(&path, state, Some(&update_time))
                        .await?
                    {
                        return Ok(ScheduleWindowClaim::Claimed);
                    }
                }
                None => {
                    if self.put_if_current(&path, state, None).await? {
                        return Ok(ScheduleWindowClaim::Claimed);
                    }
                }
            }
        }
        Err(anyhow!(
            "schedule claim changed concurrently after {SCHEDULE_CLAIM_MAX_ATTEMPTS} attempts"
        ))
    }

    async fn get_org_config_for_user(
        &self,
        user_email: &str,
    ) -> anyhow::Result<Option<OrgConfigBundle>> {
        self.get(&format!(
            "ownerProfiles/{}/config/current",
            hash_owner_email(user_email)
        ))
        .await
    }

    async fn upsert_org_config(&self, bundle: &OrgConfigBundle) -> anyhow::Result<()> {
        let owner_key = hash_owner_email(&bundle.membership.user_email);
        self.put(&format!("ownerProfiles/{owner_key}/config/current"), bundle)
            .await?;
        self.put(&format!("organizations/{}", bundle.org.id), &bundle.org)
            .await?;
        self.put(
            &format!("organizations/{}/memberships/{}", bundle.org.id, owner_key),
            &bundle.membership,
        )
        .await?;
        self.put(
            &format!(
                "organizations/{}/mailboxes/{}",
                bundle.org.id, bundle.mailbox.id
            ),
            &bundle.mailbox,
        )
        .await?;
        self.put(
            &format!("organizations/{}/policyDraft/current", bundle.org.id),
            &bundle.draft,
        )
        .await?;
        self.put(
            &format!(
                "organizations/{}/policyVersions/{}",
                bundle.org.id, bundle.policy_version.id
            ),
            &bundle.policy_version,
        )
        .await
    }

    async fn get_policy_version(
        &self,
        org_id: &str,
        policy_version_id: &str,
    ) -> anyhow::Result<Option<PolicyVersion>> {
        self.get(&format!(
            "organizations/{org_id}/policyVersions/{policy_version_id}"
        ))
        .await
    }

    async fn user_is_org_member(&self, org_id: &str, user_email: &str) -> anyhow::Result<bool> {
        Ok(self
            .get::<crate::policies::Membership>(&format!(
                "organizations/{org_id}/memberships/{}",
                hash_owner_email(user_email)
            ))
            .await?
            .is_some_and(|membership| {
                membership.status == crate::policies::MembershipStatus::Active
            }))
    }

    async fn upsert_subscription(&self, subscription: &Subscription) -> anyhow::Result<()> {
        self.put(
            &format!("subscriptions/{}", subscription.org_id),
            subscription,
        )
        .await
    }

    async fn get_subscription_for_org(&self, org_id: &str) -> anyhow::Result<Option<Subscription>> {
        self.get(&format!("subscriptions/{org_id}")).await
    }

    async fn find_subscription_by_provider_id(
        &self,
        provider_subscription_id: &str,
    ) -> anyhow::Result<Option<Subscription>> {
        let subscriptions: Vec<Subscription> = self.list("", "subscriptions").await?;
        Ok(subscriptions.into_iter().find(|subscription| {
            subscription.provider_subscription_id.as_deref() == Some(provider_subscription_id)
        }))
    }

    async fn upsert_checkout_session(&self, checkout: &CheckoutSession) -> anyhow::Result<()> {
        self.put(&format!("checkoutSessions/{}", checkout.id), checkout)
            .await
    }

    async fn get_checkout_session(&self, id: &str) -> anyhow::Result<Option<CheckoutSession>> {
        self.get(&format!("checkoutSessions/{id}")).await
    }

    async fn find_checkout_session_by_provider_id(
        &self,
        provider_subscription_id: &str,
    ) -> anyhow::Result<Option<CheckoutSession>> {
        let sessions: Vec<CheckoutSession> = self.list("", "checkoutSessions").await?;
        Ok(sessions.into_iter().find(|checkout| {
            checkout.provider_subscription_id.as_deref() == Some(provider_subscription_id)
        }))
    }

    async fn add_usage(
        &self,
        org_id: &str,
        period_key: &str,
        runs_created: u32,
        analyzed_threads: u32,
        ai_audited_threads: u32,
    ) -> anyhow::Result<()> {
        let path = format!("usageLedgers/{org_id}_{period_key}");
        for _ in 0..USAGE_UPDATE_MAX_ATTEMPTS {
            let (mut usage, update_time) = match self.get_with_update_time(&path).await? {
                Some((usage, update_time)) => (usage, Some(update_time)),
                None => (
                    UsageLedger {
                        org_id: org_id.to_string(),
                        period_key: period_key.to_string(),
                        runs_created: 0,
                        analyzed_threads: 0,
                        ai_audited_threads: 0,
                        updated_at: Utc::now(),
                    },
                    None,
                ),
            };
            usage.runs_created = usage.runs_created.saturating_add(runs_created);
            usage.analyzed_threads = usage.analyzed_threads.saturating_add(analyzed_threads);
            usage.ai_audited_threads = usage.ai_audited_threads.saturating_add(ai_audited_threads);
            usage.updated_at = Utc::now();
            if self
                .put_if_current(&path, &usage, update_time.as_deref())
                .await?
            {
                return Ok(());
            }
        }
        Err(anyhow!(
            "usage ledger changed concurrently after {USAGE_UPDATE_MAX_ATTEMPTS} attempts"
        ))
    }

    async fn get_usage_ledger(
        &self,
        org_id: &str,
        period_key: &str,
    ) -> anyhow::Result<Option<UsageLedger>> {
        self.get(&format!("usageLedgers/{org_id}_{period_key}"))
            .await
    }

    async fn create_analysis_run(&self, run: &AnalysisRun) -> anyhow::Result<()> {
        self.put(&format!("analysisRuns/{}", run.id), run).await
    }

    async fn claim_pending_analysis_run(&self, id: &str) -> anyhow::Result<Option<AnalysisRun>> {
        let path = format!("analysisRuns/{id}");
        for _ in 0..ANALYSIS_START_CLAIM_MAX_ATTEMPTS {
            let Some((mut run, update_time)) =
                self.get_with_update_time::<AnalysisRun>(&path).await?
            else {
                return Ok(None);
            };
            if run.status != AnalysisStatus::Pending {
                return Ok(None);
            }
            run.status = AnalysisStatus::Running;
            run.progress_message = "Iniciando lectura de Gmail".to_string();
            if self.put_if_current(&path, &run, Some(&update_time)).await? {
                return Ok(Some(run));
            }
        }
        Err(anyhow!(
            "analysis start claim changed concurrently after {ANALYSIS_START_CLAIM_MAX_ATTEMPTS} attempts"
        ))
    }

    async fn update_analysis_run(&self, run: &AnalysisRun) -> anyhow::Result<()> {
        self.put(&format!("analysisRuns/{}", run.id), run).await
    }

    async fn get_analysis_run(&self, id: &str) -> anyhow::Result<Option<AnalysisRun>> {
        self.get(&format!("analysisRuns/{id}")).await
    }

    async fn list_analysis_runs(&self, user_email: &str) -> anyhow::Result<Vec<AnalysisRun>> {
        let mut runs: Vec<AnalysisRun> = self.list("", "analysisRuns").await?;
        runs.retain(|run| run.user_email == user_email);
        runs.sort_by_key(|run| run.created_at);
        runs.reverse();
        Ok(runs)
    }

    async fn delete_analysis_data(&self, owner_email: &str) -> anyhow::Result<()> {
        let runs = self.list_analysis_runs(owner_email).await?;
        for run in runs {
            let run_path = format!("analysisRuns/{}", run.id);
            for thread in self.list_threads(&run.id).await? {
                let thread_path = format!("{run_path}/threads/{}", thread.id);
                for message in self.list_messages(&run.id, &thread.id).await? {
                    self.delete(&format!("{thread_path}/messages/{}", message.id))
                        .await?;
                }
                for audit_id in self.list_document_ids(&thread_path, "aiAudits").await? {
                    self.delete(&format!("{thread_path}/aiAudits/{audit_id}"))
                        .await?;
                }
                self.delete(&thread_path).await?;
            }
            let reviews: Vec<ManualReview> = self.list(&run_path, "manualReviews").await?;
            for review in reviews {
                self.delete(&format!("{run_path}/manualReviews/{}", review.id))
                    .await?;
            }
            self.delete(&run_path).await?;
        }
        let overrides_path = format!(
            "ownerProfiles/{}/manualReviewOverrides",
            hash_owner_email(owner_email)
        );
        for override_id in self.list_document_ids("", &overrides_path).await? {
            self.delete(&format!("{overrides_path}/{override_id}"))
                .await?;
        }
        Ok(())
    }

    async fn record_analysis_data_deletion(
        &self,
        audit: &AnalysisDataDeletionAudit,
    ) -> anyhow::Result<()> {
        self.put(&format!("auditLogs/{}", audit.id), audit).await
    }

    async fn upsert_thread(
        &self,
        thread: &EmailThread,
        messages: &[EmailMessage],
    ) -> anyhow::Result<()> {
        let thread_path = format!(
            "analysisRuns/{}/threads/{}",
            thread.analysis_run_id, thread.id
        );
        self.put(&thread_path, thread).await?;
        for message in messages {
            let sanitized = EmailMessage {
                body_text: None,
                ..message.clone()
            };
            self.put(
                &format!("{thread_path}/messages/{}", sanitized.id),
                &sanitized,
            )
            .await?;
        }
        Ok(())
    }

    async fn list_threads(&self, run_id: &str) -> anyhow::Result<Vec<EmailThread>> {
        let mut threads: Vec<EmailThread> = self
            .list(&format!("analysisRuns/{run_id}"), "threads")
            .await?;
        for thread in &mut threads {
            if thread.first_message_at.is_none() {
                let messages: Vec<EmailMessage> = self
                    .list(
                        &format!("analysisRuns/{run_id}/threads/{}", thread.id),
                        "messages",
                    )
                    .await?;
                thread.first_message_at = messages.iter().map(|message| message.date).min();
            }
        }
        threads.sort_by_key(|thread| {
            thread
                .first_message_at
                .or(thread.first_client_message_at)
                .unwrap_or(thread.created_at)
        });
        Ok(threads)
    }

    async fn get_thread(
        &self,
        run_id: &str,
        thread_id: &str,
    ) -> anyhow::Result<Option<EmailThread>> {
        self.get(&format!("analysisRuns/{run_id}/threads/{thread_id}"))
            .await
    }

    async fn find_threads_by_id(&self, thread_id: &str) -> anyhow::Result<Vec<EmailThread>> {
        let runs: Vec<AnalysisRun> = self.list("", "analysisRuns").await?;
        let mut matches = Vec::new();
        for run in runs {
            if let Some(thread) = self
                .get(&format!("analysisRuns/{}/threads/{thread_id}", run.id))
                .await?
            {
                matches.push(thread);
            }
        }
        Ok(matches)
    }

    async fn list_messages(
        &self,
        run_id: &str,
        thread_id: &str,
    ) -> anyhow::Result<Vec<EmailMessage>> {
        self.list(
            &format!("analysisRuns/{run_id}/threads/{thread_id}"),
            "messages",
        )
        .await
    }

    async fn add_ai_audit(
        &self,
        run_id: &str,
        thread_id: &str,
        audit: &AiAuditResult,
    ) -> anyhow::Result<()> {
        self.put(
            &format!(
                "analysisRuns/{run_id}/threads/{thread_id}/aiAudits/{}",
                Uuid::new_v4()
            ),
            audit,
        )
        .await
    }

    async fn add_manual_review(&self, run_id: &str, review: &ManualReview) -> anyhow::Result<()> {
        if self
            .get::<AnalysisRun>(&format!("analysisRuns/{run_id}"))
            .await?
            .is_none()
        {
            return Err(anyhow!("run not found"));
        }
        let path = format!("analysisRuns/{run_id}/threads/{}", review.email_thread_id);
        let mut thread = self
            .get::<EmailThread>(&path)
            .await?
            .ok_or_else(|| anyhow!("thread not found"))?;
        let is_valid_client_request = matches!(
            review.new_classification,
            crate::analysis::Classification::ValidClientRequest
        );
        thread.classification = review.new_classification.clone();
        thread.classification_source = crate::analysis::ClassificationSource::Manual;
        thread.is_valid_client_request = is_valid_client_request;
        thread.is_answered = review.is_answered;
        thread.first_client_message_id = review.first_client_message_id.clone();
        thread.first_internal_reply_message_id = review.first_internal_reply_message_id.clone();
        thread.last_internal_message_id = review.last_internal_message_id.clone();
        thread.first_client_message_at = review.first_client_message_at;
        thread.first_internal_reply_at = review.first_internal_reply_at;
        thread.last_internal_message_at = review.last_internal_message_at;
        thread.response_time_minutes = review.response_time_minutes;
        thread.resolution_time_minutes = review.resolution_time_minutes;
        thread.notes = review.notes.clone();
        thread.manual_review_required = false;
        thread.manual_override_applied = true;
        thread.updated_at = Utc::now();
        let mut canonical_review = review.clone();
        canonical_review.is_valid_client_request = is_valid_client_request;
        self.put(
            &format!("analysisRuns/{run_id}/manualReviews/{}", review.id),
            &canonical_review,
        )
        .await?;
        self.put(&path, &thread).await?;
        Ok(())
    }

    async fn upsert_manual_review_override(
        &self,
        review: &ManualReviewOverride,
    ) -> anyhow::Result<()> {
        self.put(
            &format!(
                "ownerProfiles/{}/manualReviewOverrides/{}",
                hash_owner_email(&review.owner_email),
                review.thread_id
            ),
            review,
        )
        .await
    }

    async fn get_manual_review_override(
        &self,
        owner_email: &str,
        thread_id: &str,
    ) -> anyhow::Result<Option<ManualReviewOverride>> {
        self.get(&format!(
            "ownerProfiles/{}/manualReviewOverrides/{thread_id}",
            hash_owner_email(owner_email)
        ))
        .await
    }

    async fn reconcile_manual_review_metrics_v1(
        &self,
    ) -> anyhow::Result<ManualReviewMetricsMigrationResult> {
        if self
            .get::<MigrationMarker>(MANUAL_REVIEW_METRICS_MIGRATION_PATH)
            .await?
            .is_some()
        {
            return Ok(ManualReviewMetricsMigrationResult {
                already_applied: true,
                ..Default::default()
            });
        }

        let mut runs: Vec<AnalysisRun> = self.list("", "analysisRuns").await?;
        let mut result = ManualReviewMetricsMigrationResult {
            scanned_runs: runs.len() as u64,
            ..Default::default()
        };

        for run in &mut runs {
            let mut threads = self.list_threads(&run.id).await?;
            let mut thread_changed = false;
            for thread in &mut threads {
                if reconcile_legacy_thread_classification(thread) {
                    self.put(
                        &format!("analysisRuns/{}/threads/{}", run.id, thread.id),
                        thread,
                    )
                    .await?;
                    thread_changed = true;
                    result.updated_threads += 1;
                }
            }

            let metrics = calculate_metrics(
                &threads,
                run.metrics.ai_input_tokens,
                run.metrics.ai_output_tokens,
            );
            if thread_changed || run.metrics != metrics {
                run.metrics = metrics;
                self.update_analysis_run(run).await?;
                result.updated_runs += 1;
            }
        }

        self.put(
            MANUAL_REVIEW_METRICS_MIGRATION_PATH,
            &MigrationMarker {
                version: "manual-review-metrics-v1".to_string(),
                completed_at: Utc::now(),
                scanned_runs: result.scanned_runs,
                updated_runs: result.updated_runs,
                updated_threads: result.updated_threads,
            },
        )
        .await?;

        Ok(result)
    }

    async fn reconcile_manual_review_inheritance_v2(
        &self,
    ) -> anyhow::Result<ManualReviewInheritanceMigrationResult> {
        if self
            .get::<MigrationMarker>(MANUAL_REVIEW_INHERITANCE_MIGRATION_PATH)
            .await?
            .is_some()
        {
            return Ok(ManualReviewInheritanceMigrationResult {
                already_applied: true,
                ..Default::default()
            });
        }

        let mut runs: Vec<AnalysisRun> = self.list("", "analysisRuns").await?;
        let mut result = ManualReviewInheritanceMigrationResult {
            scanned_runs: runs.len() as u64,
            ..Default::default()
        };
        let mut overrides: HashMap<String, ManualReviewOverride> = HashMap::new();

        for run in &runs {
            let reviews: Vec<ManualReview> = self
                .list(&format!("analysisRuns/{}", run.id), "manualReviews")
                .await?;
            for review in reviews {
                let messages = self.list_messages(&run.id, &review.email_thread_id).await?;
                let inherited = ManualReviewOverride {
                    owner_email: run.user_email.clone(),
                    thread_id: review.email_thread_id.clone(),
                    source_run_id: run.id.clone(),
                    message_fingerprint: message_fingerprint(&messages),
                    reviewer_label: review.reviewer_label,
                    classification: review.new_classification,
                    is_answered: review.is_answered,
                    first_client_message_id: review.first_client_message_id,
                    first_internal_reply_message_id: review.first_internal_reply_message_id,
                    last_internal_message_id: review.last_internal_message_id,
                    notes: review.notes,
                    created_at: review.created_at,
                };
                let key = format!(
                    "{}:{}",
                    inherited.owner_email.trim().to_ascii_lowercase(),
                    inherited.thread_id
                );
                if overrides
                    .get(&key)
                    .is_none_or(|existing| existing.created_at < inherited.created_at)
                {
                    overrides.insert(key, inherited);
                }
            }
        }

        for review in overrides.values() {
            self.upsert_manual_review_override(review).await?;
            result.created_overrides += 1;
        }

        let mut latest_by_owner: HashMap<String, AnalysisRun> = HashMap::new();
        for run in &runs {
            if run.status != crate::analysis::AnalysisStatus::Completed {
                continue;
            }
            let owner = run.user_email.trim().to_ascii_lowercase();
            if latest_by_owner
                .get(&owner)
                .is_none_or(|current| current.created_at < run.created_at)
            {
                latest_by_owner.insert(owner, run.clone());
            }
        }

        for run in &mut runs {
            let mut threads = self.list_threads(&run.id).await?;
            let is_latest = latest_by_owner
                .get(&run.user_email.trim().to_ascii_lowercase())
                .is_some_and(|latest| latest.id == run.id);
            let mut run_changed = false;
            if is_latest {
                for thread in &mut threads {
                    let key = format!(
                        "{}:{}",
                        run.user_email.trim().to_ascii_lowercase(),
                        thread.thread_id
                    );
                    let Some(review) = overrides.get(&key) else {
                        continue;
                    };
                    let messages = self.list_messages(&run.id, &thread.id).await?;
                    if review.message_fingerprint != message_fingerprint(&messages) {
                        continue;
                    }
                    let changed = thread.classification != review.classification
                        || thread.classification_source
                            != crate::analysis::ClassificationSource::Manual
                        || thread.manual_review_required
                        || !thread.manual_override_applied
                        || thread.notes != review.notes;
                    if changed {
                        apply_manual_review_override(thread, &messages, review);
                        self.put(
                            &format!("analysisRuns/{}/threads/{}", run.id, thread.id),
                            thread,
                        )
                        .await?;
                        result.updated_threads += 1;
                        run_changed = true;
                    }
                }
            }

            let metrics = calculate_metrics(
                &threads,
                run.metrics.ai_input_tokens,
                run.metrics.ai_output_tokens,
            );
            if run_changed || run.metrics != metrics {
                run.metrics = metrics;
                self.update_analysis_run(run).await?;
                result.updated_runs += 1;
            }
        }

        self.put(
            MANUAL_REVIEW_INHERITANCE_MIGRATION_PATH,
            &MigrationMarker {
                version: "manual-review-inheritance-v2".to_string(),
                completed_at: Utc::now(),
                scanned_runs: result.scanned_runs,
                updated_runs: result.updated_runs,
                updated_threads: result.updated_threads,
            },
        )
        .await?;

        Ok(result)
    }

    async fn upsert_mailbox_metadata(
        &self,
        owner_email: &str,
        metadata: &MailboxMetadata,
    ) -> anyhow::Result<()> {
        self.put(
            &format!("mailboxMetadata/{}", hash_owner_email(owner_email)),
            metadata,
        )
        .await
    }

    async fn get_mailbox_metadata(
        &self,
        owner_email: &str,
    ) -> anyhow::Result<Option<MailboxMetadata>> {
        self.get(&format!(
            "mailboxMetadata/{}",
            hash_owner_email(owner_email)
        ))
        .await
    }

    async fn list_filter_presets(&self, owner_email: &str) -> anyhow::Result<Vec<FilterPreset>> {
        let mut presets: Vec<FilterPreset> = self
            .list(
                &format!("ownerProfiles/{}", hash_owner_email(owner_email)),
                "filterPresets",
            )
            .await?;
        presets.retain(|preset| preset.owner_email.eq_ignore_ascii_case(owner_email));
        presets.sort_by_key(|preset| preset.created_at);
        Ok(presets)
    }

    async fn upsert_filter_preset(&self, preset: &FilterPreset) -> anyhow::Result<()> {
        self.put(
            &format!(
                "ownerProfiles/{}/filterPresets/{}",
                hash_owner_email(&preset.owner_email),
                preset.id
            ),
            preset,
        )
        .await
    }

    async fn delete_filter_preset(&self, owner_email: &str, preset_id: &str) -> anyhow::Result<()> {
        self.delete(&format!(
            "ownerProfiles/{}/filterPresets/{preset_id}",
            hash_owner_email(owner_email)
        ))
        .await
    }
}

fn json_to_firestore_fields(value: &Value) -> anyhow::Result<Map<String, Value>> {
    match value {
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| Ok((key.clone(), json_to_firestore_value(value)?)))
            .collect(),
        _ => Err(anyhow!("Firestore document root must be an object")),
    }
}

fn json_to_firestore_value(value: &Value) -> anyhow::Result<Value> {
    Ok(match value {
        Value::Null => json!({ "nullValue": null }),
        Value::Bool(v) => json!({ "booleanValue": v }),
        Value::Number(n) if n.is_i64() || n.is_u64() => json!({ "integerValue": n.to_string() }),
        Value::Number(n) => json!({ "doubleValue": n.as_f64().unwrap_or_default() }),
        Value::String(v) => json!({ "stringValue": v }),
        Value::Array(values) => json!({
            "arrayValue": {
                "values": values
                    .iter()
                    .map(json_to_firestore_value)
                    .collect::<anyhow::Result<Vec<_>>>()?
            }
        }),
        Value::Object(map) => json!({
            "mapValue": {
                "fields": map
                    .iter()
                    .map(|(key, value)| Ok((key.clone(), json_to_firestore_value(value)?)))
                    .collect::<anyhow::Result<Map<String, Value>>>()?
            }
        }),
    })
}

fn firestore_fields_to_json(fields: Map<String, Value>) -> anyhow::Result<Value> {
    let mut out = Map::new();
    for (key, value) in fields {
        out.insert(key, firestore_value_to_json(value)?);
    }
    Ok(Value::Object(out))
}

fn firestore_value_to_json(value: Value) -> anyhow::Result<Value> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("invalid Firestore value"))?;
    if let Some(v) = object.get("nullValue") {
        let _ = v;
        return Ok(Value::Null);
    }
    if let Some(v) = object.get("booleanValue") {
        return Ok(v.clone());
    }
    if let Some(v) = object.get("integerValue").and_then(|v| v.as_str()) {
        return Ok(json!(v.parse::<i64>()?));
    }
    if let Some(v) = object.get("doubleValue") {
        return Ok(v.clone());
    }
    if let Some(v) = object.get("stringValue") {
        return Ok(v.clone());
    }
    if let Some(v) = object.get("timestampValue") {
        return Ok(v.clone());
    }
    if let Some(array) = object.get("arrayValue") {
        let values = array
            .get("values")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(firestore_value_to_json)
            .collect::<anyhow::Result<Vec<_>>>()?;
        return Ok(Value::Array(values));
    }
    if let Some(map) = object.get("mapValue") {
        let fields = map
            .get("fields")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        return firestore_fields_to_json(fields);
    }
    Err(anyhow!("unsupported Firestore value shape"))
}

fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn is_precondition_conflict(status: reqwest::StatusCode, body: &str) -> bool {
    status == reqwest::StatusCode::CONFLICT
        || status == reqwest::StatusCode::PRECONDITION_FAILED
        || (status == reqwest::StatusCode::BAD_REQUEST && body.contains("FAILED_PRECONDITION"))
}

async fn json_or_google_error<T: DeserializeOwned>(
    response: reqwest::Response,
    label: &str,
) -> anyhow::Result<T> {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!("{label} failed with {status}"));
    }
    serde_json::from_str(&text).map_err(|error| anyhow!("{label} returned invalid JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{account_email_index_path, is_precondition_conflict};

    #[test]
    fn account_email_index_normalizes_and_does_not_expose_the_email() {
        let path = account_email_index_path(" Owner@Example.com ");

        assert_eq!(path, account_email_index_path("owner@example.com"));
        assert!(!path.contains("owner@example.com"));
    }

    #[test]
    fn recognizes_firestore_precondition_conflicts_without_exposing_error_bodies() {
        assert!(is_precondition_conflict(
            reqwest::StatusCode::CONFLICT,
            "anything"
        ));
        assert!(is_precondition_conflict(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{\"error\":{\"status\":\"FAILED_PRECONDITION\"}}"#
        ));
        assert!(!is_precondition_conflict(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{\"error\":{\"status\":\"INVALID_ARGUMENT\"}}"#
        ));
    }
}
