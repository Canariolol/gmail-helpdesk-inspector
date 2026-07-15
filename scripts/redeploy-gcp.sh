#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

usage() {
  cat >&2 <<'USAGE'
Usage:
  scripts/redeploy-gcp.sh [api|worker|web|all]

Environment overrides:
  GCP_PROJECT_ID   Defaults to the active gcloud project
  GCP_REGION       Defaults to us-central1
  ARTIFACT_REPO    Defaults to ghmi
  API_SERVICE      Defaults to ghmi-api
  WORKER_SERVICE   Defaults to ghmi-ai-worker
  WEB_SERVICE      Defaults to ghmi-web
  IMAGE_BASE       Defaults to ${GCP_REGION}-docker.pkg.dev/${GCP_PROJECT_ID}/${ARTIFACT_REPO}
  API_URL          Optional; used by the web deploy for API_PROXY_TARGET
  VITE_MERCADOPAGO_PUBLIC_KEY  Required to build embedded Mercado Pago checkout
USAGE
}

target="${1:-all}"
case "$target" in
  api|worker|web|all) ;;
  -h|--help)
    usage
    exit 0
    ;;
  *)
    usage
    exit 1
    ;;
esac

if ! command -v docker >/dev/null 2>&1; then
  echo "Docker is required but was not found in PATH." >&2
  exit 1
fi

if ! command -v gcloud >/dev/null 2>&1; then
  echo "gcloud is required but was not found in PATH." >&2
  exit 1
fi

active_project="$(gcloud config get-value project 2>/dev/null || true)"
if [[ "$active_project" == "(unset)" ]]; then
  active_project=""
fi

GCP_PROJECT_ID="${GCP_PROJECT_ID:-$active_project}"
GCP_REGION="${GCP_REGION:-us-central1}"
ARTIFACT_REPO="${ARTIFACT_REPO:-ghmi}"
API_SERVICE="${API_SERVICE:-ghmi-api}"
WORKER_SERVICE="${WORKER_SERVICE:-ghmi-ai-worker}"
WEB_SERVICE="${WEB_SERVICE:-ghmi-web}"
IMAGE_BASE="${IMAGE_BASE:-${GCP_REGION}-docker.pkg.dev/${GCP_PROJECT_ID}/${ARTIFACT_REPO}}"

require_var() {
  local name="$1"
  local value="${!name:-}"
  if [[ -z "$value" || "$value" == "(unset)" ]]; then
    echo "Missing $name. Export it first or set the active gcloud project." >&2
    exit 1
  fi
}

require_var GCP_PROJECT_ID
require_var GCP_REGION
require_var ARTIFACT_REPO
require_var API_SERVICE
require_var WORKER_SERVICE
require_var WEB_SERVICE
require_var IMAGE_BASE

ensure_service_exists() {
  local service="$1"
  if ! gcloud run services describe "$service" \
    --project "$GCP_PROJECT_ID" \
    --region "$GCP_REGION" >/dev/null 2>&1; then
    echo "Cloud Run service '$service' was not found in project '$GCP_PROJECT_ID' region '$GCP_REGION'." >&2
    echo "Use docs/gcp-deploy.md for the first deployment, then rerun this redeploy script." >&2
    exit 1
  fi
}

build_push_deploy() {
  local name="$1"
  local dockerfile="$2"
  local service="$3"
  shift 3

  local image="${IMAGE_BASE}/${name}:latest"

  ensure_service_exists "$service"

  echo "Building ${image}..."
  docker build -f "$dockerfile" -t "$image" .

  echo "Pushing ${image}..."
  docker push "$image"

  echo "Deploying ${service}..."
  gcloud run deploy "$service" \
    --project "$GCP_PROJECT_ID" \
    --image "$image" \
    --region "$GCP_REGION" \
    --quiet \
    "$@"
}

resolve_api_url() {
  if [[ -n "${API_URL:-}" ]]; then
    printf '%s\n' "$API_URL"
    return
  fi

  gcloud run services describe "$API_SERVICE" \
    --project "$GCP_PROJECT_ID" \
    --region "$GCP_REGION" \
    --format='value(status.url)'
}

deploy_api() {
  build_push_deploy "api" "apps/api/Dockerfile" "$API_SERVICE"
}

deploy_worker() {
  build_push_deploy "ai-worker" "apps/ai-worker/Dockerfile" "$WORKER_SERVICE"
}

deploy_web() {
  local api_url
  api_url="$(resolve_api_url)"
  if [[ -z "$api_url" ]]; then
    echo "Could not resolve API_URL for the web API_PROXY_TARGET." >&2
    echo "Export API_URL manually or deploy the API service first." >&2
    exit 1
  fi

  ensure_service_exists "$WEB_SERVICE"

  local image="${IMAGE_BASE}/web:latest"

  echo "Building ${image}..."
  docker build -f apps/web/Dockerfile \
    --build-arg "VITE_API_BASE_URL=" \
    --build-arg "VITE_MERCADOPAGO_PUBLIC_KEY=${VITE_MERCADOPAGO_PUBLIC_KEY}" \
    -t "$image" .

  echo "Pushing ${image}..."
  docker push "$image"

  echo "Deploying ${WEB_SERVICE} with API_PROXY_TARGET=${api_url}..."
  gcloud run deploy "$WEB_SERVICE" \
    --project "$GCP_PROJECT_ID" \
    --image "$image" \
    --region "$GCP_REGION" \
    --set-env-vars "API_PROXY_TARGET=${api_url}" \
    --quiet
}

echo "Project:    $GCP_PROJECT_ID"
echo "Region:     $GCP_REGION"
echo "Images:     $IMAGE_BASE"
echo "Target:     $target"

if [[ "$target" == "web" || "$target" == "all" ]] && [[ -z "${VITE_MERCADOPAGO_PUBLIC_KEY:-}" ]]; then
  echo "Missing VITE_MERCADOPAGO_PUBLIC_KEY." >&2
  echo "For sandbox: set -a; source .env.sandbox.local; set +a; scripts/redeploy-gcp.sh $target" >&2
  exit 1
fi

echo "Running pre-deploy checks..."
./scripts/check-all.sh

gcloud auth configure-docker "${GCP_REGION}-docker.pkg.dev" --quiet

case "$target" in
  api)
    deploy_api
    ;;
  worker)
    deploy_worker
    ;;
  web)
    deploy_web
    ;;
  all)
    deploy_worker
    deploy_api
    deploy_web
    ;;
esac

echo "Redeploy finished."
