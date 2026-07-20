use anyhow::{Context, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::{
    analysis::{
        AiAuditResult, AnalysisRun, EmailMessage, EmailThread, ManualReview, ManualReviewOverride,
    },
    auth::UserSession,
    billing::{Account, CheckoutSession, Subscription, UsageLedger},
    mailbox::{FilterPreset, MailboxConnection, MailboxMetadata},
    policies::{OrgConfigBundle, PolicyVersion},
    scheduler::model::{ScheduleConfig, ScheduleState},
    storage::{
        AnalysisDataDeletionAudit, MailboxConnectionRefresh,
        ManualReviewInheritanceMigrationResult, ManualReviewMetricsMigrationResult,
        ScheduleWindowClaim, StorageRepository, clear_gmail_connection,
        existing_schedule_window_claim, gmail_connection_from_legacy,
    },
};

#[derive(Clone)]
pub struct PostgresStorage {
    pool: PgPool,
}

#[derive(Default)]
struct RecordFields<'a> {
    owner_email: Option<&'a str>,
    workos_user_id: Option<&'a str>,
    workos_session_id: Option<&'a str>,
    org_id: Option<&'a str>,
    run_id: Option<&'a str>,
    thread_id: Option<&'a str>,
    provider: Option<&'a str>,
    provider_id: Option<&'a str>,
    state: Option<&'a str>,
    sort_at: Option<DateTime<Utc>>,
}

impl PostgresStorage {
    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(database_url)
            .await
            .context("failed to connect to PostgreSQL")?;
        sqlx::query("SELECT 1")
            .execute(&pool)
            .await
            .context("PostgreSQL health query failed")?;
        Ok(Self { pool })
    }

    async fn put<T: Serialize>(
        &self,
        kind: &str,
        id: &str,
        fields: RecordFields<'_>,
        value: &T,
    ) -> anyhow::Result<()> {
        self.put_into(&self.pool, kind, id, fields, value).await
    }

    async fn put_tx<T: Serialize>(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        kind: &str,
        id: &str,
        fields: RecordFields<'_>,
        value: &T,
    ) -> anyhow::Result<()> {
        self.put_into(&mut **tx, kind, id, fields, value).await
    }

    async fn put_into<'e, E, T: Serialize>(
        &self,
        executor: E,
        kind: &str,
        id: &str,
        fields: RecordFields<'_>,
        value: &T,
    ) -> anyhow::Result<()>
    where
        E: sqlx::Executor<'e, Database = Postgres>,
    {
        let data = serde_json::to_value(value).context("failed to serialize PostgreSQL record")?;
        sqlx::query(
            "INSERT INTO mira.records (kind, id, owner_email, workos_user_id, workos_session_id, org_id, run_id, thread_id, provider, provider_id, state, sort_at, data) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) \
             ON CONFLICT (kind, id) DO UPDATE SET \
               owner_email=EXCLUDED.owner_email, workos_user_id=EXCLUDED.workos_user_id, \
               workos_session_id=EXCLUDED.workos_session_id, org_id=EXCLUDED.org_id, run_id=EXCLUDED.run_id, \
               thread_id=EXCLUDED.thread_id, provider=EXCLUDED.provider, provider_id=EXCLUDED.provider_id, \
               state=EXCLUDED.state, sort_at=EXCLUDED.sort_at, data=EXCLUDED.data, updated_at=now()",
        )
        .bind(kind)
        .bind(id)
        .bind(fields.owner_email.map(normalize_email))
        .bind(fields.workos_user_id)
        .bind(fields.workos_session_id)
        .bind(fields.org_id)
        .bind(fields.run_id)
        .bind(fields.thread_id)
        .bind(fields.provider)
        .bind(fields.provider_id)
        .bind(fields.state)
        .bind(fields.sort_at)
        .bind(data)
        .execute(executor)
        .await
        .context("failed to upsert PostgreSQL record")?;
        Ok(())
    }

    async fn get<T: DeserializeOwned>(&self, kind: &str, id: &str) -> anyhow::Result<Option<T>> {
        let data =
            sqlx::query_scalar::<_, Value>("SELECT data FROM mira.records WHERE kind=$1 AND id=$2")
                .bind(kind)
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .context("failed to read PostgreSQL record")?;
        data.map(|data| serde_json::from_value(data).context("invalid PostgreSQL record"))
            .transpose()
    }

    async fn list_kind<T: DeserializeOwned>(&self, kind: &str) -> anyhow::Result<Vec<T>> {
        self.list_query("SELECT data FROM mira.records WHERE kind=$1", kind)
            .await
    }

    async fn list_query<T: DeserializeOwned>(
        &self,
        sql: &str,
        value: &str,
    ) -> anyhow::Result<Vec<T>> {
        let values = sqlx::query_scalar::<_, Value>(sql)
            .bind(value)
            .fetch_all(&self.pool)
            .await
            .context("failed to list PostgreSQL records")?;
        values
            .into_iter()
            .map(|data| serde_json::from_value(data).context("invalid PostgreSQL record"))
            .collect()
    }

    async fn list_owner<T: DeserializeOwned>(
        &self,
        kind: &str,
        owner_email: &str,
    ) -> anyhow::Result<Vec<T>> {
        let values = sqlx::query_scalar::<_, Value>(
            "SELECT data FROM mira.records WHERE kind=$1 AND owner_email=$2 ORDER BY sort_at",
        )
        .bind(kind)
        .bind(normalize_email(owner_email))
        .fetch_all(&self.pool)
        .await
        .context("failed to list owner records")?;
        values
            .into_iter()
            .map(|data| serde_json::from_value(data).context("invalid PostgreSQL record"))
            .collect()
    }

    async fn list_run<T: DeserializeOwned>(
        &self,
        kind: &str,
        run_id: &str,
    ) -> anyhow::Result<Vec<T>> {
        let values = sqlx::query_scalar::<_, Value>(
            "SELECT data FROM mira.records WHERE kind=$1 AND run_id=$2 ORDER BY sort_at",
        )
        .bind(kind)
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .context("failed to list run records")?;
        values
            .into_iter()
            .map(|data| serde_json::from_value(data).context("invalid PostgreSQL record"))
            .collect()
    }
}

