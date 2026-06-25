import json

import httpx
import pytest

from ai_worker.bedrock import (
    audit_batch_with_bedrock,
    audit_with_bedrock,
    build_batch_user_prompt,
    build_system_prompt,
)
from ai_worker.schemas import AuditPolicyContext, AuditThreadRequest, BatchAuditRequest
from ai_worker.settings import Settings


NOVA_2_LITE_MODEL_ID = "amazon.nova-2-lite-v1:0"


def payload() -> AuditThreadRequest:
    return AuditThreadRequest.model_validate(
        {
            "thread": {
                "id": "t1",
                "analysis_run_id": "r1",
                "gmail_thread_id": "g1",
                "subject": "Ayuda",
                "normalized_subject": "ayuda",
                "classification": "valid_client_request",
                "classification_source": "heuristics",
                "classification_confidence": 0.86,
                "is_valid_client_request": True,
                "is_answered": True,
                "manual_review_required": False,
                "reasons": [],
            },
            "messages": [
                {
                    "id": "m1",
                    "gmail_message_id": "m1",
                    "from_email": "client@example.com",
                    "date": "2026-06-12T10:00:00Z",
                    "subject": "Ayuda",
                    "snippet": "hola",
                    "headers": {},
                    "is_internal": False,
                    "is_external": True,
                    "is_automated": False,
                    "body_text": "Necesito ayuda",
                }
            ],
        }
    )


def batch_payload() -> BatchAuditRequest:
    return BatchAuditRequest.model_validate(
        {
            "policy_context": {
                "mailbox_email": "help@acme.test",
                "internal_domains": ["acme.test"],
                "valid_request_criteria": ["Clientes externos piden soporte"],
            },
            "threads": [
                {
                    "thread_id": "g1",
                    "subject": "Ayuda",
                    "automatic_classification": "valid_client_request",
                    "automatic_confidence": 0.86,
                    "automatic_is_valid": True,
                    "automatic_is_answered": False,
                    "automatic_manual_review_required": False,
                    "messages": [
                        {
                            "message_id": "m1",
                            "from_email": "client@example.com",
                            "date": "2026-06-12T10:00:00Z",
                            "is_internal": False,
                            "is_external": True,
                            "is_automated": False,
                            "excerpt": "Necesito ayuda",
                        }
                    ],
                }
            ],
        }
    )


@pytest.mark.asyncio
async def test_audit_parses_bedrock_response() -> None:
    async def handler(request: httpx.Request) -> httpx.Response:
        assert request.headers["authorization"] == "Bearer token"
        assert request.url == httpx.URL(
            "https://bedrock-runtime.us-east-1.amazonaws.com"
            "/model/amazon.nova-2-lite-v1%3A0/converse"
        )
        return httpx.Response(
            200,
            json={
                "output": {
                    "message": {
                        "content": [
                            {
                                "text": json.dumps(
                                    {
                                        "classification": "valid_client_request",
                                        "is_valid_client_request": True,
                                        "is_answered": True,
                                        "first_client_message_id": "m1",
                                        "first_internal_reply_message_id": None,
                                        "last_internal_message_id": None,
                                        "confidence": 0.94,
                                        "manual_review_required": False,
                                        "issues": [],
                                    }
                                )
                            }
                        ]
                    }
                },
                "usage": {"inputTokens": 123, "outputTokens": 45},
            },
        )

    transport = httpx.MockTransport(handler)
    async with httpx.AsyncClient(transport=transport) as client:
        result = await audit_with_bedrock(
            payload(),
            Settings(
                AWS_BEARER_TOKEN_BEDROCK="token",
                BEDROCK_MODEL_ID=NOVA_2_LITE_MODEL_ID,
            ),
            client,
        )

    assert result.confidence == 0.94
    assert result.input_tokens == 123
    assert result.output_tokens == 45


def test_settings_default_model_is_sonnet() -> None:
    assert Settings.model_fields["bedrock_model_id"].default == "us.anthropic.claude-sonnet-4-6"


