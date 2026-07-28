# Auditoría de concurrencia Firestore (HISTÓRICO — NO VIGENTE)

> **Este documento ya no describe el sistema.** El backend Firestore fue
> eliminado del código (2026-07-21) y la persistencia es PostgreSQL. Todos los
> mecanismos que aparecen abajo —`currentDocument.exists`,
> `currentDocument.updateTime`, los reintentos por precondición perdida— **ya no
> existen**. Las columnas "Riesgo" y "Mitigado" de esta tabla no aplican al
> código actual.
>
> En PostgreSQL la exclusión mutua se apoya en `SELECT … FOR UPDATE` dentro de
> una transacción (ver `claim_schedule_window` en `apps/api/src/postgres/mod.rs`).
> **Los tres flujos auditados aquí no han sido reauditados contra ese
> mecanismo.** Se conserva el archivo como registro de qué carreras se
> identificaron, no como evidencia de que sigan mitigadas.

Fecha: 2026-07-15. Alcance: flujos no financieros de la versión inicial.
Este documento identifica riesgos; no certifica que estén resueltos ni sustituye
una prueba con dos instancias desplegadas.

| Flujo | Lectura y escritura actual | Riesgo | Acción antes de escalar |
|---|---|---|---|
| Claim del scheduler | `claim_schedule_window` lee el documento y usa `currentDocument.exists=false` al crearlo o `currentDocument.updateTime` al reemplazar un claim vencido/fallido. Reintenta como máximo tres veces ante una precondición perdida. | Mitigado en código: un escritor concurrente no puede obtener el mismo claim; ante contención persistente se aborta en vez de duplicar el análisis/reporte. | Prueba local concurrente aprobada. Falta probar dos instancias reales contra Firestore/Cloud Run y observar el conflicto esperado. |
| Configuración de organización | `get_or_provision_org_config` y `update_org_config` leen y reescriben `ownerProfiles/.../config/current`. `sync_schedule_config_from_policy` sólo activa el scheduler si `gmailConnections/{hash}` sigue activa. | Mitigado para la ejecución: una edición tardía no reactiva el scheduler tras desconexión Gmail o `user.deleted`. Aún podría perderse una edición no crítica de otros campos. | Probar la carrera real desplegada. Añadir versión/precondición si se habilitan ediciones concurrentes de configuración. |
| Estado de análisis | El inicio manual reclama `Pending → Running` con `currentDocument.updateTime`; sólo quien gana crea la tarea en segundo plano. Workers y scheduler siguen actualizando progreso/estado terminal mediante read-modify-write. | Mitigado para inicios duplicados. Una actualización tardía de progreso o estado terminal aún podría sobrescribir otra al operar con más de una instancia. | Probar el claim contra Firestore real. Definir propietario/lease de ejecución y precondición de transición antes de más de una instancia. |
| Conexión Gmail | El refresh usa `refresh_gmail_connection`: compara la versión leída y exige conexión activa; Firestore además escribe con `currentDocument.updateTime`. | Mitigado en código: si desconexión, revocación WorkOS u otra reconexión gana la carrera, el refresh no reescribe tokens ni reactiva la conexión. | Prueba local aprobada. Falta observar la precondición perdida contra Firestore desplegado. |
| Revocación de sesiones | Logout y eventos WorkOS escriben `revoked_at` sobre sesiones concretas. | Bajo: la transición a revocada es idempotente; aún falta probar concurrencia desplegada. | Mantener las pruebas de revocación y validar en Cloud Run. |
| Lookup de cuenta por email | `accountEmailIndexes/{hash}` se crea al upsert y las cuentas legacy se migran al primer lookup. | Bajo: un índice de correo antiguo se elimina al detectarlo; no autoriza acceso por sí mismo. | Confirmar la migración perezosa desplegada y medir lecturas. |
| Contadores de uso | `add_usage` lee `usageLedgers/{org}_{periodo}` y lo reescribe sólo con `currentDocument.updateTime`; al crear uno usa `currentDocument.exists=false`. Reintenta hasta tres veces. | Mitigado para incrementos perdidos: el contador de análisis creados, hilos analizados y auditorías IA no puede sobrescribirse silenciosamente por otro escritor. | La validación de cupo y la creación del análisis aún no forman una transacción única. Probar dos instancias reales antes de escalar y revisar la reserva atómica de cupos si aumenta la concurrencia. |

Checkout y suscripciones quedan fuera de este documento por el diferimiento
explícito del incidente de pagos. El usage ledger se incluye porque cuenta
límites de uso no financieros y puede actualizarse mientras un análisis corre
en segundo plano.
