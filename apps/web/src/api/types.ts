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

export interface OrgConfig {
  org: OrgInfo;
  membership: MembershipInfo;
  mailbox: MailboxInfo;
  policy_version: PolicyVersionInfo | null;
  draft: PolicyDraft;
  setup_state: SetupState;
}

export interface PutConfigResponse {
  policy_version: PolicyVersionInfo;
  setup_state: SetupState;
}
