# Cutover Firestore → PostgreSQL

Estado al 2026-07-16: **no ejecutar todavía**. PostgreSQL ya tiene una copia
validada y una candidata de Cloud Run sin tráfico. Firestore continúa siendo
la fuente activa.

## Regla de consistencia

No existe dual-write deliberadamente: dos fuentes de verdad harían más riesgoso
el cobro y la recuperación. Por lo tanto, el cambio de tráfico sólo se puede
hacer cuando no haya escrituras de usuarios en curso.

Hoy eso se cumple porque no hay usuarios externos ni cobros habilitados. Cuando
existan, se debe implementar y anunciar una ventana breve de escritura
congelada antes del paso de tráfico. Sin esa ventana, un rollback puede perder
las escrituras que hayan ocurrido sólo en PostgreSQL.

## Precondiciones para promover

1. Ejecutar nuevamente la copia local idempotente desde Firestore y conservar
   sus conteos por tipo.
2. Verificar que la candidata de PostgreSQL tenga `Ready=True`,
   `APP_STORAGE=postgres`, la service account
   `ghmi-api-postgres-runtime` y el secreto `mira-postgres-url`.
3. Probar en la candidata login WorkOS, conexión y desconexión Gmail, crear y
   leer un análisis, scheduler, borrado y auditoría. No promover con un error
   pendiente en Cloud Logging.
4. Registrar la revisión activa actual antes de modificar tráfico:

   ```bash
   gcloud run services describe ghmi-api --region us-central1 \
     --format='value(status.traffic[0].revisionName,status.traffic[0].percent)'
   ```

5. Para un lanzamiento con usuarios, activar primero la ventana de escritura
   congelada y repetir la copia inmediatamente antes de cambiar tráfico.

## Promoción controlada

Reemplaza los nombres por las revisiones verificadas justo antes del cambio:

```bash
export PREVIOUS_REVISION='ghmi-api-00031-6lx'
export POSTGRES_REVISION='ghmi-api-00036-yad'

gcloud run services update-traffic ghmi-api --region us-central1 \
  --to-revisions "${POSTGRES_REVISION}=100"
```

Luego probar `/health`, login, Gmail, un análisis y la lectura de historial. Si
todo está correcto, observar logs y métricas antes de retirar Firestore de la
configuración de desarrollo o de revocar sus permisos.

## Rollback inmediato

Sólo usarlo si todavía no se aceptaron escrituras nuevas en PostgreSQL, o si la
ventana de escritura congelada sigue vigente:

```bash
gcloud run services update-traffic ghmi-api --region us-central1 \
  --to-revisions "${PREVIOUS_REVISION}=100"
```

Confirmar que la revisión Firestore responde y conservar la revisión
PostgreSQL en 0% para investigar. No borrar PostgreSQL ni Firestore durante el
incidente. Si hubo escrituras PostgreSQL, exportarlas y reconciliarlas antes de
informar que el rollback preservó los datos.

## Estado posterior estable

Tras un periodo de observación sin errores y con backup/export de Firestore,
retirar la identidad Firestore de la API nueva. La clave local legacy se rota
en una tarea separada; no forma parte del cambio de tráfico.
