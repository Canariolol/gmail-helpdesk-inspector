use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const GMAIL_READONLY_SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
pub const POLICY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub status: OrganizationStatus,
    pub default_timezone: String,
    pub locale: String,
    pub created_by_user_email: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationStatus {
    PrivateBeta,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    pub org_id: String,
    pub user_email: String,
    pub role: OrgRole,
    pub status: MembershipStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrgRole {
    Owner,
    Admin,
    Analyst,
    Viewer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MembershipStatus {
    Active,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mailbox {
    pub id: String,
    pub org_id: String,
    pub google_account_email: String,
    pub workspace_domain: String,
    pub display_name: String,
    pub purpose: MailboxPurpose,
    pub authorized_by_user_email: String,
    pub gmail_scope_snapshot: Vec<String>,
    pub connected_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MailboxPurpose {
    SupportShared,
    SupervisorInbox,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDraft {
    pub org_id: String,
    pub mailbox_id: String,
    pub analysis_policy: AnalysisPolicy,
    pub ai_policy: AiPolicy,
    pub schedule_report_policy: ScheduleReportPolicy,
    pub retention_policy: RetentionPolicy,
    pub updated_by_user_email: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyVersion {
    pub id: String,
    pub org_id: String,
    pub version: u64,
    pub schema_version: u32,
    pub policy_hash: String,
    pub snapshot: PolicySnapshot,
    pub created_by_user_email: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySnapshot {
    pub mailbox: MailboxSnapshot,
    pub analysis_policy: AnalysisPolicy,
    pub ai_policy: AiPolicy,
    pub schedule_report_policy: ScheduleReportPolicy,
    pub retention_policy: RetentionPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxSnapshot {
    pub id: String,
    pub google_account_email: String,
    pub workspace_domain: String,
    pub display_name: String,
    pub purpose: MailboxPurpose,
    pub gmail_scope_snapshot: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisPolicy {
    pub timezone: String,
    pub internal_domains: Vec<String>,
    pub responder_emails: Vec<String>,
    pub mailbox_aliases: Vec<String>,
    pub valid_request_criteria: Vec<String>,
    pub non_responsibility_rules: Vec<String>,
    pub ignored_senders: Vec<String>,
    pub ignored_domains: Vec<String>,
    pub ignored_keywords: Vec<String>,
    pub default_time_from: String,
    pub default_time_to: String,
    pub max_threads_per_run: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiPolicy {
    pub enabled: bool,
    pub consent_granted_at: Option<DateTime<Utc>>,
    pub provider: String,
    pub model_id: String,
    pub prompt_version: String,
    pub auto_apply_threshold: f64,
    pub manual_review_threshold: f64,
    pub max_audit_messages: u32,
    pub max_body_chars_per_message: u32,
    pub allowed_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleReportPolicy {
    pub scheduler_enabled: bool,
    pub preset: SchedulePreset,
    pub timezone: String,
    pub report_recipients: Vec<String>,
    pub report_content: ReportContentPolicy,
    pub failure_notice_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchedulePreset {
    Disabled,
    Weekdays08Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportContentPolicy {
    pub mode: ReportMode,
    pub include_subjects: bool,
    pub include_senders: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReportMode {
    MetricsOnly,
    MetricsAndReviewItems,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub retention_days: u32,
    pub delete_threads_and_messages: bool,
    pub delete_ai_audits: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgConfigBundle {
    pub org: Organization,
    pub membership: Membership,
    pub mailbox: Mailbox,
    pub draft: PolicyDraft,
    pub policy_version: PolicyVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupState {
    pub ready_for_analysis: bool,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgConfigResponse {
    pub org: Organization,
    pub membership: Membership,
    pub mailbox: Mailbox,
    pub draft: PolicyDraft,
    pub policy_version: PolicyVersion,
    pub setup_state: SetupState,
}

impl OrgConfigBundle {
    pub fn response(&self) -> OrgConfigResponse {
        OrgConfigResponse {
            org: self.org.clone(),
            membership: self.membership.clone(),
            mailbox: self.mailbox.clone(),
            draft: self.draft.clone(),
            policy_version: self.policy_version.clone(),
            setup_state: setup_state(&self.draft),
        }
    }
}

pub fn provision_default_config(user_email: &str, now: DateTime<Utc>) -> OrgConfigBundle {
    let domain = email_domain(user_email).unwrap_or_else(|| "example.com".to_string());
    let org_id = Uuid::new_v4().to_string();
    let mailbox_id = Uuid::new_v4().to_string();
    let org = Organization {
        id: org_id.clone(),
        name: suggested_org_name(&domain),
        status: OrganizationStatus::PrivateBeta,
        default_timezone: "America/Santiago".to_string(),
        locale: "es-CL".to_string(),
        created_by_user_email: user_email.to_string(),
        created_at: now,
        updated_at: now,
    };
    let membership = Membership {
        org_id: org_id.clone(),
        user_email: user_email.to_string(),
        role: OrgRole::Owner,
        status: MembershipStatus::Active,
        created_at: now,
    };
    let mailbox = Mailbox {
        id: mailbox_id.clone(),
        org_id: org_id.clone(),
        google_account_email: user_email.to_string(),
        workspace_domain: domain.clone(),
        display_name: user_email.to_string(),
        purpose: MailboxPurpose::SupportShared,
        authorized_by_user_email: user_email.to_string(),
        gmail_scope_snapshot: vec![GMAIL_READONLY_SCOPE.to_string()],
        connected_at: now,
        revoked_at: None,
    };
    let draft = PolicyDraft {
        org_id: org_id.clone(),
        mailbox_id,
        analysis_policy: AnalysisPolicy {
            timezone: org.default_timezone.clone(),
            internal_domains: vec![domain],
            responder_emails: vec![],
            mailbox_aliases: vec![user_email.to_string()],
            valid_request_criteria: vec![],
            non_responsibility_rules: vec![],
            ignored_senders: vec![],
            ignored_domains: vec![],
            ignored_keywords: vec![],
            default_time_from: "00:00".to_string(),
            default_time_to: "23:59".to_string(),
            max_threads_per_run: 50,
        },
        ai_policy: default_ai_policy(false, None),
        schedule_report_policy: ScheduleReportPolicy {
            scheduler_enabled: false,
            preset: SchedulePreset::Weekdays08Local,
            timezone: org.default_timezone.clone(),
            report_recipients: vec![],
            report_content: ReportContentPolicy {
                mode: ReportMode::MetricsOnly,
                include_subjects: false,
                include_senders: false,
            },
            failure_notice_enabled: true,
        },
        retention_policy: RetentionPolicy {
            retention_days: 30,
            delete_threads_and_messages: true,
            delete_ai_audits: true,
        },
        updated_by_user_email: user_email.to_string(),
        updated_at: now,
    };
    let policy_version = policy_version_from_draft(&mailbox, &draft, 1, user_email, now);
    OrgConfigBundle {
        org,
        membership,
        mailbox,
        draft,
        policy_version,
    }
}

pub fn policy_version_from_draft(
    mailbox: &Mailbox,
    draft: &PolicyDraft,
    version: u64,
    created_by_user_email: &str,
    now: DateTime<Utc>,
) -> PolicyVersion {
    let snapshot = PolicySnapshot {
        mailbox: MailboxSnapshot {
            id: mailbox.id.clone(),
            google_account_email: mailbox.google_account_email.clone(),
            workspace_domain: mailbox.workspace_domain.clone(),
            display_name: mailbox.display_name.clone(),
            purpose: mailbox.purpose.clone(),
            gmail_scope_snapshot: mailbox.gmail_scope_snapshot.clone(),
        },
        analysis_policy: draft.analysis_policy.clone(),
        ai_policy: draft.ai_policy.clone(),
        schedule_report_policy: draft.schedule_report_policy.clone(),
        retention_policy: draft.retention_policy.clone(),
    };
    let policy_hash = hash_policy_snapshot(&snapshot);
    PolicyVersion {
        id: Uuid::new_v4().to_string(),
        org_id: draft.org_id.clone(),
        version,
        schema_version: POLICY_SCHEMA_VERSION,
        policy_hash,
        snapshot,
        created_by_user_email: created_by_user_email.to_string(),
        created_at: now,
    }
}

pub fn setup_state(draft: &PolicyDraft) -> SetupState {
    let mut missing = Vec::new();
    if draft.analysis_policy.internal_domains.is_empty() {
        missing.push("internal_domains".to_string());
    }
    if draft.analysis_policy.valid_request_criteria.is_empty() {
        missing.push("valid_request_criteria".to_string());
    }
    if draft.schedule_report_policy.scheduler_enabled
        && draft.schedule_report_policy.report_recipients.is_empty()
    {
        missing.push("report_recipients".to_string());
    }
    SetupState {
        ready_for_analysis: missing.is_empty(),
        missing,
    }
}

pub fn retention_expires_at(created_at: DateTime<Utc>, snapshot: &PolicySnapshot) -> DateTime<Utc> {
    created_at + Duration::days(snapshot.retention_policy.retention_days as i64)
}

pub fn normalize_domains(values: Vec<String>) -> Vec<String> {
    normalize_list(values)
        .into_iter()
        .map(|value| value.trim_start_matches('@').to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

pub fn normalize_list(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let value = value.trim().to_lowercase();
        if !value.is_empty() && !out.contains(&value) {
            out.push(value);
        }
    }
    out
}

pub fn validate_timezone(value: &str) -> bool {
    value.parse::<Tz>().is_ok()
}

pub fn email_domain(email: &str) -> Option<String> {
    email
        .split_once('@')
        .map(|(_, domain)| domain.trim().to_lowercase())
        .filter(|domain| !domain.is_empty())
}

pub fn hash_owner_email(email: &str) -> String {
    let normalized = email.trim().to_lowercase();
    URL_SAFE_HASH_PREFIX.to_string() + &hex_hash(normalized.as_bytes())
}

pub fn hash_policy_snapshot(snapshot: &PolicySnapshot) -> String {
    let value = serde_json::to_vec(snapshot).unwrap_or_default();
    format!("sha256:{}", hex_hash(&value))
}

fn hex_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

const URL_SAFE_HASH_PREFIX: &str = "sha256_";

fn default_ai_policy(enabled: bool, consent_granted_at: Option<DateTime<Utc>>) -> AiPolicy {
    AiPolicy {
        enabled,
        consent_granted_at,
        provider: "bedrock".to_string(),
        model_id: "configured-server-side".to_string(),
        prompt_version: "helpdesk-auditor-v1".to_string(),
        auto_apply_threshold: 0.92,
        manual_review_threshold: 0.72,
        max_audit_messages: 14,
        max_body_chars_per_message: 280,
        allowed_fields: vec![
            "headers".to_string(),
            "participants".to_string(),
            "snippet".to_string(),
            "body_excerpt".to_string(),
        ],
    }
}

fn suggested_org_name(domain: &str) -> String {
    domain
        .split('.')
        .next()
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => "Organización".to_string(),
            }
        })
        .unwrap_or_else(|| "Organización".to_string())
}
