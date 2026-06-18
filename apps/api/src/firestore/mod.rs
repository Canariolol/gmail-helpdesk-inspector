use std::{
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    analysis::{AiAuditResult, AnalysisRun, EmailMessage, EmailThread, ManualReview},
    auth::UserSession,
    billing::{Account, CheckoutSession, Subscription, UsageLedger},
    config::{FirestoreConfig, ServiceAccountKey},
    policies::{OrgConfigBundle, PolicyVersion, hash_owner_email},
    scheduler::model::{ScheduleConfig, ScheduleState},
    storage::StorageRepository,
};

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
}

#[derive(Debug, Deserialize)]
struct FirestoreListResponse {
    #[serde(default)]
    documents: Vec<FirestoreDocument>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: u64,
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

    async fn get<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<Option<T>> {
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
        let doc: FirestoreDocument = response.error_for_status()?.json().await?;
        let value = firestore_fields_to_json(doc.fields)?;
        Ok(Some(serde_json::from_value(value)?))
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
        let response: FirestoreListResponse = self
            .client
            .get(url)
            .bearer_auth(token)
            .query(&[("pageSize", "300")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        response
            .documents
            .into_iter()
            .map(|doc| {
                let _ = &doc.name;
                let value = firestore_fields_to_json(doc.fields)?;
                serde_json::from_value(value).map_err(Into::into)
            })
            .collect()
    }
}

#[async_trait]
impl StorageRepository for FirestoreStorage {
    async fn upsert_account(&self, account: &Account) -> anyhow::Result<()> {
        self.put(&format!("accounts/{}", account.workos_user_id), account)
            .await
    }

    async fn get_account_by_workos_user_id(
        &self,
        workos_user_id: &str,
    ) -> anyhow::Result<Option<Account>> {
        self.get(&format!("accounts/{workos_user_id}")).await
    }

    async fn get_account_by_email(&self, email: &str) -> anyhow::Result<Option<Account>> {
        let accounts: Vec<Account> = self.list("", "accounts").await?;
        Ok(accounts
            .into_iter()
            .find(|account| account.email.eq_ignore_ascii_case(email)))
    }

    async fn upsert_user_session(&self, session: &UserSession) -> anyhow::Result<()> {
        self.put(&format!("users/{}", session.id), session).await
    }

    async fn get_user_session(&self, id: &str) -> anyhow::Result<Option<UserSession>> {
        self.get(&format!("users/{id}")).await
    }

    async fn find_latest_session_with_refresh_token(
        &self,
        email: &str,
    ) -> anyhow::Result<Option<UserSession>> {
        // El helper `list` pagina a 300 documentos y cada login crea un doc
        // nuevo en `users/`; con un despliegue mono-usuario alcanza de sobra.
        // Si la colección creciera, el siguiente paso es limpiar sesiones
        // antiguas o paginar con orderBy.
        let sessions: Vec<UserSession> = self.list("", "users").await?;
        Ok(sessions
            .into_iter()
            .filter(|session| {
                session.google_account_email == email
                    && (session.gmail_refresh_token_encrypted.is_some()
                        || session.refresh_token_encrypted.is_some())
            })
            .max_by_key(|session| session.updated_at))
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

    async fn upsert_usage_ledger(&self, usage: &UsageLedger) -> anyhow::Result<()> {
        self.put(
            &format!("usageLedgers/{}_{}", usage.org_id, usage.period_key),
            usage,
        )
        .await
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

    async fn get_thread(&self, thread_id: &str) -> anyhow::Result<Option<EmailThread>> {
        let runs: Vec<AnalysisRun> = self.list("", "analysisRuns").await?;
        for run in runs {
            if let Some(thread) = self
                .get(&format!("analysisRuns/{}/threads/{thread_id}", run.id))
                .await?
            {
                return Ok(Some(thread));
            }
        }
        Ok(None)
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
        thread.classification = review.new_classification.clone();
        thread.classification_source = crate::analysis::ClassificationSource::Manual;
        thread.is_valid_client_request = review.is_valid_client_request;
        thread.is_answered = review.is_answered;
        thread.first_client_message_id = review.first_client_message_id.clone();
        thread.first_internal_reply_message_id = review.first_internal_reply_message_id.clone();
        thread.last_internal_message_id = review.last_internal_message_id.clone();
        thread.first_client_message_at = review.first_client_message_at;
        thread.first_internal_reply_at = review.first_internal_reply_at;
        thread.last_internal_message_at = review.last_internal_message_at;
        thread.response_time_minutes = review.response_time_minutes;
        thread.resolution_time_minutes = review.resolution_time_minutes;
        thread.manual_review_required = false;
        thread.manual_override_applied = true;
        thread.updated_at = Utc::now();
        self.put(
            &format!("analysisRuns/{run_id}/manualReviews/{}", review.id),
            review,
        )
        .await?;
        self.put(&path, &thread).await?;
        Ok(())
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

async fn json_or_google_error<T: DeserializeOwned>(
    response: reqwest::Response,
    label: &str,
) -> anyhow::Result<T> {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!("{label} failed with {status}: {text}"));
    }
    serde_json::from_str(&text)
        .map_err(|error| anyhow!("{label} returned invalid JSON: {error}; body: {text}"))
}
