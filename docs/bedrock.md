# Bedrock Setup

The AI worker uses Amazon Bedrock Runtime with bearer-token authentication.

Required environment:

```text
AWS_BEARER_TOKEN_BEDROCK=
AWS_REGION=us-east-1
BEDROCK_MODEL_ID=amazon.nova-2-lite-v1:0
BEDROCK_BATCH_MODEL_ID=
```

The worker uses the Bedrock Converse API and expects strict JSON back from the
model. `BEDROCK_BATCH_MODEL_ID` is optional and falls back to
`BEDROCK_MODEL_ID`.

## Batch audit contract

The API makes one `/audit/batch` request for each group of up to 20 eligible
threads. A thread contains `thread_id`, `subject`, `gmail_labels`, and at most
four unique messages in chronological order:

1. First relevant external human message.
2. First later internal human reply.
3. Last external human message from the first one onward.
4. Last internal human message after the first external message.

Each message contains only `message_id`, `from_email`, `date`, `is_internal`,
`is_automated`, and `content`. `content` uses `body_text`, or the Gmail
`snippet` only when the body is empty, and is truncated locally to the policy's
`max_body_chars_per_message` (280 characters by default). The historical
`max_audit_messages` setting remains in the policy contract, but the effective
limit is always four.

Each decision returns classification and answered state, plus
`first_client_message_id`, `first_internal_reply_message_id`, and
`last_internal_message_id`. The API applies a result automatically at confidence
`>= 0.92` when the model does not request review and all referenced IDs exist.
Results from `0.72` to `0.92`, or results that request review, are saved as an AI
proposal for manual confirmation. Lower-confidence, absent, invalid-ID, or
failed decisions preserve the local heuristic and require manual review without
an AI status suggestion.

The existing split-and-retry behavior remains in the API. The batch response
limit is 6000 output tokens; this is a ceiling, not reserved consumption.
Neither service logs email bodies or full AI payloads.

## Compatible rollout

`/audit/thread` remains available temporarily so older API revisions continue
to work. `/audit/batch` accepts the legacy message field `excerpt` as an alias
for `content`, allowing deployment in the order worker, API, then web. The new
API never calls `/audit/thread`; remove that endpoint only after old API
revisions are no longer serving traffic.

Nova 2 can also be invoked through cross-region inference profile IDs such as
`us.amazon.nova-2-lite-v1:0`. If AWS rejects the in-region model ID in a target
account or region, keep the same worker code and change only `BEDROCK_MODEL_ID`.
