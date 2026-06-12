# Bedrock Setup

The AI worker uses Amazon Bedrock Runtime with bearer-token authentication.

Required environment:

```text
AWS_BEARER_TOKEN_BEDROCK=
AWS_REGION=us-east-1
BEDROCK_MODEL_ID=amazon.nova-2-lite-v1:0
```

The worker uses the Bedrock Converse API, sends text-only payloads, and expects strict JSON back from the model.
If the model returns invalid JSON or confidence below the API threshold, the
thread remains queued for manual review.

Nova 2 can also be invoked through cross-region inference profile IDs such as
`us.amazon.nova-2-lite-v1:0`. If AWS rejects the in-region model ID in a target
account or region, keep the same worker code and change only `BEDROCK_MODEL_ID`.
