import json

import httpx
import pytest

from ai_worker.bedrock import audit_with_bedrock
from ai_worker.schemas import AuditThreadRequest
from ai_worker.settings import Settings


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


@pytest.mark.asyncio
async def test_audit_parses_bedrock_response() -> None:
    async def handler(request: httpx.Request) -> httpx.Response:
        assert request.headers["authorization"] == "Bearer token"
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
            Settings(AWS_BEARER_TOKEN_BEDROCK="token"),
            client,
        )

    assert result.confidence == 0.94
    assert result.input_tokens == 123
    assert result.output_tokens == 45

