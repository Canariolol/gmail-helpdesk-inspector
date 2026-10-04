use std::sync::Arc;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use chrono::Utc;
use futures_util::future::join_all;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

use super::*;
use crate::{
    analysis::{
        AnalysisConfig, AnalysisMetrics, AnalysisStatus, Classification, ClassificationSource,
        RequestScope, TriggerType,
    },
    auth::{session_expires_at, sign_session_id},
    billing::free_plan,
    config::test_app_config,
    policies::{MembershipStatus, OrgRole, OrganizationStatus, provision_default_config},
    scheduler::model::ScheduleRunStatus,
    storage::UsageAmounts,
};

async fn test_storage() -> PostgresStorage {
    let url = std::env::var("TEST_POSTGRES_DATABASE_URL")
        .expect("TEST_POSTGRES_DATABASE_URL is required for PostgreSQL integration tests");
    let storage = PostgresStorage::connect(&url)
        .await
        .unwrap_or_else(|_| panic!("cannot connect to test PostgreSQL"));
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(
        database, "mira_test",
        "integration tests require the dedicated mira_test database"
    );
    storage
}

fn run(owner: &str, org_id: &str) -> AnalysisRun {
    let now = Utc::now();
    AnalysisRun {
        id: Uuid::new_v4().to_string(),
        user_email: owner.to_string(),
        org_id: Some(org_id.to_string()),
        mailbox_id: None,
        trigger_type: Some(TriggerType::Manual),
        policy_version_id: None,
        policy_hash: None,
        policy_snapshot: None,
        gmail_scope_snapshot: vec![],
        retention_expires_at: Some(now + chrono::Duration::days(30)),
        data_minimization_mode: None,
        config: AnalysisConfig {
            date_from: "2026-10-01".to_string(),
            date_to: "2026-10-02".to_string(),
            time_from: "00:00".to_string(),
            time_to: "23:59".to_string(),
            timezone: "America/Santiago".to_string(),
            internal_domains: vec!["example.test".to_string()],
            responder_emails: vec![],
            request_scope: RequestScope::External,
            ignored_senders: vec![],
            ignored_domains: vec![],
            ignored_keywords: vec![],
            include_labels: vec![],
            exclude_labels: vec![],
        },
        status: AnalysisStatus::Completed,
        progress_message: "Done".to_string(),
        processed_threads: 0,
        total_candidate_threads: 0,
        metrics: AnalysisMetrics::default(),
        created_at: now,
        completed_at: Some(now),
        error_message: None,
    }
}

fn thread(run: &AnalysisRun) -> EmailThread {
    serde_json::from_value(json!({
        "id": Uuid::new_v4().to_string(), "analysis_run_id": run.id, "thread_id": Uuid::new_v4().to_string(),
        "subject": "Synthetic support request", "normalized_subject": "synthetic support request",
        "classification": Classification::ValidClientRequest, "classification_source": ClassificationSource::Rules,
        "classification_confidence": 0.9, "is_valid_client_request": true, "is_answered": false,
        "manual_review_required": true, "manual_override_applied": false, "reasons":[],
        "created_at": Utc::now(), "updated_at": Utc::now()
    })).unwrap()
}

fn message() -> EmailMessage {
    serde_json::from_value(json!({
        "id":Uuid::new_v4().to_string(), "message_id":Uuid::new_v4().to_string(),
        "from_email":"client@customer.test", "to_emails":["support@example.test"], "cc_emails":[],
        "date":Utc::now(), "subject":"Synthetic request", "snippet":"Short request", "headers":{},
        "is_internal":false, "is_external":true, "is_automated":false, "body_text":"Not persisted"
    }))
    .unwrap()
}

fn review(run: &AnalysisRun, thread: &EmailThread) -> ManualReviewOverride {
    ManualReviewOverride {
        owner_email: run.user_email.clone(),
        thread_id: thread.thread_id.clone(),
        source_run_id: run.id.clone(),
        message_fingerprint: "synthetic-fingerprint".to_string(),
        review_context: Some("synthetic-context".to_string()),
        reviewer_label: run.user_email.clone(),
        classification: Classification::Misc,
        is_answered: false,
        first_client_message_id: None,
        first_internal_reply_message_id: None,
        last_internal_message_id: None,
        notes: None,
        created_at: Utc::now(),
    }
}

