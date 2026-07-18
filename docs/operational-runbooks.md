# Runbooks operativos mínimos

Estos procedimientos cubren la versión inicial de Mira Helpdesk. No incluyen
pagos: el incidente actual de Mercado Pago se trata por separado antes de
autorizar cambios en ese flujo.

## Preparación común

Usar sólo la cuenta con acceso operativo autorizado. No copiar tokens, cookies,
payloads de correo ni secretos a tickets, chats o logs manuales.

```bash
export GCP_PROJECT_ID="gmail-helpdesk-inspector"
export GCP_REGION="us-central1"
export API_SERVICE="ghmi-api"
export WORKER_SERVICE="ghmi-ai-worker"
export WEB_SERVICE="ghmi-web"
```

Para confirmar el estado de un servicio y leer errores recientes:

```bash
gcloud run services describe "$API_SERVICE" \
  --project "$GCP_PROJECT_ID" --region "$GCP_REGION" \
  --format='value(status.latestReadyRevisionName,status.url)'

gcloud run services logs read "$API_SERVICE" \
  --project "$GCP_PROJECT_ID" --region "$GCP_REGION" \
  --freshness=1h --log-filter='severity>=ERROR' --limit=50
```

Registrar hora, servicio, revisión, `request_id`/`run_id` cuando exista y el
impacto observado. No registrar direcciones de correo completas ni contenido
de mensajes.

## API no disponible o aumento de 5xx

**Síntoma:** navegador recibe 5xx, o aparecen eventos `INTERNAL_ERROR` o
`SERVICE_UNAVAILABLE`.

1. Consultar la revisión lista y los errores recientes con los comandos comunes.
2. Probar sólo liveness: `curl -fsS "$(gcloud run services describe "$API_SERVICE" --project "$GCP_PROJECT_ID" --region "$GCP_REGION" --format='value(status.url)')/health"`.
3. Correlacionar el `request_id` informado por el usuario con el evento JSON.
4. Si el incidente sigue a un deploy, detener nuevas promociones y escalar al
   responsable de deploy para rollback de la revisión anterior conocida.

**Verificación:** `/health` responde y no aparecen nuevos 5xx durante un
periodo de observación acordado. Comunicar sólo el impacto y la recuperación,
no detalles internos.

## Worker IA o Bedrock fallando

**Síntoma:** análisis fallidos con `bedrock_failed` o respuestas 502 de auditoría.

1. Consultar `ghmi-ai-worker` con el filtro de errores y correlacionar por
   `run_id` o `request_id`.
2. Verificar su liveness autenticado según `docs/gcp-deploy.md`; no exponer el
   worker ni registrar su token de identidad.
3. Confirmar que los análisis fallidos quedaron con categoría segura y no se
   reintentan manualmente si podrían duplicar trabajo.
4. Si el proveedor continúa caído, informar degradación: los análisis nuevos
   no se completan hasta recuperar el worker.

**Verificación:** un análisis de prueba autorizado completa y los eventos JSON
no contienen payloads de correo.

## Gmail desconectado o refresh token inválido

**Síntoma:** la casilla no puede analizarse o el usuario reporta reconexión.

1. Consultar errores de API por el `request_id`, sin solicitar tokens al usuario.
2. Confirmar en la aplicación si Gmail está desconectado o revocado.
3. Pedir que la persona autorizada vuelva a conectar Gmail desde la aplicación.
4. No editar tokens ni sesiones directamente en Firestore.

**Verificación:** la aplicación muestra Gmail conectado y un análisis manual
autorizado puede iniciar. Si falla, conservar sólo IDs de correlación y escalar.

## Evento WorkOS de revocación o deprovisioning

**Síntoma:** una persona sigue con acceso después de revocar su sesión o borrar
su usuario en WorkOS, o no aparece `operation=workos_webhook` tras el evento.

1. Consultar los logs de API por `operation=workos_webhook`; registrar solamente
   la hora y el tipo de evento, nunca el payload, firma ni secreto.
2. Confirmar en WorkOS que el endpoint configurado es
   `https://<ghmi-api-run-app-url>/auth/workos/webhook`, que usa HTTPS y que
   solo están seleccionados `session.revoked` y `user.deleted`.
