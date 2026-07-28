use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::mailbox::MailboxMetadata;

pub const GMAIL_READONLY_SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
pub const POLICY_SCHEMA_VERSION: u32 = 2;
pub const DEFAULT_AUTO_APPLY_THRESHOLD: f64 = 0.85;
const LEGACY_AUTO_APPLY_THRESHOLD: f64 = 0.92;
const DEFAULT_PROMPT_VERSION: &str = "helpdesk-auditor-v2";
const LEGACY_PROMPT_VERSION: &str = "helpdesk-auditor-v1";

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
    /// Distingue "todavía no sugerimos alias detectados" de "el usuario eligió
    /// dejar la lista vacía".
    #[serde(default)]
    pub mailbox_aliases_configured: bool,
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
    /// Palabras que confirman una solicitud real (p. ej. "ticket", "incidencia").
    /// Si alguna aparece en el asunto o el cuerpo de un mensaje, el hilo se rescata
    /// a Solicitud Válida y se enruta a la auditoría IA para el veredicto final.
    #[serde(default)]
    pub valid_signal_keywords: Vec<String>,
    /// Reglas opcionales para decidir si la actividad dentro de la ventana
    /// constituye una nueva solicitud diaria.
    #[serde(default)]
    pub count_historical_closures_as_valid: bool,
    #[serde(default)]
    pub count_previous_request_followups_as_valid: bool,
    #[serde(default = "default_true")]
    pub count_org_hosted_training_as_valid: bool,
    /// Etiquetas/categorías de Gmail a incluir y excluir en la recuperación de
    /// hilos. Vacías = comportamiento histórico (INBOX + pestaña Principal).
    #[serde(default)]
    pub include_labels: Vec<String>,
    #[serde(default)]
    pub exclude_labels: Vec<String>,
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
    /// Marca que a esta política ya se le aplicó el default "IA activa" (modelo opt-out).
    /// Ausente/false en documentos previos al cambio → la migración perezosa los enciende
    /// una sola vez; una vez en true, un opt-out posterior del usuario se respeta.
    #[serde(default)]
    pub ai_defaults_applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleReportPolicy {
    pub scheduler_enabled: bool,
    pub preset: SchedulePreset,
    pub timezone: String,
    #[serde(default = "default_analysis_time")]
    pub analysis_time: String,
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

fn default_true() -> bool {
    true
}

fn default_analysis_time() -> String {
    "08:00".to_string()
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
    /// Cuenta interna privilegiada: la UI nunca debe bloquear acciones por
    /// gating de setup ni por límites para estas cuentas.
    #[serde(default)]
    pub account_unrestricted: bool,
    /// Metadata de Gmail leída al conectar (etiquetas, alias, perfil). Opcional:
    /// puede no estar todavía si la sincronización en segundo plano no terminó.
    #[serde(default)]
    pub mailbox_metadata: Option<MailboxMetadata>,
}

impl OrgConfigBundle {
    pub fn response(
        &self,
        account_unrestricted: bool,
        mailbox_metadata: Option<MailboxMetadata>,
    ) -> OrgConfigResponse {
        OrgConfigResponse {
            org: self.org.clone(),
            membership: self.membership.clone(),
            mailbox: self.mailbox.clone(),
            draft: self.draft.clone(),
            policy_version: self.policy_version.clone(),
            setup_state: setup_state(&self.draft),
            account_unrestricted,
            mailbox_metadata,
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
        mailbox_aliases_configured: false,
        analysis_policy: AnalysisPolicy {
            timezone: org.default_timezone.clone(),
            internal_domains: vec![domain],
            responder_emails: vec![],
            mailbox_aliases: vec![],
            valid_request_criteria: vec![],
            non_responsibility_rules: vec![],
            ignored_senders: vec![],
            ignored_domains: vec![],
            ignored_keywords: vec![],
            valid_signal_keywords: vec![],
            count_historical_closures_as_valid: false,
            count_previous_request_followups_as_valid: false,
            count_org_hosted_training_as_valid: true,
            include_labels: vec![],
            exclude_labels: vec![],
            default_time_from: "00:00".to_string(),
            default_time_to: "23:59".to_string(),
            max_threads_per_run: 50,
        },
        // Modelo opt-out: las orgs nuevas nacen con IA activa (consentimiento sellado al
        // provisionar). El usuario puede desactivarla proactivamente en Configuración.
        ai_policy: default_ai_policy(true, Some(now)),
        schedule_report_policy: ScheduleReportPolicy {
            scheduler_enabled: false,
            preset: SchedulePreset::Weekdays08Local,
            timezone: org.default_timezone.clone(),
            analysis_time: default_analysis_time(),
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
        prompt_version: DEFAULT_PROMPT_VERSION.to_string(),
        auto_apply_threshold: DEFAULT_AUTO_APPLY_THRESHOLD,
        manual_review_threshold: 0.72,
        max_audit_messages: 4,
        max_body_chars_per_message: 280,
        allowed_fields: vec![
            "headers".to_string(),
            "participants".to_string(),
            "snippet".to_string(),
            "body_excerpt".to_string(),
        ],
        ai_defaults_applied: true,
    }
}

/// Aplica el default "IA activa" (opt-out) a una política existente. Idempotente:
/// corre una sola vez por org (marca `ai_defaults_applied`). Enciende la IA solo si
/// estaba apagada y no había sido migrada; tras esto, un opt-out posterior del usuario
/// persiste porque el flag ya quedó en true. Devuelve `true` si cambió algo (para persistir).
pub fn apply_ai_defaults_migration(ai: &mut AiPolicy, now: DateTime<Utc>) -> bool {
    if ai.ai_defaults_applied {
        return false;
    }
    ai.ai_defaults_applied = true;
    if !ai.enabled {
        ai.enabled = true;
        ai.consent_granted_at = ai.consent_granted_at.or(Some(now));
    }
    true
}

/// Migra solo el antiguo valor fijo de la interfaz; umbrales personalizados se
/// conservan.
pub fn apply_ai_threshold_migration(ai: &mut AiPolicy) -> bool {
    if ai.auto_apply_threshold != LEGACY_AUTO_APPLY_THRESHOLD {
        return false;
    }
    ai.auto_apply_threshold = DEFAULT_AUTO_APPLY_THRESHOLD;
    true
}

pub fn apply_ai_prompt_version_migration(ai: &mut AiPolicy) -> bool {
    if ai.prompt_version != LEGACY_PROMPT_VERSION {
        return false;
    }
    ai.prompt_version = DEFAULT_PROMPT_VERSION.to_string();
    true
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_orgs_default_to_ai_enabled() {
        let bundle = provision_default_config("agente@cliente.cl", Utc::now());
        assert!(bundle.draft.ai_policy.enabled);
        assert!(bundle.draft.ai_policy.consent_granted_at.is_some());
        assert!(bundle.draft.ai_policy.ai_defaults_applied);
        assert_eq!(
            bundle.draft.ai_policy.auto_apply_threshold,
            DEFAULT_AUTO_APPLY_THRESHOLD
        );
        assert_eq!(bundle.draft.schedule_report_policy.analysis_time, "08:00");
    }

    #[test]
    fn legacy_schedule_policy_defaults_to_eight() {
        let bundle = provision_default_config("agente@cliente.cl", Utc::now());
        let mut value =
            serde_json::to_value(&bundle.draft.schedule_report_policy).expect("serialize policy");
        value
            .as_object_mut()
            .expect("policy object")
            .remove("analysis_time");

        let restored: ScheduleReportPolicy =
            serde_json::from_value(value).expect("deserialize legacy policy");
        assert_eq!(restored.analysis_time, "08:00");
    }

    #[test]
    fn legacy_analysis_policy_gets_window_counting_defaults() {
        let bundle = provision_default_config("agente@cliente.cl", Utc::now());
        let mut value =
            serde_json::to_value(&bundle.draft.analysis_policy).expect("serialize policy");
        let object = value.as_object_mut().expect("policy object");
        object.remove("count_historical_closures_as_valid");
        object.remove("count_previous_request_followups_as_valid");
        object.remove("count_org_hosted_training_as_valid");

        let restored: AnalysisPolicy =
            serde_json::from_value(value).expect("deserialize legacy policy");
        assert!(!restored.count_historical_closures_as_valid);
        assert!(!restored.count_previous_request_followups_as_valid);
        assert!(restored.count_org_hosted_training_as_valid);
    }

    #[test]
    fn ai_defaults_migration_enables_legacy_off_orgs_once_and_respects_opt_out() {
        let now = Utc::now();
        // Org previa al cambio: IA apagada y sin el flag de migración.
        let mut ai = default_ai_policy(false, None);
        ai.ai_defaults_applied = false;

        // Primera carga: la migración la enciende y sella el consentimiento.
        assert!(apply_ai_defaults_migration(&mut ai, now));
        assert!(ai.enabled);
        assert!(ai.consent_granted_at.is_some());
        assert!(ai.ai_defaults_applied);

        // El usuario decide desactivarla proactivamente (opt-out).
        ai.enabled = false;
        ai.consent_granted_at = None;

        // Cargas posteriores NO la vuelven a encender: el opt-out persiste.
        assert!(!apply_ai_defaults_migration(&mut ai, now));
        assert!(!ai.enabled);
    }

    #[test]
    fn ai_threshold_migration_only_replaces_the_legacy_default() {
        let mut legacy = default_ai_policy(true, None);
        legacy.auto_apply_threshold = LEGACY_AUTO_APPLY_THRESHOLD;
        assert!(apply_ai_threshold_migration(&mut legacy));
        assert_eq!(legacy.auto_apply_threshold, DEFAULT_AUTO_APPLY_THRESHOLD);
        assert!(!apply_ai_threshold_migration(&mut legacy));

        legacy.auto_apply_threshold = 0.8;
        assert!(!apply_ai_threshold_migration(&mut legacy));
        assert_eq!(legacy.auto_apply_threshold, 0.8);
    }

    #[test]
    fn ai_prompt_migration_only_replaces_v1() {
        let mut ai = default_ai_policy(true, None);
        ai.prompt_version = LEGACY_PROMPT_VERSION.to_string();
        assert!(apply_ai_prompt_version_migration(&mut ai));
        assert_eq!(ai.prompt_version, DEFAULT_PROMPT_VERSION);
        assert!(!apply_ai_prompt_version_migration(&mut ai));
    }
}
