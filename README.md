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
threads, calculates auditable response metrics, and can use an opt-in AI auditor
through Amazon Bedrock when enabled by the organization policy.

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

## SaaS Configuration Model

The current app is no longer configured only through per-run filters. After login,
`GET /me/org/config` provisions a private-beta organization, an owner membership,
one connected mailbox, a mutable policy draft and an immutable policy version.
Analysis runs created from policy store a snapshot with org/mailbox ids, policy
version/hash, Gmail scope snapshot, retention expiry and data minimization mode.
Legacy per-run filters are still accepted for local/backward compatibility.

Key defaults for the private beta:

- One Workspace mailbox per organization.
- AI auditing is off by default and requires explicit consent.
- Retention defaults to 30 days and is stored per run as `retention_expires_at`.
- Scheduled reports default to metrics-only content.
- Gmail remains `gmail.readonly`; reports are sent via Resend, never via Gmail.

Useful authenticated endpoints:

| Endpoint | Purpose |
|----------|---------|
| `GET /me/org/config` | Read/provision organization policy config |
| `PUT /me/org/config` | Patch policy draft and create a new policy version when it changes |
| `GET /me/data-summary` | Read-only privacy/data summary |
| `GET /me/operations/status` | Read-only scheduler/operations status |
| `GET /me/operations/history` | Read-only recent operational history with redacted errors |

Pagination note: `GET /analysis-runs` and `GET /analysis-runs/:id/threads` remain backward-compatible arrays without query params. With `limit`/`page_token`, they return `{ items, next_page_token, total_count }` for beta-scale pagination.

## Scheduled Daily Analysis & Email Report

The API can run the analysis automatically using the policy preset
`weekdays_08_local`: Monday to Friday at 08:00 in each configured tenant timezone.
Tuesday to Friday cover the previous local day (00:00–23:59); Monday covers Friday
through Sunday. The report is sent with [Resend](https://resend.com), not Gmail —
the Gmail scope stays readonly.

It requires a user who has logged in at least once (the stored refresh token is
used to mint a fresh access token at run time).

### Trigger modes

1. **Internal loop** — set `SCHEDULER_ENABLED=true`. Meant for always-on hosts
   (Docker Compose, VPS). The API wakes every minute and evaluates each enabled
   schedule config in its own IANA timezone.
2. **External trigger** — `POST /internal/scheduled-analysis` authenticated
   with the `x-cron-secret` header (`CRON_SECRET` env var). Without `as_of_date`,
   it uses the same due-by-timezone logic as the internal loop. With `as_of_date`,
   it performs an explicit local-date backfill/testing run:

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
| `SCHEDULE_INTERNAL_DOMAINS` | Legacy seed: comma-separated internal domains |
| `SCHEDULE_GMAIL_MAX_THREADS` | Legacy seed: thread cap override (global default is `GMAIL_MAX_THREADS=50`) |
| `RESEND_API_KEY` | Resend API key |
| `REPORT_FROM_EMAIL` | Verified Resend sender, e.g. `Helpdesk <reportes@domain.cl>` |
| `REPORT_TO_EMAIL` | Legacy fallback/default recipients |
| `APP_ENV` | Set `production`/`prod` to reject development secret defaults |
| `RATE_LIMIT_ANALYSIS_CREATE_PER_HOUR` | Per-user manual run creation limit; default `12` |
| `RATE_LIMIT_ANALYSIS_START_PER_HOUR` | Per-user manual run start limit; default `12` |

### Configuration & state in Firestore

- `scheduleConfigs/{email}` — scheduler execution config (recipients, internal
  domains, ignored lists, timezone, thread cap, `enabled`). It can be seeded from
  legacy `SCHEDULE_*`/`REPORT_TO_EMAIL`, but in SaaS mode it is synchronized from
  `/me/org/config` policy updates.
- `scheduleStates/{email}` — last attempt per user (window, status, run id,
  whether the email went out). A window with status `completed` is never
  re-run; a stale `running` claim (>60 min) is retried. Scheduler retries
  after success therefore return `skipped`.

Missed weekdays are not backfilled automatically — use the endpoint with
`as_of_date` to fill gaps.

### Operational caveats

- While the Google OAuth app is in **Testing** publishing status, refresh
  tokens expire after 7 days; the failure email will ask the user to log in
  again. Publish/verify the OAuth app before broad SaaS launch.
- The current rate limiter is in-memory per API instance. For private beta, run
  one API instance or replace it with a distributed limiter before scaling out.
- The AI worker is stateless: the API sends per-run policy context in each audit
  request. Do not configure tenant/company prompt defaults in the worker.
- Retention expiry is stored per run, but destructive retention jobs are still a
  later production hardening task.

## Deployment Runbook

For private beta deployment guidance, see:

- `docs/deployment-beta-runbook.md`

It covers Cloud Run service order, verified environment variables, OAuth, scheduler modes, smoke tests, rollback, and known beta risks.

## Development Checks

```bash
./scripts/check-all.sh

# or individually:
cargo test --manifest-path apps/api/Cargo.toml
cargo clippy --manifest-path apps/api/Cargo.toml --all-targets -- -D warnings
python3 -m pytest apps/ai-worker/tests
npm --prefix apps/web run build
```
