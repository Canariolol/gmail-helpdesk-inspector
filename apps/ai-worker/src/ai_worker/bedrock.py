from __future__ import annotations

import json
import re

import httpx

from ai_worker.schemas import AuditThreadRequest, AuditThreadResponse, ClaudeDecision
from ai_worker.settings import Settings


SYSTEM_PROMPT = """You audit Gmail helpdesk metrics. Return strict JSON only.
Rules:
- Do not invent messages.
- Use only provided message ids.
- If evidence is insufficient, classify as ambiguous.
- Do not treat automated/no-reply messages as human client requests.
- Focus on helpdesk performance: first received client message, first internal reply, and last internal sent message.
- The provided messages are intentionally limited to those audit milestones, not the whole thread.
- Decide whether the automatic classification should stand or be corrected.
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
        "system": [{"text": SYSTEM_PROMPT}],
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
    decision = ClaudeDecision.model_validate_json(extract_json_text(text))
    usage = raw.get("usage", {})
    return AuditThreadResponse(
        **decision.model_dump(),
        input_tokens=int(usage.get("inputTokens", 0)),
        output_tokens=int(usage.get("outputTokens", 0)),
    )