def test_system_prompt_has_no_tenant_hardcodes_by_default() -> None:
    prompt = build_system_prompt(Settings())
    forbidden = ["west-ingenieria", "West Ingeniería", "catherine.trivino"]
    for value in forbidden:
        assert value not in prompt


def test_system_prompt_uses_request_policy_context() -> None:
    prompt = build_system_prompt(
        Settings(),
        AuditPolicyContext(
            mailbox_email="soporte@acme.test",
            mailbox_display_name="Soporte Acme",
            workspace_domain="acme.test",
            internal_domains=["acme.test"],
            responder_emails=["agent@acme.test"],
            mailbox_aliases=["help@acme.test"],
            valid_request_criteria=["Clientes externos piden soporte de plataforma"],
            non_responsibility_rules=["Facturación no es responsabilidad de soporte"],
            ignored_domains=["calendar.google.com"],
            prompt_version="test_v1",
        ),
    )
    assert "soporte@acme.test" in prompt
    assert "help@acme.test" in prompt
    assert "Clientes externos piden soporte de plataforma" in prompt
    assert "Facturación no es responsabilidad de soporte" in prompt


@pytest.mark.asyncio
async def test_audit_parses_fenced_json_response() -> None:
    decision = {
        "classification": "valid_client_request",
        "is_valid_client_request": True,
        "is_answered": False,
        "first_client_message_id": "m1",
        "first_internal_reply_message_id": None,
        "last_internal_message_id": None,
        "confidence": 0.92,
        "manual_review_required": True,
        "issues": ["No internal reply found."],
    }

    async def handler(_request: httpx.Request) -> httpx.Response:
        return httpx.Response(
            200,
            json={
                "output": {
                    "message": {
                        "content": [
                            {
                                "text": f"```json\n{json.dumps(decision)}\n```",
                            }
                        ]
                    }
                },
                "usage": {"inputTokens": 100, "outputTokens": 20},
            },
        )

    transport = httpx.MockTransport(handler)
    async with httpx.AsyncClient(transport=transport) as client:
        result = await audit_with_bedrock(
            payload(),
            Settings(AWS_BEARER_TOKEN_BEDROCK="token"),
            client,
        )

    assert result.confidence == 0.92
    assert result.manual_review_required is True
    assert result.input_tokens == 100
    assert result.output_tokens == 20


def test_batch_prompt_uses_one_compact_excerpt_per_message() -> None:
    prompt = build_batch_user_prompt(batch_payload())
    body = json.loads(prompt)
    message = body["threads"][0]["messages"][0]
    assert message["excerpt"] == "Necesito ayuda"
    assert "snippet" not in message
    assert "text" not in message
    assert "policy_context" not in body


@pytest.mark.asyncio
async def test_batch_audit_parses_decisions_and_uses_batch_model() -> None:
    async def handler(request: httpx.Request) -> httpx.Response:
        assert request.url == httpx.URL(
            "https://bedrock-runtime.us-east-1.amazonaws.com"
            "/model/amazon.nova-2-lite-v1%3A0/converse"
        )
        request_json = json.loads(request.content)
        assert len(request_json["system"]) == 1
        return httpx.Response(
            200,
            json={
                "output": {
                    "message": {
                        "content": [
                            {
                                "text": json.dumps(
                                    {
                                        "decisions": [
                                            {
                                                "thread_id": "g1",
                                                "classification": "valid_client_request",
                                                "is_valid_client_request": True,
                                                "is_answered": False,
                                                "confidence": 0.95,
                                                "manual_review_required": False,
                                                "issues": [],
                                            }
                                        ]
                                    }
                                )
                            }
                        ]
                    }
                },
                "usage": {"inputTokens": 321, "outputTokens": 54},
            },
        )

    transport = httpx.MockTransport(handler)
    async with httpx.AsyncClient(transport=transport) as client:
        result = await audit_batch_with_bedrock(
            batch_payload(),
            Settings(
                AWS_BEARER_TOKEN_BEDROCK="token",
                BEDROCK_BATCH_MODEL_ID=NOVA_2_LITE_MODEL_ID,
            ),
            client,
        )

    assert result.decisions[0].thread_id == "g1"
    assert result.input_tokens == 321
    assert result.output_tokens == 54
