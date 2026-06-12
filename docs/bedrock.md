# Bedrock Setup

The AI worker uses Amazon Bedrock Runtime with bearer-token authentication.

Required environment:

```text
AWS_BEARER_TOKEN_BEDROCK=
AWS_REGION=us-east-1
BEDROCK_MODEL_ID=us.anthropic.claude-sonnet-4-6
```

The worker sends text-only payloads and expects strict JSON back from the model.
If the model returns invalid JSON or confidence below the API threshold, the
thread remains queued for manual review.