async fn add_session(storage: &PostgresStorage, owner: &str) -> String {
    let now = Utc::now();
    let session = UserSession {
        id: Uuid::new_v4().to_string(),
        workos_user_id: None,
        workos_session_id: None,
        google_account_email: owner.to_string(),
        gmail_account_email: None,
        access_token_encrypted: String::new(),
        refresh_token_encrypted: None,
        gmail_access_token_encrypted: None,
        gmail_refresh_token_encrypted: None,
        expires_at: Some(session_expires_at(now)),
        revoked_at: None,
        created_at: now,
        updated_at: now,
    };
    storage.upsert_user_session(&session).await.unwrap();
    format!(
        "ghmi_session={}",
        sign_session_id(&session.id, &test_app_config().session_secret).unwrap()
    )
}

fn request(
    method: Method,
    path: &str,
    cookie: &str,
    body: Option<serde_json::Value>,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            body.map(|body| body.to_string()).unwrap_or_default(),
        ))
        .unwrap()
}

#[tokio::test]
#[ignore = "requires a dedicated PostgreSQL database; scripts/check-postgres.sh runs it"]
async fn postgres_tenant_and_role_guards_use_real_memberships() {
    let storage = test_storage().await;
    let owner_a = format!("a-{}@example.test", Uuid::new_v4());
    let owner_b = format!("b-{}@example.test", Uuid::new_v4());
    let mut org_a = provision_default_config(&owner_a, Utc::now());
    let org_b = provision_default_config(&owner_b, Utc::now());
    storage.upsert_org_config(&org_a).await.unwrap();
    storage.upsert_org_config(&org_b).await.unwrap();
    let run_a = run(&owner_a, &org_a.org.id);
    let thread_a = thread(&run_a);
    storage.create_analysis_run(&run_a).await.unwrap();
    storage.upsert_thread(&thread_a, &[]).await.unwrap();
    let cookie_a = add_session(&storage, &owner_a).await;
    let cookie_b = add_session(&storage, &owner_b).await;
    let app = crate::build_app(test_app_config(), Arc::new(storage.clone()));
    let run_path = format!("/analysis-runs/{}", run_a.id);
    let detail_path = format!("{run_path}/threads/{}", thread_a.id);
    let review_path = format!("{detail_path}/manual-review");
    for path in [&run_path, &detail_path] {
        assert_eq!(
            app.clone()
                .oneshot(request(Method::GET, path, &cookie_a, None))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(request(Method::GET, path, &cookie_b, None))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    let review = json!({"new_classification":"misc", "is_answered":false});
    assert_eq!(
        app.clone()
            .oneshot(request(
                Method::PATCH,
                &review_path,
                &cookie_b,
                Some(review.clone())
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    org_a.membership.role = OrgRole::Viewer;
    storage.upsert_org_config(&org_a).await.unwrap();
    assert_eq!(
        app.clone()
            .oneshot(request(Method::GET, &run_path, &cookie_a, None))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                Method::PATCH,
                &review_path,
                &cookie_a,
                Some(review)
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    org_a.membership.status = MembershipStatus::Disabled;
    storage.upsert_org_config(&org_a).await.unwrap();
    assert!(
        !storage
            .user_is_org_member(&org_a.org.id, &owner_a)
            .await
            .unwrap()
    );
    let status = app
        .clone()
        .oneshot(request(Method::GET, &run_path, &cookie_a, None))
        .await
        .unwrap()
        .status();
    assert!(matches!(
        status,
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ));
    org_a.membership.status = MembershipStatus::Active;
    org_a.org.status = OrganizationStatus::Disabled;
    storage.upsert_org_config(&org_a).await.unwrap();
    let status = app
        .oneshot(request(Method::GET, &run_path, &cookie_a, None))
        .await
        .unwrap()
        .status();
    assert!(matches!(
        status,
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ));
    assert_eq!(
        storage
            .get_thread(&run_a.id, &thread_a.id)
            .await
            .unwrap()
            .unwrap()
            .classification,
        Classification::ValidClientRequest
    );
}

#[tokio::test]
#[ignore = "requires a dedicated PostgreSQL database; scripts/check-postgres.sh runs it"]
async fn postgres_quota_and_billing_claims_are_atomic_and_token_owned() {
    use crate::billing::{
        BillingInterval, BillingPlanId, CheckoutSession, CheckoutSessionStatus, SubscriptionStatus,
        active_subscription_for_trial,
    };
    let storage = test_storage().await;
    let catalog: Vec<(i32, i32)> = sqlx::query_as(
        "SELECT mailboxes,members FROM billing.plans WHERE id IN ('gratis','inicial','pro')",
    )
    .fetch_all(&storage.pool)
    .await
    .unwrap();
    assert_eq!(catalog.len(), 3);
    assert!(catalog.iter().all(|limits| *limits == (1, 1)));
    let org_id = Uuid::new_v4().to_string();
    let owner = format!("quota-{}@example.test", Uuid::new_v4());
    let now = chrono::DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap();
    let provider_id = Uuid::new_v4().to_string();
    let mut paid = active_subscription_for_trial(
        org_id.clone(),
        BillingPlanId::Pro,
        Some(provider_id.clone()),
        BillingInterval::Annual,
        now,
    );
    paid.status = SubscriptionStatus::Active;
    paid.trial_ends_at = None;
    paid.current_period_start = Some(now);
    paid.current_period_end = Some(BillingInterval::Annual.period_end(now));
    storage.upsert_subscription(&paid).await.unwrap();
    let persisted = storage
        .get_subscription_for_org(&org_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.billing_interval, BillingInterval::Annual);
    assert_eq!(persisted.current_period_end, paid.current_period_end);
    let checkout = CheckoutSession {
        id: Uuid::new_v4().to_string(),
        org_id: org_id.clone(),
        account_email: owner.clone(),
        plan_id: BillingPlanId::Pro,
        status: CheckoutSessionStatus::Activated,
        provider: "mercadopago".to_string(),
        provider_subscription_id: Some(provider_id.clone()),
        billing_interval: BillingInterval::Annual,
        currency_id: "CLP".to_string(),
        amount_clp: 299_900,
        usd_reference_monthly: 29,
        trial_days: 0,
        created_at: now,
        updated_at: now,
    };
    storage.upsert_checkout_session(&checkout).await.unwrap();
    let persisted = storage
        .find_checkout_session_by_provider_id(&provider_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.id, checkout.id);
    assert_eq!(persisted.amount_clp, 299_900);
    assert_eq!(persisted.billing_interval, BillingInterval::Annual);
    assert!(
        storage
            .record_quota_alert_if_new(&org_id, "2026-10", "runs", 80)
            .await
            .unwrap()
    );
    let mut limits = free_plan().limits;
    limits.runs_per_month = 1;
    limits.retrieved_threads_per_month = 5;
    limits.ai_analyzed_threads_per_month = 3;
    let runs: Vec<_> = (0..8).map(|_| run(&owner, &org_id)).collect();
    let wanted = UsageAmounts {
        runs: 1,
        retrieved: 5,
        ai: 3,
    };
    let results = join_all(
        runs.iter()
            .map(|run| storage.reserve_analysis_usage(run, "2026-10", wanted, &limits)),
    )
    .await;
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let winner = results.iter().position(Result::is_ok).unwrap();
    let usage = storage
        .get_usage_ledger(&org_id, "2026-10")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            usage.runs_created,
            usage.retrieved_threads,
            usage.ai_analyzed_threads
        ),
        (1, 5, 3)
    );
    for _ in 0..2 {
        storage
            .settle_analysis_usage(
                &runs[winner].id,
                UsageAmounts {
                    runs: 1,
                    retrieved: 2,
                    ai: 1,
                },
            )
            .await
            .unwrap();
    }
    let usage = storage
        .get_usage_ledger(&org_id, "2026-10")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            usage.runs_created,
            usage.retrieved_threads,
            usage.ai_analyzed_threads
        ),
        (1, 2, 1)
    );
    let now = Utc::now();
    let results = join_all(["first", "second"].iter().map(|token| {
        storage.claim_billing_operation(&org_id, token, now, now - chrono::Duration::minutes(5))
    }))
    .await;
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Ok(true)))
            .count(),
        1
    );
    let winner = if results[0].as_ref().unwrap() == &true {
        "first"
    } else {
        "second"
    };
    storage
        .release_billing_operation(&org_id, "foreign-token")
        .await
        .unwrap();
    assert!(
        !storage
            .claim_billing_operation(&org_id, "new", now, now - chrono::Duration::minutes(5))
            .await
            .unwrap()
    );
    storage
        .release_billing_operation(&org_id, winner)
        .await
        .unwrap();
    assert!(
        storage
            .claim_billing_operation(&org_id, "new", now, now - chrono::Duration::minutes(5))
            .await
            .unwrap()
    );
    storage
        .release_billing_operation(&org_id, winner)
        .await
        .unwrap();
    assert!(
        !storage
            .claim_billing_operation(&org_id, "other", now, now - chrono::Duration::minutes(5))
            .await
            .unwrap()
    );
}

