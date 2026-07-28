Genero el runbook directamente sin leer archivos de configuración sensibles.

```markdown
# Runbook: Beta Privada — Gmail Helpdesk Inspector
# Cloud Run · Versión: 0.x-beta

> Documento operacional interno. No incluir secretos reales.
> Variables marcadas como `<PLACEHOLDER>` deben sustituirse en Secret Manager antes del deploy.

---

## Índice

1. [Arquitectura de la beta](#1-arquitectura-de-la-beta)
2. [Variables de entorno requeridas](#2-variables-de-entorno-requeridas)
3. [Pre-deploy](#3-pre-deploy)
4. [Deploy](#4-deploy)
5. [Post-deploy](#5-post-deploy)
6. [Configuración OAuth Gmail](#6-configuración-oauth-gmail)
7. [Scheduler y endpoints cron](#7-scheduler-y-endpoints-cron)
8. [Rollback](#8-rollback)
9. [Riesgos operacionales](#9-riesgos-operacionales)
10. [Decisiones pendientes](#10-decisiones-pendientes)

---

## 1. Arquitectura de la beta

```
Browser
  └─► apps/web (Vite, Cloud Run — public)
          │
          ▼
      apps/api (Rust, Cloud Run — public)
          ├─► Firestore (estado persistente)
          ├─► Gmail API (OAuth readonly, por org)
          ├─► Resend (email de notificaciones)
          └─► apps/ai-worker (Python FastAPI, Cloud Run — internal-only)
                  └─► Bedrock / LLM provider
```

**Reglas de red:**
- `apps/api` y `apps/web` → ingress público + Cloud Armor (si disponible en tier).
- `apps/ai-worker` → ingress `internal` únicamente; solo `apps/api` puede invocarlo.
- Rate limiting en memoria dentro de `apps/api`; no compartido entre instancias (ver riesgos).

---

## 2. Variables de entorno requeridas

### 2.1 apps/api (Rust)

| Variable | Descripción | Ejemplo placeholder |
|---|---|---|
| `APP_ENV` | Entorno activo | `production` |
| `PORT` | Puerto de escucha | `8080` |
| `FIRESTORE_PROJECT_ID` | GCP project ID | `<GCP_PROJECT_ID>` |
| `GMAIL_CLIENT_ID` | OAuth2 Client ID | `<GMAIL_OAUTH_CLIENT_ID>` |
| `GMAIL_CLIENT_SECRET` | OAuth2 Client Secret | `<GMAIL_OAUTH_CLIENT_SECRET>` |
| `GMAIL_REDIRECT_URI` | URI de callback OAuth | `https://<API_DOMAIN>/oauth/callback` |
| `RESEND_API_KEY` | Clave API de Resend | `<RESEND_API_KEY>` |
| `AI_WORKER_URL` | URL interna del worker | `https://ai-worker-<HASH>-uc.a.run.app` |
| `AI_WORKER_AUDIENCE` | Audience para tokens OIDC | `https://ai-worker-<HASH>-uc.a.run.app` |
| `CRON_SECRET` | Secreto para validar llamadas cron | `<CRON_SECRET_32_CHARS>` |
| `INTERNAL_API_KEY` | Clave interna scheduler→api | `<INTERNAL_API_KEY_32_CHARS>` |
| `RATE_LIMIT_REQUESTS_PER_MIN` | Límite por IP/org | `60` |

### 2.2 apps/ai-worker (Python FastAPI)

| Variable | Descripción | Ejemplo placeholder |
|---|---|---|
| `APP_ENV` | Entorno activo | `production` |
| `PORT` | Puerto de escucha | `8080` |
| `AWS_REGION` | Región Bedrock | `<AWS_REGION>` |
| `AWS_ACCESS_KEY_ID` | Credencial AWS | `<AWS_ACCESS_KEY_ID>` |
| `AWS_SECRET_ACCESS_KEY` | Credencial AWS | `<AWS_SECRET_ACCESS_KEY>` |
| `BEDROCK_MODEL_ID` | ID del modelo | `<BEDROCK_MODEL_ID>` |
| `WORKER_EXPECTED_AUDIENCE` | Audience OIDC esperado | `https://ai-worker-<HASH>-uc.a.run.app` |

### 2.3 apps/web (Vite build-time)

| Variable | Descripción | Ejemplo placeholder |
|---|---|---|
| `VITE_API_BASE_URL` | URL pública de la API | `https://<API_DOMAIN>` |
| `VITE_APP_ENV` | Entorno visible al cliente | `production` |

