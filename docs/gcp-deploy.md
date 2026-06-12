# Despliegue en GCP

Este runbook despliega tres servicios en Cloud Run:

- `ghmi-ai-worker`: privado, recibe llamadas solo desde la API.
- `ghmi-api`: publico, maneja OAuth Google, Gmail, Firestore y sesiones.
- `ghmi-web`: publico, sirve el frontend estatico.

## 1. Variables locales

Reemplaza `tu-project-id-real` por el ID real del proyecto GCP, no por el nombre visible del proyecto ni por el OAuth Client ID.

```bash
export GCP_PROJECT_ID="tu-project-id-real"
export GCP_REGION="us-central1"
export ARTIFACT_REPO="ghmi"
export API_SERVICE="ghmi-api"
export WORKER_SERVICE="ghmi-ai-worker"
export WEB_SERVICE="ghmi-web"
export RUN_SA="ghmi-runtime@${GCP_PROJECT_ID}.iam.gserviceaccount.com"

gcloud projects describe "$GCP_PROJECT_ID" --format="value(projectId)"
```

## 2. APIs y service account

```bash
: "${GCP_PROJECT_ID:?Ejecuta primero el bloque de Variables locales y define GCP_PROJECT_ID}"
: "${GCP_REGION:?Ejecuta primero el bloque de Variables locales y define GCP_REGION}"
: "${ARTIFACT_REPO:?Ejecuta primero el bloque de Variables locales y define ARTIFACT_REPO}"
: "${RUN_SA:?Ejecuta primero el bloque de Variables locales y define RUN_SA}"

gcloud config set project "$GCP_PROJECT_ID"

gcloud services enable \
  artifactregistry.googleapis.com \
  run.googleapis.com \
  firestore.googleapis.com \
  secretmanager.googleapis.com

gcloud artifacts repositories create "$ARTIFACT_REPO" \
  --repository-format=docker \
  --location="$GCP_REGION" \
  --description="Gmail Helpdesk Inspector images"

gcloud iam service-accounts create ghmi-runtime \
  --display-name="Gmail Helpdesk Inspector runtime"
```

Si el proyecto aun no tiene base Firestore, creala como Firestore Native en la consola de Google Cloud antes del primer analisis.

Permisos minimos del runtime:

```bash
gcloud projects add-iam-policy-binding "$GCP_PROJECT_ID" \
  --member="serviceAccount:${RUN_SA}" \
  --role="roles/datastore.user"

gcloud projects add-iam-policy-binding "$GCP_PROJECT_ID" \
  --member="serviceAccount:${RUN_SA}" \
  --role="roles/secretmanager.secretAccessor"
```

## 3. Secret Manager

Ejecuta este bloque desde la raiz del repo. Primero carga tu `.env` en la terminal:

```bash
set -a
source .env
set +a
```

Si `APP_ENCRYPTION_KEY` o `APP_SESSION_SECRET` siguen con valores de ejemplo, genera valores reales para el despliegue:

```bash
export APP_ENCRYPTION_KEY="$(openssl rand -base64 32)"
export APP_SESSION_SECRET="$(openssl rand -base64 48)"
```

Luego valida que las variables necesarias existan y subelas a Secret Manager:

```bash
for name in GOOGLE_CLIENT_SECRET APP_ENCRYPTION_KEY APP_SESSION_SECRET AWS_BEARER_TOKEN_BEDROCK; do
  if [ -z "${!name:-}" ]; then
    echo "Falta definir $name"
    exit 1
  fi
done

gcloud secrets describe google-client-secret >/dev/null 2>&1 || gcloud secrets create google-client-secret
printf '%s' "$GOOGLE_CLIENT_SECRET" | gcloud secrets versions add google-client-secret --data-file=-

gcloud secrets describe app-encryption-key >/dev/null 2>&1 || gcloud secrets create app-encryption-key
printf '%s' "$APP_ENCRYPTION_KEY" | gcloud secrets versions add app-encryption-key --data-file=-

gcloud secrets describe app-session-secret >/dev/null 2>&1 || gcloud secrets create app-session-secret
printf '%s' "$APP_SESSION_SECRET" | gcloud secrets versions add app-session-secret --data-file=-

gcloud secrets describe bedrock-token >/dev/null 2>&1 || gcloud secrets create bedrock-token
printf '%s' "$AWS_BEARER_TOKEN_BEDROCK" | gcloud secrets versions add bedrock-token --data-file=-

gcloud secrets list --filter='name:(google-client-secret OR app-encryption-key OR app-session-secret OR bedrock-token)'
```

## 4. Build de imagenes

