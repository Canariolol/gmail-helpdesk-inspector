#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

usage() {
  cat >&2 <<'USAGE'
Usage:
  scripts/redeploy-gcp.sh [--no-traffic] [api|worker|web|all]

Environment overrides:
  GCP_PROJECT_ID   Defaults to the active gcloud project
  GCP_REGION       Defaults to us-central1
  ARTIFACT_REPO    Defaults to ghmi
  API_SERVICE      Defaults to ghmi-api
  WORKER_SERVICE   Defaults to ghmi-ai-worker
  WEB_SERVICE      Defaults to ghmi-web
  IMAGE_BASE       Defaults to ${GCP_REGION}-docker.pkg.dev/${GCP_PROJECT_ID}/${ARTIFACT_REPO}
  IMAGE_TAG        Defaults to the current Git commit (12 characters)
  API_URL          Optional; used by the web deploy for API_PROXY_TARGET
  API_DEPLOY_EXTRA_ARGS  Optional; extra gcloud args for the api deploy
                         (e.g. "--update-env-vars APP_ENV=production")
  VITE_MERCADOPAGO_PUBLIC_KEY  Required to build embedded Mercado Pago checkout
  MICROSOFT_TENANT             Defaults to common
  MICROSOFT_REDIRECT_URL_PROD  Defaults to WEB_BASE_URL of the deployed service
                               + /mailbox/connect/microsoft/callback

Microsoft Graph credentials are read automatically from .keys (or .env) and sent
as plain env vars; no extra flags needed. Without them the API deploys fine and
simply does not offer the Microsoft provider.

Options:
  --no-traffic     Creates a revision tagged candidate without moving user traffic
USAGE
}

target="all"
target_set=false
no_traffic=false
for arg in "$@"; do
  case "$arg" in
    --no-traffic) no_traffic=true ;;
    api|worker|web|all)
      if [[ "$target_set" == true ]]; then
        usage
        exit 1
      fi
      target="$arg"
      target_set=true
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 1
      ;;
  esac
done

deploy_traffic_args=()
if [[ "$no_traffic" == true ]]; then
  deploy_traffic_args=(--no-traffic --tag=candidate)
fi

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
IMAGE_TAG="${IMAGE_TAG:-$(git rev-parse --short=12 HEAD)}"

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
require_var IMAGE_TAG

if ! git diff --quiet || ! git diff --cached --quiet || [[ -n "$(git ls-files --others --exclude-standard)" ]]; then
  echo "Refusing to deploy a dirty Git worktree. Commit or remove changes first." >&2
  exit 1
fi

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

print_candidate_url() {
  local service="$1"
  local service_url
  service_url="$(gcloud run services describe "$service" \
    --project "$GCP_PROJECT_ID" \
    --region "$GCP_REGION" \
    --format='value(status.url)')"
  if [[ -n "$service_url" ]]; then
    echo "Candidate URL: https://candidate---${service_url#https://}"
  fi
}

