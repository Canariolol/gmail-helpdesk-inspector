export type AnalysisStatus = "pending" | "running" | "completed" | "failed";
export type Classification =
  | "valid_client_request"
  | "internal"
  | "automated"
  | "newsletter"
  | "spam"
  | "misc"
  | "ambiguous";

export interface ClassificationBreakdown {
  valid_client_request: number;
  internal: number;
  automated: number;
  newsletter: number;
  spam: number;
  misc: number;
  ambiguous: number;
}

export interface Metrics {
  total_threads: number;
  valid_requests: number;
  answered: number;
  unanswered: number;
  ignored: number;
  ambiguous: number;
  pending_review: number;
  manual_overrides: number;
  avg_first_response_minutes: number | null;
  median_first_response_minutes: number | null;
  p90_first_response_minutes: number | null;
  avg_resolution_minutes: number | null;
  median_resolution_minutes: number | null;
  report_confidence: number;
  ai_input_tokens: number;
  ai_output_tokens: number;
  classification_breakdown?: ClassificationBreakdown;
}

export interface AnalysisRun {
  id: string;
  user_email: string;
  config: {
    date_from: string;
    date_to: string;
    time_from: string;
    time_to: string;
    timezone: string;
    internal_domains: string[];
    ignored_senders: string[];
    ignored_domains: string[];
    ignored_keywords: string[];
  };
  status: AnalysisStatus;
  progress_message: string;
  processed_threads: number;
  total_candidate_threads: number;
  metrics: Metrics;
  created_at: string;
  completed_at: string | null;
  error_message: string | null;
}

export interface EmailThread {
  id: string;
  analysis_run_id: string;
  gmail_thread_id: string;
  subject: string;
  classification: Classification;
  classification_source: string;
  classification_confidence: number;
  is_valid_client_request: boolean;
  is_answered: boolean;
  first_message_at?: string | null;
  first_client_message_id: string | null;
  first_internal_reply_message_id: string | null;
  last_internal_message_id: string | null;
  first_client_message_at: string | null;
  first_internal_reply_at: string | null;
  last_internal_message_at: string | null;
  response_time_minutes: number | null;
  resolution_time_minutes: number | null;
  manual_review_required: boolean;
  manual_override_applied: boolean;
  reasons: string[];
  notes: string | null;
  created_at: string;
}

export interface EmailMessage {
  id: string;
  from_email: string;
  from_name: string | null;
  to_emails: string[];
  cc_emails: string[];
  date: string;
  subject: string;
  snippet: string;
  is_internal: boolean;
  is_external: boolean;
  is_automated: boolean;
}

export interface ThreadDetail {
  thread: EmailThread;
  messages: EmailMessage[];
}

// ---- Account / Billing ----

export type BillingPlanId = "inicial" | "pro" | "equipo";
export type SubscriptionStatus = "pending" | "trialing" | "active" | "past_due" | "cancelled" | "expired";

export interface PlanLimits {
  mailboxes: number;
  members: number;
  runs_per_month: number;
  candidate_threads_per_month: number;
  ai_audited_threads_per_month: number;
  report_recipients: number;
  retention_days: number;
}

export interface BillingPlan {
  id: BillingPlanId;
  name: string;
  usd_reference_monthly: number;
  clp_monthly: number;
  trial_days: number;
  limits: PlanLimits;
  highlighted: boolean;
}

export interface EntitlementSnapshot {
  allowed: boolean;
  reason: string | null;
  subscription_status: SubscriptionStatus | null;
  plan: BillingPlan | null;
  cancel_at_period_end: boolean;
  current_period_end: string | null;
  trial_ends_at: string | null;
}

export interface AccountStatus {
  account_email: string;
  workos_user_id: string | null;
  org_id: string;
  gmail_connected: boolean;
  gmail_account_email: string | null;
  entitlement: EntitlementSnapshot;
}

export interface CheckoutSession {
  id: string;
  checkout_url: string | null;
  plan_id: BillingPlanId;
  status: "pending" | "provider_created" | "activated" | "failed";
  currency_id: "CLP";
  amount_clp: number;
  usd_reference_monthly: number;
  trial_days: number;
}

export interface CheckoutSessionResponse {
  session: CheckoutSession;
}

export interface UsageLedger {
  org_id: string;
  period_key: string;
  runs_created: number;
  candidate_threads: number;
  ai_audited_threads: number;
  updated_at: string;
}

export interface UsageResponse {
  period_key: string;
  usage: UsageLedger;
  limits: PlanLimits | null;
}

// ---- Org Config (SaaS setup) ----

export interface OrgInfo {
  id: string;
  name: string;
  default_timezone: string;
  locale: string;
}

export interface MembershipInfo {
  role: string;
}

export interface MailboxInfo {
  id: string;
  google_account_email: string;
  workspace_domain: string;
  gmail_scope_snapshot: string[];
}

export interface PolicyVersionInfo {
  id: string;
  version: number;
  policy_hash: string;
}

export interface AnalysisPolicy {
  timezone: string;
  internal_domains: string[];
  responder_emails: string[];
  mailbox_aliases: string[];
  valid_request_criteria: string[];
  non_responsibility_rules: string[];
  ignored_senders: string[];
  ignored_domains: string[];
  ignored_keywords: string[];
  default_time_from: string;
  default_time_to: string;
  max_threads_per_run: number;
}

