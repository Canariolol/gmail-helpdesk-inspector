# Runbook de despliegue — Beta privada

> Basado en el runbook generado por Claude Code para TASK-025 y corregido con nombres reales verificados en `README.md`, `apps/api/src/config/mod.rs` y `apps/ai-worker/src/ai_worker/settings.py`.
>
> No incluir secretos reales en este documento. Usar placeholders seguros y Secret Manager.

## 1. Arquitectura beta

```text
Browser
  └─► apps/web (Vite)
          │ VITE_API_BASE_URL
          ▼
      apps/api (Rust)
          ├─► PostgreSQL (Supabase, schemas `mira` y `billing`)
          ├─► Gmail API OAuth readonly
          ├─► Resend
          └─► apps/ai-worker (Python FastAPI, stateless)
                  └─► Amazon Bedrock
```

Principios de producción beta:

- Gmail scope se mantiene en `gmail.readonly`.
- El worker IA es stateless: la API envía `policy_context` por request.
- Los reportes salen por Resend, no por Gmail.
- `APP_ENV=production` debe rechazar secretos de desarrollo.
- El rate limit actual es in-memory por instancia; para beta usar una sola instancia API o aceptar explícitamente ese riesgo.

## 2. Variables verificadas

### API (`apps/api`)

| Variable | Uso |
|---|---|
| `APP_ENV` | `production`/`prod` activa guardrails de secretos. |
| `PORT` / `API_PORT` | Puerto API. `PORT` tiene prioridad. |
| `WEB_BASE_URL` | URL pública del frontend para redirects/cookies. |
| `API_BASE_URL` | URL pública de API. También influye en cookie secure. |
| `APP_COOKIE_SECURE` | Override de cookie secure. |
| `APP_COOKIE_SAMESITE` | Override SameSite. Default `Lax`; mantenerlo con el proxy mismo-origen. `None` solo es necesario para una API llamada directamente desde otro sitio y exige revisar CSRF. |
| `APP_STORAGE` | `postgres` (default) en producción: Supabase, schemas `mira` y `billing`. `memory` solo dev/local. Cualquier otro valor es error. |
| `POSTGRES_DATABASE_URL` | Requerida con `APP_STORAGE=postgres`. En Cloud Run viene del secreto `mira-postgres-url`. |
| `APP_ENCRYPTION_KEY` | Requerida en producción; no usar default dev. |
| `APP_SESSION_SECRET` | Requerida en producción; no usar default dev. |
| `GOOGLE_CLIENT_ID` | OAuth Google client id. |
| `GOOGLE_CLIENT_SECRET` | OAuth Google secret. |
| `GOOGLE_REDIRECT_URL` | Callback OAuth. Default local: `/gmail/connect/callback`. |
| `GMAIL_MAX_THREADS` | Límite global de threads por run; default `50`. |
| `GCP_PROJECT_ID` | Ya no la lee la API. Cloud Run la inyecta sola; se mantiene por conveniencia operativa. |
| `AI_WORKER_URL` | URL del worker. |
| `AI_WORKER_AUDIENCE` | Audience OIDC opcional para invocar worker protegido. |
| `AI_APPLY_CONFIDENCE_THRESHOLD` | Default `0.92`. |
| `SCHEDULER_ENABLED` | Activa loop interno scheduler. |
| `CRON_SECRET` | Secreto del endpoint `POST /internal/scheduled-analysis`. |
| `SCHEDULE_USER_EMAIL` | Seed legacy scheduler. |
| `SCHEDULE_INTERNAL_DOMAINS` | Seed legacy scheduler. |
| `SCHEDULE_GMAIL_MAX_THREADS` | Seed legacy scheduler. |
| `RESEND_API_KEY` | Requerida si scheduler habilitado. |
| `REPORT_FROM_EMAIL` | Requerida si scheduler habilitado. |
| `REPORT_TO_EMAIL` | Fallback/seed legacy de destinatarios. |
| `RATE_LIMIT_ANALYSIS_CREATE_PER_HOUR` | Default `12`. |
| `RATE_LIMIT_ANALYSIS_START_PER_HOUR` | Default `12`. |

### AI worker (`apps/ai-worker`)

| Variable | Uso |
|---|---|
| `AWS_BEARER_TOKEN_BEDROCK` | Bearer token para Bedrock runtime cuando aplique. |
| `AWS_REGION` | Default `us-east-1`. |
| `BEDROCK_MODEL_ID` | Default `us.anthropic.claude-sonnet-4-6`. |
| `BEDROCK_BATCH_MODEL_ID` | Opcional; modelo para clasificación batch. Vacío reutiliza `BEDROCK_MODEL_ID`. |
| `DESK_MAILBOX`, `ANALYZED_MAILBOX`, `INTERNAL_DOMAIN`, `DESK_MEMBERS` | Legacy fallback genérico. No configurar datos tenant/company en producción SaaS. |

### Web (`apps/web`)