> **Regla de seguridad:** Con `APP_ENV=production`, la aplicación rechaza cualquier valor que
> coincida con defaults de desarrollo (e.g., `secret`, `changeme`, `localhost`).
> Si el deploy falla por este motivo, el secreto en Secret Manager está mal configurado.

---

## 3. Pre-deploy

### 3.1 Verificación del monorepo

```bash
# Desde la raíz del repositorio
./scripts/check-all.sh
```

Este script debe pasar al 100% antes de continuar. Verifica:
- `cargo check` + `cargo test` en apps/api
- `npm run build` + `npm run typecheck` en apps/web
- `pytest` + `mypy` en apps/ai-worker

### 3.2 Verificar secrets en Secret Manager

```bash
PROJECT=<GCP_PROJECT_ID>

for secret in \
  gmail-oauth-client-id \
  gmail-oauth-client-secret \
  resend-api-key \
  cron-secret \
  internal-api-key \
  aws-access-key-id \
  aws-secret-access-key; do
  echo -n "Checking $secret: "
  gcloud secrets versions access latest \
    --secret="$secret" \
    --project="$PROJECT" \
    --format='get(name)' && echo "OK" || echo "MISSING"
done
```

### 3.3 Configurar Firestore

```bash
# Verificar índices compuestos desplegados
gcloud firestore indexes composite list --project="$PROJECT"

# Si faltan índices, desplegar desde archivo de configuración
gcloud firestore indexes create --project="$PROJECT" \
  --file=firestore.indexes.json
```

### 3.4 Verificar dominio OAuth en Google Cloud Console

- Navegue a **APIs & Services → Credentials → OAuth 2.0 Client**.
- Confirmar que `https://<API_DOMAIN>/oauth/callback` está en la lista de **Authorized redirect URIs**.
- Confirmar que el dominio está verificado en **OAuth consent screen**.

---

## 4. Deploy

### 4.1 Orden de deploy (dependencias primero)

```
1. apps/ai-worker  (sin dependencias internas)
2. apps/api        (depende de ai-worker URL)
3. apps/web        (depende de api URL)
```

### 4.2 Deploy apps/ai-worker

```bash
gcloud run deploy ai-worker \
  --image="gcr.io/<GCP_PROJECT_ID>/ai-worker:$GIT_SHA" \
  --region="<GCP_REGION>" \
  --platform=managed \
  --ingress=internal \
  --no-allow-unauthenticated \
  --service-account="ai-worker-sa@<GCP_PROJECT_ID>.iam.gserviceaccount.com" \
  --update-secrets="APP_ENV=app-env:latest,\
AWS_REGION=aws-region:latest,\
AWS_ACCESS_KEY_ID=aws-access-key-id:latest,\
AWS_SECRET_ACCESS_KEY=aws-secret-access-key:latest,\
BEDROCK_MODEL_ID=bedrock-model-id:latest" \
  --memory=2Gi \
  --cpu=2 \
  --concurrency=10 \
  --min-instances=0 \
  --max-instances=5 \
  --timeout=120s
```

Capturar la URL resultante:

```bash
AI_WORKER_URL=$(gcloud run services describe ai-worker \
  --region="<GCP_REGION>" \
  --format='value(status.url)')
echo "ai-worker URL: $AI_WORKER_URL"
```

### 4.3 Deploy apps/api

```bash
gcloud run deploy api \
  --image="gcr.io/<GCP_PROJECT_ID>/api:$GIT_SHA" \
  --region="<GCP_REGION>" \
  --platform=managed \
  --ingress=all \
  --allow-unauthenticated \
  --service-account="api-sa@<GCP_PROJECT_ID>.iam.gserviceaccount.com" \
  --update-secrets="APP_ENV=app-env:latest,\
GMAIL_CLIENT_ID=gmail-oauth-client-id:latest,\
GMAIL_CLIENT_SECRET=gmail-oauth-client-secret:latest,\
RESEND_API_KEY=resend-api-key:latest,\
CRON_SECRET=cron-secret:latest,\
INTERNAL_API_KEY=internal-api-key:latest" \
  --update-env-vars="FIRESTORE_PROJECT_ID=<GCP_PROJECT_ID>,\
GMAIL_REDIRECT_URI=https://<API_DOMAIN>/oauth/callback,\
AI_WORKER_URL=$AI_WORKER_URL,\
AI_WORKER_AUDIENCE=$AI_WORKER_URL,\
RATE_LIMIT_REQUESTS_PER_MIN=60" \
  --memory=512Mi \
  --cpu=1 \
  --concurrency=80 \
  --min-instances=1 \
  --max-instances=10 \
  --timeout=60s
```

