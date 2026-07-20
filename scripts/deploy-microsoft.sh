#!/usr/bin/env bash
#
# Despliega la API con las credenciales de Microsoft Graph como variables de
# entorno en texto plano (decisión del responsable, 2026-07-20: no usar Secret
# Manager para este secreto).
#
# El secreto se lee desde un archivo local ignorado por Git y nunca se escribe en
# la línea de comandos que tipeas, así que no queda en el historial del shell.
# SÍ queda dentro del spec de la revisión de Cloud Run: legible por cualquiera
# con roles/run.viewer y persistente en las revisiones viejas aunque después lo
# rotes. Para moverlo a Secret Manager más adelante, ver el final de este archivo.
#
# Uso:
#   scripts/deploy-microsoft.sh --check      # solo valida las credenciales
#   scripts/deploy-microsoft.sh              # revisión candidata, sin tráfico
#   scripts/deploy-microsoft.sh --promote    # mueve el 100% del tráfico
#
# Variables opcionales:
#   KEYS_FILE  archivo con las credenciales (default: .keys en la raíz)

set -euo pipefail

cd "$(dirname "$0")/.."

KEYS_FILE="${KEYS_FILE:-.keys}"
REDIRECT_URL="${MICROSOFT_REDIRECT_URL:-https://mira.ninfasolutions.com/mailbox/connect/microsoft/callback}"
TENANT="${MICROSOFT_TENANT:-common}"
API_SERVICE="${API_SERVICE:-ghmi-api}"
GCP_REGION="${GCP_REGION:-us-central1}"
GCP_PROJECT_ID="${GCP_PROJECT_ID:-gmail-helpdesk-inspector}"

promote=false
check_only=false
case "${1:-}" in
  --promote) promote=true ;;
  --check) check_only=true ;;
  "") ;;
  *)
    echo "Uso: $0 [--check | --promote]" >&2
    exit 1
    ;;
esac

if [[ ! -f "$KEYS_FILE" ]]; then
  echo "No encuentro $KEYS_FILE. Definí KEYS_FILE si está en otra ruta." >&2
  exit 1
fi

# Lee una clave del archivo sin ejecutarlo (un `source` correría cualquier cosa
# que hubiera ahí dentro). Acepta los nombres que usa la UI de Azure y también
# los nombres finales de la variable de entorno.
read_key() {
  local wanted
  for wanted in "$@"; do
    local value
    value="$(sed -nE "s/^[[:space:]]*${wanted}[[:space:]]*=[[:space:]]*//p" "$KEYS_FILE" | head -n1)"
    value="${value%\"}"; value="${value#\"}"
    value="${value%\'}"; value="${value#\'}"
    if [[ -n "$value" ]]; then
      printf '%s' "$value"
      return 0
    fi
  done
  return 1
}

CLIENT_ID="$(read_key MICROSOFT_CLIENT_ID client_id)" || {
  echo "Falta el client id en $KEYS_FILE (MICROSOFT_CLIENT_ID o client_id)." >&2
  exit 1
}
CLIENT_SECRET="$(read_key MICROSOFT_CLIENT_SECRET secret_value)" || {
  echo "Falta el secreto en $KEYS_FILE (MICROSOFT_CLIENT_SECRET o secret_value)." >&2
  exit 1
}

guid='^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'

if [[ ! "$CLIENT_ID" =~ $guid ]]; then
  echo "El client id no tiene forma de GUID. Revisá que sea el 'Application (client) ID'." >&2
  exit 1
fi

# El error nº1 de este flujo: Azure muestra 'Value' y 'Secret ID' uno al lado del
# otro, y el Secret ID es un GUID que no sirve para autenticar. Falla acá y no
# con un AADSTS7000215 críptico en producción.
if [[ "$CLIENT_SECRET" =~ $guid ]]; then
  echo "El secreto tiene forma de GUID: copiaste el 'Secret ID' en vez del 'Value'." >&2
  echo "Volvé a Azure > Certificates & secrets y copiá la columna Value." >&2
  exit 1
fi

