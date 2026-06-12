# Gmail Helpdesk Metrics Inspector

## TL;DR

```bash
./scripts/local-up.sh
```

This builds and starts the local web, API, and AI worker with Docker Compose.
Open `http://127.0.0.1:5173` when the services are ready.
Docker publishes local ports on loopback only.

Open-source MVP for auditing a Gmail inbox used as a lightweight help desk.

The app reads Gmail with the minimum readonly scope, classifies support-like
threads, calculates auditable response metrics, and uses Claude Sonnet 4.6 on
Amazon Bedrock as a mandatory quality auditor.

## Stack

- Web: React, TypeScript, Vite, TanStack Query, Recharts
- API: Rust, Axum
- Storage: Firestore Native mode
- AI worker: Python, FastAPI, Pydantic
- Local runtime: Docker Compose

## Quick Start

1. Copy the environment template:

   ```bash
   cp .env.example .env
   ```

2. Fill Google OAuth, Firestore, and Bedrock values in `.env`.
   Add your Gmail account as an OAuth test user in Google Cloud Console.

3. Start the stack:

   ```bash
   ./scripts/local-up.sh
   ```

4. Open `http://127.0.0.1:5173`.

## Firestore Notes

Runtime storage targets Firestore directly. The API supports three Firestore
auth sources, in this order:

1. `FIRESTORE_BEARER_TOKEN`
2. `GOOGLE_APPLICATION_CREDENTIALS` service-account JSON
3. Cloud Run metadata server

For local/Docker development, prefer a service account key because the API can
use it to mint fresh access tokens automatically:

```bash
mkdir -p secrets
cp /path/to/service-account.json secrets/gcp-service-account.json
```

Then set:

```env
GOOGLE_APPLICATION_CREDENTIALS=/run/secrets/gcp-service-account.json
FIRESTORE_BEARER_TOKEN=
```

`/secrets/gcp-service-account.json` is also mounted for local compatibility, but
`/run/secrets/gcp-service-account.json` is the recommended path.

`FIRESTORE_BEARER_TOKEN` is only a short-lived fallback for quick debugging.
It commonly expires after about one hour.

## Privacy Defaults

- Gmail scope is limited to `https://www.googleapis.com/auth/gmail.readonly`.
- The app never sends, labels, deletes, or modifies Gmail messages.
- Full email body text is fetched only during analysis/auditing and is not
  persisted in Firestore.
- Stored data is limited to metadata, snippets, headers, participants,
  classification decisions, reasons, metrics, and token usage.

## Scheduled Daily Analysis & Email Report

The API can run the analysis automatically Monday to Friday at 08:00
(America/Santiago) and email a Spanish-language report (metrics, findings, and
threads that need manual review) via [Resend](https://resend.com). Tuesday to
Friday cover the previous day (00:00–23:59); Monday covers Friday 00:00 through
Sunday 23:59. The report is sent with Resend, not Gmail — the Gmail scope stays
readonly.

It requires a user who has logged in at least once (the stored refresh token is
used to mint a fresh access token at run time).

### Trigger modes

1. **Internal loop** — set `SCHEDULER_ENABLED=true`. Meant for always-on hosts
   (Docker Compose, VPS). The API wakes every minute and fires once per
   weekday from 08:00 local time.
2. **External trigger** — `POST /internal/scheduled-analysis` authenticated
   with the `x-cron-secret` header (`CRON_SECRET` env var). Meant for Cloud
   Scheduler or a host crontab; also handy for manual testing and backfill:

   ```bash
   curl -s -X POST http://localhost:8080/internal/scheduled-analysis \
     -H "x-cron-secret: $CRON_SECRET" -H "Content-Type: application/json" \
     -d '{"as_of_date":"2026-06-12"}'   # optional; defaults to today in SCL
   ```

Both modes share the same core and the same idempotency lock, so they can be
combined safely. If `CRON_SECRET` is unset the endpoint answers 404.

### Environment variables

| Variable | Purpose |
|----------|---------|
| `SCHEDULER_ENABLED` | Enables the internal loop (`false` by default) |
| `CRON_SECRET` | Shared secret for the external trigger endpoint |
| `SCHEDULE_USER_EMAIL` | Seed: Gmail account to analyze |
| `SCHEDULE_INTERNAL_DOMAINS` | Seed: comma-separated internal domains |
| `SCHEDULE_GMAIL_MAX_THREADS` | Seed: thread cap override (useful for Monday's 3-day window; global default is `GMAIL_MAX_THREADS=50`) |
| `RESEND_API_KEY` | Resend API key |
| `REPORT_FROM_EMAIL` | Verified Resend sender, e.g. `Helpdesk <reportes@domain.cl>` |
| `REPORT_TO_EMAIL` | Comma-separated default recipients |

### Configuration & state in Firestore

- `scheduleConfigs/{email}` — analysis parameters (recipients, internal
  domains, ignored lists, timezone, thread cap, `enabled`). Seeded once from
  the `SCHEDULE_*`/`REPORT_TO_EMAIL` env vars when empty; afterwards edit the
  document directly (all fields have safe defaults).
- `scheduleStates/{email}` — last attempt per user (window, status, run id,
  whether the email went out). A window with status `completed` is never
  re-run; a stale `running` claim (>60 min) is retried. Scheduler retries
  after success therefore return `skipped`.

Missed weekdays are not backfilled automatically — use the endpoint with
`as_of_date` to fill gaps.

### Operational caveats

- While the Google OAuth app is in **Testing** publishing status, refresh
  tokens expire after 7 days; the failure email will ask the user to log in
  again. Publish the OAuth app to Production to avoid this.
- Session lookup lists the `users` collection (page size 300). Fine for a
  single-user deployment; clean old session docs if logins accumulate.

## Development Checks

```bash
cargo test --manifest-path apps/api/Cargo.toml
python3 -m pytest apps/ai-worker/tests
npm --prefix apps/web run build
```