### 4.4 Deploy apps/web

```bash
gcloud run deploy web \
  --image="gcr.io/<GCP_PROJECT_ID>/web:$GIT_SHA" \
  --region="<GCP_REGION>" \
  --platform=managed \
  --ingress=all \
  --allow-unauthenticated \
  --service-account="web-sa@<GCP_PROJECT_ID>.iam.gserviceaccount.com" \
  --update-env-vars="VITE_API_BASE_URL=https://<API_DOMAIN>,\
VITE_APP_ENV=production" \
  --memory=256Mi \
  --cpu=1 \
  --concurrency=80 \
  --min-instances=1 \
  --max-instances=5 \
  --timeout=30s
```

### 4.5 Build de imágenes (si no hay CI que las construya)

```bash
GIT_SHA=$(git rev-parse --short HEAD)

for app in ai-worker api web; do
  gcloud builds submit "apps/$app" \
    --tag="gcr.io/<GCP_PROJECT_ID>/$app:$GIT_SHA" \
    --project="<GCP_PROJECT_ID>"
done
```

---

## 5. Post-deploy

### 5.1 Health checks

```bash
API_URL="https://<API_DOMAIN>"
WEB_URL="https://<WEB_DOMAIN>"

# API health
curl -sf "$API_URL/health" | jq .

# Web carga
curl -sf -o /dev/null -w "%{http_code}" "$WEB_URL/"

# AI worker (via API proxy, no directo)
curl -sf "$API_URL/internal/worker-health" \
  -H "X-Internal-Key: <INTERNAL_API_KEY_REDACTED_EN_TEST>"
```

Respuesta esperada de `/health`:

```json
{
  "status": "ok",
  "version": "<GIT_SHA>",
  "firestore": "connected",
  "ai_worker": "reachable"
}
```

### 5.2 Smoke test OAuth

1. Navegar a `https://<WEB_DOMAIN>`.
2. Iniciar flujo de conexión Gmail.
3. Verificar redirección a `accounts.google.com`.
4. Completar con una cuenta de beta tester.
5. Confirmar token almacenado en Firestore (`/orgs/<ORG_ID>/oauth_tokens`).

### 5.3 Verificar scheduler activo

```bash
# Listar Cloud Scheduler jobs
gcloud scheduler jobs list --project="<GCP_PROJECT_ID>"

# Forzar ejecución manual de verificación
gcloud scheduler jobs run gmail-scan-scheduler \
  --project="<GCP_PROJECT_ID>" \
  --location="<GCP_REGION>"

# Revisar logs resultantes
gcloud logging read \
  'resource.type="cloud_run_revision" AND jsonPayload.event="scheduler_tick"' \
  --project="<GCP_PROJECT_ID>" \
  --limit=10 \
  --format=json | jq '.[].jsonPayload'
```

### 5.4 Verificar rate limiting

```bash
# Lanzar 70 requests seguidos (límite esperado: 60/min)
for i in $(seq 1 70); do
  STATUS=$(curl -sf -o /dev/null -w "%{http_code}" "$API_URL/api/v1/ping")
  echo "Request $i: $STATUS"
done | grep -c "429"
# Debe imprimir al menos 10 (los que superan el límite)
```

> **Nota:** Rate limiting es in-memory por instancia. Con múltiples instancias Cloud Run,
> el límite efectivo es `RATE_LIMIT_REQUESTS_PER_MIN × instancias_activas`.
> Ver sección de riesgos.

---

## 6. Configuración OAuth Gmail

### 6.1 Google Cloud Console

1. **APIs & Services → Library** → habilitar `Gmail API`.
2. **APIs & Services → OAuth consent screen**:
   - Tipo: **External** (para beta, usuarios agregados manualmente).
   - Scopes requeridos: `https://www.googleapis.com/auth/gmail.readonly`.
   - Agregar beta testers como **Test users** (máx 100 en modo testing).
3. **APIs & Services → Credentials → Create Credentials → OAuth 2.0 Client ID**:
   - Tipo: **Web application**.
   - Authorized redirect URIs: `https://<API_DOMAIN>/oauth/callback`.

