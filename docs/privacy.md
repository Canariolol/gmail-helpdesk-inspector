# Privacy

This MVP is designed for private/local use against a Gmail account controlled by
the operator.

## Gmail Access

The only OAuth scope used is:

```text
https://www.googleapis.com/auth/gmail.readonly
```

The API does not call Gmail endpoints that send, delete, label, archive, or
modify messages.

## Stored Data

Firestore stores normalized metadata only:

- thread and message ids
- dates
- participants
- subject and snippets
- selected headers
- classification decisions and reasons
- audit findings
- aggregate metrics and AI token usage

Full email bodies are fetched temporarily during analysis and AI audit, sent as
plain text to the worker, and discarded after the audit response is processed.

## AI Audit

The Bedrock worker receives text-only thread content. Attachments, images,
base64 payloads, and multimodal artifacts are intentionally excluded.

