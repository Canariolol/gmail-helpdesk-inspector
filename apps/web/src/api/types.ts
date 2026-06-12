export type AnalysisStatus = "pending" | "running" | "completed" | "failed";
export type Classification =
  | "valid_client_request"
  | "internal"
  | "automated"
  | "newsletter"
  | "spam"
  | "misc"
  | "ambiguous";

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
  report_confidence: number;
  ai_input_tokens: number;
  ai_output_tokens: number;
}

export interface AnalysisRun {
  id: string;
  user_email: string;
  config: {
    date_from: string;
    date_to: string;
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
  first_client_message_id: string | null;
  first_internal_reply_message_id: string | null;
  response_time_minutes: number | null;
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

