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
ownerProfiles/{ownerHash}/manualReviewOverrides/{gmailThreadId}
systemMigrations/manual-review-metrics-v1
systemMigrations/manual-review-inheritance-v2
```

`analysisRuns/{runId}` stores precalculated metrics so the dashboard does not
scan every thread on each load.

The API creates the `manual-review-metrics-v1` marker after reconciling legacy
manual reviews and recalculating affected run metrics. The migration is
idempotent and runs automatically before the API starts serving traffic.

`manualReviewOverrides` stores the latest human decision per Gmail thread,
together with a fingerprint of its message IDs. A later analysis inherits the
decision only while that fingerprint remains unchanged.

The v2 migration builds these overrides from existing manual reviews and
repairs the latest completed analysis for each account.

Recommended query indexes:

- `analysisRuns.status`
- `threads.classification`
- `threads.isAnswered`
- `threads.manualReviewRequired`
- `threads.firstClientMessageAt`