3. Confirmar que la revisión activa tiene `WORKOS_WEBHOOK_SECRET` inyectado,
   sin imprimir su valor. Si no lo tiene, escalar para corregir configuración y
   desplegar; no editar sesiones o credenciales en Firestore.
4. Después de recuperar el endpoint, reenviar el evento desde WorkOS. La
   operación es idempotente: `session.revoked` invalida la sesión local ligada
   al `sid`; `user.deleted` además desconecta Gmail local y desactiva scheduler.

**Verificación:** el evento responde 2xx, la sesión afectada ya no autoriza
`/auth/me` y, para `user.deleted`, no queda una conexión Gmail activa ni un
scheduler habilitado. Comunicar el estado de acceso sin incluir datos del
evento.

## Scheduler omitido o reporte no enviado

**Síntoma:** no aparece el análisis/reporte esperado, o existen
`scheduled_analysis_failed`/`report_delivery_failed`.

1. Revisar API por `run_id` y la ventana afectada.
2. Confirmar si Cloud Scheduler es la fuente activa; en Cloud Run,
   `SCHEDULER_ENABLED` debe permanecer apagado si el job externo dispara la ruta
   interna.
3. Reintentar sólo con el procedimiento autorizado de scheduler y después de
   revisar idempotencia; no ejecutar dos disparos para la misma ventana.
4. Si el análisis completó pero Resend falló, no repetir el análisis: tratar
   únicamente la entrega del reporte según el estado persistido.

**Verificación:** un único estado terminal para la ventana y reporte enviado o
incidente de entrega documentado.

## Análisis atascado

**Síntoma:** un run mantiene estado `running` más tiempo del esperado o no
actualiza su progreso.

1. Registrar `run_id`, hora de inicio y el `request_id` de creación si existe.
2. Consultar errores de API y worker por esos identificadores.
3. No iniciar un segundo análisis para la misma ventana mientras se investiga.
4. Si Firestore no respondió, seguir el runbook de Firestore; si el worker
   falló, seguir el runbook de IA/Bedrock.

**Verificación:** el run alcanza un estado terminal o queda identificado para
recuperación manual sin duplicar análisis ni reportes.

## Firestore no disponible

**Síntoma:** la API devuelve 5xx relacionados con lectura/escritura o el
análisis queda sin poder persistir estado.

1. Confirmar liveness de API y consultar sus eventos de error; `/health` no
   certifica disponibilidad de Firestore.
2. Revisar el estado de Firestore y cuotas en la consola con acceso autorizado.
3. No borrar ni restaurar datos durante el incidente. PITR y delete protection
   están habilitados; la prueba de recuperación sigue pendiente. No ejecutar
   una restauración in-place de `(default)` durante el diagnóstico.
4. Escalar antes de reiniciar o desplegar: los análisis en curso pueden requerir
   revisión de su estado persistido.

**Verificación:** nuevas lecturas/escrituras autorizadas funcionan y no quedan
runs en estado `running` sin diagnóstico.

## Drill o recuperación Firestore con PITR

PITR y delete protection están habilitados en `(default)`. Para recuperar una
base completa o realizar un drill, no borres ni restaures `(default)` in-place:
ese procedimiento implica downtime. Con autorización operativa y presupuesto
para una base temporal, clona el punto de recuperación a una base nueva.

1. Consultar el primer instante disponible y elegir un `SNAPSHOT_TIME` de un
   minuto completo, posterior a `earliestVersionTime`:

   ```bash
   gcloud firestore databases describe --project "$GCP_PROJECT_ID" \
     --database='(default)' \
     --format='yaml(earliestVersionTime,versionRetentionPeriod)'
   ```

2. Crear un clone con un ID nuevo; no apuntar `ghmi-api` a esta base ni ejecutar
   análisis sobre ella:

   ```bash
   export SOURCE_DATABASE="projects/${GCP_PROJECT_ID}/databases/(default)"
   export SNAPSHOT_TIME="<RFC3339-al-minuto-dentro-de-la-ventana-PITR>"
   export DRILL_DATABASE="restore-drill-<fecha>"

   gcloud firestore databases clone \
     --project "$GCP_PROJECT_ID" \
     --source-database="$SOURCE_DATABASE" \
     --snapshot-time="$SNAPSHOT_TIME" \
     --destination-database="$DRILL_DATABASE"
   ```