```bash
export IMAGE_BASE="${GCP_REGION}-docker.pkg.dev/${GCP_PROJECT_ID}/${ARTIFACT_REPO}"

gcloud auth configure-docker "${GCP_REGION}-docker.pkg.dev"

docker build -f apps/ai-worker/Dockerfile \
  -t "${IMAGE_BASE}/ai-worker:latest" .
docker push "${IMAGE_BASE}/ai-worker:latest"

docker build -f apps/api/Dockerfile \
  -t "${IMAGE_BASE}/api:latest" .
docker push "${IMAGE_BASE}/api:latest"
```

## 5. Desplegar worker privado

```bash
gcloud run deploy "$WORKER_SERVICE" \
  --image "${IMAGE_BASE}/ai-worker:latest" \
  --region "$GCP_REGION" \
  --service-account "$RUN_SA" \
  --no-allow-unauthenticated \
  --set-env-vars "AWS_REGION=us-east-1,BEDROCK_MODEL_ID=amazon.nova-2-lite-v1:0" \
  --set-secrets "AWS_BEARER_TOKEN_BEDROCK=bedrock-token:latest"

export WORKER_URL="$(gcloud run services describe "$WORKER_SERVICE" --region "$GCP_REGION" --format='value(status.url)')"
```

Autoriza a la API para invocar el worker:

```bash
gcloud run services add-iam-policy-binding "$WORKER_SERVICE" \
  --region "$GCP_REGION" \
  --member="serviceAccount:${RUN_SA}" \
  --role="roles/run.invoker"
```

## 6. Desplegar API

Primer despliegue para obtener URL:

```bash
gcloud run deploy "$API_SERVICE" \
  --image "${IMAGE_BASE}/api:latest" \
  --region "$GCP_REGION" \
  --service-account "$RUN_SA" \
  --allow-unauthenticated \
  --cpu 1 \
  --memory 512Mi \
  --no-cpu-throttling \
  --set-env-vars "APP_STORAGE=firestore,GCP_PROJECT_ID=${GCP_PROJECT_ID},FIRESTORE_DATABASE_ID=(default),AI_WORKER_URL=${WORKER_URL},AI_WORKER_AUDIENCE=${WORKER_URL},WEB_BASE_URL=https://placeholder.invalid,API_BASE_URL=https://placeholder.invalid,APP_COOKIE_SECURE=true,APP_COOKIE_SAMESITE=None,GOOGLE_CLIENT_ID=${GOOGLE_CLIENT_ID},GOOGLE_REDIRECT_URL=https://placeholder.invalid/auth/google/callback,GMAIL_MAX_THREADS=100" \
  --set-secrets "GOOGLE_CLIENT_SECRET=google-client-secret:latest,APP_ENCRYPTION_KEY=app-encryption-key:latest,APP_SESSION_SECRET=app-session-secret:latest"

export API_URL="$(gcloud run services describe "$API_SERVICE" --region "$GCP_REGION" --format='value(status.url)')"
```

Actualiza la API con su URL. El redirect OAuth final se configura despues de desplegar la web:

```bash
gcloud run services update "$API_SERVICE" \
  --region "$GCP_REGION" \
  --update-env-vars "API_BASE_URL=${API_URL}"
```

## 7. Build y despliegue web

```bash
docker build -f apps/web/Dockerfile \
  --build-arg "VITE_API_BASE_URL=" \
  -t "${IMAGE_BASE}/web:latest" .
docker push "${IMAGE_BASE}/web:latest"
```

Despliega:

```bash
gcloud run deploy "$WEB_SERVICE" \
  --image "${IMAGE_BASE}/web:latest" \
  --region "$GCP_REGION" \
  --allow-unauthenticated \
  --set-env-vars "API_PROXY_TARGET=${API_URL}"

export WEB_URL="$(gcloud run services describe "$WEB_SERVICE" --region "$GCP_REGION" --format='value(status.url)')"
```

Actualiza la API con la URL final del frontend y el redirect que pasa por el proxy de la web:

```bash
gcloud run services update "$API_SERVICE" \
  --region "$GCP_REGION" \
  --update-env-vars "WEB_BASE_URL=${WEB_URL},GOOGLE_REDIRECT_URL=${WEB_URL}/auth/google/callback"
```

Agrega `GOOGLE_REDIRECT_URL` en tu OAuth Client de Google Cloud:

```text
${WEB_URL}/auth/google/callback
```

## 8. Smoke test

```bash
curl -fsS "${API_URL}/health"
```

Si tu usuario tiene permiso `run.invoker` sobre el worker privado, tambien puedes probarlo asi:

```bash
curl -fsS "${WORKER_URL}/health" -H "Authorization: Bearer $(gcloud auth print-identity-token --audiences="${WORKER_URL}")"
```

Abre `WEB_URL` en el navegador e inicia sesion con Google.

## 9. Dominio propio

Para este proyecto usa dos subdominios:

- Web: `helpdesk.tu-dominio.cl` -> servicio `ghmi-web`.
- API: `api-helpdesk.tu-dominio.cl` -> servicio `ghmi-api`.

