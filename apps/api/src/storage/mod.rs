use std::{collections::HashMap, sync::Arc};

use anyhow::anyhow;
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;

use crate::{
    analysis::{AiAuditResult, AnalysisRun, EmailMessage, EmailThread, ManualReview},
    auth::UserSession,
    billing::{Account, CheckoutSession, Subscription, UsageLedger},
    mailbox::{FilterPreset, MailboxMetadata},
    policies::{OrgConfigBundle, PolicyVersion},
    scheduler::model::{ScheduleConfig, ScheduleState},
};

#[async_trait]
pub trait StorageRepository: Send + Sync {
    async fn upsert_account(&self, account: &Account) -> anyhow::Result<()>;
    async fn get_account_by_workos_user_id(
        &self,
        workos_user_id: &str,
    ) -> anyhow::Result<Option<Account>>;
    async fn get_account_by_email(&self, email: &str) -> anyhow::Result<Option<Account>>;
    async fn upsert_user_session(&self, session: &UserSession) -> anyhow::Result<()>;
    async fn get_user_session(&self, id: &str) -> anyhow::Result<Option<UserSession>>;
    /// Sesión más reciente del usuario que tenga refresh token; la usa el
    /// análisis programado para operar sin cookie de sesión.
    async fn find_latest_session_with_refresh_token(
        &self,
        email: &str,
    ) -> anyhow::Result<Option<UserSession>>;
    async fn list_schedule_configs(&self) -> anyhow::Result<Vec<ScheduleConfig>>;
    async fn upsert_schedule_config(&self, config: &ScheduleConfig) -> anyhow::Result<()>;
    async fn get_schedule_state(&self, user_email: &str) -> anyhow::Result<Option<ScheduleState>>;
    async fn upsert_schedule_state(&self, state: &ScheduleState) -> anyhow::Result<()>;
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
    async fn upsert_usage_ledger(&self, usage: &UsageLedger) -> anyhow::Result<()>;
    async fn get_usage_ledger(
        &self,
        org_id: &str,
        period_key: &str,
    ) -> anyhow::Result<Option<UsageLedger>>;
    async fn create_analysis_run(&self, run: &AnalysisRun) -> anyhow::Result<()>;
    async fn update_analysis_run(&self, run: &AnalysisRun) -> anyhow::Result<()>;
    async fn get_analysis_run(&self, id: &str) -> anyhow::Result<Option<AnalysisRun>>;
    async fn list_analysis_runs(&self, user_email: &str) -> anyhow::Result<Vec<AnalysisRun>>;
    async fn upsert_thread(
        &self,
        thread: &EmailThread,
        messages: &[EmailMessage],
    ) -> anyhow::Result<()>;
    async fn list_threads(&self, run_id: &str) -> anyhow::Result<Vec<EmailThread>>;
    async fn get_thread(&self, thread_id: &str) -> anyhow::Result<Option<EmailThread>>;
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
    accounts: HashMap<String, Account>,
    runs: HashMap<String, AnalysisRun>,
    threads: HashMap<String, EmailThread>,
    messages: HashMap<String, Vec<EmailMessage>>,
    audits: HashMap<String, Vec<AiAuditResult>>,
    reviews: Vec<ManualReview>,
    schedule_configs: HashMap<String, ScheduleConfig>,
    schedule_states: HashMap<String, ScheduleState>,
    org_configs: HashMap<String, OrgConfigBundle>,
    policy_versions: HashMap<String, PolicyVersion>,
    subscriptions: HashMap<String, Subscription>,
    checkout_sessions: HashMap<String, CheckoutSession>,
    usage_ledgers: HashMap<String, UsageLedger>,
    mailbox_metadata: HashMap<String, MailboxMetadata>,
    filter_presets: HashMap<String, FilterPreset>,
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

    async fn find_latest_session_with_refresh_token(
        &self,
        email: &str,
    ) -> anyhow::Result<Option<UserSession>> {
        Ok(self
            .inner
            .read()
            .await
            .sessions
            .values()
            .filter(|session| {
                session.google_account_email == email
                    && (session.gmail_refresh_token_encrypted.is_some()
                        || session.refresh_token_encrypted.is_some())
            })
            .max_by_key(|session| session.updated_at)
            .cloned())
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

    async fn upsert_usage_ledger(&self, usage: &UsageLedger) -> anyhow::Result<()> {
        self.inner.write().await.usage_ledgers.insert(
            format!("{}:{}", usage.org_id, usage.period_key),
            usage.clone(),
        );
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

    async fn upsert_thread(
        &self,
        thread: &EmailThread,
        messages: &[EmailMessage],
    ) -> anyhow::Result<()> {
        let mut inner = self.inner.write().await;
        inner.threads.insert(thread.id.clone(), thread.clone());
        inner.messages.insert(
            format!("{}:{}", thread.analysis_run_id, thread.id),
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
                    .get(&format!("{run_id}:{}", thread.id))
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

    async fn get_thread(&self, thread_id: &str) -> anyhow::Result<Option<EmailThread>> {
        Ok(self.inner.read().await.threads.get(thread_id).cloned())
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
            .get(&format!("{run_id}:{thread_id}"))
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
            .entry(thread_id.to_string())
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
            .get_mut(&review.email_thread_id)
            .ok_or_else(|| anyhow!("thread not found"))?;
        if thread.analysis_run_id != run_id {
            return Err(anyhow!("thread does not belong to run"));
        }
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
        inner.reviews.push(ManualReview {
            id: review.id.clone(),
            email_thread_id: review.email_thread_id.clone(),
            reviewer_label: review.reviewer_label.clone(),
            new_classification: review.new_classification.clone(),
            is_valid_client_request: review.is_valid_client_request,
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
        });
        Ok(())
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
    use crate::scheduler::model::ScheduleRunStatus;

    fn session(id: &str, email: &str, refresh: Option<&str>, age_minutes: i64) -> UserSession {
        let at = Utc::now() - Duration::minutes(age_minutes);
        UserSession {
            id: id.to_string(),
            workos_user_id: Some(format!("workos-{id}")),
            google_account_email: email.to_string(),
            gmail_account_email: Some(email.to_string()),
            access_token_encrypted: "access".to_string(),
            refresh_token_encrypted: refresh.map(ToOwned::to_owned),
            gmail_access_token_encrypted: Some("access".to_string()),
            gmail_refresh_token_encrypted: refresh.map(ToOwned::to_owned),
            created_at: at,
            updated_at: at,
        }
    }

    #[tokio::test]
    async fn finds_latest_session_with_refresh_token_for_email() {
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

        let found = storage
            .find_latest_session_with_refresh_token("a@x.cl")
            .await
            .unwrap()
            .expect("session expected");
        assert_eq!(found.id, "newer");
    }

    #[tokio::test]
    async fn returns_none_when_no_session_has_refresh_token() {
        let storage = MemoryStorage::default();
        storage
            .upsert_user_session(&session("s1", "a@x.cl", None, 5))
            .await
            .unwrap();

        let found = storage
            .find_latest_session_with_refresh_token("a@x.cl")
            .await
            .unwrap();
        assert!(found.is_none());
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
