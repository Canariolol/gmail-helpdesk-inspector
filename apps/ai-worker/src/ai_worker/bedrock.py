from __future__ import annotations

import json
import re

import httpx

from ai_worker.schemas import AuditPolicyContext, AuditThreadRequest, AuditThreadResponse, BedrockDecision
from ai_worker.settings import Settings


def _bullet_list(values: list[str], fallback: str) -> str:
    cleaned = [value.strip() for value in values if value.strip()]
    if not cleaned:
        return fallback
    return "\n".join(f"  - {value}" for value in cleaned)


def _fallback_policy_context(settings: Settings) -> AuditPolicyContext:
    internal_domains = [settings.internal_domain] if settings.internal_domain else []
    responder_emails = [settings.desk_mailbox] if settings.desk_mailbox else []
    mailbox_email = settings.analyzed_mailbox or settings.desk_mailbox
    return AuditPolicyContext(
        mailbox_email=mailbox_email,
        mailbox_display_name=mailbox_email,
        workspace_domain=settings.internal_domain,
        internal_domains=internal_domains,
        responder_emails=responder_emails,
        valid_request_criteria=[
            "External human clients ask the configured support/helpdesk team for help, service, access, incident handling, or follow-up."
        ],
        non_responsibility_rules=[
            "Messages clearly addressed to another team, person, vendor, newsletter, spam, or automated system are not valid client requests for this helpdesk."
        ],
        prompt_version="legacy_fallback_v1",
    )


def build_system_prompt(settings: Settings, policy: AuditPolicyContext | None = None) -> str:
    policy = policy or _fallback_policy_context(settings)
    mailbox_labels = [policy.mailbox_email, *policy.mailbox_aliases]
    responder_labels = [*policy.responder_emails]
    internal_labels = [*policy.internal_domains]
    return f"""You audit one email thread for a Gmail inbox used as a helpdesk/support mailbox. Your job is to decide whether the thread is a VALID client request for the configured organization policy and whether it was answered. Return strict JSON only, matching the requested schema exactly.

Tenant policy context:
- Analyzed mailbox: {policy.mailbox_email or "not specified"}
- Mailbox display name: {policy.mailbox_display_name or "not specified"}
- Workspace/domain: {policy.workspace_domain or "not specified"}
- Mailbox aliases / support addresses:
{_bullet_list(mailbox_labels, "  - not specified")}
- Internal domains:
{_bullet_list(internal_labels, "  - infer only from is_internal/is_external flags")}
- Explicit responder emails:
{_bullet_list(responder_labels, "  - infer internal human responders from message flags")}

Valid request criteria from the organization policy:
{_bullet_list(policy.valid_request_criteria, "  - External human clients ask the configured helpdesk/support team for help, service, access, incident handling, or follow-up.")}

Out-of-scope / non-responsibility rules from the organization policy:
{_bullet_list(policy.non_responsibility_rules, "  - Messages clearly meant for another team/person, automated mail, newsletters, spam, or generic non-support topics are not valid client requests for this helpdesk.")}

Ignored hints from policy (use as supporting evidence, not as the only criterion):
- Ignored senders:
{_bullet_list(policy.ignored_senders, "  - none")}
- Ignored domains:
{_bullet_list(policy.ignored_domains, "  - none")}
- Ignored subject keywords:
{_bullet_list(policy.ignored_keywords, "  - none")}

Classification guidance:
- "valid_client_request": an external, human client asks for something this configured helpdesk should handle under the valid request criteria.
- "automated": automated/system/no-reply/notification mail.
- "newsletter": promotions, marketing or newsletters.
- "spam": spam.
- "internal": only internal staff, no external human client.
- "misc": clearly not a request this configured helpdesk owns.
- "ambiguous": evidence is insufficient or policy ownership is genuinely unclear.

Gmail label hints (field "gmail_labels", use as supporting evidence, not the sole criterion):
- CATEGORY_PROMOTIONS strongly suggests promotions/newsletter; CATEGORY_SOCIAL and CATEGORY_FORUMS suggest social/forum notifications rather than a support request.
- CATEGORY_PERSONAL, INBOX and IMPORTANT are neutral and do not by themselves indicate a valid request.

Answer guidance:
- is_answered=true only if a real HUMAN internal/responder reply after the client's relevant message exists.
- Automated acknowledgements and ticket auto-replies do NOT count as answered.
- Use only the message ids provided; never invent messages or ids.
- Pick first_client_message_id, first_internal_reply_message_id and last_internal_message_id from the provided ids when applicable, else null.
- The messages are compact excerpts; use "ambiguous" + manual_review_required=true if the evidence is insufficient.
- The automatic_classification field is only a prior hint from a rule-based pass; correct it freely.
- Write every string in the "issues" array in Spanish.
- Output must match the requested JSON schema exactly."""