| Variable | Uso |
|---|---|
| `VITE_API_BASE_URL` | URL pública de API. Default local `http://localhost:8080`. |

## 3. Pre-deploy

1. Confirmar que no hay secretos reales en archivos versionados.
2. Ejecutar checks:

```bash
./scripts/check-all.sh
```

3. Confirmar Google Cloud:
   - Gmail API habilitada.
   - OAuth consent screen con scope `https://www.googleapis.com/auth/gmail.readonly`.
   - Test users cargados mientras OAuth esté en Testing.
   - Redirect autorizado: `https://<WEB_DOMAIN>/gmail/connect/callback` o el valor real de `GOOGLE_REDIRECT_URL`.
4. Confirmar PostgreSQL (Supabase):
   - `POSTGRES_DATABASE_URL` apunta al proyecto correcto y el secreto
     `mira-postgres-url` tiene versión vigente.
   - Migraciones aplicadas: `scripts/apply-supabase-migrations.sh`.
   - Respaldo reciente: `scripts/backup-postgres.sh`.
5. Confirmar Resend:
   - Dominio/sender verificado.
   - `REPORT_FROM_EMAIL` usa un sender autorizado.
6. Confirmar secretos en Secret Manager, sin imprimir valores:

```bash
PROJECT=<GCP_PROJECT_ID>
for secret in \
  app-encryption-key \
  app-session-secret \
  google-client-id \
  google-client-secret \
  resend-api-key \
  cron-secret; do
  gcloud secrets versions list "$secret" --project="$PROJECT" --limit=1 >/dev/null && echo "$secret OK"
done
```

## 4. Orden de despliegue recomendado

1. `apps/ai-worker`.
2. `apps/api`, apuntando a `AI_WORKER_URL`.
3. `apps/web`, apuntando a `VITE_API_BASE_URL`.
4. Scheduler externo opcional o loop interno.

## 5. Cloud Run — recomendaciones beta

### AI worker

- Ingress: preferir interno/protegido si API y worker comparten proyecto/VPC/IAM.
- Auth: si se protege con IAM, configurar `AI_WORKER_AUDIENCE` en API.
- No configurar contexto tenant vía env; el contexto llega por request.
- Timeout mayor que API si Bedrock puede tardar, pero evitar colas largas.

Variables mínimas:

```text
AWS_BEARER_TOKEN_BEDROCK=<secret>
AWS_REGION=<region>
BEDROCK_MODEL_ID=<model-id>
BEDROCK_BATCH_MODEL_ID=<optional-cheaper-model-id>
```

### API

Para beta privada segura:

- `APP_ENV=production`.
- `APP_STORAGE=postgres` (+ `POSTGRES_DATABASE_URL`).
- `APP_COOKIE_SECURE=true` si API/web usan HTTPS.
- `APP_COOKIE_SAMESITE=None` si frontend y API quedan en dominios distintos.
- `max-instances=1` recomendado hasta tener rate limit distribuido y claims robustos multi-instancia.

Variables mínimas ejemplo:

```text
APP_ENV=production
APP_STORAGE=postgres
POSTGRES_DATABASE_URL=<desde el secreto mira-postgres-url>
WEB_BASE_URL=https://<WEB_DOMAIN>
API_BASE_URL=https://<API_DOMAIN>
GOOGLE_REDIRECT_URL=https://<WEB_DOMAIN>/gmail/connect/callback
GCP_PROJECT_ID=<GCP_PROJECT_ID>
AI_WORKER_URL=https://<AI_WORKER_SERVICE_URL>
SCHEDULER_ENABLED=false
RATE_LIMIT_ANALYSIS_CREATE_PER_HOUR=12
RATE_LIMIT_ANALYSIS_START_PER_HOUR=12
```

Secretos mínimos:

```text
APP_ENCRYPTION_KEY=<secret>
APP_SESSION_SECRET=<secret>
GOOGLE_CLIENT_ID=<secret>
GOOGLE_CLIENT_SECRET=<secret>
RESEND_API_KEY=<secret>
REPORT_FROM_EMAIL=<verified-sender>
CRON_SECRET=<secret-if-external-scheduler>
```

### Web

Build/deploy con:

```text
VITE_API_BASE_URL=https://<API_DOMAIN>
```

La landing/waitlist actual usa `mailto:`; no requiere backend de leads.

## 6. Scheduler

Opciones:

### Opción A — loop interno

Configurar:

```text
SCHEDULER_ENABLED=true
RESEND_API_KEY=<secret>
REPORT_FROM_EMAIL=<verified-sender>
```

Ventaja: menos piezas externas.
Riesgo: requiere instancia API viva; con escalado múltiple hay que vigilar claims/idempotencia.

### Opción B — Cloud Scheduler externo

Endpoint real:

```http
POST /internal/scheduled-analysis
x-cron-secret: <CRON_SECRET>
Content-Type: application/json
```