export interface AiPolicy {
  enabled: boolean;
  consent_granted_at: string | null;
  provider: string;
  model_id: string;
  prompt_version: string;
  auto_apply_threshold: number;
  manual_review_threshold: number;
  max_audit_messages: number;
  max_body_chars_per_message: number;
  allowed_fields: string[];
}

export interface ReportContentPolicy {
  mode: "metrics_only" | "metrics_and_review_items";
  include_subjects: boolean;
  include_senders: boolean;
}

export interface ScheduleReportPolicy {
  scheduler_enabled: boolean;
  preset: string;
  timezone: string;
  report_recipients: string[];
  report_content: ReportContentPolicy;
  failure_notice_enabled: boolean;
}

export interface RetentionPolicy {
  retention_days: number;
  delete_threads_and_messages: boolean;
  delete_ai_audits: boolean;
}

export interface PolicyDraft {
  analysis_policy: AnalysisPolicy;
  ai_policy: AiPolicy;
  schedule_report_policy: ScheduleReportPolicy;
  retention_policy: RetentionPolicy;
}

export interface SetupState {
  ready_for_analysis: boolean;
  missing: string[];
}

export interface GmailLabel {
  id: string;
  name: string;
  label_type?: string;
}

export interface GmailSendAs {
  email: string;
  display_name?: string;
  is_primary?: boolean;
  is_default?: boolean;
  treat_as_alias?: boolean;
}

export interface GmailProfile {
  email_address: string;
  messages_total?: number;
  threads_total?: number;
}

export interface MailboxMetadata {
  profile?: GmailProfile | null;
  labels: GmailLabel[];
  send_as: GmailSendAs[];
  filters_count: number;
  synced_at: string;
}

export interface FilterPreset {
  id: string;
  owner_email: string;
  name: string;
  include_labels: string[];
  exclude_labels: string[];
  ignored_senders: string[];
  ignored_domains: string[];
  ignored_keywords: string[];
  is_default: boolean;
  created_at: string;
  updated_at: string;
}

export interface OrgConfig {
  org: OrgInfo;
  membership: MembershipInfo;
  mailbox: MailboxInfo;
  policy_version: PolicyVersionInfo | null;
  draft: PolicyDraft;
  setup_state: SetupState;
  /** Cuenta interna privilegiada: la UI nunca bloquea por gating de setup. */
  account_unrestricted?: boolean;
  /** Metadata de Gmail leída al conectar (etiquetas, alias, perfil). */
  mailbox_metadata?: MailboxMetadata | null;
}

export interface PutConfigResponse {
  policy_version: PolicyVersionInfo;
  setup_state: SetupState;
}

// ---- Privacy & Data Summary ----

export interface DataSummaryAccount {
  google_account_email: string;
  gmail_scope_snapshot: string[];
  mailbox_connected: boolean;
  mailbox_revoked_at: string | null;
}

export interface DataSummaryOrg {
  id: string;
  name: string;
  role: string;
  policy_version: number;
  setup_ready: boolean;
  setup_missing: string[];
}

export interface DataSummaryPrivacy {
  data_minimization_mode: string;
  ai_enabled: boolean;
  ai_consent_granted_at: string | null;
  retention_days: number;
  report_mode: "metrics_only" | "metrics_and_review_items";
}

export interface DataSummaryStoredData {
  analysis_runs_count: number;
  threads_count: number;
  messages_count: number;
  ai_audit_records_count: number | null;
}

export interface DataSummaryAction {
  available: boolean;
  reason: string;
}

export interface DataSummaryActions {
  disconnect_gmail: DataSummaryAction;
  delete_analysis_data: DataSummaryAction;
  delete_account_data: DataSummaryAction;
}

export interface DataSummary {
  account: DataSummaryAccount;
  org: DataSummaryOrg;
  privacy: DataSummaryPrivacy;
  stored_data: DataSummaryStoredData;
  actions: DataSummaryActions;
}

// ---- Operations / Scheduler Status ----

export type ScheduleRunStatus = "running" | "completed" | "failed";

export interface ScheduleState {
  user_email: string;
  window_date_from: string;
  window_date_to: string;
  status: ScheduleRunStatus;
  run_id: string | null;
  email_sent: boolean;
  error_message: string | null;
  started_at: string;
  updated_at: string;
}

export interface OperationsHistoryEntry {
  id: string;
  kind: "analysis_run" | "scheduler_attempt" | string;
  status: string;
  started_at: string;
  finished_at: string | null;
  run_id: string | null;
  trigger_type: string | null;
  window_date_from: string | null;
  window_date_to: string | null;
  processed_threads: number | null;
  total_candidate_threads: number | null;
  error_category: string | null;
  error_redacted: string | null;
}

export interface OperationsHistory {
  entries: OperationsHistoryEntry[];
  total_count: number;
}

export interface OperationsStatus {
  scheduler: {
    enabled: boolean;
    timezone: string;
    preset: string;
    recipients_count: number;
    next_run_estimate: string | null;
    last_state: ScheduleState | null;
    last_error_redacted: string | null;
  };
  policy: {
    org_id: string;
    policy_version: number;
    policy_hash: string;
    setup_ready: boolean;
    setup_missing: string[];
  };
}