#[async_trait]
impl StorageRepository for PostgresStorage {
    async fn ping(&self) -> anyhow::Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn upsert_account(&self, account: &Account) -> anyhow::Result<()> {
        self.put(
            "account",
            &account.workos_user_id,
            RecordFields {
                owner_email: Some(&account.email),
                workos_user_id: Some(&account.workos_user_id),
                org_id: Some(&account.org_id),
                sort_at: Some(account.updated_at),
                ..Default::default()
            },
            account,
        )
        .await
    }

    async fn get_account_by_workos_user_id(&self, id: &str) -> anyhow::Result<Option<Account>> {
        self.get("account", id).await
    }

    async fn get_account_by_email(&self, email: &str) -> anyhow::Result<Option<Account>> {
        let data = sqlx::query_scalar::<_, Value>(
            "SELECT data FROM mira.records WHERE kind='account' AND owner_email=$1",
        )
        .bind(normalize_email(email))
        .fetch_optional(&self.pool)
        .await?;
        data.map(|value| serde_json::from_value(value).context("invalid account record"))
            .transpose()
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
            anyhow::bail!("invalid WorkOS account rebind");
        }
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query(
            "UPDATE mira.records SET id=$1, owner_email=$2, workos_user_id=$1, org_id=$3, \
             sort_at=$4, data=$5, updated_at=now() \
             WHERE kind='account' AND id=$6 AND workos_user_id=$6 AND owner_email=$2",
        )
        .bind(&account.workos_user_id)
        .bind(normalize_email(&account.email))
        .bind(&account.org_id)
        .bind(account.updated_at)
        .bind(serde_json::to_value(account).context("failed to serialize rebound account")?)
        .bind(&previous.workos_user_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed != 1 {
            anyhow::bail!("previous WorkOS account changed before rebind");
        }

        let sessions = sqlx::query_scalar::<_, Value>(
            "SELECT data FROM mira.records WHERE kind='user_session' AND owner_email=$1 FOR UPDATE",
        )
        .bind(normalize_email(&account.email))
        .fetch_all(&mut *tx)
        .await?;
        for value in sessions {
            let mut session: UserSession = serde_json::from_value(value)
                .context("invalid PostgreSQL user session during account rebind")?;
            session.revoked_at = Some(now);
            session.updated_at = now;
            self.put_tx(
                &mut tx,
                "user_session",
                &session.id,
                RecordFields {
                    owner_email: Some(&session.google_account_email),
                    workos_user_id: session.workos_user_id.as_deref(),
                    workos_session_id: session.workos_session_id.as_deref(),
                    sort_at: Some(session.updated_at),
                    ..Default::default()
                },
                &session,
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn upsert_user_session(&self, session: &UserSession) -> anyhow::Result<()> {
        self.put(
            "user_session",
            &session.id,
            RecordFields {
                owner_email: Some(&session.google_account_email),
                workos_user_id: session.workos_user_id.as_deref(),
                workos_session_id: session.workos_session_id.as_deref(),
                sort_at: Some(session.updated_at),
                ..Default::default()
            },
            session,
        )
        .await
    }

    async fn get_user_session(&self, id: &str) -> anyhow::Result<Option<UserSession>> {
        self.get("user_session", id).await
    }

    async fn upsert_gmail_connection(&self, connection: &MailboxConnection) -> anyhow::Result<()> {
        self.put(
            "gmail_connection",
            &normalize_email(&connection.owner_email),
            RecordFields {
                owner_email: Some(&connection.owner_email),
                state: Some(if connection.revoked_at.is_some() {
                    "revoked"
                } else {
                    "active"
                }),
                sort_at: Some(connection.updated_at),
                ..Default::default()
            },
            connection,
        )
        .await
    }

    async fn get_gmail_connection(
        &self,
        owner_email: &str,
    ) -> anyhow::Result<Option<MailboxConnection>> {
        let id = normalize_email(owner_email);
        if let Some(connection) = self.get("gmail_connection", &id).await? {
            return Ok(Some(connection));
        }

        let legacy = self
            .list_owner::<UserSession>("user_session", owner_email)
            .await?
            .iter()
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
        self.upsert_gmail_connection(&connection).await?;
        for mut session in self
            .list_owner::<UserSession>("user_session", owner_email)
            .await?
        {
            clear_gmail_connection(&mut session, connection.updated_at);
            self.upsert_user_session(&session).await?;
        }
        Ok(Some(connection))
    }

    async fn refresh_gmail_connection(
        &self,
        previous: &MailboxConnection,
        updated: &MailboxConnection,
    ) -> anyhow::Result<MailboxConnectionRefresh> {
        let id = normalize_email(&previous.owner_email);
        let previous_data = serde_json::to_value(previous)?;
        let updated_data = serde_json::to_value(updated)?;
        let changed = sqlx::query(
            "UPDATE mira.records SET data=$1, state=$2, sort_at=$3, updated_at=now() \
             WHERE kind='gmail_connection' AND id=$4 AND data=$5",
        )
        .bind(updated_data)
        .bind(if updated.revoked_at.is_some() {
            "revoked"
        } else {
            "active"
        })
        .bind(updated.updated_at)
        .bind(id)
        .bind(previous_data)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(if changed == 1 {
            MailboxConnectionRefresh::Updated
        } else {
            MailboxConnectionRefresh::ConnectionChanged
        })
    }

    async fn revoke_user_sessions(
        &self,
        owner_email: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        for mut session in self
            .list_owner::<UserSession>("user_session", owner_email)
            .await?
        {
            session.revoked_at = Some(now);
            session.updated_at = now;
            self.upsert_user_session(&session).await?;
        }
        Ok(())
    }

    async fn revoke_user_session_by_workos_session_id(
        &self,
        workos_session_id: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let values = sqlx::query_scalar::<_, Value>(
            "SELECT data FROM mira.records WHERE kind='user_session' AND workos_session_id=$1",
        )
        .bind(workos_session_id)
        .fetch_all(&self.pool)
        .await?;
        for value in values {
            let mut session: UserSession = serde_json::from_value(value)?;
            session.revoked_at = Some(now);
            session.updated_at = now;
            self.upsert_user_session(&session).await?;
        }
        Ok(())
    }

    async fn disconnect_gmail(&self, owner_email: &str, now: DateTime<Utc>) -> anyhow::Result<()> {
        if let Some(mut connection) = self.get_gmail_connection(owner_email).await? {
            connection.access_token_encrypted.clear();
            connection.refresh_token_encrypted = None;
            connection.revoked_at = Some(now);
            connection.updated_at = now;
            self.upsert_gmail_connection(&connection).await?;
        }
        for mut session in self
            .list_owner::<UserSession>("user_session", owner_email)
            .await?
        {
            clear_gmail_connection(&mut session, now);
            self.upsert_user_session(&session).await?;
        }
        Ok(())
    }

    async fn list_schedule_configs(&self) -> anyhow::Result<Vec<ScheduleConfig>> {
        self.list_kind("schedule_config").await
    }

    async fn upsert_schedule_config(&self, config: &ScheduleConfig) -> anyhow::Result<()> {
        self.put(
            "schedule_config",
            &normalize_email(&config.user_email),
            RecordFields {
                owner_email: Some(&config.user_email),
                sort_at: Some(config.updated_at),
                ..Default::default()
            },
            config,
        )
        .await
    }

    async fn get_schedule_state(&self, user_email: &str) -> anyhow::Result<Option<ScheduleState>> {
        self.get("schedule_state", &normalize_email(user_email))
            .await
    }

    async fn upsert_schedule_state(&self, state: &ScheduleState) -> anyhow::Result<()> {
        self.put(
            "schedule_state",
            &normalize_email(&state.user_email),
            RecordFields {
                owner_email: Some(&state.user_email),
                state: Some(schedule_state_name(state)),
                sort_at: Some(state.updated_at),
                ..Default::default()
            },
            state,
        )
        .await
    }

    async fn claim_schedule_window(
        &self,
        state: &ScheduleState,
        stale_before: DateTime<Utc>,
    ) -> anyhow::Result<ScheduleWindowClaim> {
        let id = normalize_email(&state.user_email);
        let mut tx = self.pool.begin().await?;
        let current = sqlx::query_scalar::<_, Value>(
            "SELECT data FROM mira.records WHERE kind='schedule_state' AND id=$1 FOR UPDATE",
        )
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(value) = current {
            let existing: ScheduleState = serde_json::from_value(value)?;
            if let Some(result) = existing_schedule_window_claim(&existing, state, stale_before) {
                tx.commit().await?;
                return Ok(result);
            }
        }
        self.put_tx(
            &mut tx,
            "schedule_state",
            &id,
            RecordFields {
                owner_email: Some(&state.user_email),
                state: Some(schedule_state_name(state)),
                sort_at: Some(state.updated_at),
                ..Default::default()
            },
            state,
        )
        .await?;
        tx.commit().await?;
        Ok(ScheduleWindowClaim::Claimed)
    }

    async fn get_org_config_for_user(
        &self,
        user_email: &str,
    ) -> anyhow::Result<Option<OrgConfigBundle>> {
        self.get("org_config", &normalize_email(user_email)).await
    }

    async fn upsert_org_config(&self, bundle: &OrgConfigBundle) -> anyhow::Result<()> {
        let owner = normalize_email(&bundle.membership.user_email);
        self.put(
            "org_config",
            &owner,
            RecordFields {
                owner_email: Some(&bundle.membership.user_email),
                org_id: Some(&bundle.org.id),
                sort_at: Some(bundle.org.updated_at),
                ..Default::default()
            },
            bundle,
        )
        .await?;
        self.put(
            "organization",
            &bundle.org.id,
            RecordFields {
                org_id: Some(&bundle.org.id),
                sort_at: Some(bundle.org.updated_at),
                ..Default::default()
            },
            &bundle.org,
        )
        .await?;
        self.put(
            "membership",
            &format!("{}:{owner}", bundle.org.id),
            RecordFields {
                owner_email: Some(&bundle.membership.user_email),
                org_id: Some(&bundle.org.id),
                ..Default::default()
            },
            &bundle.membership,
        )
        .await?;
        self.put(
            "mailbox",
            &format!("{}:{}", bundle.org.id, bundle.mailbox.id),
            RecordFields {
                org_id: Some(&bundle.org.id),
                ..Default::default()
            },
            &bundle.mailbox,
        )
        .await?;
        self.put(
            "policy_draft",
            &bundle.org.id,
            RecordFields {
                org_id: Some(&bundle.org.id),
                ..Default::default()
            },
            &bundle.draft,
        )
        .await?;
        self.put(
            "policy_version",
            &format!("{}:{}", bundle.org.id, bundle.policy_version.id),
            RecordFields {
                org_id: Some(&bundle.org.id),
                sort_at: Some(bundle.policy_version.created_at),
                ..Default::default()
            },
            &bundle.policy_version,
        )
        .await
    }

    async fn get_policy_version(
        &self,
        org_id: &str,
        policy_version_id: &str,
    ) -> anyhow::Result<Option<PolicyVersion>> {
        self.get("policy_version", &format!("{org_id}:{policy_version_id}"))
            .await
    }

    async fn user_is_org_member(&self, org_id: &str, user_email: &str) -> anyhow::Result<bool> {
        let membership: Option<crate::policies::Membership> = self
            .get(
                "membership",
                &format!("{}:{}", org_id, normalize_email(user_email)),
            )
            .await?;
        Ok(membership.is_some_and(|membership| {
            membership.status == crate::policies::MembershipStatus::Active
        }))
    }

    async fn upsert_subscription(&self, subscription: &Subscription) -> anyhow::Result<()> {
        self.put(
            "subscription",
            &subscription.org_id,
            RecordFields {
                org_id: Some(&subscription.org_id),
                provider: Some(&subscription.provider),
                provider_id: subscription.provider_subscription_id.as_deref(),
                state: Some(subscription_state_name(subscription)),
                sort_at: Some(subscription.updated_at),
                ..Default::default()
            },
            subscription,
        )
        .await
    }

    async fn get_subscription_for_org(&self, org_id: &str) -> anyhow::Result<Option<Subscription>> {
        self.get("subscription", org_id).await
    }

    async fn find_subscription_by_provider_id(
        &self,
        provider_id: &str,
    ) -> anyhow::Result<Option<Subscription>> {
        find_provider_record(&self.pool, "subscription", provider_id).await
    }

    async fn upsert_checkout_session(&self, checkout: &CheckoutSession) -> anyhow::Result<()> {
        self.put(
            "checkout",
            &checkout.id,
            RecordFields {
                owner_email: Some(&checkout.account_email),
                org_id: Some(&checkout.org_id),
                provider: Some(&checkout.provider),
                provider_id: checkout.provider_subscription_id.as_deref(),
                state: Some(checkout_state_name(checkout)),
                sort_at: Some(checkout.updated_at),
                ..Default::default()
            },
            checkout,
        )
        .await
    }

    async fn get_checkout_session(&self, id: &str) -> anyhow::Result<Option<CheckoutSession>> {
        self.get("checkout", id).await
    }

    async fn find_checkout_session_by_provider_id(
        &self,
        provider_id: &str,
    ) -> anyhow::Result<Option<CheckoutSession>> {
        find_provider_record(&self.pool, "checkout", provider_id).await
    }

    async fn add_usage(
        &self,
        org_id: &str,
        period_key: &str,
        runs_created: u32,
        analyzed_threads: u32,
        ai_audited_threads: u32,
    ) -> anyhow::Result<()> {
        let id = format!("{org_id}:{period_key}");
        sqlx::query(
            "INSERT INTO mira.records (kind,id,org_id,sort_at,data) VALUES \
             ('usage_ledger',$1,$2,now(),jsonb_build_object('org_id',$2,'period_key',$3,'runs_created',$4,'analyzed_threads',$5,'ai_audited_threads',$6,'updated_at',to_jsonb(now()))) \
             ON CONFLICT (kind,id) DO UPDATE SET sort_at=now(), updated_at=now(), data=jsonb_build_object( \
               'org_id',EXCLUDED.data->'org_id','period_key',EXCLUDED.data->'period_key', \
               'runs_created',to_jsonb(LEAST(4294967295::bigint,COALESCE((mira.records.data->>'runs_created')::bigint,0)+$4)), \
               'analyzed_threads',to_jsonb(LEAST(4294967295::bigint,COALESCE((mira.records.data->>'analyzed_threads')::bigint,COALESCE((mira.records.data->>'candidate_threads')::bigint,0))+$5)), \
               'ai_audited_threads',to_jsonb(LEAST(4294967295::bigint,COALESCE((mira.records.data->>'ai_audited_threads')::bigint,0)+$6)), \
               'updated_at',to_jsonb(now()))",
        )
        .bind(id).bind(org_id).bind(period_key).bind(i64::from(runs_created)).bind(i64::from(analyzed_threads)).bind(i64::from(ai_audited_threads))
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_usage_ledger(
        &self,
        org_id: &str,
        period_key: &str,
    ) -> anyhow::Result<Option<UsageLedger>> {
        self.get("usage_ledger", &format!("{org_id}:{period_key}"))
            .await
    }

    async fn create_analysis_run(&self, run: &AnalysisRun) -> anyhow::Result<()> {
        self.upsert_analysis_run(run).await
    }

    async fn claim_pending_analysis_run(&self, id: &str) -> anyhow::Result<Option<AnalysisRun>> {
        let data = sqlx::query_scalar::<_, Value>(
            "UPDATE mira.records SET state='running', updated_at=now(), data=jsonb_set(jsonb_set(data,'{status}',to_jsonb('running'::text),true),'{progress_message}',to_jsonb('Iniciando lectura de Gmail'::text),true) \
             WHERE kind='analysis_run' AND id=$1 AND state='pending' RETURNING data",
        ).bind(id).fetch_optional(&self.pool).await?;
        data.map(|value| serde_json::from_value(value).context("invalid claimed analysis run"))
            .transpose()
    }

    async fn update_analysis_run(&self, run: &AnalysisRun) -> anyhow::Result<()> {
        self.upsert_analysis_run(run).await
    }

    async fn get_analysis_run(&self, id: &str) -> anyhow::Result<Option<AnalysisRun>> {
        self.get("analysis_run", id).await
    }

    async fn list_analysis_runs(&self, user_email: &str) -> anyhow::Result<Vec<AnalysisRun>> {
        let values = sqlx::query_scalar::<_, Value>("SELECT data FROM mira.records WHERE kind='analysis_run' AND owner_email=$1 ORDER BY sort_at DESC")
            .bind(normalize_email(user_email)).fetch_all(&self.pool).await?;
        values
            .into_iter()
            .map(|value| serde_json::from_value(value).context("invalid analysis run"))
            .collect()
    }

    async fn delete_analysis_data(&self, owner_email: &str) -> anyhow::Result<()> {
        sqlx::query(
            "WITH owned_runs AS (SELECT id FROM mira.records WHERE kind='analysis_run' AND owner_email=$1) \
             DELETE FROM mira.records WHERE kind='manual_review_override' AND owner_email=$1 \
             OR kind='analysis_run' AND owner_email=$1 \
             OR run_id IN (SELECT id FROM owned_runs)",
        ).bind(normalize_email(owner_email)).execute(&self.pool).await?;
        Ok(())
    }

    async fn record_analysis_data_deletion(
        &self,
        audit: &AnalysisDataDeletionAudit,
    ) -> anyhow::Result<()> {
        self.put(
            "analysis_deletion_audit",
            &audit.id,
            RecordFields {
                owner_email: Some(&audit.owner_hash),
                state: Some(deletion_state_name(audit)),
                sort_at: Some(audit.requested_at),
                ..Default::default()
            },
            audit,
        )
        .await
    }

    async fn upsert_thread(
        &self,
        thread: &EmailThread,
        messages: &[EmailMessage],
    ) -> anyhow::Result<()> {
        let thread_key = thread_key(&thread.analysis_run_id, &thread.id);
        self.put(
            "email_thread",
            &thread_key,
            RecordFields {
                run_id: Some(&thread.analysis_run_id),
                thread_id: Some(&thread.id),
                sort_at: Some(
                    thread
                        .first_message_at
                        .or(thread.first_client_message_at)
                        .unwrap_or(thread.created_at),
                ),
                ..Default::default()
            },
            thread,
        )
        .await?;
        for message in messages {
            let sanitized = EmailMessage {
                body_text: None,
                ..message.clone()
            };
            self.put(
                "email_message",
                &message_key(&thread.analysis_run_id, &thread.id, &sanitized.id),
                RecordFields {
                    run_id: Some(&thread.analysis_run_id),
                    thread_id: Some(&thread.id),
                    sort_at: Some(sanitized.date),
                    ..Default::default()
                },
                &sanitized,
            )
            .await?;
        }
        Ok(())
    }

    async fn list_threads(&self, run_id: &str) -> anyhow::Result<Vec<EmailThread>> {
        let mut threads = self.list_run::<EmailThread>("email_thread", run_id).await?;
        for thread in &mut threads {
            if thread.first_message_at.is_none() {
                thread.first_message_at = self
                    .list_messages(run_id, &thread.id)
                    .await?
                    .iter()
                    .map(|message| message.date)
                    .min();
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
        self.get("email_thread", &thread_key(run_id, thread_id))
            .await
    }

    async fn find_threads_by_id(&self, thread_id: &str) -> anyhow::Result<Vec<EmailThread>> {
        let values = sqlx::query_scalar::<_, Value>(
            "SELECT data FROM mira.records WHERE kind='email_thread' AND thread_id=$1",
        )
        .bind(thread_id)
        .fetch_all(&self.pool)
        .await?;
        values
            .into_iter()
            .map(|value| serde_json::from_value(value).context("invalid thread"))
            .collect()
    }

    async fn list_messages(
        &self,
        run_id: &str,
        thread_id: &str,
    ) -> anyhow::Result<Vec<EmailMessage>> {
        let values = sqlx::query_scalar::<_, Value>("SELECT data FROM mira.records WHERE kind='email_message' AND run_id=$1 AND thread_id=$2 ORDER BY sort_at")
            .bind(run_id).bind(thread_id).fetch_all(&self.pool).await?;
        values
            .into_iter()
            .map(|value| serde_json::from_value(value).context("invalid email message"))
            .collect()
    }

    async fn add_ai_audit(
        &self,
        run_id: &str,
        thread_id: &str,
        audit: &AiAuditResult,
    ) -> anyhow::Result<()> {
        self.put(
            "ai_audit",
            &Uuid::new_v4().to_string(),
            RecordFields {
                run_id: Some(run_id),
                thread_id: Some(thread_id),
                sort_at: Some(Utc::now()),
                ..Default::default()
            },
            audit,
        )
        .await
    }

    async fn add_manual_review(&self, run_id: &str, review: &ManualReview) -> anyhow::Result<()> {
        if self.get_analysis_run(run_id).await?.is_none() {
            return Err(anyhow!("run not found"));
        }
        let mut thread = self
            .get_thread(run_id, &review.email_thread_id)
            .await?
            .ok_or_else(|| anyhow!("thread not found"))?;
        let is_valid = matches!(
            review.new_classification,
            crate::analysis::Classification::ValidClientRequest
        );
        thread.classification = review.new_classification.clone();
        thread.classification_source = crate::analysis::ClassificationSource::Manual;
        thread.is_valid_client_request = is_valid;
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
        let mut canonical = review.clone();
        canonical.is_valid_client_request = is_valid;
        self.put(
            "manual_review",
            &format!("{run_id}:{}", review.id),
            RecordFields {
                run_id: Some(run_id),
                thread_id: Some(&review.email_thread_id),
                sort_at: Some(review.created_at),
                ..Default::default()
            },
            &canonical,
        )
        .await?;
        self.upsert_thread(&thread, &[]).await
    }

    async fn upsert_manual_review_override(
        &self,
        review: &ManualReviewOverride,
    ) -> anyhow::Result<()> {
        self.put(
            "manual_review_override",
            &format!(
                "{}:{}",
                normalize_email(&review.owner_email),
                review.thread_id
            ),
            RecordFields {
                owner_email: Some(&review.owner_email),
                thread_id: Some(&review.thread_id),
                run_id: Some(&review.source_run_id),
                sort_at: Some(review.created_at),
                ..Default::default()
            },
            review,
        )
        .await
    }

    async fn get_manual_review_override(
        &self,
        owner_email: &str,
        thread_id: &str,
    ) -> anyhow::Result<Option<ManualReviewOverride>> {
        self.get(
            "manual_review_override",
            &format!("{}:{thread_id}", normalize_email(owner_email)),
        )
        .await
    }

    async fn reconcile_manual_review_metrics_v1(
        &self,
    ) -> anyhow::Result<ManualReviewMetricsMigrationResult> {
        // ponytail: Firestore already runs this reconciliation before export; add a data migration only if the importer finds pre-v1 records.
        Ok(ManualReviewMetricsMigrationResult {
            already_applied: true,
            ..Default::default()
        })
    }

    async fn reconcile_manual_review_inheritance_v2(
        &self,
    ) -> anyhow::Result<ManualReviewInheritanceMigrationResult> {
        // ponytail: Firestore already runs this reconciliation before export; add a data migration only if the importer finds pre-v2 records.
        Ok(ManualReviewInheritanceMigrationResult {
            already_applied: true,
            ..Default::default()
        })
    }

    async fn upsert_mailbox_metadata(
        &self,
        owner_email: &str,
        metadata: &MailboxMetadata,
    ) -> anyhow::Result<()> {
        self.put(
            "mailbox_metadata",
            &normalize_email(owner_email),
            RecordFields {
                owner_email: Some(owner_email),
                sort_at: Some(metadata.synced_at),
                ..Default::default()
            },
            metadata,
        )
        .await
    }

    async fn get_mailbox_metadata(
        &self,
        owner_email: &str,
    ) -> anyhow::Result<Option<MailboxMetadata>> {
        self.get("mailbox_metadata", &normalize_email(owner_email))
            .await
    }

    async fn list_filter_presets(&self, owner_email: &str) -> anyhow::Result<Vec<FilterPreset>> {
        self.list_owner("filter_preset", owner_email).await
    }

    async fn upsert_filter_preset(&self, preset: &FilterPreset) -> anyhow::Result<()> {
        self.put(
            "filter_preset",
            &preset.id,
            RecordFields {
                owner_email: Some(&preset.owner_email),
                sort_at: Some(preset.created_at),
                ..Default::default()
            },
            preset,
        )
        .await
    }

    async fn delete_filter_preset(&self, owner_email: &str, preset_id: &str) -> anyhow::Result<()> {
        sqlx::query(
            "DELETE FROM mira.records WHERE kind='filter_preset' AND id=$1 AND owner_email=$2",
        )
        .bind(preset_id)
        .bind(normalize_email(owner_email))
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

impl PostgresStorage {
    async fn upsert_analysis_run(&self, run: &AnalysisRun) -> anyhow::Result<()> {
        self.put(
            "analysis_run",
            &run.id,
            RecordFields {
                owner_email: Some(&run.user_email),
                org_id: run.org_id.as_deref(),
                state: Some(analysis_state_name(run)),
                sort_at: Some(run.created_at),
                ..Default::default()
            },
            run,
        )
        .await
    }

    pub(crate) async fn replace_usage_ledger_for_import(
        &self,
        usage: &UsageLedger,
    ) -> anyhow::Result<()> {
        self.put(
            "usage_ledger",
            &format!("{}:{}", usage.org_id, usage.period_key),
            RecordFields {
                org_id: Some(&usage.org_id),
                sort_at: Some(usage.updated_at),
                ..Default::default()
            },
            usage,
        )
        .await
    }

    pub(crate) async fn import_ai_audit(
        &self,
        id: &str,
        run_id: &str,
        thread_id: &str,
        audit: &AiAuditResult,
    ) -> anyhow::Result<()> {
        self.put(
            "ai_audit",
            id,
            RecordFields {
                run_id: Some(run_id),
                thread_id: Some(thread_id),
                ..Default::default()
            },
            audit,
        )
        .await
    }

    pub(crate) async fn clear_for_firestore_import(&self) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM mira.records")
            .execute(&self.pool)
            .await
            .context("failed to clear PostgreSQL snapshot destination")?;
        Ok(())
    }

    pub(crate) async fn record_count(&self) -> anyhow::Result<u64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mira.records")
            .fetch_one(&self.pool)
            .await
            .context("failed to count PostgreSQL snapshot records")?;
        count
            .try_into()
            .context("PostgreSQL snapshot record count is negative")
    }
}

async fn find_provider_record<T: DeserializeOwned>(
    pool: &PgPool,
    kind: &str,
    provider_id: &str,
) -> anyhow::Result<Option<T>> {
    let data = sqlx::query_scalar::<_, Value>(
        "SELECT data FROM mira.records WHERE kind=$1 AND provider_id=$2 LIMIT 1",
    )
    .bind(kind)
    .bind(provider_id)
    .fetch_optional(pool)
    .await?;
    data.map(|value| serde_json::from_value(value).context("invalid provider record"))
        .transpose()
}

fn normalize_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}
fn thread_key(run_id: &str, thread_id: &str) -> String {
    format!("{run_id}:{thread_id}")
}
fn message_key(run_id: &str, thread_id: &str, message_id: &str) -> String {
    format!("{run_id}:{thread_id}:{message_id}")
}
fn enum_name<T: Serialize>(value: &T) -> &'static str {
    match serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .as_deref()
    {
        Some("pending") => "pending",
        Some("running") => "running",
        Some("completed") => "completed",
        Some("failed") => "failed",
        Some("active") => "active",
        Some("trialing") => "trialing",
        Some("past_due") => "past_due",
        Some("cancelled") => "cancelled",
        Some("expired") => "expired",
        Some("provider_created") => "provider_created",
        Some("activated") => "activated",
        Some("requested") => "requested",
        Some("disabled") => "disabled",
        _ => "unknown",
    }
}
fn schedule_state_name(state: &ScheduleState) -> &'static str {
    enum_name(&state.status)
}
fn subscription_state_name(subscription: &Subscription) -> &'static str {
    enum_name(&subscription.status)
}
fn checkout_state_name(checkout: &CheckoutSession) -> &'static str {
    enum_name(&checkout.status)
}
fn analysis_state_name(run: &AnalysisRun) -> &'static str {
    enum_name(&run.status)
}
fn deletion_state_name(audit: &AnalysisDataDeletionAudit) -> &'static str {
    enum_name(&audit.status)
}
