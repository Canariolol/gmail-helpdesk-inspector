from __future__ import annotations

import json
import re

import httpx

from ai_worker.schemas import AuditThreadRequest, AuditThreadResponse, BedrockDecision
from ai_worker.settings import Settings


def build_system_prompt(settings: Settings) -> str:
    return f"""You audit the email inbox of a Level-1 (N1) IT service desk ("Mesa de Servicio") at West Ingeniería, a Chilean technology company. The desk serves many external client companies; all email is in Spanish (Chilean). For one email thread you decide whether it is a VALID client request that this N1 desk should handle, and whether it was answered. Return strict JSON only, matching the requested schema exactly.

Context:
- The analyzed inbox is the desk supervisor's mailbox ({settings.analyzed_mailbox}), which receives a SUPERSET of the desk's mail. A message landing in this inbox is NOT automatically a desk request.
- The real desk address is {settings.desk_mailbox}. A thread is most likely for the desk when {settings.desk_mailbox} is a direct (To) recipient. If it only appears in CC, or the mail is addressed to a specific person or another area, be skeptical.
- Internal staff use @{settings.internal_domain} addresses. The N1 desk members are: {settings.desk_members}.

VALID desk request (classification "valid_client_request", is_valid_client_request=true):
- An external, human client (not an automated/system/no-reply sender) asks the desk for help: incidents, access/password problems, technical support for their products/systems (e.g. logistics/forestry systems, GPS equipment, latency), or a follow-up on such a request. The topic is broad, so do NOT reject a thread just because of its subject. Long or messy threads can still be valid.

NOT valid for this desk (set is_valid_client_request=false and pick the closest category):
- "automated": automated/system/no-reply/notification mail (GCP, AWS, calendars, mailer-daemon, monitoring).
- "newsletter": promotions, marketing or newsletters. "spam": spam.
- "internal": only internal staff, no external human client.
- "misc": a client who explicitly asks to deal with someone who is NOT an N1 desk member (Sales, a specific account manager, a named person not in the desk list), even if {settings.desk_mailbox} is CC'd — this is not the desk's responsibility. Also anything else that is clearly not a client support request.

Set manual_review_required=true (usually with classification "ambiguous") when:
- The client asks to talk to a person who is NOT an N1 desk member, BUT the mail also describes a genuine desk-type issue (access/system/GPS/etc.). Let a human decide; lean valid only if the desk-type issue is clearly the point.
- The thread was answered/handled by someone who is NOT an N1 desk member.
- Evidence is genuinely insufficient or ambiguous.

is_answered: true only if a real HUMAN reply from a desk member (internal, non-automated, after the client's message) exists. Automated acknowledgements ("hemos recibido su solicitud", ticket auto-replies) do NOT count as answered.

Output rules:
- Use only the message ids provided; never invent messages or ids. Pick first_client_message_id, first_internal_reply_message_id and last_internal_message_id from the provided ids when applicable, else null.
- The provided messages are a compact view with short body excerpts; this is enough to judge validity. Only use "ambiguous" + manual_review_required=true if the evidence is truly insufficient.
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
        "system": [{"text": build_system_prompt(settings)}],
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
