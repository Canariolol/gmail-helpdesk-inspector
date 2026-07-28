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

env_value() {
  awk -F= -v key="$1" '$1 == key {print $2}' .env | tail -n 1 | tr -d '"' | tr -d "'"
}

app_storage="$(env_value APP_STORAGE)"
app_storage="${app_storage:-postgres}"

case "$app_storage" in
  postgres)
    if [[ -z "$(env_value POSTGRES_DATABASE_URL)" ]]; then
      echo "APP_STORAGE=postgres requires POSTGRES_DATABASE_URL in .env." >&2
      exit 1
    fi
    ;;
  memory)
    ;;
  *)
    echo "Unsupported APP_STORAGE=$app_storage (expected postgres or memory)." >&2
    exit 1
    ;;
esac

echo "Starting Gmail Helpdesk Inspector..."
echo "Web:        http://127.0.0.1:5173"
echo "API health: http://127.0.0.1:8080/health"
echo "Worker:     http://127.0.0.1:8090/health"

exec docker compose up --build "$@"
