# Firestore Model

Runtime storage uses Firestore Native mode.

## Local Authentication

Use a service account key for local/Docker development:

```bash
mkdir -p secrets
cp /path/to/service-account.json secrets/gcp-service-account.json
```

Then configure:

```env
GOOGLE_APPLICATION_CREDENTIALS=/run/secrets/gcp-service-account.json
FIRESTORE_BEARER_TOKEN=
```

The Docker Compose setup also mounts the same directory at `/secrets` for
compatibility with local `.env` files that use
`/secrets/gcp-service-account.json`.

The API signs short-lived Google access tokens from that key automatically, so
you do not need to paste a bearer token every hour.

```text
users/{userId}
analysisRuns/{runId}
analysisRuns/{runId}/threads/{threadId}
analysisRuns/{runId}/threads/{threadId}/messages/{messageId}
analysisRuns/{runId}/threads/{threadId}/aiAudits/{auditId}
analysisRuns/{runId}/manualReviews/{reviewId}
```

`analysisRuns/{runId}` stores precalculated metrics so the dashboard does not
scan every thread on each load.

Recommended query indexes:

- `analysisRuns.status`
- `threads.classification`
- `threads.isAnswered`
- `threads.manualReviewRequired`
- `threads.firstClientMessageAt`