build_push_deploy() {
  local name="$1"
  local dockerfile="$2"
  local service="$3"
  shift 3

  local image="${IMAGE_BASE}/${name}:${IMAGE_TAG}"

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
    "${deploy_traffic_args[@]}" \
    "$@"

  if [[ "$no_traffic" == true ]]; then
    print_candidate_url "$service"
  fi
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

# Lee una clave de un archivo de credenciales local sin ejecutarlo (un `source`
# correría cualquier cosa que hubiera ahí dentro).
read_local_key() {
  local file="$1"; shift
  [[ -f "$file" ]] || return 1
  local wanted value
  for wanted in "$@"; do
    value="$(sed -nE "s/^[[:space:]]*${wanted}[[:space:]]*=[[:space:]]*//p" "$file" | head -n1)"
    value="${value%\"}"; value="${value#\"}"
    value="${value%\'}"; value="${value#\'}"
    if [[ -n "$value" ]]; then
      printf '%s' "$value"
      return 0
    fi
  done
  return 1
}

# Credenciales de Microsoft Graph. Decisión 2026-07-20: no van a Secret Manager,
# se despliegan como variables de entorno leídas desde un archivo local ignorado
# por Git, para que no queden en el historial del shell.
#
# Si no hay credenciales, la API se despliega igual y simplemente no ofrece el
# botón de Microsoft. No hace falta declarar nada al invocar el script.
#
# Escribe en el array global MICROSOFT_ARGS en vez de imprimir a stdout: si
# imprimiera, el llamador tendría que capturarlo con `$(...)` o `< <(...)`, la
# función correría en una subshell y un `exit 1` de las validaciones no abortaría
# el deploy — se desplegaría sin Microsoft en silencio.
MICROSOFT_ARGS=()
microsoft_env_args() {
  MICROSOFT_ARGS=()
  local client_id secret
  client_id="${MICROSOFT_CLIENT_ID:-$(read_local_key .keys MICROSOFT_CLIENT_ID client_id \
    || read_local_key .env MICROSOFT_CLIENT_ID || true)}"
  secret="${MICROSOFT_CLIENT_SECRET:-$(read_local_key .keys MICROSOFT_CLIENT_SECRET secret_value \
    || read_local_key .env MICROSOFT_CLIENT_SECRET || true)}"

  if [[ -z "$client_id" || -z "$secret" ]]; then
    echo "Microsoft: sin credenciales locales; se despliega sin el proveedor." >&2
    return 0
  fi

  local guid='^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'
  if [[ ! "$client_id" =~ $guid ]]; then
    echo "MICROSOFT_CLIENT_ID no tiene forma de GUID; revisá el 'Application (client) ID'." >&2
    exit 1
  fi
  # Azure muestra 'Value' y 'Secret ID' juntos; el Secret ID es un GUID que no
  # autentica. Fallar acá evita un AADSTS7000215 críptico en producción.
  if [[ "$secret" =~ $guid ]]; then
    echo "MICROSOFT_CLIENT_SECRET tiene forma de GUID: es el 'Secret ID', no el 'Value'." >&2
    exit 1
  fi
  # gcloud separa --update-env-vars por comas: un secreto con coma se desplegaría
  # truncado en silencio y fallaría recién en el primer login real.
  if [[ "$secret" == *,* || "$secret" == *" "* ]]; then
    echo "MICROSOFT_CLIENT_SECRET contiene coma o espacio; no se puede pasar sin truncarse." >&2
    exit 1
  fi

  # El redirect NO se lee de .env: ahí vive el de localhost, y desplegarlo a
  # producción rompería el login con AADSTS50011. Se deriva del WEB_BASE_URL
  # que ya tiene el servicio desplegado.
  local redirect="${MICROSOFT_REDIRECT_URL_PROD:-}"
  if [[ -z "$redirect" ]]; then
    local web_base
    web_base="$(gcloud run services describe "$API_SERVICE" \
      --project "$GCP_PROJECT_ID" --region "$GCP_REGION" \
      --format='value(spec.template.spec.containers[0].env.filter("name:WEB_BASE_URL").extract(value))' \
      2>/dev/null | tr -d '[]' | tr -d "'")"
    if [[ -z "$web_base" || "$web_base" != https://* ]]; then
      echo "No pude derivar el redirect de Microsoft desde WEB_BASE_URL del servicio." >&2
      echo "Exportá MICROSOFT_REDIRECT_URL_PROD con la URL HTTPS registrada en Azure." >&2
      exit 1
    fi
    redirect="${web_base%/}/mailbox/connect/microsoft/callback"
  fi

  echo "Microsoft: habilitado (client id ${client_id:0:8}…, redirect ${redirect})." >&2
  MICROSOFT_ARGS=(
    --update-env-vars
    "MICROSOFT_CLIENT_ID=${client_id},MICROSOFT_CLIENT_SECRET=${secret},MICROSOFT_REDIRECT_URL=${redirect},MICROSOFT_TENANT=${MICROSOFT_TENANT:-common}"
  )
}

# Confirma que la API realmente esté ofreciendo Microsoft. Una variable que no
# llegó produce un botón ausente y ningún error visible; esto lo delata ahora.
verify_microsoft_enabled() {
  local base_url
  base_url="$(resolve_api_url)"
  if [[ "$no_traffic" == true ]]; then
    base_url="https://candidate---${base_url#https://}"
  fi
  local providers
  providers="$(curl -fsS --max-time 30 "${base_url}/mailbox/providers" 2>/dev/null || true)"
  if [[ "$providers" == *'"microsoft"'* ]]; then
    echo "Microsoft: verificado en ${base_url}/mailbox/providers"
  else
    echo "AVISO: la API no está ofreciendo Microsoft tras el deploy." >&2
    echo "  GET ${base_url}/mailbox/providers -> ${providers:-(sin respuesta)}" >&2
  fi
}

deploy_api() {
  # Llamada directa, no en subshell: así un fallo de validación aborta el deploy.
  microsoft_env_args
  # Sin comillas a propósito en API_DEPLOY_EXTRA_ARGS: permite pasar varios
  # argumentos gcloud separados. Las de Microsoft sí van citadas, porque el
  # secreto no debe partirse por word splitting.
  # shellcheck disable=SC2086
  build_push_deploy "api" "apps/api/Dockerfile" "$API_SERVICE" \
    --update-env-vars "APP_ENV=production" \
    "${MICROSOFT_ARGS[@]}" ${API_DEPLOY_EXTRA_ARGS:-}

  if [[ ${#MICROSOFT_ARGS[@]} -gt 0 ]]; then
    verify_microsoft_enabled
  fi
}

deploy_worker() {
  build_push_deploy "ai-worker" "apps/ai-worker/Dockerfile" "$WORKER_SERVICE" \
    --update-env-vars "APP_ENV=production"
}

deploy_web() {
  local api_url
  api_url="$(resolve_api_url)"
  if [[ -z "$api_url" ]]; then
    echo "Could not resolve API_URL for the web API_PROXY_TARGET." >&2
    echo "Export API_URL manually or deploy the API service first." >&2
    exit 1
  fi
  if [[ "$no_traffic" == true && "$target" == "all" ]]; then
    api_url="https://candidate---${api_url#https://}"
  fi

  ensure_service_exists "$WEB_SERVICE"

  local image="${IMAGE_BASE}/web:${IMAGE_TAG}"

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
    --quiet \
    "${deploy_traffic_args[@]}"

  if [[ "$no_traffic" == true ]]; then
    print_candidate_url "$WEB_SERVICE"
  fi
}

echo "Project:    $GCP_PROJECT_ID"
echo "Region:     $GCP_REGION"
echo "Images:     $IMAGE_BASE"
echo "Image tag:  $IMAGE_TAG"
echo "Target:     $target"
if [[ "$no_traffic" == true ]]; then
  echo "Traffic:    candidate tag only (no user traffic)"
fi

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

if [[ "$no_traffic" == true ]]; then
  echo "Candidate revision tagged 'candidate' without untagged user traffic."
fi

echo "Redeploy finished."
