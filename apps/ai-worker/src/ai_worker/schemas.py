from __future__ import annotations

from datetime import datetime
from typing import Literal

from pydantic import BaseModel, Field


Classification = Literal[
    "valid_client_request",
    "internal",
    "automated",
    "newsletter",
    "spam",
    "misc",
    "ambiguous",
]


class EmailThread(BaseModel):
    id: str
    analysis_run_id: str
    gmail_thread_id: str
    subject: str
    normalized_subject: str
    classification: Classification
    classification_source: str
    classification_confidence: float
    is_valid_client_request: bool
    is_answered: bool
    first_client_message_id: str | None = None
    first_internal_reply_message_id: str | None = None
    last_internal_message_id: str | None = None
    manual_review_required: bool
    reasons: list[str] = Field(default_factory=list)


class EmailMessage(BaseModel):
    id: str
    gmail_message_id: str
    from_email: str
    from_name: str | None = None
    to_emails: list[str] = Field(default_factory=list)
    cc_emails: list[str] = Field(default_factory=list)
    date: datetime
    subject: str
    snippet: str
    headers: dict = Field(default_factory=dict)
    is_internal: bool
    is_external: bool
    is_automated: bool
    body_text: str | None = None


class AuditPolicyContext(BaseModel):
    mailbox_email: str = ""
    mailbox_display_name: str = ""
    workspace_domain: str = ""
    internal_domains: list[str] = Field(default_factory=list)
    responder_emails: list[str] = Field(default_factory=list)
    mailbox_aliases: list[str] = Field(default_factory=list)
    valid_request_criteria: list[str] = Field(default_factory=list)
    non_responsibility_rules: list[str] = Field(default_factory=list)
    ignored_senders: list[str] = Field(default_factory=list)
    ignored_domains: list[str] = Field(default_factory=list)
    ignored_keywords: list[str] = Field(default_factory=list)
    prompt_version: str = ""
    allowed_fields: list[str] = Field(default_factory=list)


class AuditThreadRequest(BaseModel):
    thread: EmailThread
    messages: list[EmailMessage]
    policy_context: AuditPolicyContext | None = None


class AuditThreadResponse(BaseModel):
    classification: Classification
    is_valid_client_request: bool
    is_answered: bool
    first_client_message_id: str | None = None
    first_internal_reply_message_id: str | None = None
    last_internal_message_id: str | None = None
    confidence: float = Field(ge=0, le=1)
    manual_review_required: bool
    issues: list[str] = Field(default_factory=list)
    input_tokens: int = Field(ge=0)
    output_tokens: int = Field(ge=0)


class BedrockDecision(BaseModel):
    classification: Classification
    is_valid_client_request: bool
    is_answered: bool
    first_client_message_id: str | None = None
    first_internal_reply_message_id: str | None = None
    last_internal_message_id: str | None = None
    confidence: float = Field(ge=0, le=1)
    manual_review_required: bool
    issues: list[str] = Field(default_factory=list)