# gcloud separa --update-env-vars por comas y el script de deploy hace word
# splitting: una coma o un espacio en el secreto se desplegaría truncado en
# silencio. Un secreto truncado falla recién al primer login real.
if [[ "$CLIENT_SECRET" == *,* || "$CLIENT_SECRET" == *" "* ]]; then
  echo "El secreto contiene coma o espacio y no se puede pasar por esta vía sin truncarse." >&2
  echo "Generá otro secreto en Azure, o usá Secret Manager para este valor." >&2
  exit 1
fi

echo "==> Credenciales validadas"
echo "    client id:  ${CLIENT_ID:0:8}…  (GUID)"
echo "    secreto:    ${#CLIENT_SECRET} caracteres, no-GUID"
echo "    redirect:   $REDIRECT_URL"
echo "    tenant:     $TENANT"
echo

if [[ "$check_only" == true ]]; then
  echo "Solo validación (--check). No se desplegó nada."
  exit 0
fi

deploy_args=(
  --update-env-vars
  "MICROSOFT_CLIENT_ID=${CLIENT_ID},MICROSOFT_CLIENT_SECRET=${CLIENT_SECRET},MICROSOFT_REDIRECT_URL=${REDIRECT_URL},MICROSOFT_TENANT=${TENANT}"
)

if [[ "$promote" == true ]]; then
  echo "==> Desplegando y moviendo el 100% del tráfico"
  API_DEPLOY_EXTRA_ARGS="${deploy_args[*]}" scripts/redeploy-gcp.sh api
  base_url="$(gcloud run services describe "$API_SERVICE" \
    --region="$GCP_REGION" --project="$GCP_PROJECT_ID" --format='value(status.url)')"
else
  echo "==> Desplegando revisión candidata sin tráfico"
  API_DEPLOY_EXTRA_ARGS="${deploy_args[*]}" scripts/redeploy-gcp.sh --no-traffic api
  service_url="$(gcloud run services describe "$API_SERVICE" \
    --region="$GCP_REGION" --project="$GCP_PROJECT_ID" --format='value(status.url)')"
  base_url="https://candidate---${service_url#https://}"
fi

echo
echo "==> Verificando que la API ofrezca Microsoft"
providers="$(curl -fsS --max-time 30 "${base_url}/mailbox/providers" || true)"
echo "    GET ${base_url}/mailbox/providers"
echo "    -> ${providers:-(sin respuesta)}"

if [[ "$providers" != *'"microsoft"'* ]]; then
  echo
  echo "FALLÓ: la API no está ofreciendo Microsoft." >&2
  echo "Revisá que las cuatro variables hayan quedado en la revisión:" >&2
  echo "  gcloud run services describe $API_SERVICE --region=$GCP_REGION --format=yaml | grep -A1 MICROSOFT_CLIENT_ID" >&2
  exit 1
fi

echo
echo "OK: la API ofrece google y microsoft."
if [[ "$promote" == false ]]; then
  echo
  echo "La revisión candidata NO tiene tráfico de usuarios. Para promoverla:"
  echo "  scripts/deploy-microsoft.sh --promote"
fi

# --- Si más adelante querés mover el secreto a Secret Manager ---
#
#   printf '%s' "$(sed -nE 's/^secret_value=//p' .keys)" | \
#     gcloud secrets create microsoft-client-secret \
#       --project=gmail-helpdesk-inspector --replication-policy=automatic --data-file=-
#   gcloud secrets add-iam-policy-binding microsoft-client-secret \
#     --project=gmail-helpdesk-inspector \
#     --member="serviceAccount:ghmi-api-postgres-runtime@gmail-helpdesk-inspector.iam.gserviceaccount.com" \
#     --role="roles/secretmanager.secretAccessor"
#
# y en este script cambiar --update-env-vars por:
#   --update-secrets MICROSOFT_CLIENT_SECRET=microsoft-client-secret:latest
# dejando MICROSOFT_CLIENT_ID/REDIRECT_URL/TENANT en --update-env-vars (no son
# secretos). El código de la API no cambia: Cloud Run monta el secreto como
# variable de entorno igual.
#
# Ojo: eso NO borra el valor de las revisiones ya desplegadas. Para eso hay que
# borrar esas revisiones y rotar el secreto en Azure.
