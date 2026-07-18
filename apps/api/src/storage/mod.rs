use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{
    analysis::{
        AiAuditResult, AnalysisRun, AnalysisStatus, EmailMessage, EmailThread, ManualReview,
        ManualReviewOverride, apply_manual_review_override, calculate_metrics, message_fingerprint,
        reconcile_legacy_thread_classification,
    },
    auth::UserSession,
    billing::{Account, CheckoutSession, Subscription, UsageLedger},
    mailbox::{FilterPreset, GmailConnection, MailboxMetadata},
    policies::{OrgConfigBundle, PolicyVersion},
    scheduler::model::{ScheduleConfig, ScheduleRunStatus, ScheduleState},
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManualReviewMetricsMigrationResult {
    pub already_applied: bool,
    pub scanned_runs: u64,
    pub updated_runs: u64,
    pub updated_threads: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManualReviewInheritanceMigrationResult {
    pub already_applied: bool,
    pub scanned_runs: u64,
    pub created_overrides: u64,
    pub updated_runs: u64,
    pub updated_threads: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisDataDeletionStatus {
    Requested,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisDataDeletionAudit {
    pub id: String,
    pub owner_hash: String,
    pub status: AnalysisDataDeletionStatus,
    pub requested_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Resultado de intentar reservar una ventana del scheduler. La operación debe
/// ser atómica en cada implementación de storage para que dos instancias no
/// ejecuten el mismo análisis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleWindowClaim {
    Claimed,
    AlreadyCompleted,
    AlreadyRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmailConnectionRefresh {
    Updated,
    ConnectionChanged,
}

#[async_trait]
pub trait StorageRepository: Send + Sync {
    /// Chequeo liviano de readiness. Por defecto no consulta nada; cada
    /// backend con conexión real (PostgreSQL) lo sobreescribe.
    async fn ping(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn upsert_account(&self, account: &Account) -> anyhow::Result<()>;
    async fn get_account_by_workos_user_id(
        &self,
        workos_user_id: &str,
    ) -> anyhow::Result<Option<Account>>;
    async fn get_account_by_email(&self, email: &str) -> anyhow::Result<Option<Account>>;
    async fn rebind_account_workos_user_id(
        &self,
        previous: &Account,
        account: &Account,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()>;
    async fn upsert_user_session(&self, session: &UserSession) -> anyhow::Result<()>;
    async fn get_user_session(&self, id: &str) -> anyhow::Result<Option<UserSession>>;
    async fn upsert_gmail_connection(&self, connection: &GmailConnection) -> anyhow::Result<()>;
    async fn get_gmail_connection(
        &self,
        owner_email: &str,
    ) -> anyhow::Result<Option<GmailConnection>>;
    async fn refresh_gmail_connection(
        &self,
        previous: &GmailConnection,
        updated: &GmailConnection,
    ) -> anyhow::Result<GmailConnectionRefresh>;
    async fn revoke_user_sessions(
        &self,
        owner_email: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()>;
    async fn revoke_user_session_by_workos_session_id(
        &self,
        workos_session_id: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()>;
    async fn disconnect_gmail(&self, owner_email: &str, now: DateTime<Utc>) -> anyhow::Result<()>;
    async fn list_schedule_configs(&self) -> anyhow::Result<Vec<ScheduleConfig>>;
    async fn upsert_schedule_config(&self, config: &ScheduleConfig) -> anyhow::Result<()>;
    async fn get_schedule_state(&self, user_email: &str) -> anyhow::Result<Option<ScheduleState>>;
    async fn upsert_schedule_state(&self, state: &ScheduleState) -> anyhow::Result<()>;
    async fn claim_schedule_window(
        &self,
        state: &ScheduleState,
        stale_before: DateTime<Utc>,
    ) -> anyhow::Result<ScheduleWindowClaim>;
    async fn get_org_config_for_user(
        &self,
        user_email: &str,
    ) -> anyhow::Result<Option<OrgConfigBundle>>;
    async fn upsert_org_config(&self, bundle: &OrgConfigBundle) -> anyhow::Result<()>;
    async fn get_policy_version(
        &self,
        org_id: &str,
        policy_version_id: &str,
    ) -> anyhow::Result<Option<PolicyVersion>>;
    async fn user_is_org_member(&self, org_id: &str, user_email: &str) -> anyhow::Result<bool>;
    async fn upsert_subscription(&self, subscription: &Subscription) -> anyhow::Result<()>;
    async fn get_subscription_for_org(&self, org_id: &str) -> anyhow::Result<Option<Subscription>>;
    async fn find_subscription_by_provider_id(
        &self,
        provider_subscription_id: &str,
    ) -> anyhow::Result<Option<Subscription>>;
    async fn upsert_checkout_session(&self, checkout: &CheckoutSession) -> anyhow::Result<()>;
    async fn get_checkout_session(&self, id: &str) -> anyhow::Result<Option<CheckoutSession>>;
    async fn find_checkout_session_by_provider_id(
        &self,
        provider_subscription_id: &str,
    ) -> anyhow::Result<Option<CheckoutSession>>;
    async fn add_usage(
        &self,
        org_id: &str,
        period_key: &str,
        runs_created: u32,
        analyzed_threads: u32,
        ai_audited_threads: u32,
    ) -> anyhow::Result<()>;
    async fn get_usage_ledger(
        &self,
        org_id: &str,
        period_key: &str,
    ) -> anyhow::Result<Option<UsageLedger>>;
    async fn create_analysis_run(&self, run: &AnalysisRun) -> anyhow::Result<()>;
    async fn claim_pending_analysis_run(&self, id: &str) -> anyhow::Result<Option<AnalysisRun>>;
    async fn update_analysis_run(&self, run: &AnalysisRun) -> anyhow::Result<()>;
    async fn get_analysis_run(&self, id: &str) -> anyhow::Result<Option<AnalysisRun>>;
    async fn list_analysis_runs(&self, user_email: &str) -> anyhow::Result<Vec<AnalysisRun>>;
    async fn delete_analysis_data(&self, owner_email: &str) -> anyhow::Result<()>;
    async fn record_analysis_data_deletion(
        &self,
        audit: &AnalysisDataDeletionAudit,
    ) -> anyhow::Result<()>;
    async fn upsert_thread(
        &self,
        thread: &EmailThread,
        messages: &[EmailMessage],
    ) -> anyhow::Result<()>;
    async fn list_threads(&self, run_id: &str) -> anyhow::Result<Vec<EmailThread>>;
    async fn get_thread(
        &self,
        run_id: &str,
        thread_id: &str,
    ) -> anyhow::Result<Option<EmailThread>>;
    async fn find_threads_by_id(&self, thread_id: &str) -> anyhow::Result<Vec<EmailThread>>;
    async fn list_messages(
        &self,
        run_id: &str,
        thread_id: &str,
    ) -> anyhow::Result<Vec<EmailMessage>>;
    async fn add_ai_audit(
        &self,
        run_id: &str,
        thread_id: &str,
        audit: &AiAuditResult,
    ) -> anyhow::Result<()>;
    async fn add_manual_review(&self, run_id: &str, review: &ManualReview) -> anyhow::Result<()>;
    async fn upsert_manual_review_override(
        &self,
        review: &ManualReviewOverride,
    ) -> anyhow::Result<()>;
    async fn get_manual_review_override(
        &self,
        owner_email: &str,
        gmail_thread_id: &str,
    ) -> anyhow::Result<Option<ManualReviewOverride>>;
    async fn reconcile_manual_review_metrics_v1(
        &self,
    ) -> anyhow::Result<ManualReviewMetricsMigrationResult>;
    async fn reconcile_manual_review_inheritance_v2(
        &self,
    ) -> anyhow::Result<ManualReviewInheritanceMigrationResult>;
    async fn upsert_mailbox_metadata(
        &self,
        owner_email: &str,
        metadata: &MailboxMetadata,
    ) -> anyhow::Result<()>;
    async fn get_mailbox_metadata(
        &self,
        owner_email: &str,
    ) -> anyhow::Result<Option<MailboxMetadata>>;
    async fn list_filter_presets(&self, owner_email: &str) -> anyhow::Result<Vec<FilterPreset>>;
    async fn upsert_filter_preset(&self, preset: &FilterPreset) -> anyhow::Result<()>;
    async fn delete_filter_preset(&self, owner_email: &str, preset_id: &str) -> anyhow::Result<()>;
}

#[derive(Default, Clone)]
pub struct MemoryStorage {
    inner: Arc<RwLock<MemoryInner>>,
}

#[derive(Default)]
struct MemoryInner {
    sessions: HashMap<String, UserSession>,
    gmail_connections: HashMap<String, GmailConnection>,
    accounts: HashMap<String, Account>,
    runs: HashMap<String, AnalysisRun>,
    threads: HashMap<String, EmailThread>,
    messages: HashMap<String, Vec<EmailMessage>>,
    audits: HashMap<String, Vec<AiAuditResult>>,
    reviews: Vec<(String, ManualReview)>,
    schedule_configs: HashMap<String, ScheduleConfig>,
    schedule_states: HashMap<String, ScheduleState>,
    org_configs: HashMap<String, OrgConfigBundle>,
    policy_versions: HashMap<String, PolicyVersion>,
    subscriptions: HashMap<String, Subscription>,
    checkout_sessions: HashMap<String, CheckoutSession>,
    usage_ledgers: HashMap<String, UsageLedger>,
    mailbox_metadata: HashMap<String, MailboxMetadata>,
    filter_presets: HashMap<String, FilterPreset>,
    manual_review_overrides: HashMap<String, ManualReviewOverride>,
    analysis_data_deletion_audits: HashMap<String, AnalysisDataDeletionAudit>,
    manual_review_metrics_v1_applied: bool,
    manual_review_inheritance_v2_applied: bool,
}

#[cfg(test)]
impl MemoryStorage {
    pub async fn analysis_data_deletion_audits(&self) -> Vec<AnalysisDataDeletionAudit> {
        let mut audits: Vec<_> = self
            .inner
            .read()
            .await
            .analysis_data_deletion_audits
            .values()
            .cloned()
            .collect();
        audits.sort_by(|left, right| left.id.cmp(&right.id));
        audits
    }
}

fn thread_storage_key(run_id: &str, thread_id: &str) -> String {
    format!("{run_id}:{thread_id}")
}

fn override_storage_key(owner_email: &str, gmail_thread_id: &str) -> String {
    format!(
        "{}:{gmail_thread_id}",
        owner_email.trim().to_ascii_lowercase()
    )
}

pub fn clear_gmail_connection(session: &mut UserSession, now: DateTime<Utc>) {
    session.gmail_account_email = None;
    session.gmail_access_token_encrypted = None;
    session.gmail_refresh_token_encrypted = None;
    session.access_token_encrypted.clear();
    session.refresh_token_encrypted = None;
    session.updated_at = now;
}

fn owner_key(owner_email: &str) -> String {
    owner_email.trim().to_ascii_lowercase()
}

pub(crate) fn existing_schedule_window_claim(
    existing: &ScheduleState,
    candidate: &ScheduleState,
    stale_before: DateTime<Utc>,
) -> Option<ScheduleWindowClaim> {
    let same_window = existing.window_date_from == candidate.window_date_from
        && existing.window_date_to == candidate.window_date_to;
    if !same_window {
        return None;
    }

    match existing.status {
        ScheduleRunStatus::Completed => Some(ScheduleWindowClaim::AlreadyCompleted),
        ScheduleRunStatus::Running if existing.started_at > stale_before => {
            Some(ScheduleWindowClaim::AlreadyRunning)
        }
        ScheduleRunStatus::Running | ScheduleRunStatus::Failed => None,
    }
}

/// Compatibilidad de una sola vez para credenciales creadas antes de que la
/// conexión Gmail se separara de la sesión web.
pub fn gmail_connection_from_legacy(session: &UserSession) -> Option<GmailConnection> {
    let access_token_encrypted = session.gmail_access_token_encrypted.clone().or_else(|| {
        (!session.access_token_encrypted.trim().is_empty())
            .then(|| session.access_token_encrypted.clone())
    })?;
    Some(GmailConnection {
        owner_email: session.google_account_email.clone(),
        gmail_account_email: session.gmail_account_email.clone()?,
        access_token_encrypted,
        refresh_token_encrypted: session
            .gmail_refresh_token_encrypted
            .clone()
            .or(session.refresh_token_encrypted.clone()),
        connected_at: session.created_at,
        updated_at: session.updated_at,
        revoked_at: None,
    })
}

#[async_trait]
impl StorageRepository for MemoryStorage {
    async fn upsert_account(&self, account: &Account) -> anyhow::Result<()> {
        self.inner
            .write()
            .await
            .accounts
            .insert(account.workos_user_id.clone(), account.clone());
        Ok(())
    }

    async fn get_account_by_workos_user_id(
        &self,
        workos_user_id: &str,
    ) -> anyhow::Result<Option<Account>> {
        Ok(self
            .inner
            .read()
            .await
            .accounts
            .get(workos_user_id)
            .cloned())
    }

    async fn get_account_by_email(&self, email: &str) -> anyhow::Result<Option<Account>> {
        let normalized = email.trim().to_lowercase();
        Ok(self
            .inner
            .read()
            .await
            .accounts
            .values()
            .find(|account| account.email.trim().eq_ignore_ascii_case(&normalized))
            .cloned())
    }

    async fn rebind_account_workos_user_id(
        &self,
        previous: &Account,
        account: &Account,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        if previous.workos_user_id == account.workos_user_id
            || !previous.email.eq_ignore_ascii_case(&account.email)
        {
            return Err(anyhow!("invalid WorkOS account rebind"));
        }
        let mut inner = self.inner.write().await;
        let Some(stored) = inner.accounts.get(&previous.workos_user_id) else {
            return Err(anyhow!("previous WorkOS account not found"));
        };
        if !stored.email.eq_ignore_ascii_case(&account.email) {
            return Err(anyhow!("previous WorkOS account email mismatch"));
        }
        inner.accounts.remove(&previous.workos_user_id);
        inner
            .accounts
            .insert(account.workos_user_id.clone(), account.clone());
        for session in inner.sessions.values_mut() {
            if session
                .google_account_email
                .eq_ignore_ascii_case(&account.email)
            {
                session.revoked_at = Some(now);
                session.updated_at = now;
            }
        }
        Ok(())
    }

    async fn upsert_user_session(&self, session: &UserSession) -> anyhow::Result<()> {
        self.inner
            .write()
            .await
            .sessions
            .insert(session.id.clone(), session.clone());
        Ok(())
    }

    async fn get_user_session(&self, id: &str) -> anyhow::Result<Option<UserSession>> {
        Ok(self.inner.read().await.sessions.get(id).cloned())
    }

    async fn upsert_gmail_connection(&self, connection: &GmailConnection) -> anyhow::Result<()> {
        self.inner
            .write()
            .await
            .gmail_connections
            .insert(owner_key(&connection.owner_email), connection.clone());
        Ok(())
    }

    async fn get_gmail_connection(
        &self,
        owner_email: &str,
    ) -> anyhow::Result<Option<GmailConnection>> {
        let key = owner_key(owner_email);
        if let Some(connection) = self.inner.read().await.gmail_connections.get(&key).cloned() {
            return Ok(Some(connection));
        }
        let legacy = self
            .inner
            .read()
            .await
            .sessions
            .values()
            .filter(|session| {
                session
                    .google_account_email
                    .eq_ignore_ascii_case(owner_email)
            })
            .filter_map(gmail_connection_from_legacy)
            .max_by_key(|connection| {
                (
                    connection.refresh_token_encrypted.is_some(),
                    connection.updated_at,
                )
            });
        let Some(connection) = legacy else {
            return Ok(None);
        };

        let mut inner = self.inner.write().await;
        inner.gmail_connections.insert(key, connection.clone());
        for session in inner.sessions.values_mut() {
            if session
                .google_account_email
                .eq_ignore_ascii_case(owner_email)
            {
                clear_gmail_connection(session, connection.updated_at);
            }
        }
        Ok(Some(connection))
    }

    async fn refresh_gmail_connection(
        &self,
        previous: &GmailConnection,
        updated: &GmailConnection,
    ) -> anyhow::Result<GmailConnectionRefresh> {
        let mut inner = self.inner.write().await;
        let Some(current) = inner
            .gmail_connections
            .get(&owner_key(&previous.owner_email))
        else {
            return Ok(GmailConnectionRefresh::ConnectionChanged);
        };
        if current.updated_at != previous.updated_at || current.revoked_at.is_some() {
            return Ok(GmailConnectionRefresh::ConnectionChanged);
        }
        inner
            .gmail_connections
            .insert(owner_key(&updated.owner_email), updated.clone());
        Ok(GmailConnectionRefresh::Updated)
    }

    async fn revoke_user_sessions(
        &self,
        owner_email: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        for session in self.inner.write().await.sessions.values_mut() {
            if session
                .google_account_email
                .eq_ignore_ascii_case(owner_email)
            {
                session.revoked_at = Some(now);
                session.updated_at = now;
            }
        }
        Ok(())
    }

    async fn revoke_user_session_by_workos_session_id(
        &self,
        workos_session_id: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        for session in self.inner.write().await.sessions.values_mut() {
            if session.workos_session_id.as_deref() == Some(workos_session_id) {
                session.revoked_at = Some(now);
                session.updated_at = now;
            }
        }
        Ok(())
    }

    async fn disconnect_gmail(&self, owner_email: &str, now: DateTime<Utc>) -> anyhow::Result<()> {
        let mut inner = self.inner.write().await;
        if let Some(connection) = inner.gmail_connections.get_mut(&owner_key(owner_email)) {
            connection.access_token_encrypted.clear();
            connection.refresh_token_encrypted = None;
            connection.revoked_at = Some(now);
            connection.updated_at = now;
        }
        for session in inner.sessions.values_mut() {
            if session
                .google_account_email
                .eq_ignore_ascii_case(owner_email)
            {
                clear_gmail_connection(session, now);
            }
        }
        Ok(())
    }

    async fn list_schedule_configs(&self) -> anyhow::Result<Vec<ScheduleConfig>> {
        Ok(self
            .inner
            .read()
            .await
            .schedule_configs
            .values()
            .cloned()
            .collect())
    }

    async fn upsert_schedule_config(&self, config: &ScheduleConfig) -> anyhow::Result<()> {
        self.inner
            .write()
            .await
            .schedule_configs
            .insert(config.user_email.clone(), config.clone());
        Ok(())
    }

    async fn get_schedule_state(&self, user_email: &str) -> anyhow::Result<Option<ScheduleState>> {
        Ok(self
            .inner
            .read()
            .await
            .schedule_states
            .get(user_email)
            .cloned())
    }

    async fn upsert_schedule_state(&self, state: &ScheduleState) -> anyhow::Result<()> {
        self.inner
            .write()
            .await
            .schedule_states
            .insert(state.user_email.clone(), state.clone());
        Ok(())
    }

    async fn claim_schedule_window(
        &self,
        state: &ScheduleState,
        stale_before: DateTime<Utc>,
    ) -> anyhow::Result<ScheduleWindowClaim> {
        let mut inner = self.inner.write().await;
        if let Some(existing) = inner.schedule_states.get(&state.user_email)
            && let Some(result) = existing_schedule_window_claim(existing, state, stale_before)
        {
            return Ok(result);
        }
        inner
            .schedule_states
            .insert(state.user_email.clone(), state.clone());
        Ok(ScheduleWindowClaim::Claimed)
    }

    async fn get_org_config_for_user(
        &self,
        user_email: &str,
    ) -> anyhow::Result<Option<OrgConfigBundle>> {
        Ok(self
            .inner
            .read()
            .await
            .org_configs
            .get(&user_email.trim().to_lowercase())
            .cloned())
    }

    async fn upsert_org_config(&self, bundle: &OrgConfigBundle) -> anyhow::Result<()> {
        let mut inner = self.inner.write().await;
        inner.org_configs.insert(
            bundle.membership.user_email.trim().to_lowercase(),
            bundle.clone(),
        );
        inner.policy_versions.insert(
            format!(
                "{}:{}",
                bundle.policy_version.org_id, bundle.policy_version.id
            ),
            bundle.policy_version.clone(),
        );
        Ok(())
    }

    async fn get_policy_version(
        &self,
        org_id: &str,
        policy_version_id: &str,
    ) -> anyhow::Result<Option<PolicyVersion>> {
        Ok(self
            .inner
            .read()
            .await
            .policy_versions
            .get(&format!("{org_id}:{policy_version_id}"))
            .cloned())
    }

    async fn user_is_org_member(&self, org_id: &str, user_email: &str) -> anyhow::Result<bool> {
        Ok(self.inner.read().await.org_configs.values().any(|bundle| {
            bundle.org.id == org_id
                && bundle
                    .membership
                    .user_email
                    .eq_ignore_ascii_case(user_email)
                && bundle.membership.status == crate::policies::MembershipStatus::Active
        }))
    }

    async fn upsert_subscription(&self, subscription: &Subscription) -> anyhow::Result<()> {
        self.inner
            .write()
            .await
            .subscriptions
            .insert(subscription.org_id.clone(), subscription.clone());
        Ok(())
    }

    async fn get_subscription_for_org(&self, org_id: &str) -> anyhow::Result<Option<Subscription>> {
        Ok(self.inner.read().await.subscriptions.get(org_id).cloned())
    }

    async fn find_subscription_by_provider_id(
        &self,
        provider_subscription_id: &str,
    ) -> anyhow::Result<Option<Subscription>> {
        Ok(self
            .inner
            .read()
            .await
            .subscriptions
            .values()
            .find(|subscription| {
                subscription.provider_subscription_id.as_deref() == Some(provider_subscription_id)
            })
            .cloned())
    }

    async fn upsert_checkout_session(&self, checkout: &CheckoutSession) -> anyhow::Result<()> {
        self.inner
            .write()
            .await
            .checkout_sessions
            .insert(checkout.id.clone(), checkout.clone());
        Ok(())
    }

    async fn get_checkout_session(&self, id: &str) -> anyhow::Result<Option<CheckoutSession>> {
        Ok(self.inner.read().await.checkout_sessions.get(id).cloned())
    }

    async fn find_checkout_session_by_provider_id(
        &self,
        provider_subscription_id: &str,
    ) -> anyhow::Result<Option<CheckoutSession>> {
        Ok(self
            .inner
            .read()
            .await
            .checkout_sessions
            .values()
            .find(|checkout| {
                checkout.provider_subscription_id.as_deref() == Some(provider_subscription_id)
            })
            .cloned())
    }

    async fn add_usage(
        &self,
        org_id: &str,
        period_key: &str,
        runs_created: u32,
        analyzed_threads: u32,
        ai_audited_threads: u32,
    ) -> anyhow::Result<()> {
        let mut inner = self.inner.write().await;
        let usage = inner
            .usage_ledgers
            .entry(format!("{org_id}:{period_key}"))
            .or_insert_with(|| UsageLedger {
                org_id: org_id.to_string(),
                period_key: period_key.to_string(),
                runs_created: 0,
                analyzed_threads: 0,
                ai_audited_threads: 0,
                updated_at: Utc::now(),
            });
        usage.runs_created = usage.runs_created.saturating_add(runs_created);
        usage.analyzed_threads = usage.analyzed_threads.saturating_add(analyzed_threads);
        usage.ai_audited_threads = usage.ai_audited_threads.saturating_add(ai_audited_threads);
        usage.updated_at = Utc::now();
        Ok(())
    }

    async fn get_usage_ledger(
        &self,
        org_id: &str,
        period_key: &str,
    ) -> anyhow::Result<Option<UsageLedger>> {
        Ok(self
            .inner
            .read()
            .await
            .usage_ledgers
            .get(&format!("{org_id}:{period_key}"))
            .cloned())
    }

    async fn create_analysis_run(&self, run: &AnalysisRun) -> anyhow::Result<()> {
        self.inner
            .write()
            .await
            .runs
            .insert(run.id.clone(), run.clone());
        Ok(())
    }

    async fn claim_pending_analysis_run(&self, id: &str) -> anyhow::Result<Option<AnalysisRun>> {
        let mut inner = self.inner.write().await;
        let Some(run) = inner.runs.get_mut(id) else {
            return Ok(None);
        };
        if run.status != AnalysisStatus::Pending {
            return Ok(None);
        }
        run.status = AnalysisStatus::Running;
        run.progress_message = "Iniciando lectura de Gmail".to_string();
        Ok(Some(run.clone()))
    }

    async fn update_analysis_run(&self, run: &AnalysisRun) -> anyhow::Result<()> {
        self.inner
            .write()
            .await
            .runs
            .insert(run.id.clone(), run.clone());
        Ok(())
    }

    async fn get_analysis_run(&self, id: &str) -> anyhow::Result<Option<AnalysisRun>> {
        Ok(self.inner.read().await.runs.get(id).cloned())
    }

    async fn list_analysis_runs(&self, user_email: &str) -> anyhow::Result<Vec<AnalysisRun>> {
        let mut runs: Vec<_> = self
            .inner
            .read()
            .await
            .runs
            .values()
            .filter(|run| run.user_email == user_email)
            .cloned()
            .collect();
        runs.sort_by_key(|run| run.created_at);
        runs.reverse();
        Ok(runs)
    }

    async fn delete_analysis_data(&self, owner_email: &str) -> anyhow::Result<()> {
        let owner = owner_email.trim();
        let mut inner = self.inner.write().await;
        let run_ids: HashSet<String> = inner
            .runs
            .values()
            .filter(|run| run.user_email.eq_ignore_ascii_case(owner))
            .map(|run| run.id.clone())
            .collect();
        inner.runs.retain(|id, _| !run_ids.contains(id));
        inner
            .threads
            .retain(|_, thread| !run_ids.contains(&thread.analysis_run_id));
        for run_id in &run_ids {
            let prefix = format!("{run_id}:");
            inner.messages.retain(|key, _| !key.starts_with(&prefix));
            inner.audits.retain(|key, _| !key.starts_with(&prefix));
        }
        inner
            .reviews
            .retain(|(run_id, _)| !run_ids.contains(run_id));
        inner
            .manual_review_overrides
            .retain(|_, review| !review.owner_email.eq_ignore_ascii_case(owner));
        Ok(())
    }

    async fn record_analysis_data_deletion(
        &self,
        audit: &AnalysisDataDeletionAudit,
    ) -> anyhow::Result<()> {
        self.inner
            .write()
            .await
            .analysis_data_deletion_audits
            .insert(audit.id.clone(), audit.clone());
        Ok(())
    }

    async fn upsert_thread(
        &self,
        thread: &EmailThread,
        messages: &[EmailMessage],
    ) -> anyhow::Result<()> {
        let mut inner = self.inner.write().await;
        inner.threads.insert(
            thread_storage_key(&thread.analysis_run_id, &thread.id),
            thread.clone(),
        );
        inner.messages.insert(
            thread_storage_key(&thread.analysis_run_id, &thread.id),
            messages.to_vec(),
        );
        Ok(())
    }

    async fn list_threads(&self, run_id: &str) -> anyhow::Result<Vec<EmailThread>> {
        let inner = self.inner.read().await;
        let mut threads: Vec<_> = inner
            .threads
            .values()
            .filter(|thread| thread.analysis_run_id == run_id)
            .cloned()
            .collect();
        for thread in &mut threads {
            if thread.first_message_at.is_none() {
                thread.first_message_at = inner
                    .messages
                    .get(&thread_storage_key(run_id, &thread.id))
                    .and_then(|messages| messages.iter().map(|message| message.date).min());
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
        Ok(self
            .inner
            .read()
            .await
            .threads
            .get(&thread_storage_key(run_id, thread_id))
            .cloned())
    }

    async fn find_threads_by_id(&self, thread_id: &str) -> anyhow::Result<Vec<EmailThread>> {
        Ok(self
            .inner
            .read()
            .await
            .threads
            .values()
            .filter(|thread| thread.id == thread_id)
            .cloned()
            .collect())
    }

    async fn list_messages(
        &self,
        run_id: &str,
        thread_id: &str,
    ) -> anyhow::Result<Vec<EmailMessage>> {
        Ok(self
            .inner
            .read()
            .await
            .messages
            .get(&thread_storage_key(run_id, thread_id))
            .cloned()
            .unwrap_or_default())
    }

    async fn add_ai_audit(
        &self,
        _run_id: &str,
        thread_id: &str,
        audit: &AiAuditResult,
    ) -> anyhow::Result<()> {
        self.inner
            .write()
            .await
            .audits
            .entry(thread_storage_key(_run_id, thread_id))
            .or_default()
            .push(audit.clone());
        Ok(())
    }

    async fn add_manual_review(&self, run_id: &str, review: &ManualReview) -> anyhow::Result<()> {
        let mut inner = self.inner.write().await;
        if !inner.runs.contains_key(run_id) {
            return Err(anyhow!("run not found"));
        }
        let thread = inner
            .threads
            .get_mut(&thread_storage_key(run_id, &review.email_thread_id))
            .ok_or_else(|| anyhow!("thread not found"))?;
        if thread.analysis_run_id != run_id {
            return Err(anyhow!("thread does not belong to run"));
        }
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
        inner.reviews.push((
            run_id.to_string(),
            ManualReview {
                id: review.id.clone(),
                email_thread_id: review.email_thread_id.clone(),
                reviewer_label: review.reviewer_label.clone(),
                new_classification: review.new_classification.clone(),
                is_valid_client_request,
                is_answered: review.is_answered,
                first_client_message_id: review.first_client_message_id.clone(),
                first_internal_reply_message_id: review.first_internal_reply_message_id.clone(),
                last_internal_message_id: review.last_internal_message_id.clone(),
                first_client_message_at: review.first_client_message_at,
                first_internal_reply_at: review.first_internal_reply_at,
                last_internal_message_at: review.last_internal_message_at,
                response_time_minutes: review.response_time_minutes,
                resolution_time_minutes: review.resolution_time_minutes,
                notes: review.notes.clone(),
                created_at: review.created_at,
            },
        ));
        Ok(())
    }

    async fn upsert_manual_review_override(
        &self,
        review: &ManualReviewOverride,
    ) -> anyhow::Result<()> {
        self.inner.write().await.manual_review_overrides.insert(
            override_storage_key(&review.owner_email, &review.gmail_thread_id),
            review.clone(),
        );
        Ok(())
    }

    async fn get_manual_review_override(
        &self,
        owner_email: &str,
        gmail_thread_id: &str,
    ) -> anyhow::Result<Option<ManualReviewOverride>> {
        Ok(self
            .inner
            .read()
            .await
            .manual_review_overrides
            .get(&override_storage_key(owner_email, gmail_thread_id))
            .cloned())
    }

    async fn reconcile_manual_review_metrics_v1(
        &self,
    ) -> anyhow::Result<ManualReviewMetricsMigrationResult> {
        let mut inner = self.inner.write().await;
        if inner.manual_review_metrics_v1_applied {
            return Ok(ManualReviewMetricsMigrationResult {
                already_applied: true,
                ..Default::default()
            });
        }

        let run_ids: Vec<String> = inner.runs.keys().cloned().collect();
        let mut result = ManualReviewMetricsMigrationResult {
            scanned_runs: run_ids.len() as u64,
            ..Default::default()
        };

        for run_id in run_ids {
            let thread_ids: Vec<String> = inner
                .threads
                .values()
                .filter(|thread| thread.analysis_run_id == run_id)
                .map(|thread| thread.id.clone())
                .collect();

            let mut thread_changed = false;
            for thread_id in thread_ids {
                if let Some(thread) = inner
                    .threads
                    .get_mut(&thread_storage_key(&run_id, &thread_id))
                    && reconcile_legacy_thread_classification(thread)
                {
                    thread_changed = true;
                    result.updated_threads += 1;
                }
            }

            let threads: Vec<EmailThread> = inner
                .threads
                .values()
                .filter(|thread| thread.analysis_run_id == run_id)
                .cloned()
                .collect();
            if let Some(run) = inner.runs.get_mut(&run_id) {
                let metrics = calculate_metrics(
                    &threads,
                    run.metrics.ai_input_tokens,
                    run.metrics.ai_output_tokens,
                );
                if thread_changed || run.metrics != metrics {
                    run.metrics = metrics;
                    result.updated_runs += 1;
                }
            }
        }

        inner.manual_review_metrics_v1_applied = true;
        Ok(result)
    }

    async fn reconcile_manual_review_inheritance_v2(
        &self,
    ) -> anyhow::Result<ManualReviewInheritanceMigrationResult> {
        let mut inner = self.inner.write().await;
        if inner.manual_review_inheritance_v2_applied {
            return Ok(ManualReviewInheritanceMigrationResult {
                already_applied: true,
                ..Default::default()
            });
        }

        let mut result = ManualReviewInheritanceMigrationResult {
            scanned_runs: inner.runs.len() as u64,
            ..Default::default()
        };
        let reviews = inner.reviews.clone();
        for (run_id, review) in reviews {
            let Some(run) = inner.runs.get(&run_id) else {
                continue;
            };
            let messages = inner
                .messages
                .get(&thread_storage_key(&run_id, &review.email_thread_id))
                .cloned()
                .unwrap_or_default();
            let inherited = ManualReviewOverride {
                owner_email: run.user_email.clone(),
                gmail_thread_id: review.email_thread_id.clone(),
                source_run_id: run_id,
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
            let key = override_storage_key(&inherited.owner_email, &inherited.gmail_thread_id);
            let should_replace = inner
                .manual_review_overrides
                .get(&key)
                .is_none_or(|existing| existing.created_at < inherited.created_at);
            if should_replace {
                inner.manual_review_overrides.insert(key, inherited);
                result.created_overrides += 1;
            }
        }

        let mut latest_by_owner: HashMap<String, AnalysisRun> = HashMap::new();
        for run in inner.runs.values() {
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

        let runs = inner.runs.values().cloned().collect::<Vec<_>>();
        for run in &runs {
            let thread_keys = inner
                .threads
                .iter()
                .filter(|(_, thread)| thread.analysis_run_id == run.id)
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            let mut run_changed = false;
            let is_latest = latest_by_owner
                .get(&run.user_email.trim().to_ascii_lowercase())
                .is_some_and(|latest| latest.id == run.id);
            if is_latest {
                for thread_key in &thread_keys {
                    let Some(snapshot) = inner.threads.get(thread_key).cloned() else {
                        continue;
                    };
                    let Some(review) = inner
                        .manual_review_overrides
                        .get(&override_storage_key(
                            &run.user_email,
                            &snapshot.gmail_thread_id,
                        ))
                        .cloned()
                    else {
                        continue;
                    };
                    let messages = inner.messages.get(thread_key).cloned().unwrap_or_default();
                    if review.message_fingerprint != message_fingerprint(&messages) {
                        continue;
                    }
                    let changed = snapshot.classification != review.classification
                        || snapshot.classification_source
                            != crate::analysis::ClassificationSource::Manual
                        || snapshot.manual_review_required
                        || !snapshot.manual_override_applied
                        || snapshot.notes != review.notes;
                    if changed {
                        if let Some(thread) = inner.threads.get_mut(thread_key) {
                            apply_manual_review_override(thread, &messages, &review);
                        }
                        result.updated_threads += 1;
                        run_changed = true;
                    }
                }
            }

            let threads = inner
                .threads
                .values()
                .filter(|thread| thread.analysis_run_id == run.id)
                .cloned()
                .collect::<Vec<_>>();
            if let Some(stored_run) = inner.runs.get_mut(&run.id) {
                let metrics = calculate_metrics(
                    &threads,
                    stored_run.metrics.ai_input_tokens,
                    stored_run.metrics.ai_output_tokens,
                );
                if run_changed || stored_run.metrics != metrics {
                    stored_run.metrics = metrics;
                    result.updated_runs += 1;
                }
            }
        }

        inner.manual_review_inheritance_v2_applied = true;
        Ok(result)
    }

    async fn upsert_mailbox_metadata(
        &self,
        owner_email: &str,
        metadata: &MailboxMetadata,
    ) -> anyhow::Result<()> {
        self.inner
            .write()
            .await
            .mailbox_metadata
            .insert(owner_email.trim().to_lowercase(), metadata.clone());
        Ok(())
    }

    async fn get_mailbox_metadata(
        &self,
        owner_email: &str,
    ) -> anyhow::Result<Option<MailboxMetadata>> {
        Ok(self
            .inner
            .read()
            .await
            .mailbox_metadata
            .get(&owner_email.trim().to_lowercase())
            .cloned())
    }

    async fn list_filter_presets(&self, owner_email: &str) -> anyhow::Result<Vec<FilterPreset>> {
        let owner = owner_email.trim().to_lowercase();
        let mut presets: Vec<FilterPreset> = self
            .inner
            .read()
            .await
            .filter_presets
            .values()
            .filter(|preset| preset.owner_email.trim().to_lowercase() == owner)
            .cloned()
            .collect();
        presets.sort_by_key(|preset| preset.created_at);
        Ok(presets)
    }

    async fn upsert_filter_preset(&self, preset: &FilterPreset) -> anyhow::Result<()> {
        self.inner
            .write()
            .await
            .filter_presets
            .insert(preset.id.clone(), preset.clone());
        Ok(())
    }

    async fn delete_filter_preset(&self, owner_email: &str, preset_id: &str) -> anyhow::Result<()> {
        let owner = owner_email.trim().to_lowercase();
        let mut inner = self.inner.write().await;
        if inner
            .filter_presets
            .get(preset_id)
            .is_some_and(|preset| preset.owner_email.trim().to_lowercase() == owner)
        {
            inner.filter_presets.remove(preset_id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;
    use crate::analysis::{
        AnalysisConfig, AnalysisMetrics, AnalysisStatus, Classification, ClassificationSource,
    };
    use crate::scheduler::model::ScheduleRunStatus;

    fn session(id: &str, email: &str, refresh: Option<&str>, age_minutes: i64) -> UserSession {
        let at = Utc::now() - Duration::minutes(age_minutes);
        UserSession {
            id: id.to_string(),
            workos_user_id: Some(format!("workos-{id}")),
            workos_session_id: None,
            google_account_email: email.to_string(),
            gmail_account_email: Some(email.to_string()),
            access_token_encrypted: "access".to_string(),
            refresh_token_encrypted: refresh.map(ToOwned::to_owned),
            gmail_access_token_encrypted: Some("access".to_string()),
            gmail_refresh_token_encrypted: refresh.map(ToOwned::to_owned),
            expires_at: None,
            revoked_at: None,
            created_at: at,
            updated_at: at,
        }
    }

    #[tokio::test]
    async fn migrates_legacy_gmail_credentials_without_keeping_them_in_sessions() {
        let storage = MemoryStorage::default();
        storage
            .upsert_user_session(&session("old", "a@x.cl", Some("r1"), 120))
            .await
            .unwrap();
        storage
            .upsert_user_session(&session("newest-no-refresh", "a@x.cl", None, 1))
            .await
            .unwrap();
        storage
            .upsert_user_session(&session("newer", "a@x.cl", Some("r2"), 30))
            .await
            .unwrap();
        storage
            .upsert_user_session(&session("other-user", "b@x.cl", Some("r3"), 0))
            .await
            .unwrap();

        let connection = storage
            .get_gmail_connection("a@x.cl")
            .await
            .unwrap()
            .expect("connection expected");
        assert_eq!(connection.refresh_token_encrypted.as_deref(), Some("r2"));
        for id in ["old", "newest-no-refresh", "newer"] {
            let session = storage.get_user_session(id).await.unwrap().unwrap();
            assert!(session.gmail_account_email.is_none());
            assert!(session.gmail_access_token_encrypted.is_none());
            assert!(session.gmail_refresh_token_encrypted.is_none());
        }
    }

    #[tokio::test]
    async fn migrates_a_legacy_connection_even_without_a_refresh_token() {
        let storage = MemoryStorage::default();
        storage
            .upsert_user_session(&session("s1", "a@x.cl", None, 5))
            .await
            .unwrap();

        let connection = storage
            .get_gmail_connection("a@x.cl")
            .await
            .unwrap()
            .expect("connection expected");
        assert!(connection.refresh_token_encrypted.is_none());
    }

    #[tokio::test]
    async fn disconnect_gmail_clears_every_session_for_the_owner() {
        let storage = MemoryStorage::default();
        storage
            .upsert_user_session(&session("first", "owner@example.com", Some("r1"), 10))
            .await
            .unwrap();
        storage
            .upsert_user_session(&session("second", "owner@example.com", Some("r2"), 5))
            .await
            .unwrap();
        storage
            .upsert_user_session(&session("other", "other@example.com", Some("r3"), 0))
            .await
            .unwrap();
        storage
            .upsert_gmail_connection(&GmailConnection {
                owner_email: "owner@example.com".to_string(),
                gmail_account_email: "owner@example.com".to_string(),
                access_token_encrypted: "access".to_string(),
                refresh_token_encrypted: Some("r2".to_string()),
                connected_at: Utc::now(),
                updated_at: Utc::now(),
                revoked_at: None,
            })
            .await
            .unwrap();

        storage
            .disconnect_gmail("OWNER@example.com", Utc::now())
            .await
            .unwrap();

        for id in ["first", "second"] {
            let session = storage.get_user_session(id).await.unwrap().unwrap();
            assert!(session.gmail_account_email.is_none());
            assert!(session.gmail_access_token_encrypted.is_none());
            assert!(session.gmail_refresh_token_encrypted.is_none());
            assert!(session.access_token_encrypted.is_empty());
            assert!(session.refresh_token_encrypted.is_none());
        }
        let connection = storage
            .get_gmail_connection("owner@example.com")
            .await
            .unwrap()
            .expect("revoked connection retained for state");
        assert!(connection.revoked_at.is_some());
        assert!(connection.access_token_encrypted.is_empty());
        assert!(connection.refresh_token_encrypted.is_none());
        assert!(
            storage
                .get_user_session("other")
                .await
                .unwrap()
                .unwrap()
                .gmail_account_email
                .is_some()
        );
    }

    #[tokio::test]
    async fn refresh_does_not_restore_a_gmail_connection_after_disconnect() {
        let storage = MemoryStorage::default();
        let previous = GmailConnection {
            owner_email: "owner@example.com".to_string(),
            gmail_account_email: "owner@example.com".to_string(),
            access_token_encrypted: "old-access".to_string(),
            refresh_token_encrypted: Some("old-refresh".to_string()),
            connected_at: Utc::now(),
            updated_at: Utc::now(),
            revoked_at: None,
        };
        storage.upsert_gmail_connection(&previous).await.unwrap();
        let mut refreshed = previous.clone();
        refreshed.access_token_encrypted = "new-access".to_string();
        refreshed.updated_at = Utc::now();

        storage
            .disconnect_gmail("owner@example.com", Utc::now())
            .await
            .unwrap();
        assert_eq!(
            storage
                .refresh_gmail_connection(&previous, &refreshed)
                .await
                .unwrap(),
            GmailConnectionRefresh::ConnectionChanged
        );
        let stored = storage
            .get_gmail_connection("owner@example.com")
            .await
            .unwrap()
            .expect("revoked connection retained");
        assert!(stored.revoked_at.is_some());
        assert!(stored.access_token_encrypted.is_empty());
        assert!(stored.refresh_token_encrypted.is_none());
    }

    #[tokio::test]
    async fn revoke_user_sessions_only_revokes_the_owner_sessions() {
        let storage = MemoryStorage::default();
        storage
            .upsert_user_session(&session("first", "owner@example.com", Some("r1"), 10))
            .await
            .unwrap();
        storage
            .upsert_user_session(&session("second", "owner@example.com", Some("r2"), 5))
            .await
            .unwrap();
        storage
            .upsert_user_session(&session("other", "other@example.com", Some("r3"), 0))
            .await
            .unwrap();

        storage
            .revoke_user_sessions("OWNER@example.com", Utc::now())
            .await
            .unwrap();

        for id in ["first", "second"] {
            assert!(
                storage
                    .get_user_session(id)
                    .await
                    .unwrap()
                    .unwrap()
                    .revoked_at
                    .is_some()
            );
        }
        assert!(
            storage
                .get_gmail_connection("owner@example.com")
                .await
                .unwrap()
                .expect("scheduler credential remains available")
                .refresh_token_encrypted
                .is_some()
        );
        assert!(
            storage
                .get_user_session("other")
                .await
                .unwrap()
                .unwrap()
                .revoked_at
                .is_none()
        );
    }

    #[tokio::test]
    async fn rebind_account_preserves_org_and_revokes_previous_sessions() {
        let storage = MemoryStorage::default();
        let now = Utc::now();
        let previous = Account {
            workos_user_id: "workos-staging".to_string(),
            email: "owner@example.com".to_string(),
            name: Some("Previous name".to_string()),
            org_id: "org-preserved".to_string(),
            created_at: now,
            updated_at: now,
        };
        let account = Account {
            workos_user_id: "workos-production".to_string(),
            name: Some("Current name".to_string()),
            updated_at: now,
            ..previous.clone()
        };
        storage.upsert_account(&previous).await.unwrap();
        storage
            .upsert_user_session(&session("owner", "owner@example.com", Some("r1"), 0))
            .await
            .unwrap();
        storage
            .upsert_user_session(&session("other", "other@example.com", Some("r2"), 0))
            .await
            .unwrap();

        storage
            .rebind_account_workos_user_id(&previous, &account, now)
            .await
            .unwrap();

        assert!(
            storage
                .get_account_by_workos_user_id("workos-staging")
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            storage
                .get_account_by_workos_user_id("workos-production")
                .await
                .unwrap()
                .unwrap()
                .org_id,
            "org-preserved"
        );
        assert!(
            storage
                .get_user_session("owner")
                .await
                .unwrap()
                .unwrap()
                .revoked_at
                .is_some()
        );
        assert!(
            storage
                .get_user_session("other")
                .await
                .unwrap()
                .unwrap()
                .revoked_at
                .is_none()
        );
    }

    #[tokio::test]
    async fn manual_review_metrics_migration_reconciles_once_and_preserves_tokens() {
        let storage = MemoryStorage::default();
        let now = Utc::now();
        let metrics = AnalysisMetrics {
            ai_input_tokens: 77,
            ai_output_tokens: 33,
            ..Default::default()
        };
        storage
            .create_analysis_run(&AnalysisRun {
                id: "legacy-run".to_string(),
                user_email: "owner@example.com".to_string(),
                org_id: None,
                mailbox_id: None,
                trigger_type: None,
                policy_version_id: None,
                policy_hash: None,
                policy_snapshot: None,
                gmail_scope_snapshot: vec![],
                retention_expires_at: None,
                data_minimization_mode: None,
                config: AnalysisConfig {
                    date_from: "2026-06-01".to_string(),
                    date_to: "2026-06-02".to_string(),
                    time_from: "00:00".to_string(),
                    time_to: "23:59".to_string(),
                    timezone: "America/Santiago".to_string(),
                    internal_domains: vec!["example.com".to_string()],
                    ignored_senders: vec![],
                    ignored_domains: vec![],
                    ignored_keywords: vec![],
                    include_labels: vec![],
                    exclude_labels: vec![],
                },
                status: AnalysisStatus::Completed,
                progress_message: "done".to_string(),
                processed_threads: 2,
                total_candidate_threads: 2,
                metrics,
                created_at: now,
                completed_at: Some(now),
                error_message: None,
            })
            .await
            .unwrap();

        let legacy_thread =
            |id: &str, classification: Classification, is_valid_client_request: bool| EmailThread {
                id: id.to_string(),
                analysis_run_id: "legacy-run".to_string(),
                gmail_thread_id: format!("gmail-{id}"),
                subject: "Legacy".to_string(),
                normalized_subject: "legacy".to_string(),
                classification,
                classification_source: ClassificationSource::Manual,
                classification_confidence: 1.0,
                is_valid_client_request,
                is_answered: false,
                first_message_at: Some(now),
                first_client_message_id: None,
                first_internal_reply_message_id: None,
                last_internal_message_id: None,
                first_client_message_at: Some(now),
                first_internal_reply_at: None,
                last_internal_message_at: None,
                response_time_minutes: None,
                resolution_time_minutes: None,
                manual_review_required: false,
                manual_override_applied: true,
                reasons: vec![],
                notes: Some("nota histórica".to_string()),
                created_at: now,
                updated_at: now,
            };
        storage
            .upsert_thread(
                &legacy_thread("ambiguous-valid", Classification::Ambiguous, true),
                &[],
            )
            .await
            .unwrap();
        storage
            .upsert_thread(
                &legacy_thread("misc-valid", Classification::Misc, true),
                &[],
            )
            .await
            .unwrap();

        let first = storage.reconcile_manual_review_metrics_v1().await.unwrap();
        assert!(!first.already_applied);
        assert_eq!(first.scanned_runs, 1);
        assert_eq!(first.updated_runs, 1);
        assert_eq!(first.updated_threads, 2);

        let threads = storage.list_threads("legacy-run").await.unwrap();
        let promoted = threads
            .iter()
            .find(|thread| thread.id == "ambiguous-valid")
            .unwrap();
        assert_eq!(promoted.classification, Classification::ValidClientRequest);
        assert!(promoted.is_valid_client_request);
        assert!(!promoted.manual_review_required);
        let demoted = threads
            .iter()
            .find(|thread| thread.id == "misc-valid")
            .unwrap();
        assert_eq!(demoted.classification, Classification::Misc);
        assert!(!demoted.is_valid_client_request);

        let run = storage
            .get_analysis_run("legacy-run")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(run.metrics.total_threads, 2);
        assert_eq!(run.metrics.valid_requests, 1);
        assert_eq!(run.metrics.ambiguous, 0);
        assert_eq!(run.metrics.ignored, 1);
        assert_eq!(run.metrics.ai_input_tokens, 77);
        assert_eq!(run.metrics.ai_output_tokens, 33);

        let second = storage.reconcile_manual_review_metrics_v1().await.unwrap();
        assert!(second.already_applied);
        assert_eq!(second.scanned_runs, 0);
        assert_eq!(second.updated_runs, 0);
        assert_eq!(second.updated_threads, 0);
    }

    #[tokio::test]
    async fn manual_review_inheritance_only_applies_to_unchanged_latest_threads() {
        let storage = MemoryStorage::default();
        let now = Utc::now();
        let make_run = |id: &str, created_at| AnalysisRun {
            id: id.to_string(),
            user_email: "owner@example.com".to_string(),
            org_id: None,
            mailbox_id: None,
            trigger_type: None,
            policy_version_id: None,
            policy_hash: None,
            policy_snapshot: None,
            gmail_scope_snapshot: vec![],
            retention_expires_at: None,
            data_minimization_mode: None,
            config: AnalysisConfig {
                date_from: "2026-06-01".to_string(),
                date_to: "2026-06-02".to_string(),
                time_from: "00:00".to_string(),
                time_to: "23:59".to_string(),
                timezone: "America/Santiago".to_string(),
                internal_domains: vec!["example.com".to_string()],
                ignored_senders: vec![],
                ignored_domains: vec![],
                ignored_keywords: vec![],
                include_labels: vec![],
                exclude_labels: vec![],
            },
            status: AnalysisStatus::Completed,
            progress_message: "done".to_string(),
            processed_threads: 2,
            total_candidate_threads: 2,
            metrics: AnalysisMetrics {
                ai_input_tokens: 21,
                ai_output_tokens: 8,
                ..Default::default()
            },
            created_at,
            completed_at: Some(created_at),
            error_message: None,
        };
        storage
            .create_analysis_run(&make_run("old-run", now - Duration::hours(1)))
            .await
            .unwrap();
        storage
            .create_analysis_run(&make_run("new-run", now))
            .await
            .unwrap();

        let make_thread = |run_id: &str, id: &str| EmailThread {
            id: id.to_string(),
            analysis_run_id: run_id.to_string(),
            gmail_thread_id: id.to_string(),
            subject: id.to_string(),
            normalized_subject: id.to_string(),
            classification: Classification::Ambiguous,
            classification_source: ClassificationSource::Ai,
            classification_confidence: 0.8,
            is_valid_client_request: false,
            is_answered: false,
            first_message_at: Some(now),
            first_client_message_id: Some(format!("{id}-client")),
            first_internal_reply_message_id: None,
            last_internal_message_id: None,
            first_client_message_at: Some(now),
            first_internal_reply_at: None,
            last_internal_message_at: None,
            response_time_minutes: None,
            resolution_time_minutes: None,
            manual_review_required: true,
            manual_override_applied: false,
            reasons: vec![],
            notes: None,
            created_at: now,
            updated_at: now,
        };
        let make_message = |id: &str| EmailMessage {
            id: id.to_string(),
            gmail_message_id: id.to_string(),
            from_email: "client@example.net".to_string(),
            from_name: None,
            to_emails: vec!["support@example.com".to_string()],
            cc_emails: vec![],
            date: now,
            subject: "Ayuda".to_string(),
            snippet: "Ayuda".to_string(),
            headers: serde_json::json!({}),
            is_internal: false,
            is_external: true,
            is_automated: false,
            body_text: None,
        };

        for run_id in ["old-run", "new-run"] {
            storage
                .upsert_thread(
                    &make_thread(run_id, "unchanged"),
                    &[make_message("unchanged-client")],
                )
                .await
                .unwrap();
        }
        storage
            .upsert_thread(
                &make_thread("old-run", "changed"),
                &[make_message("changed-client")],
            )
            .await
            .unwrap();
        storage
            .upsert_thread(
                &make_thread("new-run", "changed"),
                &[
                    make_message("changed-client"),
                    make_message("changed-new-message"),
                ],
            )
            .await
            .unwrap();

        for thread_id in ["unchanged", "changed"] {
            storage
                .add_manual_review(
                    "old-run",
                    &ManualReview {
                        id: format!("review-{thread_id}"),
                        email_thread_id: thread_id.to_string(),
                        reviewer_label: "owner@example.com".to_string(),
                        new_classification: Classification::ValidClientRequest,
                        is_valid_client_request: true,
                        is_answered: false,
                        first_client_message_id: Some(format!("{thread_id}-client")),
                        first_internal_reply_message_id: None,
                        last_internal_message_id: None,
                        first_client_message_at: Some(now),
                        first_internal_reply_at: None,
                        last_internal_message_at: None,
                        response_time_minutes: None,
                        resolution_time_minutes: None,
                        notes: Some("confirmado".to_string()),
                        created_at: now,
                    },
                )
                .await
                .unwrap();
        }

        let migration = storage
            .reconcile_manual_review_inheritance_v2()
            .await
            .unwrap();
        assert!(!migration.already_applied);
        assert_eq!(migration.created_overrides, 2);
        assert_eq!(migration.updated_threads, 1);

        let unchanged = storage
            .get_thread("new-run", "unchanged")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.classification, Classification::ValidClientRequest);
        assert_eq!(
            unchanged.classification_source,
            ClassificationSource::Manual
        );
        assert!(!unchanged.manual_review_required);
        assert!(unchanged.manual_override_applied);

        let changed = storage
            .get_thread("new-run", "changed")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(changed.classification, Classification::Ambiguous);
        assert!(changed.manual_review_required);
        assert!(!changed.manual_override_applied);

        let run = storage.get_analysis_run("new-run").await.unwrap().unwrap();
        assert_eq!(run.metrics.valid_requests, 1);
        assert_eq!(run.metrics.ambiguous, 1);
        assert_eq!(run.metrics.pending_review, 1);
        assert_eq!(run.metrics.ai_input_tokens, 21);
        assert_eq!(run.metrics.ai_output_tokens, 8);

        let second = storage
            .reconcile_manual_review_inheritance_v2()
            .await
            .unwrap();
        assert!(second.already_applied);
    }

    #[tokio::test]
    async fn schedule_config_and_state_round_trip() {
        let storage = MemoryStorage::default();
        let config = ScheduleConfig {
            user_email: "a@x.cl".to_string(),
            enabled: true,
            recipients: vec!["jefa@x.cl".to_string()],
            internal_domains: vec!["x.cl".to_string()],
            ignored_senders: vec![],
            ignored_domains: vec![],
            ignored_keywords: vec![],
            timezone: "America/Santiago".to_string(),
            gmail_max_threads: Some(120),
            updated_at: Utc::now(),
        };
        storage.upsert_schedule_config(&config).await.unwrap();
        let configs = storage.list_schedule_configs().await.unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].user_email, "a@x.cl");
        assert_eq!(configs[0].gmail_max_threads, Some(120));

        let state = ScheduleState {
            user_email: "a@x.cl".to_string(),
            window_date_from: "2026-06-11".to_string(),
            window_date_to: "2026-06-11".to_string(),
            status: ScheduleRunStatus::Completed,
            run_id: Some("run-1".to_string()),
            email_sent: true,
            error_message: None,
            started_at: Utc::now(),
            updated_at: Utc::now(),
        };
        storage.upsert_schedule_state(&state).await.unwrap();
        let loaded = storage
            .get_schedule_state("a@x.cl")
            .await
            .unwrap()
            .expect("state expected");
        assert_eq!(loaded.status, ScheduleRunStatus::Completed);
        assert_eq!(loaded.run_id.as_deref(), Some("run-1"));
        assert!(
            storage
                .get_schedule_state("b@x.cl")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn usage_additions_do_not_lose_concurrent_updates() {
        let storage = MemoryStorage::default();
        let first = storage.clone();
        let second = storage.clone();
        let (first_result, second_result) = tokio::join!(
            first.add_usage("org-1", "2026-07", 1, 7, 3),
            second.add_usage("org-1", "2026-07", 1, 11, 5),
        );
        first_result.unwrap();
        second_result.unwrap();

        let usage = storage
            .get_usage_ledger("org-1", "2026-07")
            .await
            .unwrap()
            .expect("usage ledger expected");
        assert_eq!(usage.runs_created, 2);
        assert_eq!(usage.analyzed_threads, 18);
        assert_eq!(usage.ai_audited_threads, 8);
    }

    #[tokio::test]
    async fn concurrent_start_claims_only_allow_one_analysis() {
        let storage = MemoryStorage::default();
        let now = Utc::now();
        storage
            .create_analysis_run(&AnalysisRun {
                id: "run-1".to_string(),
                user_email: "owner@example.com".to_string(),
                org_id: Some("org-1".to_string()),
                mailbox_id: None,
                trigger_type: None,
                policy_version_id: None,
                policy_hash: None,
                policy_snapshot: None,
                gmail_scope_snapshot: vec![],
                retention_expires_at: None,
                data_minimization_mode: None,
                config: AnalysisConfig {
                    date_from: "2026-07-01".to_string(),
                    date_to: "2026-07-01".to_string(),
                    time_from: "00:00".to_string(),
                    time_to: "23:59".to_string(),
                    timezone: "America/Santiago".to_string(),
                    internal_domains: vec![],
                    ignored_senders: vec![],
                    ignored_domains: vec![],
                    ignored_keywords: vec![],
                    include_labels: vec![],
                    exclude_labels: vec![],
                },
                status: AnalysisStatus::Pending,
                progress_message: "Listo para analizar".to_string(),
                processed_threads: 0,
                total_candidate_threads: 0,
                metrics: AnalysisMetrics::default(),
                created_at: now,
                completed_at: None,
                error_message: None,
            })
            .await
            .unwrap();

        let first = storage.clone();
        let second = storage.clone();
        let (first_result, second_result) = tokio::join!(
            first.claim_pending_analysis_run("run-1"),
            second.claim_pending_analysis_run("run-1"),
        );
        let claims = [first_result.unwrap(), second_result.unwrap()];
        assert_eq!(claims.iter().filter(|claim| claim.is_some()).count(), 1);

        let run = storage
            .get_analysis_run("run-1")
            .await
            .unwrap()
            .expect("analysis run expected");
        assert_eq!(run.status, AnalysisStatus::Running);
    }

    #[tokio::test]
    async fn filter_presets_round_trip_and_scope_by_owner() {
        let storage = MemoryStorage::default();
        let now = Utc::now();
        let preset = FilterPreset {
            id: "p1".to_string(),
            owner_email: "a@x.cl".to_string(),
            name: "Soporte".to_string(),
            include_labels: vec!["CATEGORY_PERSONAL".to_string()],
            exclude_labels: vec![],
            ignored_senders: vec![],
            ignored_domains: vec![],
            ignored_keywords: vec![],
            is_default: true,
            created_at: now,
            updated_at: now,
        };
        storage.upsert_filter_preset(&preset).await.unwrap();

        let mine = storage.list_filter_presets("a@x.cl").await.unwrap();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].name, "Soporte");
        assert!(
            storage
                .list_filter_presets("b@x.cl")
                .await
                .unwrap()
                .is_empty()
        );

        storage.delete_filter_preset("a@x.cl", "p1").await.unwrap();
        assert!(
            storage
                .list_filter_presets("a@x.cl")
                .await
                .unwrap()
                .is_empty()
        );
    }
}