3. Esperar la operación y verificar la base nueva con
   `gcloud firestore databases describe --database="$DRILL_DATABASE"`. Registrar
   sólo hora, snapshot, duración y resultado; no exportar correos, documentos o
   secretos al registro operativo.
4. Eliminar la base de drill únicamente con autorización posterior y registrar
   el coste observado. Una restauración in-place de `(default)` requiere un
   procedimiento de incidente separado y aprobación explícita.

## Respaldo y restauración PostgreSQL (Supabase)

Desde el cutover, la fuente de verdad es el schema `mira` en Supabase (plan
Free: sin PITR gestionado). El respaldo es manual desde el equipo de la persona
responsable con `scripts/backup-postgres.sh`; los dumps quedan en `backups/`
(fuera de Git) y rotan a los últimos 14.

**Respaldar** (la URL es la de Secret Manager `mira-postgres-url`; no dejarla
en el historial de shell ni en archivos versionados):

```bash
POSTGRES_DATABASE_URL="$(gcloud secrets versions access latest --secret=mira-postgres-url)" \
  scripts/backup-postgres.sh
```

El script verifica la integridad del dump (`pg_restore --list`) y que
contenga `mira.records` y `mira.schema_migrations`; si falla, el respaldo no
es válido.

**Restaurar (drill o recuperación a base limpia):**

1. Levantar un PostgreSQL desechable o usar una base vacía. Para un drill
   local:

   ```bash
   docker run -d --rm --name mira-restore -e POSTGRES_PASSWORD=test -p 55432:5432 postgres:17-alpine
   psql "postgresql://postgres:test@127.0.0.1:55432/postgres" \
     -c "CREATE ROLE anon NOLOGIN" -c "CREATE ROLE authenticated NOLOGIN" -c "CREATE ROLE service_role NOLOGIN"
   ```

   (Los roles sólo son necesarios porque la migración los referencia; en
   Supabase ya existen.)

2. Restaurar el dump más reciente:

   ```bash
   pg_restore --no-owner --no-acl \
     -d "postgresql://postgres:test@127.0.0.1:55432/postgres" \
     backups/postgres/mira-<fecha>.dump
   ```

3. Verificar conteos contra los del momento del respaldo:

   ```bash
   psql "postgresql://postgres:test@127.0.0.1:55432/postgres" -tA \
     -c "SELECT kind, count(*) FROM mira.records GROUP BY kind ORDER BY kind"
   ```

4. Registrar en la bitácora fecha, dump usado, conteos y duración. No copiar
   contenido de correos al registro.

**Recuperación real hacia Supabase:** restaurar sobre una base/proyecto nuevo
de Supabase, nunca in-place sobre el proyecto activo con la API sirviendo
tráfico. Pausar Cloud Scheduler y detener el tráfico de `ghmi-api` antes de
reemplazar datos; después apuntar `mira-postgres-url` (nueva versión del
secreto) a la base restaurada y desplegar. Requiere autorización operativa
explícita.

Evidencia del drill 2026-07-18: ciclo completo backup→restore contra
PostgreSQL 17 local con la migración real, 500 registros sintéticos y la tabla
de migraciones; conteos e índices coincidieron y la rotación conservó
exactamente `BACKUP_KEEP` dumps.

## Solicitud de borrar análisis

**Síntoma:** una persona solicita eliminar sus análisis derivados.

1. Verificar identidad mediante la sesión autenticada; no aceptar tokens o
   contraseñas por correo.
2. Usar la acción existente de Privacidad que exige `BORRAR MIS ANALISIS`.
3. Confirmar que no quedan runs, hilos, mensajes derivados, auditorías IA ni
   revisiones del usuario afectado.
4. Confirmar en `auditLogs/{request_id}` el estado `completed`; el documento
   conserva sólo el hash de la cuenta y las marcas de tiempo.
5. Comunicar la finalización sin prometer borrado de cuenta: ese flujo aún no
   está implementado.

**Verificación:** la pantalla de datos muestra cero análisis para la cuenta y
el evento de auditoría asociado al `x-request-id` queda en `completed` sin PII.