#[tokio::test]
#[ignore = "requires a dedicated PostgreSQL database; scripts/check-postgres.sh runs it"]
async fn postgres_first_schedule_claim_and_public_database_access_are_safe() {
    let storage = test_storage().await;
    let now = Utc::now();
    let schedule = ScheduleState {
        user_email: format!("schedule-{}@example.test", Uuid::new_v4()),
        window_date_from: "2026-10-01".to_string(),
        window_date_to: "2026-10-02".to_string(),
        status: ScheduleRunStatus::Running,
        run_id: None,
        email_sent: false,
        error_message: None,
        started_at: now,
        updated_at: now,
    };
    let claims = join_all(
        (0..8).map(|_| storage.claim_schedule_window(&schedule, now - chrono::Duration::hours(1))),
    )
    .await;
    assert_eq!(
        claims
            .iter()
            .filter(|result| matches!(result, Ok(ScheduleWindowClaim::Claimed)))
            .count(),
        1
    );
    for role in ["anon", "authenticated", "service_role"] {
        for relation in [
            "mira.records",
            "billing.subscriptions",
            "billing.checkout_sessions",
        ] {
            let mut tx = storage.pool.begin().await.unwrap();
            sqlx::query(&format!("SET LOCAL ROLE {role}"))
                .execute(&mut *tx)
                .await
                .unwrap();
            assert!(
                sqlx::query(&format!("SELECT 1 FROM {relation} LIMIT 1"))
                    .fetch_all(&mut *tx)
                    .await
                    .is_err()
            );
            tx.rollback().await.unwrap();
        }
    }
}

