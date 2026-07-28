#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

for command in cargo python3 npm; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Missing required command: $command" >&2
    exit 1
  fi
done

if ! cargo watch --version >/dev/null 2>&1; then
  echo "cargo-watch is required for Rust hot reload." >&2
  echo "Install it once with: cargo install cargo-watch --locked" >&2
  exit 1
fi

ENV_FILE="${ENV_FILE:-.env}"
if [[ ! -f "$ENV_FILE" ]]; then
  echo "Missing $ENV_FILE. Create it first." >&2
  exit 1
fi

set -a
source "$ENV_FILE"
if [[ -n "${APP_ENV_OVERRIDE:-}" && -f "$APP_ENV_OVERRIDE" ]]; then
  source "$APP_ENV_OVERRIDE"
fi
set +a

case "${GOOGLE_APPLICATION_CREDENTIALS:-}" in
  /run/secrets/*|/secrets/*)
    export GOOGLE_APPLICATION_CREDENTIALS="$ROOT_DIR/secrets/${GOOGLE_APPLICATION_CREDENTIALS##*/}"
    ;;
esac

if [[ -n "${GOOGLE_APPLICATION_CREDENTIALS:-}" && ! -f "$GOOGLE_APPLICATION_CREDENTIALS" ]]; then
  echo "Missing service account JSON at $GOOGLE_APPLICATION_CREDENTIALS." >&2
  exit 1
fi

if [[ "${AI_WORKER_URL:-}" == "http://ai-worker:8090" ]]; then
  export AI_WORKER_URL="http://127.0.0.1:8090"
fi
export VITE_API_BASE_URL="${VITE_API_BASE_URL:-${API_BASE_URL:-http://127.0.0.1:8080}}"

pids=()
cleanup() {
  ((${#pids[@]})) && kill "${pids[@]}" 2>/dev/null || true
  wait "${pids[@]}" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

echo "Starting Gmail Helpdesk Inspector with hot reload..."
echo "Web:        http://127.0.0.1:5173"
echo "API health: http://127.0.0.1:${API_PORT:-8080}/health"
echo "Worker:     http://127.0.0.1:8090/health"

(cd apps/api && cargo watch -q -x run) &
pids+=("$!")

PYTHONPATH="$ROOT_DIR/apps/ai-worker/src" python3 -m uvicorn ai_worker.main:app \
  --host 127.0.0.1 --port 8090 --reload --reload-dir apps/ai-worker/src &
pids+=("$!")

npm --prefix apps/web run dev -- --host 127.0.0.1 &
pids+=("$!")

wait -n "${pids[@]}"
