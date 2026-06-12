# Gmail Helpdesk Metrics Inspector

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

3. Start the stack:

   ```bash
   docker compose up --build
   ```

4. Open `http://localhost:5173`.

## Firestore Notes

Runtime storage targets Firestore directly. The API supports three Firestore
auth sources, in this order:

1. `FIRESTORE_BEARER_TOKEN`
2. `GOOGLE_APPLICATION_CREDENTIALS` service-account JSON
3. Cloud Run metadata server

For quick local testing against a real GCP project:

```bash
export FIRESTORE_BEARER_TOKEN="$(gcloud auth application-default print-access-token)"
```

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

