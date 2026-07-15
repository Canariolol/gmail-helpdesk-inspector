# Auditoría de concurrencia Firestore

Fecha: 2026-07-15. Alcance: flujos no financieros de la versión inicial.
Este documento identifica riesgos; no certifica que estén resueltos ni sustituye
una prueba con dos instancias desplegadas.

| Flujo | Lectura y escritura actual | Riesgo | Acción antes de escalar |
|---|---|---|---|
| Claim del scheduler | `claim_schedule_window` lee el documento y usa `currentDocument.exists=false` al crearlo o `currentDocument.updateTime` al reemplazar un claim vencido/fallido. Reintenta como máximo tres veces ante una precondición perdida. | Mitigado en código: un escritor concurrente no puede obtener el mismo claim; ante contención persistente se aborta en vez de duplicar el análisis/reporte. | Prueba local concurrente aprobada. Falta probar dos instancias reales contra Firestore/Cloud Run y observar el conflicto esperado. |
| Configuración de organización | `get_or_provision_org_config` y `update_org_config` leen y reescriben `ownerProfiles/.../config/current`. `sync_schedule_config_from_policy` sólo activa el scheduler si `gmailConnections/{hash}` sigue activa. | Mitigado para la ejecución: una edición tardía no reactiva el scheduler tras desconexión Gmail o `user.deleted`. Aún podría perderse una edición no crítica de otros campos. | Probar la carrera real desplegada. Añadir versión/precondición si se habilitan ediciones concurrentes de configuración. |
| Estado de análisis | Workers y scheduler actualizan el mismo `analysisRuns/{id}` mediante read-modify-write. | Una actualización tardía puede sobrescribir progreso o estado terminal más reciente. | Definir propietario/lease de ejecución y precondición de transición antes de más de una instancia. |
| Conexión Gmail | El refresh usa `refresh_gmail_connection`: compara la versión leída y exige conexión activa; Firestore además escribe con `currentDocument.updateTime`. | Mitigado en código: si desconexión, revocación WorkOS u otra reconexión gana la carrera, el refresh no reescribe tokens ni reactiva la conexión. | Prueba local aprobada. Falta observar la precondición perdida contra Firestore desplegado. |
| Revocación de sesiones | Logout y eventos WorkOS escriben `revoked_at` sobre sesiones concretas. | Bajo: la transición a revocada es idempotente; aún falta probar concurrencia desplegada. | Mantener las pruebas de revocación y validar en Cloud Run. |
| Lookup de cuenta por email | `accountEmailIndexes/{hash}` se crea al upsert y las cuentas legacy se migran al primer lookup. | Bajo: un índice de correo antiguo se elimina al detectarlo; no autoriza acceso por sí mismo. | Confirmar la migración perezosa desplegada y medir lecturas. |

Pagos, usage ledger, checkout y suscripciones quedan fuera de este documento
por el diferimiento explícito del incidente actual. Sus precondiciones y
transacciones se revisarán al retomar pagos.