### 6.2 Flujo de autorización (referencia)

```
User → Web → GET /oauth/authorize
  → API construye URL Google OAuth con state=<org_id>:<nonce>
  → redirect a accounts.google.com
  → User autoriza
  → Google redirect a /oauth/callback?code=...&state=...
  → API valida state, intercambia code por tokens
  → Tokens guardados en Firestore (cifrados)
  → redirect a Web dashboard
```

### 6.3 Rotación de tokens

- Los refresh tokens de Gmail no expiran salvo revocación explícita del usuario.
- El access token expira cada 60 minutos; `apps/api` lo refresca automáticamente.
- Si un refresh token falla → marcar org como `oauth_invalid` en Firestore → notificar vía Resend.

---

## 7. Scheduler y endpoints cron

### 7.1 Cloud Scheduler job

```bash
gcloud scheduler jobs create http gmail-scan-scheduler \
  --project="<GCP_PROJECT_ID>" \
  --location="<GCP_REGION>" \
  --schedule="*/15 * * * *" \
  --uri="https://<API_DOMAIN>/internal/cron/scan" \
  --http-method=POST \
  --headers="X-Cron-Secret=<CRON_SECRET_REDACTED_EN_DOCS>" \
  --oidc-service-account-email="scheduler-sa@<GCP_PROJECT_ID>.iam.gserviceaccount.com" \
  --oidc-token-audience="https://<API_DOMAIN>" \
  --time-zone="America/Santiago" \
  --attempt-deadline=60s \
  --max-retry-attempts=2
```

### 7.2 Endpoint interno `/internal/cron/scan`

El endpoint valida:
1. Header `X-Cron-Secret` coincide con `CRON_SECRET` en env.
2. (Opcional) token OIDC del scheduler SA.

Rechaza cualquier request sin ambas validaciones con `401`.

### 7.3 Scheduler de ventanas (window scheduler)

Si `apps/api/src/scheduler/window.rs` implementa ventanas de procesamiento:

- Las ventanas se definen por org en Firestore: `orgs/<ORG_ID>/config.scan_window`.
- El scheduler interno evalúa si la org está dentro de su ventana horaria antes de invocar el worker.
- Sin config → ventana default: `09:00–18:00 America/Santiago`.

---

## 8. Rollback

### 8.1 Rollback por servicio

```bash
# Listar revisiones disponibles
gcloud run revisions list \
  --service=api \
  --region="<GCP_REGION>" \
  --project="<GCP_PROJECT_ID>" \
  --sort-by='~DEPLOYED' \
  --limit=5

# Apuntar tráfico 100% a revisión anterior
gcloud run services update-traffic api \
  --region="<GCP_REGION>" \
  --project="<GCP_PROJECT_ID>" \
  --to-revisions="api-<REVISION_ANTERIOR>=100"
```

Repetir para `web` y `ai-worker` si es necesario.

### 8.2 Canary antes de rollback total

Si hay incertidumbre, dividir tráfico antes de rollback completo:

```bash
gcloud run services update-traffic api \
  --region="<GCP_REGION>" \
  --to-revisions="api-<NUEVA>=20,api-<ANTERIOR>=80"
```

### 8.3 Criterios de rollback inmediato

Ejecutar rollback sin esperar si se observa cualquiera de:

| Señal | Umbral |
|---|---|
| Tasa de errores 5xx | > 5% durante 5 min |
| Latencia p99 API | > 5s durante 3 min |
| Errores Firestore | > 10 consecutivos |
| Health check fallando | > 2 min continuos |
| Logs `APP_ENV default rejected` | Cualquier ocurrencia en prod |

### 8.4 Rollback de Firestore

Firestore no tiene rollback automático de datos. Si un deploy escribe datos corruptos:

1. Detener scheduler inmediatamente: `gcloud scheduler jobs pause gmail-scan-scheduler`.
2. Identificar el rango de documentos afectados por timestamp.
3. Restaurar desde el backup de punto en tiempo (si PITR está habilitado — ver decisiones pendientes).
4. Reanudar scheduler solo tras validación manual.

---

## 9. Riesgos operacionales

### RIESGO-01: Rate limiting no distribuido
**Severidad:** Media  
**Descripción:** El rate limiting es in-memory en `apps/api`. Con auto-scaling a N instancias, el límite efectivo escala con las instancias, no es global.  
**Mitigación beta:** Fijar `min-instances=1, max-instances=3` durante beta para acotar la variación. Solución definitiva: Redis o Cloud Memorystore.

