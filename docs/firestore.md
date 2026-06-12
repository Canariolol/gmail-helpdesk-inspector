# Firestore Model

Runtime storage uses Firestore Native mode.

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

