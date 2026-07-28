# Mira Helpdesk

## TL;DR

```bash
./scripts/local-up.sh
```

This builds and starts the local web, API, and AI worker with Docker Compose.
Open `http://127.0.0.1:5173` when the services are ready.
Docker publishes local ports on loopback only.

Open-source MVP for auditing a Gmail inbox used as a lightweight help desk.

The app reads Gmail with the minimum readonly scope, classifies support-like
threads, calculates auditable response metrics, and uses an AI auditor through
Amazon Bedrock by default. Each organization can disable it from Configuration.

## Stack

- Web: React, TypeScript, Vite, TanStack Query, Recharts
- API: Rust, Axum
- Storage: PostgreSQL (Supabase, schemas `mira` and `billing`)
- AI worker: Python, FastAPI, Pydantic
- Local runtime: Docker Compose

## Quick Start

1. Copy the environment template:

   ```bash
   cp .env.example .env
   ```

2. Fill WorkOS AuthKit, Mercado Pago, Google Gmail OAuth, PostgreSQL, and
   Bedrock values in `.env`. Add your Gmail account as an OAuth test user in
   Google Cloud Console while the Google app is in Testing.

3. Start the stack:

   ```bash
   ./scripts/local-up.sh
   ```

4. Open `http://127.0.0.1:5173`.

For development with hot reload, install the Rust watcher once and start the
three services directly on the host:

```bash
cargo install cargo-watch --locked
./scripts/local-dev.sh
```

Use the same optional environment override as Docker when needed:

```bash
APP_ENV_OVERRIDE=.env.sandbox.local ./scripts/local-dev.sh
```

## Database Notes

Runtime storage is PostgreSQL (Supabase), split across two schemas. Both are
deliberately revoked from `anon`, `authenticated`, and `service_role`, so the
Supabase Table Editor and the PostgREST API cannot read them. Use the SQL
Editor (or a direct `psql`) to inspect them.

- **`mira`** — a single polymorphic table, `mira.records` (`kind`, `id`, plus
  indexed lookup columns and a `data JSONB` payload). Everything that is
  *not* billing lives here: accounts, user sessions, mailbox connections,
  analysis runs/threads/messages, manual review overrides, schedule
  config/state, org policy config, filter presets, and the provider
  waitlist. One catch-all table by design — see
  `apps/api/migrations/0001_mira_records.sql`.
- **`billing`** — normalized tables for everything billing-related:
  `billing.plans` (a read-only mirror of the plan catalog, see below),
  `billing.subscriptions`, `billing.checkout_sessions`, `billing.usage_ledger`,
  and `billing.quota_alerts`. Added in
  `apps/api/migrations/0002_billing_schema.sql`, which also backfills any
  pre-existing `mira.records` rows of `kind IN ('subscription', 'checkout',
  'usage_ledger')` — those old rows are left in place afterwards (not
  deleted) as a rollback safety net until a follow-up cleanup migration.

**Why billing got its own schema:** subscriptions/checkouts/usage started out
as JSONB rows in `mira.records` like everything else, but billing data
benefits from real columns, foreign keys (`billing.subscriptions.plan_id`
references `billing.plans.id`), and being easy to query/join directly in the
SQL Editor for support and ops. This was a storage-layer change only — the
`StorageRepository` trait in `apps/api/src/storage/mod.rs` already abstracted
these operations, so `apps/api/src/http/mod.rs` and the rest of the business
logic did not change; only the PostgreSQL implementation
(`apps/api/src/postgres/mod.rs`) did.

**`billing.plans` is a mirror, not the source of truth.** Plan prices and
limits (Free/Inicial/Pro) are still hardcoded in `apps/api/src/billing.rs`
(`public_plans()` / `free_plan()`), covered by unit tests that assert plan
invariants (e.g. the AI quota is always the strictest one). The migration
seeds `billing.plans` from those same values so `billing.subscriptions` can
have a real foreign key and so ops can `JOIN` against plan names/prices in
SQL — but the API never reads `billing.plans` to decide what to charge or
enforce. Changing a price still means changing `billing.rs` and passing its
tests, on purpose: no runtime-editable pricing without that safety net.

Set the connection string in `.env`:

```env
APP_STORAGE=postgres
POSTGRES_DATABASE_URL=postgresql://...
```

`APP_STORAGE=memory` runs without any database and is for local/dev only.

Apply migrations with `scripts/apply-supabase-migrations.sh` (needs
`SUPABASE_ACCESS_TOKEN` and `SUPABASE_PROJECT_REF`). Take a manual backup with
`scripts/backup-postgres.sh`.

## Privacy Defaults

- Gmail scope is limited to `https://www.googleapis.com/auth/gmail.readonly`.
- The app never sends, labels, deletes, or modifies Gmail messages.
- Full email body text is fetched only during analysis/auditing and is not
  persisted in the database.
- Stored data is limited to metadata, snippets, headers, participants,
  classification decisions, reasons, metrics, and token usage.

## SaaS Configuration Model

The current app is no longer configured only through per-run filters. Account
authentication uses WorkOS AuthKit. Gmail OAuth is a separate mailbox connection
step and is only used with the readonly Gmail scope after the account has an
active subscription or trial. `GET /me/org/config` provisions an initial-release
organization, an owner membership, one mailbox record, a mutable policy draft
and an immutable policy version.
Analysis runs created from policy store a snapshot with org/mailbox ids, policy
version/hash, Gmail scope snapshot, retention expiry and data minimization mode.
Legacy per-run filters are still accepted for local/backward compatibility.

Key defaults for the initial release:

- One Gmail mailbox per organization for now.
- WorkOS AuthKit is the account login layer; Gmail OAuth is only the mailbox
  connection layer.