FENCED_JSON_RE = re.compile(r"^\s*```(?:json)?\s*(.*?)\s*```\s*$", re.DOTALL | re.IGNORECASE)


def extract_json_text(text: str) -> str:
    match = FENCED_JSON_RE.match(text)
    if match:
        return match.group(1).strip()
    return text.strip()


def build_user_prompt(payload: AuditThreadRequest) -> str:
    compact_messages = [
        {
            "message_id": message.id,
            "from": message.from_email,
            "to": message.to_emails,
            "cc": message.cc_emails,
            "date": message.date.isoformat(),
            "subject": message.subject,
            "is_internal": message.is_internal,
            "is_external": message.is_external,
            "is_automated": message.is_automated,
            "snippet": message.snippet,
            "text": message.body_text or "",
        }
        for message in payload.messages
    ]
    return json.dumps(
        {
            "thread_id": payload.thread.gmail_thread_id,
            "subject": payload.thread.subject,
            "gmail_labels": payload.gmail_labels,
            "automatic_classification": {
                "classification": payload.thread.classification,
                "classification_source": payload.thread.classification_source,
                "confidence": payload.thread.classification_confidence,
                "is_valid_client_request": payload.thread.is_valid_client_request,
                "is_answered": payload.thread.is_answered,
                "last_internal_message_id": payload.thread.last_internal_message_id,
                "reasons": payload.thread.reasons,
            },
            "messages": compact_messages,
            "required_output_schema": {
                "classification": "valid_client_request|internal|automated|newsletter|spam|misc|ambiguous",
                "is_valid_client_request": "boolean",
                "is_answered": "boolean",
                "first_client_message_id": "string|null",
                "first_internal_reply_message_id": "string|null",
                "last_internal_message_id": "string|null",
                "confidence": "number 0..1",
                "manual_review_required": "boolean",
                "issues": "string[]",
            },
        },
        ensure_ascii=False,
    )


async def audit_with_bedrock(
    payload: AuditThreadRequest,
    settings: Settings,
    client: httpx.AsyncClient | None = None,
) -> AuditThreadResponse:
    if not settings.aws_bearer_token_bedrock:
        raise RuntimeError("AWS_BEARER_TOKEN_BEDROCK is required")

    request_body = {
        "system": [{"text": build_system_prompt(settings, payload.policy_context)}],
        "messages": [
            {
                "role": "user",
                "content": [{"text": build_user_prompt(payload)}],
            }
        ],
        "inferenceConfig": {
            "maxTokens": 1200,
            "temperature": 0,
        },
    }

    owns_client = client is None
    if client is None:
        client = httpx.AsyncClient(timeout=60)
    try:
        response = await client.post(
            settings.invoke_url,
            headers={
                "Authorization": f"Bearer {settings.aws_bearer_token_bedrock}",
                "Content-Type": "application/json",
                "Accept": "application/json",
            },
            json=request_body,
        )
        response.raise_for_status()
        raw = response.json()
    finally:
        if owns_client:
            await client.aclose()

    text = (
        raw.get("output", {})
        .get("message", {})
        .get("content", [{}])[0]
        .get("text", "")
    )
    decision = BedrockDecision.model_validate_json(extract_json_text(text))
    usage = raw.get("usage", {})
    return AuditThreadResponse(
        **decision.model_dump(),
        input_tokens=int(usage.get("inputTokens", 0)),
        output_tokens=int(usage.get("outputTokens", 0)),
    )
