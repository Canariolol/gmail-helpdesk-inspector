#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if ! command -v docker >/dev/null 2>&1; then
  echo "Docker is required but was not found in PATH." >&2
  exit 1
fi

if ! docker compose version >/dev/null 2>&1; then
  echo "Docker Compose is required but is not available through 'docker compose'." >&2
  exit 1
fi

if [[ ! -f .env ]]; then
  echo "Missing .env. Create it first:" >&2
  echo "  cp .env.example .env" >&2
  exit 1
fi

google_credentials="$(awk -F= '$1 == "GOOGLE_APPLICATION_CREDENTIALS" {print $2}' .env | tail -n 1 | tr -d '"' | tr -d "'")"
firestore_bearer="$(awk -F= '$1 == "FIRESTORE_BEARER_TOKEN" {print $2}' .env | tail -n 1 | tr -d '"' | tr -d "'")"

if [[ -z "$firestore_bearer" && -z "$google_credentials" ]]; then
  echo "Firestore auth is not configured." >&2
  echo "Set GOOGLE_APPLICATION_CREDENTIALS=/run/secrets/gcp-service-account.json in .env." >&2
  exit 1
fi

if [[ "$google_credentials" == /run/secrets/* ]]; then
  local_secret_path="secrets/${google_credentials#/run/secrets/}"
  if [[ ! -f "$local_secret_path" ]]; then
    echo "Missing service account JSON at $local_secret_path." >&2
    echo "Copy your GCP service account key there, or update GOOGLE_APPLICATION_CREDENTIALS." >&2
    exit 1
  fi
elif [[ "$google_credentials" == /secrets/* ]]; then
  local_secret_path="secrets/${google_credentials#/secrets/}"
  if [[ ! -f "$local_secret_path" ]]; then
    echo "Missing service account JSON at $local_secret_path." >&2
    echo "Copy your GCP service account key there, or update GOOGLE_APPLICATION_CREDENTIALS." >&2
    exit 1
  fi
fi

echo "Starting Gmail Helpdesk Inspector..."
echo "Web:        http://localhost:5173"
echo "API health: http://localhost:8080/health"
echo "Worker:     http://localhost:8090/health"

exec docker compose up --build "$@"