## Solicitud de eliminación completa de cuenta (procedimiento manual)

Mientras no exista el flujo automático, la eliminación de cuenta se atiende de
forma manual.

- **Canal:** `soporte@ninfasolutions.com` (confirmar antes de publicar en la
  página de privacidad).
- **Responsable:** la persona operadora del proyecto.
- **Plazo objetivo:** confirmar recepción dentro de 3 días hábiles y completar
  dentro de 15 días corridos.

Checklist por solicitud (registrar fecha y resultado de cada paso en una
entrada de la bitácora, sin PII más allá del hash de la cuenta):

1. Verificar identidad: responder al mismo correo de la cuenta afectada y
   pedir una confirmación explícita desde ese correo. Nunca pedir contraseñas
   ni tokens.
2. Si hay suscripción activa cuando existan cobros: cancelarla primero y
   confirmar que no habrá cargos futuros.
3. Pedir a la persona (o ejecutar con su autorización escrita):
   desconexión de Gmail desde la app (revoca el grant) y borrado de análisis
   con la confirmación `BORRAR MIS ANALISIS`.
4. Revocar todas las sesiones de la cuenta.
5. Eliminar manualmente en la base: configuración, presets, conexión Gmail,
   estados de scheduler y la cuenta, conservando sólo los registros de
   auditoría de borrado y los financieros/legales exigibles.
6. Eliminar la persona usuaria en WorkOS (o documentar por qué se conserva).
7. Verificar que un login nuevo con ese correo crearía una cuenta vacía.
8. Enviar confirmación final por correo y registrar el cierre en la bitácora.

**Verificación:** consulta por hash de cuenta sin resultados en `mira.records`
(salvo auditorías de borrado) y confirmación enviada.

## Incidente de seguridad

**Síntoma:** exposición posible de secreto, cookie, token, datos de correo o
acceso no autorizado.

1. Detener la propagación: no copiar más evidencia sensible ni desplegar cambios
   improvisados.
2. Registrar hora, sistema, impacto aparente y responsable que recibió el aviso.
3. Escalar al responsable de seguridad para decidir rotación de secretos,
   revocación de sesiones y comunicación a personas afectadas.
4. Preservar IDs y metadatos mínimos; no adjuntar bodies, headers ni tokens.

**Verificación:** la exposición queda contenida, las credenciales afectadas se
rotan bajo autorización y se documentan las acciones correctivas.

## Rollback de un servicio Cloud Run

**Síntoma:** un despliegue nuevo causa 5xx sostenidos, rompe OAuth o introduce
un riesgo de seguridad. El impacto queda limitado al servicio elegido; este
procedimiento no restaura datos de Firestore.

1. Detener nuevas promociones y elegir la última revisión conocida como sana
   usando la evidencia del despliegue anterior, no sólo su fecha.
2. Listar revisiones sin cambiar tráfico:

   ```bash
   export ROLLBACK_SERVICE="$API_SERVICE" # también admite "$WEB_SERVICE" o "$WORKER_SERVICE"
   gcloud run revisions list --service "$ROLLBACK_SERVICE" \
     --project "$GCP_PROJECT_ID" --region "$GCP_REGION" --limit=5
   ```

3. Con autorización operativa explícita, dirigir todo el tráfico a la revisión
   elegida:

   ```bash
   gcloud run services update-traffic "$ROLLBACK_SERVICE" \
     --project "$GCP_PROJECT_ID" --region "$GCP_REGION" \
     --to-revisions="<REVISION_SANA>=100"
   ```

4. Registrar servicio, revisión retirada, revisión recuperada, hora e impacto;
   no registrar payloads, correos ni secretos. Para más contexto de despliegue,
   ver `docs/deployment-beta-runbook.md`, sección «Rollback».

**Verificación:** confirmar la revisión que recibe 100% del tráfico con
`gcloud run services describe`, probar `/health` para API/web y la ruta
autenticada indicada en `docs/gcp-deploy.md` para el worker; observar 5xx antes
de comunicar recuperación. Si hay datos afectados, escalar: PITR y delete
protection están activos, pero la prueba real de recuperación sigue pendiente.