#[tokio::test]
#[ignore = "requires a dedicated PostgreSQL database; scripts/check-postgres.sh runs it"]
async fn postgres_deletion_wins_against_workers_without_recreating_data() {
    let storage = test_storage().await;
    let owner = format!("delete-{}@example.test", Uuid::new_v4());
    let foreign_owner = format!("keep-{}@example.test", Uuid::new_v4());
    let foreign = run(&foreign_owner, &Uuid::new_v4().to_string());
    storage.create_analysis_run(&foreign).await.unwrap();
    for _ in 0..4 {
        let run = run(&owner, &Uuid::new_v4().to_string());
        let thread = thread(&run);
        let messages = vec![message()];
        storage.create_analysis_run(&run).await.unwrap();
        storage.upsert_thread(&thread, &messages).await.unwrap();
        let review = review(&run, &thread);
        storage
            .upsert_manual_review_override(&review)
            .await
            .unwrap();
        let (deleted, _thread_write, _run_write, _review_write) = tokio::join!(
            storage.delete_analysis_data(&owner),
            storage.upsert_thread(&thread, &messages),
            storage.update_analysis_run(&run),
            storage.upsert_manual_review_override(&review),
        );
        deleted.unwrap();
        assert!(storage.get_analysis_run(&run.id).await.unwrap().is_none());
        assert!(storage.update_analysis_run(&run).await.is_err());
        assert!(storage.upsert_thread(&thread, &messages).await.is_err());
        assert!(
            storage
                .upsert_manual_review_override(&review)
                .await
                .is_err()
        );
        assert!(storage.list_threads(&run.id).await.unwrap().is_empty());
        assert!(
            storage
                .list_messages(&run.id, &thread.id)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            storage
                .get_manual_review_override(&owner, &thread.thread_id)
                .await
                .unwrap()
                .is_none()
        );
        let children: i64 = sqlx::query_scalar("SELECT count(*) FROM mira.records WHERE run_id=$1")
            .bind(&run.id)
            .fetch_one(&storage.pool)
            .await
            .unwrap();
        assert_eq!(children, 0);
    }
    assert!(
        storage
            .get_analysis_run(&foreign.id)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
#[ignore = "requires a dedicated PostgreSQL database; scripts/check-postgres.sh runs it"]
async fn postgres_retention_removes_expired_children_and_keeps_other_tenants() {
    let storage = test_storage().await;
    let owner = format!("expired-{}@example.test", Uuid::new_v4());
    let foreign_owner = format!("retained-{}@example.test", Uuid::new_v4());
    let mut expired = run(&owner, &Uuid::new_v4().to_string());
    let now = Utc::now();
    expired.retention_expires_at = Some(now - chrono::Duration::seconds(1));
    let fresh = run(&owner, expired.org_id.as_deref().unwrap());
    let foreign = run(&foreign_owner, &Uuid::new_v4().to_string());
    let mut legacy = run(&owner, expired.org_id.as_deref().unwrap());
    legacy.retention_expires_at = None;
    let mut aged_legacy = run(&owner, expired.org_id.as_deref().unwrap());
    aged_legacy.retention_expires_at = None;
    aged_legacy.created_at = now - chrono::Duration::days(91);
    aged_legacy.completed_at = Some(aged_legacy.created_at);
    for run in [&expired, &fresh, &foreign, &legacy, &aged_legacy] {
        storage.create_analysis_run(run).await.unwrap();
        let thread = thread(run);
        storage.upsert_thread(&thread, &[message()]).await.unwrap();
        storage
            .upsert_manual_review_override(&review(run, &thread))
            .await
            .unwrap();
    }
    assert!(storage.purge_expired_analysis_data(now).await.unwrap() >= 2);
    assert!(
        storage
            .get_analysis_run(&expired.id)
            .await
            .unwrap()
            .is_none()
    );
    let children: i64 = sqlx::query_scalar("SELECT count(*) FROM mira.records WHERE run_id=$1")
        .bind(&expired.id)
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(children, 0);
    assert!(
        storage
            .get_analysis_run(&aged_legacy.id)
            .await
            .unwrap()
            .is_none()
    );
    let children: i64 = sqlx::query_scalar("SELECT count(*) FROM mira.records WHERE run_id=$1")
        .bind(&aged_legacy.id)
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(children, 0);
    for retained in [&fresh, &foreign, &legacy] {
        assert!(
            storage
                .get_analysis_run(&retained.id)
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(storage.list_threads(&retained.id).await.unwrap().len(), 1);
        let children: i64 = sqlx::query_scalar("SELECT count(*) FROM mira.records WHERE run_id=$1")
            .bind(&retained.id)
            .fetch_one(&storage.pool)
            .await
            .unwrap();
        assert_eq!(children, 3);
    }
}

#[tokio::test]
#[ignore = "requires a dedicated PostgreSQL database; scripts/check-postgres.sh runs it"]
async fn postgres_stale_metadata_and_refresh_cannot_overwrite_a_new_mailbox() {
    use crate::mailbox::{GmailProfile, MailboxMetadata, MailboxProviderKind};
    use crate::storage::MailboxConnectionRefresh;
    let storage = test_storage().await;
    let owner = format!("mailbox-{}@example.test", Uuid::new_v4());
    let now = Utc::now();
    let old: MailboxConnection = serde_json::from_value(json!({
        "owner_email":owner,"provider":"google","mailbox_email":"old@example.test",
        "access_token_encrypted":"synthetic-old","connected_at":now,"updated_at":now,
    }))
    .unwrap();
    storage.upsert_gmail_connection(&old).await.unwrap();
    let metadata = MailboxMetadata {
        profile: Some(GmailProfile {
            email_address: old.mailbox_email.clone(),
            messages_total: 0,
            threads_total: 0,
        }),
        labels: vec![],
        send_as: vec![],
        filters_count: 0,
        folders_truncated: false,
        synced_at: now,
    };
    assert!(
        storage
            .save_current_mailbox_metadata(&old, &metadata)
            .await
            .unwrap()
    );
    let mut current = old.clone();
    current.provider = MailboxProviderKind::Microsoft;
    current.mailbox_email = "new@example.test".to_string();
    current.connected_at = now + chrono::Duration::seconds(1);
    current.updated_at = current.connected_at;
    current.access_token_encrypted = "synthetic-new".to_string();
    storage.upsert_gmail_connection(&current).await.unwrap();
    let mut new_metadata = metadata.clone();
    new_metadata.profile.as_mut().unwrap().email_address = current.mailbox_email.clone();
    new_metadata.synced_at = current.connected_at;
    assert!(
        storage
            .save_current_mailbox_metadata(&current, &new_metadata)
            .await
            .unwrap()
    );
    assert!(
        !storage
            .save_current_mailbox_metadata(&old, &metadata)
            .await
            .unwrap()
    );
    let mut stale_refresh = old.clone();
    stale_refresh.access_token_encrypted = "synthetic-refreshed-old".to_string();
    stale_refresh.updated_at = current.updated_at + chrono::Duration::seconds(1);
    assert!(matches!(
        storage
            .refresh_gmail_connection(&old, &stale_refresh)
            .await
            .unwrap(),
        MailboxConnectionRefresh::ConnectionChanged
    ));
    assert_eq!(
        storage
            .get_mailbox_metadata(&owner)
            .await
            .unwrap()
            .unwrap()
            .profile
            .unwrap()
            .email_address,
        "new@example.test"
    );
    let saved = storage.get_gmail_connection(&owner).await.unwrap().unwrap();
    assert_eq!(saved.mailbox_email, "new@example.test");
    assert_eq!(saved.access_token_encrypted, "synthetic-new");
}
