# Cutover Firestore → PostgreSQL

Estado al 2026-07-17: **PostgreSQL recibe 100% de tráfico en
`ghmi-api-00043-dp2`; login WorkOS, lectura Gmail y un análisis real fueron
validados**. La instantánea final fue validada y Cloud Scheduler sigue pausado.
Firestore se conserva como rollback hasta completar scheduler, borrado y
auditoría en los smoke tests autenticados.

## Regla de consistencia

No existe dual-write deliberadamente: dos fuentes de verdad harían más riesgoso
el cobro y la recuperación. Por lo tanto, el cambio de tráfico sólo se puede
hacer cuando no haya escrituras de usuarios en curso.

Para este cutover hay un tester recurrente, pero no usuarios externos ni cobros
habilitados. La persona responsable autorizó una ventana breve de escritura
congelada: pedir al tester pausar el uso de Mira, pausar Cloud Scheduler, esperar las requests en
curso, ejecutar la instantánea final y sólo entonces cambiar tráfico. Sin esa
ventana, un rollback puede perder las escrituras que hayan ocurrido sólo en
PostgreSQL.

La ventana está activa y el job ya fue pausado con:

```bash
gcloud scheduler jobs pause ghmi-daily-report --location us-central1
```

No reanudar el job hasta que la validación posterior sea satisfactoria o el
tráfico haya vuelto a Firestore.

## Precondiciones para promover

1. Ejecutar nuevamente la instantánea local desde Firestore y conservar sus
   conteos por tipo. La operación reemplaza `mira.records`; sólo debe apuntar
   al destino candidato sin escrituras de usuarios.
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

5. Confirmar con el tester que no usará Mira durante el cambio, pausar Cloud
   Scheduler y repetir la copia inmediatamente antes de cambiar tráfico.

## Promoción controlada

Reemplaza los nombres por las revisiones verificadas justo antes del cambio:

```bash
export PREVIOUS_REVISION='ghmi-api-00031-6lx'
export POSTGRES_REVISION='ghmi-api-00037-vov'

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

Al terminar un rollback válido, reanudar el scheduler:

```bash
gcloud scheduler jobs resume ghmi-daily-report --location us-central1
```

## Estado posterior estable

Tras un periodo de observación sin errores y con backup/export de Firestore,
retirar la identidad Firestore de la API nueva. La clave local legacy se rota
en una tarea separada; no forma parte del cambio de tráfico.

## Validación funcional restante (checklist ejecutable)

Estado 2026-07-18: login WorkOS, lectura Gmail y análisis manual ya validados
en la revisión activa. Falta lo siguiente; marcar cada punto con fecha al
completarlo.

1. **Scheduler** — reanudado y disparado manualmente el 2026-07-18: `200` en
   `/internal/scheduled-analysis` tras restaurar `CRON_SECRET` y
   `RESEND_API_KEY` en `ghmi-api-00049-2qb` (la revisión PostgreSQL los había
   perdido; el primer disparo devolvió 404). Falta observar la primera
   ejecución programada completa (lunes 08:00 America/Santiago) con reporte
   entregado.
2. **Lectura de configuración** — abrir Configuración con la cuenta tester y
   verificar que carga política, límites y estado de IA sin errores.
3. **Borrado de análisis + auditoría** — con la cuenta tester (idealmente con
   un análisis desechable): Privacidad → `BORRAR MIS ANALISIS`; verificar
   cero análisis visibles y el registro de auditoría `completed` para el
   `request_id` en PostgreSQL. Requiere sesión de la persona usuaria.
4. **Reconexión Gmail OAuth** — desconectar Gmail desde Cuenta y reconectar la
   misma casilla; verificar que un análisis nuevo funciona después. Requiere
   sesión de la persona usuaria.
5. **Webhook WorkOS real** — provocar un `session.revoked` real (cerrar sesión
   desde el dashboard de WorkOS o «Cerrar todas las sesiones») y confirmar en
   Cloud Logging `operation=workos_webhook` con HTTP 204.
6. **Auditoría por lotes desplegada** — tras el próximo deploy del worker/API,
   confirmar en Cloud Logging llamadas a `/audit/batch`, ninguna a
   `/audit/thread`, y comparar tokens con la referencia 2600/470.
7. **Periodo de observación** — una semana sin errores nuevos en las alertas
   creadas el 2026-07-18 antes de retirar Firestore como rollback y exportar su
   respaldo final.
