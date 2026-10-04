import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";

async function loadTypescript(relativePath) {
  const source = await readFile(new URL(relativePath, import.meta.url), "utf8");
  const result = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext } });
  return import(`data:text/javascript;base64,${Buffer.from(result.outputText).toString("base64")}`);
}

const { initDraft, buildPutBody } = await loadTypescript("../src/lib/configuration.ts");
const { todayInHelpdeskTz } = await loadTypescript("../src/components/filters/dateUtils.ts");
const { isInboxLabel, labelToken, isAnalyzableLabel } = await loadTypescript("../src/components/filters/labels.ts");
const { deriveAccessState } = await loadTypescript("../src/views/access/accessState.ts");

test("a new tenant requires an explicit AI authorization", () => {
  const body = buildPutBody(initDraft(null));
  assert.equal(body.ai_policy.enabled, false);
  assert.equal(body.ai_policy.consent_confirmed, false);
});

test("saving settings preserves hidden limits, recipients, responders and report privacy choices", () => {
  const config = {
    org: { name: "Support", default_timezone: "Europe/Madrid" },
    mailbox: { google_account_email: "support@example.test" },
    draft: {
      analysis_policy: {
        internal_domains: ["example.test"], responder_emails: ["agent@example.test"], request_scope: "internal",
        mailbox_aliases: [], valid_request_criteria: ["Access requests, except billing changes"], non_responsibility_rules: [],
        ignored_senders: ["alerts@example.test"], ignored_domains: [], ignored_keywords: [],
        max_threads_per_run: 200, default_time_from: "07:00", default_time_to: "19:00",
      },
      ai_policy: { enabled: false, auto_apply_threshold: 0.9, manual_review_threshold: 0.7, max_audit_messages: 8, max_body_chars_per_message: 500 },
      schedule_report_policy: { scheduler_enabled: false, analysis_time: "09:35", days_of_week: [2, 6], report_recipients: ["supervisor@example.test"], report_content: { mode: "metrics_and_review_items", include_subjects: true, include_senders: false } },
      retention_policy: { retention_days: 60 },
    },
  };
  const body = buildPutBody(initDraft(config));
  assert.deepEqual(body.analysis_policy.responder_emails, ["agent@example.test"]);
  assert.deepEqual(body.analysis_policy.ignored_senders, ["alerts@example.test"]);
  assert.equal(body.analysis_policy.request_scope, "internal");
  assert.deepEqual(body.analysis_policy.valid_request_criteria, ["Access requests, except billing changes"]);
  assert.equal(Object.hasOwn(body.analysis_policy, "max_threads_per_run"), false);
  assert.equal(Object.hasOwn(body.analysis_policy, "default_time_from"), false);
  assert.deepEqual(body.schedule_report_policy.report_recipients, ["supervisor@example.test"]);
  assert.deepEqual(body.schedule_report_policy.days_of_week, [2, 6]);
  assert.equal(body.schedule_report_policy.analysis_time, "09:35");
  assert.equal(body.schedule_report_policy.report_content.include_subjects, true);
  assert.equal(body.schedule_report_policy.report_content.include_senders, false);
});

test("today uses the tenant timezone across a calendar-day boundary", () => {
  const ActualDate = globalThis.Date;
  globalThis.Date = class extends ActualDate { constructor(...args) { super(...(args.length ? args : ["2026-06-01T01:00:00Z"])); } };
  try {
    assert.equal(todayInHelpdeskTz("America/Santiago"), "2026-05-31");
    assert.equal(todayInHelpdeskTz("Europe/Madrid"), "2026-06-01");
  } finally { globalThis.Date = ActualDate; }
});

test("Graph and IMAP inboxes retain provider IDs and exclude non-mail folders", () => {
  const inbox = { id: "provider-immutable-folder-id", name: "Entrada", label_type: "inbox" };
  assert.equal(isInboxLabel(inbox), true);
  assert.equal(labelToken(inbox), inbox.id);
  assert.equal(isAnalyzableLabel({ id: "Trash", name: "Eliminados", label_type: "trash" }), false);
});

test("revoked mailbox authorization requires reconnection while a paid cancellation keeps access", () => {
  const account = { entitlement: { allowed: true, subscription_status: "cancelled" }, gmail_connected: true, mailbox_needs_reauth: false };
  assert.deepEqual(deriveAccessState(account), { kind: "ready" });
  assert.deepEqual(deriveAccessState({ ...account, mailbox_needs_reauth: true }), { kind: "connect_gmail" });
});
