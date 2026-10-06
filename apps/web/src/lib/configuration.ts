import type { AnalysisRun, OrgConfig, RequestScope } from "../api/types";

export function buildRetryAnalysisPayload(config: AnalysisRun["config"]) {
  return {
    date_from: config.date_from,
    date_to: config.date_to,
    time_from: config.time_from,
    time_to: config.time_to,
  };
}

export type WizardDraft = {
  orgName: string;
  orgTimezone: string;
  internalDomainsText: string;
  responderEmailsText: string;
  requestScope: RequestScope;
  mailboxAliasesText: string;
  validCriteriaText: string;
  nonResponsibilityText: string;
  ignoredDomainsText: string;
  ignoredSendersText: string;
  ignoredKeywordsText: string;
  validSignalKeywordsText: string;
  countHistoricalClosuresAsValid: boolean;
  countPreviousRequestFollowupsAsValid: boolean;
  countOrgHostedTrainingAsValid: boolean;
  aiEnabled: boolean;
  aiConsentChecked: boolean;
  autoApplyThreshold: number;
  manualReviewThreshold: number;
  maxAuditMessages: number;
  maxBodyCharsPerMessage: number;
  schedulerEnabled: boolean;
  analysisTime: string;
  daysOfWeek: number[];
  reportRecipientsText: string;
  reportMode: "metrics_only" | "metrics_and_review_items";
  includeReportSubjects: boolean;
  includeReportSenders: boolean;
  retentionDays: number;
};

export function splitLines(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((s) => s.trim())
    .filter(Boolean);
}

export function initDraft(config: OrgConfig | null): WizardDraft {
  if (!config) {
    return {
      orgName: "",
      orgTimezone: "America/Santiago",
      internalDomainsText: "",
      responderEmailsText: "",
      requestScope: "external",
      mailboxAliasesText: "",
      validCriteriaText: "",
      nonResponsibilityText: "",
      ignoredDomainsText: "google.com\ncalendar.google.com",
      ignoredSendersText: "",
      ignoredKeywordsText: "newsletter\nboletín\npromoción",
      validSignalKeywordsText: "",
      countHistoricalClosuresAsValid: false,
      countPreviousRequestFollowupsAsValid: false,
      countOrgHostedTrainingAsValid: true,
      aiEnabled: false,
      aiConsentChecked: false,
      autoApplyThreshold: 0.92,
      manualReviewThreshold: 0.72,
      maxAuditMessages: 4,
      maxBodyCharsPerMessage: 280,
      schedulerEnabled: false,
      analysisTime: "08:00",
      daysOfWeek: [1, 2, 3, 4, 5],
      reportRecipientsText: "",
      reportMode: "metrics_only",
      includeReportSubjects: false,
      includeReportSenders: false,
      retentionDays: 30,
    };
  }
  const { draft, org } = config;
  const detectedAliases = (config.mailbox_metadata?.send_as ?? [])
    .map((address) => address.email.trim())
    .filter(
      (email) =>
        email &&
        email.toLowerCase() !== config.mailbox.google_account_email.toLowerCase(),
    );
  const mailboxAliases =
    draft.mailbox_aliases_configured || draft.analysis_policy.mailbox_aliases.length > 0
      ? draft.analysis_policy.mailbox_aliases
      : detectedAliases;
  return {
    orgName: org.name,
    orgTimezone: org.default_timezone,
    internalDomainsText: draft.analysis_policy.internal_domains.join("\n"),
    responderEmailsText: draft.analysis_policy.responder_emails.join("\n"),
    requestScope: draft.analysis_policy.request_scope ?? "external",
    mailboxAliasesText: mailboxAliases.join("\n"),
    validCriteriaText: draft.analysis_policy.valid_request_criteria.join("\n"),
    nonResponsibilityText: draft.analysis_policy.non_responsibility_rules.join("\n"),
    ignoredDomainsText: draft.analysis_policy.ignored_domains.join("\n"),
    ignoredSendersText: draft.analysis_policy.ignored_senders.join("\n"),
    ignoredKeywordsText: draft.analysis_policy.ignored_keywords.join("\n"),
    validSignalKeywordsText: (draft.analysis_policy.valid_signal_keywords ?? []).join("\n"),
    countHistoricalClosuresAsValid:
      draft.analysis_policy.count_historical_closures_as_valid ?? false,
    countPreviousRequestFollowupsAsValid:
      draft.analysis_policy.count_previous_request_followups_as_valid ?? false,
    countOrgHostedTrainingAsValid:
      draft.analysis_policy.count_org_hosted_training_as_valid ?? true,
    aiEnabled: draft.ai_policy.enabled,
    aiConsentChecked: draft.ai_policy.enabled,
    autoApplyThreshold: draft.ai_policy.auto_apply_threshold,
    manualReviewThreshold: draft.ai_policy.manual_review_threshold,
    maxAuditMessages: draft.ai_policy.max_audit_messages,
    maxBodyCharsPerMessage: draft.ai_policy.max_body_chars_per_message,
    schedulerEnabled: draft.schedule_report_policy.scheduler_enabled,
    analysisTime: draft.schedule_report_policy.analysis_time ?? "08:00",
    daysOfWeek: draft.schedule_report_policy.days_of_week ?? [1, 2, 3, 4, 5],
    reportRecipientsText: draft.schedule_report_policy.report_recipients.join("\n"),
    reportMode: draft.schedule_report_policy.report_content.mode,
    includeReportSubjects: draft.schedule_report_policy.report_content.include_subjects,
    includeReportSenders: draft.schedule_report_policy.report_content.include_senders,
    retentionDays: draft.retention_policy.retention_days,
  };
}

export function buildPutBody(d: WizardDraft, finalize = false) {
  return {
    finalize,
    org: { name: d.orgName.trim(), default_timezone: d.orgTimezone },
    analysis_policy: {
      timezone: d.orgTimezone,
      internal_domains: splitLines(d.internalDomainsText),
      responder_emails: splitLines(d.responderEmailsText),
      request_scope: d.requestScope,
      mailbox_aliases: splitLines(d.mailboxAliasesText),
      valid_request_criteria: splitLines(d.validCriteriaText),
      non_responsibility_rules: splitLines(d.nonResponsibilityText),
      ignored_senders: splitLines(d.ignoredSendersText),
      ignored_domains: splitLines(d.ignoredDomainsText),
      ignored_keywords: splitLines(d.ignoredKeywordsText),
      valid_signal_keywords: splitLines(d.validSignalKeywordsText),
      count_historical_closures_as_valid: d.countHistoricalClosuresAsValid,
      count_previous_request_followups_as_valid: d.countPreviousRequestFollowupsAsValid,
      count_org_hosted_training_as_valid: d.countOrgHostedTrainingAsValid,
    },
    ai_policy: {
      enabled: d.aiEnabled,
      consent_confirmed: d.aiConsentChecked,
      auto_apply_threshold: d.autoApplyThreshold,
      manual_review_threshold: d.manualReviewThreshold,
      max_audit_messages: d.maxAuditMessages,
      max_body_chars_per_message: d.maxBodyCharsPerMessage,
    },
    schedule_report_policy: {
      scheduler_enabled: d.schedulerEnabled,
      timezone: d.orgTimezone,
      analysis_time: d.analysisTime,
      days_of_week: d.daysOfWeek,
      report_recipients: splitLines(d.reportRecipientsText),
      report_content: { mode: d.reportMode, include_subjects: d.includeReportSubjects, include_senders: d.includeReportSenders },
    },
    retention_policy: { retention_days: d.retentionDays },
  };
}