El worker queda privado y no necesita dominio propio.

```bash
export BASE_DOMAIN="tu-dominio.cl"
export WEB_DOMAIN="helpdesk.${BASE_DOMAIN}"
export API_DOMAIN="api-helpdesk.${BASE_DOMAIN}"
```

Verifica propiedad del dominio base en Google:

```bash
gcloud domains list-user-verified
gcloud domains verify "$BASE_DOMAIN"
```

Crea los mappings en Cloud Run:

```bash
gcloud beta run domain-mappings create \
  --service "$WEB_SERVICE" \
  --domain "$WEB_DOMAIN" \
  --region "$GCP_REGION"

gcloud beta run domain-mappings create \
  --service "$API_SERVICE" \
  --domain "$API_DOMAIN" \
  --region "$GCP_REGION"
```

Obtén los registros DNS que debes crear:

```bash
gcloud beta run domain-mappings describe \
  --domain "$WEB_DOMAIN" \
  --region "$GCP_REGION" \
  --format="yaml(status.resourceRecords)"

gcloud beta run domain-mappings describe \
  --domain "$API_DOMAIN" \
  --region "$GCP_REGION" \
  --format="yaml(status.resourceRecords)"
```

Crea esos registros en el proveedor DNS que uses. Si usas Cloudflare, ponlos primero como `DNS only` hasta que el certificado de Google quede activo.

Actualiza la API para que use los dominios finales:

```bash
export WEB_URL="https://${WEB_DOMAIN}"
export API_URL="https://${API_DOMAIN}"

gcloud run services update "$API_SERVICE" \
  --region "$GCP_REGION" \
  --update-env-vars "WEB_BASE_URL=${WEB_URL},API_BASE_URL=${API_URL},GOOGLE_REDIRECT_URL=${WEB_URL}/auth/google/callback"
```

Reconstruye y redespliega la web. Para evitar cookies third-party entre dos dominios `*.run.app`, la web debe llamar a la API por el mismo origen y el servidor web proxy reenvia a Cloud Run API:

```bash
docker build -f apps/web/Dockerfile \
  --build-arg "VITE_API_BASE_URL=" \
  -t "${IMAGE_BASE}/web:latest" .
docker push "${IMAGE_BASE}/web:latest"

gcloud run deploy "$WEB_SERVICE" \
  --image "${IMAGE_BASE}/web:latest" \
  --region "$GCP_REGION" \
  --allow-unauthenticated \
  --set-env-vars "API_PROXY_TARGET=${API_URL}"
```

En el OAuth Client de Google agrega:

- URI de redireccionamiento: `https://${WEB_DOMAIN}/auth/google/callback`.
- Origen JavaScript autorizado: `https://${WEB_DOMAIN}`.

## 10. Programador diario (Cloud Scheduler)

El analisis programado corre de lunes a viernes a las 08:00 (America/Santiago)
y envia el reporte por correo via Resend. En Cloud Run usa Cloud Scheduler con
`SCHEDULER_ENABLED=false`: el servicio puede escalar a cero y el loop interno
no es confiable ahi. El loop interno (`SCHEDULER_ENABLED=true`) es para hosts
always-on como docker-compose o una VPS.

Sube los secretos nuevos:

```bash
export CRON_SECRET="$(openssl rand -base64 32)"
# RESEND_API_KEY debe venir de tu cuenta de Resend (cargala en el .env).

gcloud secrets describe cron-secret >/dev/null 2>&1 || gcloud secrets create cron-secret
printf '%s' "$CRON_SECRET" | gcloud secrets versions add cron-secret --data-file=-

gcloud secrets describe resend-api-key >/dev/null 2>&1 || gcloud secrets create resend-api-key
printf '%s' "$RESEND_API_KEY" | gcloud secrets versions add resend-api-key --data-file=-
```

Actualiza la API con la configuracion del reporte. El prefijo `^|^` cambia el
separador de gcloud de coma a `|`, necesario porque `REPORT_TO_EMAIL` y
`SCHEDULE_INTERNAL_DOMAINS` pueden llevar comas (con el `.env` cargado en la
terminal, las variables se expanden solas):

```bash
gcloud run services update "$API_SERVICE" \
  --region "$GCP_REGION" \
  --update-env-vars "^|^SCHEDULER_ENABLED=false|REPORT_FROM_EMAIL=${REPORT_FROM_EMAIL}|REPORT_TO_EMAIL=${REPORT_TO_EMAIL}|SCHEDULE_USER_EMAIL=${SCHEDULE_USER_EMAIL}|SCHEDULE_INTERNAL_DOMAINS=${SCHEDULE_INTERNAL_DOMAINS}|SCHEDULE_GMAIL_MAX_THREADS=${SCHEDULE_GMAIL_MAX_THREADS}" \
  --update-secrets "CRON_SECRET=cron-secret:latest,RESEND_API_KEY=resend-api-key:latest"
```

