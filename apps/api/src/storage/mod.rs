use std::{collections::HashMap, sync::Arc};

use anyhow::anyhow;
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;

use crate::{
    analysis::{AiAuditResult, AnalysisRun, EmailMessage, EmailThread, ManualReview},
    auth::UserSession,
};

#[async_trait]
pub trait StorageRepository: Send + Sync {
    async fn upsert_user_session(&self, session: &UserSession) -> anyhow::Result<()>;
    async fn get_user_session(&self, id: &str) -> anyhow::Result<Option<UserSession>>;
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
}

#[derive(Default, Clone)]
pub struct MemoryStorage {
    inner: Arc<RwLock<MemoryInner>>,
}

#[derive(Default)]
struct MemoryInner {
    sessions: HashMap<String, UserSession>,
    runs: HashMap<String, AnalysisRun>,
    threads: HashMap<String, EmailThread>,
    messages: HashMap<String, Vec<EmailMessage>>,
    audits: HashMap<String, Vec<AiAuditResult>>,
    reviews: Vec<ManualReview>,
}

#[async_trait]
impl StorageRepository for MemoryStorage {
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
        let thread = inner
            .threads
            .get_mut(&review.email_thread_id)
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
        if !inner.runs.contains_key(run_id) {
            return Err(anyhow!("run not found"));
        }
        Ok(())
    }
}
