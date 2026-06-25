# Bedrock Setup

The AI worker uses Amazon Bedrock Runtime with bearer-token authentication.

Required environment:

```text
AWS_BEARER_TOKEN_BEDROCK=
AWS_REGION=us-east-1
BEDROCK_MODEL_ID=amazon.nova-2-lite-v1:0
BEDROCK_BATCH_MODEL_ID=
```

The worker uses the Bedrock Converse API, sends compact text-only batches of up
to 20 threads, and expects strict JSON back from the model. Ambiguous or
conflicting decisions are escalated to the existing detailed per-thread audit.
`BEDROCK_BATCH_MODEL_ID` is optional and falls back to `BEDROCK_MODEL_ID`.
If the model returns invalid JSON or confidence below the API threshold, the
thread remains queued for manual review.

Nova 2 can also be invoked through cross-region inference profile IDs such as
`us.amazon.nova-2-lite-v1:0`. If AWS rejects the in-region model ID in a target
account or region, keep the same worker code and change only `BEDROCK_MODEL_ID`.