Habilita Cloud Scheduler y crea el job (el deadline alto importa: el endpoint
espera el analisis completo antes de responder):

```bash
gcloud services enable cloudscheduler.googleapis.com

gcloud scheduler jobs create http ghmi-daily-report \
  --location "$GCP_REGION" \
  --schedule "0 8 * * 1-5" \
  --time-zone "America/Santiago" \
  --uri "${API_URL}/internal/scheduled-analysis" \
  --http-method POST \
  --headers "x-cron-secret=${CRON_SECRET}" \
  --attempt-deadline 1800s
```

Para probar el job sin esperar a las 08:00:

```bash
gcloud scheduler jobs run ghmi-daily-report --location "$GCP_REGION"
gcloud scheduler jobs describe ghmi-daily-report --location "$GCP_REGION" --format='value(status)'
```

Notas:

- La primera ejecucion siembra `scheduleConfigs/{email}` en Firestore desde las
  variables `SCHEDULE_*`; despues edita ese documento directamente para cambiar
  destinatarios, dominios o listas ignoradas.
- Una ventana ya completada no se repite (estado en `scheduleStates/{email}`),
  asi que los reintentos de Cloud Scheduler tras un exito devuelven `skipped`.
- Para rellenar un dia perdido dispara manualmente con
  `-d '{"as_of_date":"YYYY-MM-DD"}'`.
- Mientras la app OAuth este en estado "Testing", los refresh token caducan a
  los 7 dias y llegara un correo pidiendo iniciar sesion de nuevo; publica la
  app a Production para evitarlo.

## Troubleshooting login en Cloud Run

Si el callback de Google funciona pero la web vuelve al login y `/auth/me` responde `401`, revisa:

1. En modo recomendado, la web debe estar construida con `VITE_API_BASE_URL=` y desplegada con `API_PROXY_TARGET=${API_URL}`.
2. La API debe tener `WEB_BASE_URL` igual al origen exacto de la web.
3. El OAuth Client debe usar como redirect la URL de la web: `${WEB_URL}/auth/google/callback`.
4. La API debe usar ese mismo redirect:

```bash
gcloud run services update "$API_SERVICE" \
  --region "$GCP_REGION" \
  --update-env-vars "WEB_BASE_URL=${WEB_URL},GOOGLE_REDIRECT_URL=${WEB_URL}/auth/google/callback"
```

Si aun llamas directo desde web a API en dominios distintos, la API debe tener cookies cross-site activas:

```bash
gcloud run services update "$API_SERVICE" \
  --region "$GCP_REGION" \
  --update-env-vars "APP_COOKIE_SECURE=true,APP_COOKIE_SAMESITE=None"
```

Luego mira la causa exacta en logs:

```bash
gcloud run services logs read "$API_SERVICE" \
  --region "$GCP_REGION" \
  --limit 50 \
  --log-filter 'textPayload:"auth rejected"'
```

Si el log dice `ghmi_session cookie missing`, el navegador no esta enviando la cookie. En `*.run.app` esto puede pasar por bloqueo de cookies de terceros; usa dominios propios bajo el mismo dominio base para web y API.

## Troubleshooting analisis muy lento

La API inicia el analisis en background despues de responder `POST /analysis-runs/:id/start`. En Cloud Run, el modo por defecto puede asignar CPU solo mientras hay requests activos; eso vuelve muy lento cualquier trabajo en background.

Configura la API con CPU siempre disponible durante la vida de la instancia:

```bash
gcloud run services update "$API_SERVICE" \
  --region "$GCP_REGION" \
  --cpu 1 \
  --memory 512Mi \
  --no-cpu-throttling
```

Esto no mantiene la instancia viva para siempre. Cloud Run igual puede escalar a cero; simplemente evita que el analisis quede casi congelado despues de que termina el request que lo inicio.

## Notas importantes

- Cloud Run inyecta `PORT`; los contenedores ya lo respetan.
- En Cloud Run no uses `GOOGLE_APPLICATION_CREDENTIALS`; la API usa metadata server y la service account runtime.
- El worker queda privado. La API envia un identity token si `AI_WORKER_AUDIENCE` esta configurado.
- Si cambias `WEB_URL` o `API_URL`, revisa CORS (`WEB_BASE_URL`) y OAuth redirect (`GOOGLE_REDIRECT_URL`).
- Cloud Run Domain Mapping esta en Preview y Google no lo recomienda para servicios productivos criticos. Para algo mas robusto, usa un External Application Load Balancer delante de Cloud Run.
- Si usas Cloudflare con proxy, evita activar `Always Use HTTPS` mientras Google valida o renueva certificados.