- Billing is enforced before Gmail connection, manual analysis, and scheduled
  analysis when `BILLING_ENFORCEMENT_ENABLED=true`.
- Plans charge in CLP through Mercado Pago. USD prices are reference copy only.
- `Pro` is the only plan with a 30-day trial.
- AI auditing is on by default and can be disabled at any time from
  Configuration. Re-enabling it requires explicit confirmation.
- Retention defaults to 30 days and is stored per run as `retention_expires_at`.
- Scheduled reports default to metrics-only content.
- Gmail remains `gmail.readonly`; reports are sent via Resend, never via Gmail.

Useful authenticated endpoints:

| Endpoint | Purpose |
|----------|---------|
| `GET /auth/workos/login` | Start WorkOS AuthKit account login/signup |
| `GET /me/account` | Read account, Gmail connection, and entitlement state |
| `GET /me/usage` | Read current billing-period usage ledger |
| `POST /checkout/subscriptions` | Create an embedded Mercado Pago subscription in CLP |
| `GET /gmail/connect/login` | Connect the audited Gmail mailbox with readonly scope |
| `POST /gmail/disconnect` | Revoke the Gmail connection and clear its stored credentials |
| `GET /me/org/config` | Read/provision organization policy config |
| `PUT /me/org/config` | Patch policy draft and create a new policy version when it changes |
| `GET /me/data-summary` | Read-only privacy/data summary |
| `DELETE /me/analysis-data` | Permanently delete the authenticated user's derived analysis data after confirmation |
| `GET /me/operations/status` | Read-only scheduler/operations status |
| `GET /me/operations/history` | Read-only recent operational history with redacted errors |

Useful public/provider endpoints:

| Endpoint | Purpose |
|----------|---------|
| `GET /public/plans` | Public billing plan catalog for pricing UI |
| `POST /billing/mercadopago/webhook` | Mercado Pago subscription notification receiver |
| `POST /auth/workos/webhook` | Signed WorkOS user/session lifecycle receiver |

Pagination note: `GET /analysis-runs` and `GET /analysis-runs/:id/threads` remain backward-compatible arrays without query params. With `limit`/`page_token`, they return `{ items, next_page_token, total_count }` for initial-release scale pagination.

## Scheduled Daily Analysis & Email Report

The API can run the analysis automatically Monday to Friday at the whole hour
selected by each tenant (`08:00` by default) in its configured timezone.
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
   it uses the same due-by-timezone logic as the internal loop. Configure one
   Configure Cloud Scheduler for the four available report slots on weekdays
   (`0 8,14,18,22 * * 1-5`, `America/Santiago`); the API selects only tenants
   whose configured hour is due and its idempotency lock prevents reruns.
   With `as_of_date`,
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
| `WORKOS_CLIENT_ID` | WorkOS AuthKit client id |
| `WORKOS_API_KEY` | WorkOS API key; never commit real values |
| `WORKOS_REDIRECT_URI` | WorkOS callback URL, e.g. `/auth/workos/callback` |
| `WORKOS_COOKIE_SECRET` | Secret used for WorkOS OAuth state cookie signing |
| `WORKOS_WEBHOOK_SECRET` | WorkOS endpoint secret used to verify lifecycle events |
| `BILLING_ENFORCEMENT_ENABLED` | Enables subscription/trial guards; defaults on in production |
| `MERCADOPAGO_ACCESS_TOKEN` | Mercado Pago access token for CLP subscriptions |
| `MERCADOPAGO_WEBHOOK_SECRET` | Shared webhook secret for receiver validation |
| `RATE_LIMIT_ANALYSIS_CREATE_PER_HOUR` | Per-user manual run creation limit; default `12` |
| `RATE_LIMIT_ANALYSIS_START_PER_HOUR` | Per-user manual run start limit; default `12` |

### Configuration & state in the database

Both live in `mira.records`, keyed by `(kind, id)` with `id = {email}`:

- `kind='schedule_config'` — scheduler execution config (recipients, internal
  domains, ignored lists, timezone, thread cap, `enabled`). It can be seeded from
  legacy `SCHEDULE_*`/`REPORT_TO_EMAIL`, but in SaaS mode it is synchronized from
  `/me/org/config` policy updates.
- `kind='schedule_state'` — last attempt per user (window, status, run id,
  whether the email went out). A window with status `completed` is never
  re-run; a stale `running` claim (>60 min) is retried. Scheduler retries
  after success therefore return `skipped`.

Missed weekdays are not backfilled automatically — use the endpoint with
`as_of_date` to fill gaps.

### Operational caveats

- While the Google OAuth app is in **Testing** publishing status, refresh
  tokens expire after 7 days; the failure email will ask the user to log in
  again. Publish/verify the OAuth app before broad SaaS launch.
- The current rate limiter is in-memory per API instance. For the initial release, run
  one API instance or replace it with a distributed limiter before scaling out.
- The AI worker is stateless: the API sends per-run policy context once per
  classification batch and again only for threads escalated to detailed audit.
  Do not configure tenant/company prompt defaults in the worker.
- Retention expiry is stored per run, but destructive retention jobs are still a
  later production hardening task.

## Deployment Runbook

For initial release deployment guidance, see:

- `docs/env-domains.md`
- `docs/deployment-beta-runbook.md`

They cover sandbox/live variables, `mira.ninfasolutions.com`, Cloud Run service order, OAuth, scheduler modes, smoke tests, rollback, and known initial-release risks.

## Development Checks

```bash
./scripts/check-all.sh

# or individually:
cargo test --manifest-path apps/api/Cargo.toml
cargo clippy --manifest-path apps/api/Cargo.toml --all-targets -- -D warnings
python3 -m pytest apps/ai-worker/tests
npm --prefix apps/web run build
```