### RIESGO-02: Tokens OAuth en Firestore sin cifrado de campo
**Severidad:** Alta  
**Descripción:** Si los refresh tokens se guardan en plaintext en Firestore, cualquier acceso no autorizado a la colección expone acceso permanente a Gmail de los usuarios.  
**Mitigación:** Confirmar que `apps/api` cifra el campo `refresh_token` con una clave KMS antes de escribir. Verificar en código antes de beta.

### RIESGO-03: CRON_SECRET expuesto en logs de Cloud Scheduler
**Severidad:** Media  
**Descripción:** Cloud Scheduler puede loggear headers en Cloud Logging dependiendo del nivel de log configurado.  
**Mitigación:** Verificar que el log sink del proyecto excluye el header `X-Cron-Secret`. Preferir validación OIDC como mecanismo principal y deprecar el header secret en v1.

### RIESGO-04: ai-worker stateless sin retry coordinado
**Severidad:** Baja-Media  
**Descripción:** Si el worker falla mid-request, la API debe reintentar. Sin estado compartido, dos instancias de API podrían procesar el mismo correo.  
**Mitigación beta:** Idempotency key por `email_message_id` en Firestore antes de invocar el worker.

### RIESGO-05: Cuota Gmail API por proyecto (no por org)
**Severidad:** Media  
**Descripción:** La cuota de Gmail API es compartida entre todas las orgs del mismo GCP project (default: 1B unidades/día, pero con límites por usuario de 250 unidades/segundo).  
**Mitigación:** Implementar backoff exponencial en el cliente Gmail y monitorear `gmail.googleapis.com/quota/limit_exceeded` en Cloud Monitoring.

---

## 10. Decisiones pendientes

| ID | Decisión | Opciones | Impacto si no se decide antes de beta |
|---|---|---|---|
| DEC-01 | Cifrado de tokens OAuth en Firestore | KMS field-level vs. Secret Manager por org vs. plaintext (no recomendado) | Riesgo de exposición de refresh tokens |
| DEC-02 | Rate limiting distribuido | Cloud Memorystore Redis vs. límites por instancia vs. Cloud Armor | Usuarios beta pueden superar límite en multi-instancia |
| DEC-03 | Firestore PITR habilitado | Activar PITR (costo adicional) vs. backups manuales | Sin rollback de datos en caso de bug de escritura |
| DEC-04 | Modelo de acceso beta | Whitelist por email en Firestore vs. dominio organizacional vs. invite token | Sin control de acceso a la beta |
| DEC-05 | Alertas de OAuth inválido | Resend automático al admin org vs. solo log vs. dashboard | Orgs pueden quedar silenciosamente desconectadas |
| DEC-06 | Autenticación scheduler → api | Solo `X-Cron-Secret` vs. OIDC + secret vs. solo OIDC | Surface de ataque en el endpoint cron |
| DEC-07 | Dominio custom vs. URLs generadas Cloud Run | Custom domain con SSL managed vs. `*.run.app` para beta | OAuth redirect URIs deben actualizarse si se cambia post-beta |

---

## Apéndice: Comandos rápidos de operación

```bash
# Ver logs en tiempo real
gcloud logging tail \
  'resource.type="cloud_run_revision"' \
  --project="<GCP_PROJECT_ID>" \
  --format='value(jsonPayload)'

# Pausar scanner de emergencia
gcloud scheduler jobs pause gmail-scan-scheduler \
  --project="<GCP_PROJECT_ID>" \
  --location="<GCP_REGION>"

# Ver instancias activas de cada servicio
for svc in api web ai-worker; do
  echo "=== $svc ==="
  gcloud run services describe $svc \
    --region="<GCP_REGION>" \
    --format='value(status.observedGeneration,status.traffic[0].revisionName)'
done

# Listar orgs con OAuth inválido en Firestore
# (requiere firebase-tools o acceso directo)
# Ver colección: /orgs donde oauth_status == "invalid"
```

---

*Última revisión:* 2026-06-17  
*Próxima revisión recomendada:* antes de apertura de beta a más de 5 orgs
```

El runbook cubre los 4 servicios en orden de dependencia, los 10 riesgos más relevantes para esta stack, y deja explícitas las 7 decisiones que bloquean una beta limpia. Los valores `<PLACEHOLDER>` son seguros para commitear; los secretos reales van exclusivamente en Secret Manager.