Sin `as_of_date`, evalúa due-by-timezone. Con `as_of_date`, realiza backfill/testing explícito.
Para permitir una hora distinta por organización, configurar un único job por
hora de lunes a viernes (`0 * * * 1-5`). La API filtra las configuraciones que
corresponden y evita duplicados mediante su claim idempotente.

Ejemplo seguro con placeholder:

```bash
curl -X POST "https://<API_DOMAIN>/internal/scheduled-analysis" \
  -H "x-cron-secret: <CRON_SECRET>" \
  -H "Content-Type: application/json" \
  -d '{}'
```

Si `CRON_SECRET` no está configurado, el endpoint responde 404.

## 7. Smoke tests post-deploy

1. Abrir `https://<WEB_DOMAIN>` y verificar landing pública.
2. Click en “Ya tengo acceso” / OAuth.
3. Confirmar redirección a Google.
4. Login con beta tester Workspace.
5. Verificar `/auth/me` desde UI autenticada.
6. Abrir Configuración:
   - org provisionada;
   - casilla Workspace;
   - scope readonly visible;
   - policy draft editable.
7. Guardar política mínima:
   - organización;
   - dominio interno;
   - criterios válidos;
   - IA desactivada inicialmente.
8. Crear análisis pequeño.
9. Verificar:
   - run pasa de pending/running/completed;
   - hilos visibles;
   - manual review funciona;
   - `GET /me/operations/status` no expone secretos.
10. Si scheduler está activo, ejecutar trigger manual y revisar `scheduleStates`.

## 8. Observabilidad mínima durante beta

Vigilar:

- errores 401/403 OAuth;
- errores 429 de análisis manual;
- fallos de Gmail refresh token;
- fallos Resend;
- fallos AI worker/Bedrock;
- latencia de `/analysis-runs/:id/start`;
- logs de scheduler con mensajes redactados.

No loggear:

- refresh tokens;
- access tokens;
- cuerpos completos de correo;
- headers con `x-cron-secret`;
- payloads de AI con excerpts de emails.

## 9. Rollback

Rollback por servicio:

```bash
gcloud run revisions list --service=<SERVICE> --region=<REGION> --limit=5

gcloud run services update-traffic <SERVICE> \
  --region=<REGION> \
  --to-revisions=<PREVIOUS_REVISION>=100
```

Criterios para rollback inmediato:

- 5xx sostenidos en API.
- Login OAuth roto para testers.
- Scheduler genera runs duplicados.
- Errores de autorización/ownership.
- Logs con secretos o contenido sensible.
- `APP_ENV=production` falla por secretos default.

Si hay datos sospechosos:

1. Pausar scheduler externo o poner `SCHEDULER_ENABLED=false` en siguiente revisión.
2. Detener nuevas ejecuciones manuales si es necesario bajando rate limits.
3. Identificar ventana/documentos afectados.
4. Restaurar desde backup/PITR si está habilitado.

## 10. Riesgos conocidos

| Riesgo | Estado beta | Mitigación recomendada |
|---|---|---|
| Rate limit in-memory | Pendiente distribuido | API `max-instances=1` o Redis/Firestore limiter antes de escalar. |
| Retention delete job | Pendiente | No prometer borrado automático; ejecutar tarea posterior. |
| Disconnect/delete flows | Diferido | Mantener visibles deshabilitados hasta aprobar semántica. |
| OAuth Testing refresh tokens | Riesgo beta | Mantener testers informados; publicar/verificar OAuth antes de ampliar. |
| Scheduler multi-instancia | Parcialmente mitigado por state/claim | Revisar antes de subir instancias. |
| Waitlist mailto | Suficiente beta | Sustituir por backend/CRM antes de campañas públicas. |

## 11. Decisiones pendientes antes de beta ampliada

1. ¿API Cloud Run queda temporalmente en `max-instances=1` o se implementa limiter distribuido?
2. ¿Se usará scheduler interno, Cloud Scheduler externo o ambos?
3. ¿Cuándo se implementa retention delete job real?
4. ¿Cuál será la política exacta de disconnect/delete account?
5. ¿Se requiere dominio custom desde beta o bastan URLs Cloud Run?
6. ¿Quién recibe alertas operativas de OAuth/Resend/scheduler?

## 12. Checklist final

- [ ] `./scripts/check-all.sh` OK.
- [ ] OAuth redirect URI coincide con `GOOGLE_REDIRECT_URL`.
- [ ] Google consent screen menciona `gmail.readonly`.
- [ ] Secretos en Secret Manager; no en repo.
- [ ] `APP_ENV=production` configurado.
- [ ] `APP_ENCRYPTION_KEY` y `APP_SESSION_SECRET` no son defaults.
- [ ] Firestore accesible por service account API.
- [ ] Resend sender verificado.
- [ ] AI worker sin hardcodes tenant/company.
- [ ] Rate limit / `max-instances` decidido para beta.
- [ ] Scheduler modo elegido y probado.
- [ ] Smoke test OAuth + análisis manual completado.
- [ ] Privacy/Terms/OAuth verification agregados al backlog antes de beta ampliada.
