# Gmail Helpdesk Metrics Inspector

## TL;DR

```bash
./scripts/local-up.sh
```

This builds and starts the local web, API, and AI worker with Docker Compose.
Open `http://localhost:5173` when the services are ready.

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

4. Open `http://localhost:5173`.

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

## Development Checks

```bash
cargo test --manifest-path apps/api/Cargo.toml
python3 -m pytest apps/ai-worker/tests
npm --prefix apps/web run build
```
