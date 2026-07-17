# Plan de preparación para producción — Mira Helpdesk

> Fecha de creación: 2026-06-25  
> Objetivo: lanzar una versión inicial pagada y controlada en pocos días, manteniendo el producto simple y evitando infraestructura prematura.  
> Alcance: pagos, configuración de producción, seguridad, privacidad, control de datos, observabilidad, operación y release.  
> Fuera de alcance inmediato: conexión multiproveedor completa, plataforma de observabilidad autohospedada y escalado horizontal. La migración de persistencia a PostgreSQL/Supabase pasa a ser un prerrequisito del lanzamiento y no incluye migrar la autenticación.

---

## 0. Cómo usar este documento

Este archivo es el checklist maestro de preparación para producción.

La bitácora cronológica está en
[`production-readiness-log.md`](production-readiness-log.md). Al completar un
paso se debe actualizar el checklist pertinente y añadir allí: qué se hizo,
motivo, evidencia y qué sigue. Los estados locales no sustituyen una validación
desplegada.

### Estados

- `[ ]` Pendiente.
- `[x]` Terminado y verificado.
- `[-]` Decidido conscientemente como deuda aceptada para la versión inicial.

### Regla para marcar una tarea como terminada

Una tarea solo se marca `[x]` cuando:

1. La implementación o configuración existe.
2. Fue probada en un entorno equivalente a producción.
3. Existe evidencia verificable.
4. Se documentó el comportamiento operativo o la forma de recuperación.

Ejemplos de evidencia:

- Resultado de un test automatizado.
- Captura o ID de una transacción sandbox.
- Nombre de una revisión de Cloud Run.
- Alerta creada en Cloud Monitoring.
- Entrada actualizada en un runbook.
- Registro de una prueba de restauración.

### Prioridades

| Prioridad | Significado |
|---|---|
| **P0** | Bloqueador para comenzar a cobrar a usuarios externos. |
| **P1** | Debe quedar listo antes del lanzamiento si afecta conversión/confianza, o inmediatamente después si es operativo. |
| **P2** | Mejora importante, pero puede esperar a que exista uso real. |
| **P3** | Escalabilidad o madurez posterior. No debe retrasar el lanzamiento. |

---

# 1. Definición del lanzamiento

## 1.1 Alcance de la primera versión

- [x] Definir explícitamente el lanzamiento como **versión inicial pagada y controlada**.
- [ ] Definir el máximo inicial de organizaciones permitidas.
  - Recomendación: entre 3 y 10 organizaciones.
- [ ] Definir si todos los usuarios serán incorporados mediante onboarding acompañado.
- [ ] Definir qué proveedores de correo estarán disponibles.
  - Para este lanzamiento: Gmail y Google Workspace.
  - Microsoft e IMAP permanecen fuera de alcance (debatible, quiero hacer lo posible por incluir esto, porque es un diferenciador y funcionalidad muy potente).
- [ ] Definir si los reportes automáticos estarán habilitados desde el primer día.
- [ ] Definir un único canal de soporte para la versión inicial.
  - Correo recomendado: `soporte@<dominio>`.
- [ ] Definir horario y plazo objetivo de respuesta de soporte.
- [ ] Definir quién puede autorizar accesos internos privilegiados.
- [ ] Revisar que la landing, precios, términos y onboarding no usen lenguaje de “beta”.
- [ ] Retocar la landing para que la primera impresión se sienta completa y confiable, sin rediseño grande.

### Criterio de aceptación

- [ ] Existe una descripción de alcance de una página como máximo.
- [ ] Se conocen las funciones incluidas y excluidas.
- [ ] No se promete conexión multiproveedor ni borrado automático si todavía no existe.

## 1.2 Política de deuda aceptada

- [ ] Crear una lista corta de riesgos aceptados para la versión inicial.
- [ ] Cada riesgo aceptado debe tener:
  - Responsable.
  - Mitigación temporal.
  - Fecha de revisión.
  - Condición que obliga a resolverlo.
- [ ] No aceptar como deuda ningún riesgo P0.

Ejemplos razonables de deuda aceptada:

- `[-]` Rate limiter distribuido, mientras la API esté limitada a una instancia.
- [x] Migración de persistencia a PostgreSQL/Supabase antes de habilitar cobros.
  - Motivo: el usuario decidió no consolidar datos de clientes sobre Firestore;
    se mantiene WorkOS para identidad y sesión.
  - Mitigación hasta el cutover: Firestore permanece como fuente actual,
    protegida con PITR y protección contra borrado.
  - Condición de cierre: importación validada, candidata PostgreSQL verificada y
    rollback documentado. No se habilitan pagos antes de ello.
- `[-]` Panel administrativo completo, mientras exista un procedimiento operativo manual.
- `[-]` Tests E2E exhaustivos, mientras los flujos críticos tengan pruebas manuales repetibles.

## 1.3 Decisión actual de alcance

- [x] El release público no se comunica como beta.
- [x] Gmail y Google Workspace son el alcance de correo del lanzamiento inicial.
- [-] Multiproveedor completo no bloquea el lanzamiento inicial.
  - Motivo: requiere refactor de adaptador, credenciales, UX y pruebas de proveedores.
  - Mitigación: no prometer Microsoft/IMAP en landing, precios, onboarding ni términos.
  - Condición para subir prioridad: primer cliente de pago bloqueado por Microsoft 365, IMAP o hosting genérico.
- [x] Retoque visual de landing permitido antes del lanzamiento si se mantiene acotado a copy/CSS.

---

# 2. Gate maestro de lanzamiento

No comenzar a cobrar a usuarios externos hasta completar todos los puntos siguientes:

- [ ] Checkout embebido de Mercado Pago terminado.
- [ ] Webhook de Mercado Pago autenticado y probado.
- [x] Billing enforcement activo.
  - Evidencia 2026-07-15: revisión Cloud Run de `ghmi-api` expone
    `BILLING_ENFORCEMENT_ENABLED=true`.
- [ ] `APP_ENV=production` activo.
- [x] API limitada temporalmente a una instancia.
  - Evidencia 2026-07-15: `ghmi-api-00030-vxc` tiene `maxScale=1`.
- [ ] Flujo de pago probado de extremo a extremo.
- [x] Errores internos sanitizados.
- [x] Sesiones con expiración y revocación server-side.
  - Evidencia: 30 días absolutos, logout revoca la fila de sesión y tests de cookie revocada/expirada.
- [x] Desconexión de Gmail disponible.
  - Evidencia: revoca en Google, invalida tokens en todas las sesiones de la cuenta y bloquea análisis nuevos.
- [x] Procedimiento de borrado de análisis disponible.
  - Evidencia: confirmación escrita, borrado idempotente de runs y sus datos derivados; la eliminación de cuenta sigue pendiente.
- [x] PITR o mecanismo de respaldo equivalente habilitado.
  - Evidencia 2026-07-15: Firestore `(default)` en `southamerica-west1` expone
    `POINT_IN_TIME_RECOVERY_ENABLED` y `DELETE_PROTECTION_ENABLED`. Falta una
    prueba de restauración controlada.
- [ ] Alertas mínimas operativas activas.
- [ ] Uptime checks activos.
- [ ] Política de privacidad publicada.
- [ ] Términos y condiciones publicados.
- [x] Copy sobre uso de IA consistente con el comportamiento real.
  - Configuración declara proveedor, campos, límites, adjuntos excluidos,
    opt-out y aplicación al próximo análisis; la política legal sigue pendiente.
- [x] Check de release local en verde.
  - Evidencia: `scripts/check-all.sh` — Rust fmt, 151 tests, Clippy, 10 tests del worker y build web (2026-07-15).
- [ ] Smoke test final realizado en producción.
- [x] Procedimiento de rollback documentado.
  - Evidencia: `docs/operational-runbooks.md`, sección «Rollback de un
    servicio Cloud Run». Requiere autorización operativa para ejecutar el
    cambio de tráfico; no sustituye una prueba de restauración Firestore.

### Decisión final

- [ ] **GO:** todos los bloqueadores P0 están cerrados.
- [ ] **NO-GO:** existe al menos un bloqueador P0 abierto.

---

# 3. Mercado Pago y billing

Prioridad: **P0**

## 3.1 Contrato funcional del checkout embebido

Contrato implementado (pendiente de prueba sandbox): `Card Payment Brick` tokeniza la tarjeta
en el navegador; el frontend envía únicamente `card_token_id` y email del pagador; la API crea
`/preapproval` con `status=authorized`. No hay `init_point`, URL de checkout ni redirect. La API
concede el acceso solo tras recibir `authorized`, y el webhook continúa siendo la sincronización
de estados con Mercado Pago.

- [ ] Documentar el flujo exacto del checkout embebido.
- [ ] Definir qué componente de Mercado Pago se utilizará.
- [ ] Definir qué datos de tarjeta pasan directamente a Mercado Pago.
- [ ] Confirmar que el backend nunca recibe ni persiste:
  - Número completo de tarjeta.
  - CVV.
  - Datos PCI sensibles.
- [ ] Definir qué identificador devuelve el frontend al backend.
- [ ] Definir cuándo se crea una `CheckoutSession`.
- [ ] Definir cuándo se crea o actualiza una `Subscription`.
- [ ] Definir cuándo se concede acceso.
- [ ] Confirmar que el frontend nunca decide por sí solo que un pago está aprobado.
- [ ] Usar Mercado Pago como fuente de verdad para estado y fechas.
- [ ] Definir el comportamiento si el usuario cierra la pestaña durante el pago.
- [ ] Definir el comportamiento si el frontend recibe éxito, pero el webhook todavía no llega.
- [ ] Definir el comportamiento si el webhook llega antes que la respuesta del checkout.

## 3.2 Estados de checkout y suscripción

- [ ] Revisar la tabla de estados internos.
- [ ] Mapear cada estado relevante de Mercado Pago a un estado interno.
- [ ] Diferenciar:
  - Checkout iniciado.
  - Pago pendiente.
  - Pago aprobado.
  - Pago rechazado.
  - Suscripción activa.
  - Suscripción pausada.
  - Cobro vencido.
  - Suscripción cancelada.
  - Trial activo.
  - Trial finalizado.
- [ ] Definir estados desconocidos como no autorizados por defecto.
- [ ] Registrar el estado original del proveedor para diagnóstico.
- [ ] Evitar que un evento antiguo revierta un estado más nuevo.
- [ ] Guardar timestamp del último evento procesado.

## 3.3 Seguridad del webhook

- [ ] Configurar `MERCADOPAGO_WEBHOOK_SECRET`.
- [ ] Hacer obligatorio el secreto cuando `APP_ENV=production`.
- [ ] Rechazar webhooks sin firma.
- [ ] Rechazar webhooks con firma inválida.
- [ ] Validar que `data.id` coincida con el identificador firmado.
- [ ] Validar antigüedad del timestamp de firma.
- [ ] Definir una ventana máxima de tolerancia.
  - Recomendación inicial: 5 minutos, ajustable según documentación del proveedor.
- [ ] Implementar protección contra replay.
- [ ] Registrar un identificador único de evento si Mercado Pago lo entrega.
- [ ] Hacer el procesamiento idempotente.
- [ ] Responder rápidamente al webhook.
- [ ] Evitar que una operación lenta del proveedor bloquee innecesariamente la recepción.
- [ ] No registrar secretos ni payloads sensibles.

## 3.4 Idempotencia y concurrencia

- [ ] Evitar que dos clics creen dos suscripciones.
- [ ] Crear una clave idempotente por organización, plan e intento.
- [ ] Deshabilitar temporalmente el botón mientras existe una solicitud activa.
- [ ] Tratar webhooks duplicados como éxito sin duplicar efectos.
- [ ] Evitar crear varias suscripciones activas para una misma organización.
- [ ] Evitar incrementar uso o activar beneficios dos veces.
- [ ] Probar dos requests de checkout simultáneos.
- [ ] Probar dos webhooks simultáneos.
- [ ] Definir cómo se resuelve una suscripción duplicada en el proveedor.

## 3.5 Reconciliación

- [ ] Crear una forma de consultar el estado real en Mercado Pago.
- [ ] Crear una operación de reconciliación manual por suscripción.
- [ ] Considerar una tarea periódica para reconciliar:
  - Checkouts pendientes demasiado tiempo.
  - Suscripciones activas localmente sin confirmación reciente.
  - Pagos vencidos.
  - Cancelaciones.
- [ ] Definir cuánto tiempo puede permanecer un checkout pendiente.
- [ ] Marcar checkouts abandonados o expirados.
- [ ] Alertar por diferencias entre el estado local y Mercado Pago.

## 3.6 Cancelación, reactivación y cambio de plan

- [ ] Confirmar semántica real de cancelación con Mercado Pago.
- [ ] Confirmar si cancelar detiene cobros inmediatamente o al fin del período.
- [ ] No mostrar “mantienes acceso hasta fin de período” si Mercado Pago no garantiza esa conducta.
- [ ] Obtener `current_period_end` desde el proveedor cuando sea posible.
- [ ] Probar cancelación.
- [ ] Probar webhook posterior a cancelación.
- [ ] Probar reactivación.
- [ ] Probar cambio de plan ascendente.
- [ ] Probar cambio de plan descendente.
- [ ] Definir prorrateo o ausencia de prorrateo.
- [ ] Explicarlo en la UI y los términos.
- [ ] Evitar cambios de plan durante un checkout pendiente.

## 3.7 Matriz mínima de pruebas sandbox

- [ ] Pago aprobado.
- [ ] Pago rechazado.
- [ ] Pago pendiente.
- [ ] Fondos insuficientes.
- [ ] Tarjeta vencida.
- [ ] Datos de tarjeta inválidos.
- [ ] Usuario abandona checkout.
- [ ] Timeout del frontend.
- [ ] Timeout del backend.
- [ ] Mercado Pago responde 4xx.
- [ ] Mercado Pago responde 5xx.
- [ ] Webhook válido.
- [ ] Webhook sin firma.
- [ ] Webhook con firma incorrecta.
- [ ] Webhook duplicado.
- [ ] Webhook atrasado.
- [ ] Webhooks fuera de orden.
- [ ] Trial activado.
- [ ] Trial finalizado.
- [ ] Renovación aprobada.
- [ ] Renovación rechazada.
- [ ] Cancelación.
- [ ] Reactivación.
- [ ] Cambio de plan.
- [ ] Refresh del navegador durante el checkout.
- [ ] Apertura del mismo checkout en dos pestañas.

### Evidencia requerida

- [ ] Tabla con resultado de cada caso.
- [ ] ID de operación sandbox.
- [ ] Estado esperado y estado observado.
- [ ] Logs sanitizados asociados mediante `request_id` o `checkout_id`.

### Ejecución sandbox — 2026-07-14

- [x] Stack local levantado con credenciales sandbox aisladas del `.env` existente; API y callback público temporal responden `/health`.
- [x] Webhook sandbox configurado para `subscription_preapproval` desde el MCP de Mercado Pago.
- [x] Corregida la inyección de `VITE_MERCADOPAGO_PUBLIC_KEY` en el build Docker de la web.
- [x] Corregido el flujo Mira Free: elegir un plan inicia checkout nuevo; cambiar o cancelar se reserva para suscripciones activas.
- [x] Secreto completo de webhook cargado localmente y API recreada, sin exponerlo.
- [x] Cloud Run sandbox corregido: `ghmi-api-00022-gvm` sirve con enforcement activo y credenciales server-side desde Secret Manager; `/health` responde OK.
- [x] Frontend corregido desplegado en `ghmi-web-00016-hpl` al 100%: Mira Free deriva a checkout nuevo en vez de `/me/subscription/change-plan`.
- [x] Errores de Mercado Pago corregidos en `ghmi-api-00023-jk2`: los 4xx/5xx ya no se convierten en 500 internos ni exponen payloads; se registran operación, estado y `request_id`. Evidencia: 136 tests y Clippy OK.
- [x] Callback sandbox configurado manualmente en Mercado Pago por el responsable de la cuenta; pendiente validar su primera entrega real.
- [x] Credencial vendedora sandbox verificada sin exponer secretos: el proceso local carga `.env.sandbox.local` y `/users/me` identifica `TESTUSER2731260096360294218`.
- [x] Diagnosticado el primer 400 real de `/preapproval`: la tarjeta se tokenizó (`201`), pero Mercado Pago rechazó `back_url=http://localhost:5173`. Sandbox local corregido para usar la URL HTTPS del frontend desplegado y stack reiniciado.
- [x] Diagnosticado el segundo 400 en Cloud Run (`request_id=a3dc0162-9dac-4278-b823-a5389525e656`): `Card token was generated without cvv validation`. La public key desplegada coincide con `.env.sandbox.local`; una preapproval aislada con las mismas credenciales y datos fue autorizada y luego cancelada, confirmando que el fallo quedó en el token del navegador/autorrelleno del CVV.
- [x] `scripts/redeploy-gcp.sh` ahora valida la public key antes de iniciar un despliegue `web/all` y muestra el comando sandbox, evitando desplegar worker/API y abortar recién al llegar a la web.
- [ ] Crear la preapproval desde el Brick, recibir la notificación y validar la suscripción mediante una prueba manual sandbox.
- [ ] Repetir el checkout usando el correo de una cuenta compradora de prueba distinta de la cuenta vendedora sandbox. Mercado Pago devolvió 500 y no creó ninguna preapproval con correos genéricos.
- [ ] Verificar firma HMAC en el evento real.

---

# 4. Configuración real de producción en GCP

Prioridad: **P0**

## 4.1 Variables y secretos

- [ ] Configurar `APP_ENV=production`.
- [x] Configurar explícitamente `BILLING_ENFORCEMENT_ENABLED=true`.
- [ ] Configurar `MERCADOPAGO_ACCESS_TOKEN` mediante Secret Manager.
- [ ] Configurar `MERCADOPAGO_WEBHOOK_SECRET` mediante Secret Manager.
- [ ] Migrar y rotar `WORKOS_API_KEY` hacia Secret Manager.
  - La inspección de configuración 2026-07-15 detectó que aún está inyectada
    como variable de texto plano; no registrar ni repetir su valor.
  - Avance 2026-07-16: la clave staging quedó como versión inicial y la nueva
    clave production como versión 2 de `workos-api-key`; sólo `ghmi-runtime`
    recibió acceso de lectura. La candidata `ghmi-api-00033-xut` usa esa
    versión; la revisión activa conserva el valor literal hasta completar la
    promoción, por eso este ítem no está terminado todavía.
- [x] Registrar el callback de AuthKit de Mira en la nueva aplicación WorkOS.
  - Evidencia 2026-07-16: el primer callback fue creado en staging. Tras
    recibir las credenciales production, se confirmó que allí no existía y se
    creó de nuevo con HTTP 201. Falta desplegar una revisión candidata antes de
    dirigir tráfico hacia ella.
- [-] Posponer la migración y rotación de `WORKOS_COOKIE_SECRET`.
  - Decisión de la persona responsable 2026-07-16: no se harán cambios en
    Secret Manager ni se invalidarán sesiones durante el cutover PostgreSQL.
    Se retomará al definir una estrategia de secretos separada (por ejemplo,
    Infisical) o al planificar una invalidación de sesiones.
- [x] Configurar `WORKOS_WEBHOOK_SECRET` mediante Secret Manager y registrar el endpoint WorkOS.
  - Evidencia 2026-07-15: corresponde al entorno staging y permanece enlazado
    a la revisión activa de `ghmi-api`.
- [x] Preparar el webhook WorkOS de producción y su secreto aislado.
  - Evidencia 2026-07-16: WorkOS production tiene habilitado
    `https://ghmi-api-io54uhmrxa-uc.a.run.app/auth/workos/webhook`, sólo para
    `session.revoked` y `user.deleted`; su firma coincide con
    `workos-production-webhook-secret` en Secret Manager.
  - La candidata PostgreSQL `ghmi-api-00036-yad` aceptó un evento sintético
    firmado con HTTP 204 y registró `operation=workos_webhook`; no había una
    sesión con el ID de prueba. Aún falta observar una entrega real de WorkOS.
- [x] Confirmar `APP_ENCRYPTION_KEY` fuerte.
- [x] Confirmar `APP_SESSION_SECRET` fuerte.
- [x] Confirmar `GOOGLE_CLIENT_SECRET` en Secret Manager.
- [x] Confirmar `CRON_SECRET` fuerte.
- [x] Confirmar `RESEND_API_KEY` en Secret Manager.
- [ ] Confirmar que ningún secreto real aparece como variable de texto plano.
- [ ] Confirmar que ningún secreto está versionado.
- [ ] Documentar rotación de cada secreto.
- [ ] Definir responsable de rotación.

### Inventario de inyección — 2026-07-16

- `APP_ENCRYPTION_KEY`, `APP_SESSION_SECRET`, `GOOGLE_CLIENT_SECRET`,
  `CRON_SECRET`, `RESEND_API_KEY`, los secretos sandbox de Mercado Pago y
  `WORKOS_WEBHOOK_SECRET` se inyectan desde Secret Manager.
- Solo `WORKOS_API_KEY` y `WORKOS_COOKIE_SECRET` permanecen como valores
  literales en la revisión activa. La migración debe incluir una API key nueva
  emitida desde WorkOS y la rotación de la cookie cerrará las sesiones actuales.
- La deuda de la cookie se acepta para este cutover temprano: no bloquea una
  revisión candidata ni autoriza modificar Secret Manager. Debe reevaluarse
  antes de ampliar el acceso de usuarios o cambiar la estrategia de secretos.
- `scripts/check-secrets.sh` no detectó patrones de credenciales en archivos
  versionados; ese control es acotado y no sustituye la rotación ni una revisión
  humana de secretos.

### Auditoría de fortaleza e inyección — 2026-07-16

- Se leyó cada secreto sólo en memoria y sin registrar su valor. Las versiones
  vigentes de `app-encryption-key`, `app-session-secret`,
  `google-client-secret`, `cron-secret` y `resend-api-key` superan el mínimo
  aplicable, no contienen los defaults de desarrollo conocidos y están
  inyectadas desde Secret Manager.
- La candidata `ghmi-api-00033-xut` referencia `workos-api-key:2` y
  `workos-production-webhook-secret:1`. La revisión que recibe 100% del
  tráfico (`ghmi-api-00031-6lx`) conserva `WORKOS_API_KEY`,
  `WORKOS_COOKIE_SECRET` y la firma webhook staging como valores anteriores.
  Es intencional hasta promover una revisión comprobada; no equivale a una
  migración completa de secretos.

## 4.2 Validaciones de arranque

- [x] Hacer que producción rechace `APP_STORAGE=memory`.
- [x] Hacer que producción rechace billing activo sin credenciales completas.
- [x] Hacer que producción rechace webhook sin secreto.
- [x] Hacer que producción rechace webhook WorkOS sin `WORKOS_WEBHOOK_SECRET`.
- [x] Hacer que producción rechace URLs HTTP.
- [x] Validar `APP_COOKIE_SECURE=true`.
- [x] Validar valores permitidos de `APP_COOKIE_SAMESITE`.
- [x] Validar que `WEB_BASE_URL` y `API_BASE_URL` tengan orígenes esperados.
- [x] Validar que `GOOGLE_REDIRECT_URL` corresponda al entorno.
- [x] Validar que `WORKOS_REDIRECT_URI` corresponda al entorno.
- [x] Validar que `AI_WORKER_AUDIENCE` exista cuando el worker es privado.

Las callbacks OAuth/WorkOS pueden usar el origen HTTPS de API o el de web. En
la arquitectura same-origin, se recomienda el de web: el proxy reenvía la ruta
a API y la cookie se entrega bajo el origen con que navega la persona usuaria.

## 4.3 Escalado temporal

- [x] Configurar `max-instances=1` para `ghmi-api`.
  - Evidencia 2026-07-16: `autoscaling.knative.dev/maxScale=1` se conserva en
    la revisión activa `ghmi-api-00031-6lx`.
- [x] Confirmar que el scheduler externo sigue operativo.
  - Evidencia 2026-07-16: `ghmi-daily-report` está `ENABLED` y su última
    ejecución observada el 2026-07-15 respondió `200` desde
    `/internal/scheduled-analysis`.
- [x] Mantener `SCHEDULER_ENABLED=false` si Cloud Scheduler es la fuente elegida.
  - Evidencia 2026-07-16: la revisión activa de `ghmi-api` lo declara
    explícitamente como `false`.
- [x] No habilitar simultáneamente scheduler interno y externo sin una razón documentada.
  - Evidencia 2026-07-16: Cloud Scheduler es la única fuente habilitada y la
    API conserva el scheduler interno desactivado.
- [ ] Confirmar que la concurrencia máxima no produce análisis simultáneos inesperados.
  - Observación 2026-07-16: `ghmi-api-00031-6lx` tiene
    `containerConcurrency=80` y `maxScale=1`. El límite de instancias no
    serializa hasta 80 requests dentro de la instancia; no se reduce aún a 1
    porque el análisis programado puede mantener una request activa durante un
    período largo. La candidata debe probar requests concurrentes contra
    Firestore real antes de cambiar este valor o escalar.
- [x] Documentar qué tareas impiden escalar horizontalmente.
  - El rate limiter está en memoria por instancia (`Mutex<HashMap<...>>`), por
    lo que cada instancia tendría su propio cupo.
  - Los incrementos de uso usan una precondición Firestore y reintento; la
    validación del cupo y la creación del análisis aún no forman una
    transacción única.
  - Checkout, suscripción y webhook de billing todavía encadenan lecturas y
    escrituras independientes; sus carreras se resolverán al retomar pagos.
  - El claim del scheduler **no** es ya un bloqueo: Firestore usa
    `update_time` como precondición condicional y la prueba de claims
    concurrentes está verde.
  - Mantener `maxScale=1` hasta resolver los tres puntos anteriores y ejecutar
    una prueba de concurrencia desplegada.

### Inspección GCP — 2026-07-15

- `ghmi-api`, `ghmi-web` y `ghmi-ai-worker` están actualmente en `maxScale=20`.
- Firestore `(default)` en `southamerica-west1` tiene PITR y delete protection desactivados.
- La revisión `ghmi-api-00028-7lj` no tiene `APP_ENV`; por tanto aún no activa
  las validaciones de producción. Sí tiene Firestore, cookies seguras,
  `SameSite=None` y billing enforcement configurados explícitamente.
- Sus callbacks Google/WorkOS usan el origen web y `ghmi-web` proxya al origen
  API, configuración compatible con la validación local desde 2026-07-15.
- No se cambiaron recursos productivos durante esta inspección: reducir capacidad o habilitar protecciones de base puede afectar disponibilidad y coste, por lo que requiere aprobación explícita antes de aplicar.

### Revalidación GCP — 2026-07-15

Consulta de sólo lectura: `ghmi-api` permanece en `ghmi-api-00028-7lj` con
`maxScale=20`; `APP_ENV` sigue ausente. Se confirmaron Firestore, cookies
seguras, `SameSite=None` y billing enforcement. Firestore sigue en
`southamerica-west1` sin PITR ni delete protection. No se consultaron valores
de secretos ni se modificó infraestructura.

### Aplicación GCP — 2026-07-15

Cambios realizados por la persona operadora y confirmados después mediante
consulta de solo lectura:

- `ghmi-api` quedó en `maxScale=1` (revisión `ghmi-api-00030-vxc`) para evitar
  escalado horizontal mientras el rate limit sigue en memoria.
- Firestore `(default)` en `southamerica-west1` tiene PITR y delete protection
  habilitados. Aún no existe evidencia de una restauración controlada.
- `WORKOS_WEBHOOK_SECRET` está referenciado desde Secret Manager por la API y
  el endpoint fue creado en WorkOS. El handler nuevo sigue pendiente de deploy
  y de pruebas firmadas con una cuenta desechable.
- La misma consulta encontró `WORKOS_API_KEY` y `WORKOS_COOKIE_SECRET` como
  variables de texto plano. Deben rotarse y moverse a Secret Manager antes del
  siguiente despliegue de producción pagada; para el cutover temprano se
  acepta explícitamente diferir esa rotación y no tocar Secret Manager.
- Uptime checks y alertas permanecen diferidos por decisión operativa: una
  comprobación externa podría despertar una instancia sin tráfico. Antes del
  lanzamiento real se debe decidir su coste y volver a evaluar este punto.

### Revalidación de dominio — 2026-07-16

- El mapping `mira.ninfasolutions.com -> ghmi-web` informa `Ready=True` y
  `CertificateProvisioned=True` desde 2026-07-16 04:47 UTC. El CNAME público
  sigue apuntando a `ghs.googlehosted.com`; el resolver local puede demorar en
  descartar su caché anterior.
- `ghmi-web` ya proxya al URL actual de API (`ghmi-api-io54uhmrxa-uc.a.run.app`),
  pero `API_BASE_URL` de la API conserva un URL `run.app` anterior. El deploy
  candidato debe alinear `API_BASE_URL` con el URL actual de API junto con
  `WEB_BASE_URL`, `GOOGLE_REDIRECT_URL` y `WORKOS_REDIRECT_URI`.

### Scheduler externo — 2026-07-16

- `ghmi-daily-report` está habilitado, corre de lunes a viernes a las 08:00
  `America/Santiago` y llama directamente a
  `/internal/scheduled-analysis` de `ghmi-api`.
- La revisión activa declara `SCHEDULER_ENABLED=false`; no hay scheduler
  interno compitiendo con Cloud Scheduler.
- Cloud Logging muestra una ejecución `200` el 2026-07-15 (6,2 s). También
  se observó un `504` el 2026-07-14 al alcanzar los 300 s de timeout de Cloud
  Run. Se corrigió en `ghmi-api-00031-6lx`: API y Cloud Scheduler usan ahora
  un límite explícito de 1.800 s. `/health` de la revisión respondió `200`.

## 4.4 Revisión segura

- [x] Preparar el despliegue de una revisión sin tráfico.
  - `scripts/redeploy-gcp.sh --no-traffic <servicio>` usa el tag `candidate`,
    conserva el tráfico existente e imprime la URL directa para el smoke test.
    Con `all`, la web candidata usa la API candidata.
    Fue validado con una ejecución simulada antes de la candidata real.
- [x] Desplegar una revisión sin tráfico.
  - Evidencia 2026-07-16: `ghmi-api-00033-xut` fue creada desde la imagen
    `api:758a21aad6e1`, con tag `candidate` y 0% de tráfico. Cambia sólo la
    configuración WorkOS y URLs públicas de API necesarias para la candidata.
  - Avance PostgreSQL 2026-07-16: `ghmi-api-00037-vov` conserva la candidata
    PostgreSQL sin tráfico bajo el tag `postgres` y actualiza sólo
    `GOOGLE_REDIRECT_URL` a
    `https://mira.ninfasolutions.com/gmail/connect/callback`.
- [x] Probar `/health`.
  - Evidencia 2026-07-16: `GET` a la URL directa de `candidate` devolvió 200.
    La URL directa del tag `postgres` de `ghmi-api-00037-vov` también devolvió
    200, sin errores Cloud Logging observados.
- [ ] Probar login WorkOS.
  - Preflight aprobado: `/auth/workos/login` de la candidata devolvió 307 y su
    destino contiene el callback de Mira y el `WORKOS_CLIENT_ID` production.
    Se repitió en `ghmi-api-00037-vov` después del cambio Google y conserva el
    callback público de WorkOS.
    El login completo se prueba al enviar tráfico controlado: la callback
    pública aún llega al proxy web que sirve la revisión activa.
- [ ] Probar conexión Gmail.
  - **Acción externa necesaria:** en Google Cloud Console > Google Auth
    Platform > Clients, abrir el cliente Web usado por `GOOGLE_CLIENT_ID` y
    añadir exactamente `https://mira.ninfasolutions.com/gmail/connect/callback`
    en «Authorized redirect URIs». No crear otro cliente ni cambiar su secreto.
- [ ] Probar lectura de configuración.
- [ ] Probar checkout sandbox.
- [ ] Probar recepción de webhook.
- [ ] Probar creación y ejecución de análisis.
- [ ] Probar scheduler manualmente.
- [ ] Enviar tráfico progresivamente.
  - 0% → pruebas internas.
  - 10% → smoke test.
  - 100% → solo después de validar logs.

### Evidencia requerida

- [ ] Nombre de revisión Cloud Run.
- [ ] Lista de variables configuradas, sin valores.
- [ ] Resultado de smoke test.
- [ ] Confirmación de tráfico y rollback disponible.

---

# 5. Sesiones, cookies y revocación

Prioridad: **P0**

## 5.1 Modelo de sesión

Decisión implementada: una sesión web dura 30 días desde el login, sin extensión por actividad.
Las sesiones antiguas sin `expires_at` requieren iniciar sesión otra vez al desplegar esta versión.

- [x] Añadir `expires_at` a la sesión server-side.
- [x] Añadir `revoked_at` o estado equivalente.
- [x] Validar expiración en cada request autenticado.
- [x] Rechazar sesiones revocadas.
- [x] Evitar depender únicamente del `Max-Age` de la cookie.
- [x] Definir duración de sesión.
  - Recomendación inicial: 7–30 días según sensibilidad y UX.
- [x] Definir si la actividad extiende la sesión.
- [x] Definir una duración absoluta máxima.
- [ ] Registrar `last_seen_at` solo si aporta valor operativo.

## 5.2 Logout

- [x] Invalidar la sesión en el servidor al cerrar sesión.
- [x] Borrar la cookie del navegador.
- [x] Probar reutilización de una cookie copiada después del logout.
- [x] La cookie reutilizada debe responder 401.
- [x] Implementar “cerrar todas las sesiones” para soporte/admin o usuario.
  - Disponible para la persona usuaria desde Cuenta, con confirmación explícita.
  - Revoca todas sus sesiones web server-side y borra la cookie actual; no desconecta Gmail ni detiene el scheduler.
  - Firestore pagina el barrido actual de sesiones; una consulta indexada por propietario evita recorrer toda la colección si el volumen lo exige.
- [x] Documentar qué ocurre al cambiar contraseña en WorkOS.
  - Mira extrae solo el `sid` del access token devuelto por AuthKit y no conserva access ni refresh token de WorkOS. Una sesión local nueva queda vinculada a ese `sid` para que `session.revoked` la invalide.
  - Las sesiones locales creadas antes de este cambio no tienen `sid`; expiran a los 30 días o se revocan con «Cerrar todas las sesiones».
  - «Cerrar todas las sesiones» sigue revocando sesiones de Mira, no la sesión hospedada de WorkOS ni el grant de Google.
  - Referencia: [sesiones de AuthKit](https://workos.com/docs/authkit/sessions).
- [x] Evaluar invalidación cuando WorkOS suspende al usuario.
  - Implementado localmente: `POST /auth/workos/webhook` valida `workos-signature` (`t`, `v1`, HMAC-SHA256 de `t.body`) sobre el cuerpo crudo y rechaza más de cinco minutos de desfase.
  - `session.revoked` invalida solo la sesión local asociada al `sid`. `user.deleted` revoca las sesiones locales del propietario, borra localmente la conexión Gmail y desactiva scheduler/mailbox; no llama a Google para revocar el grant remoto.
  - Las escrituras son idempotentes por estado. El procesamiento es síncrono y no guarda el ID del evento: si el volumen exige cola, deduplicación o recuperación fuera de los reintentos de WorkOS, se deberá añadir antes de escalar.
  - Pendiente: definir una transición segura para `user.updated` y eventos de membresía; no se interpretan como suspensión mientras el modelo de organización no tenga ese contrato.
  - Referencias: [eventos WorkOS](https://workos.com/docs/events) y [provisionamiento de directorio](https://workos.com/docs/authkit/directory-provisioning).

## 5.3 Separar sesión y conexión de Gmail

- [x] Diseñar el refresh token de Gmail como credencial de la conexión de casilla.
  - `gmailConnections/{hash_del_propietario}` almacena una conexión por casilla en esta primera versión, fuera de `users/{session_id}`.
- [x] Evitar que el scheduler dependa de una sesión web histórica.
  - El scheduler lee y actualiza la conexión Gmail; ya no consulta sesiones web.
- [x] Asociar credenciales Gmail a:
  - Organización y mailbox mediante el perfil de propietario existente (una casilla por organización en esta versión).
  - Usuario autorizador mediante `owner_email`, coherente con `Mailbox.authorized_by_user_email`.
- [x] Mantenerlas cifradas.
  - Access y refresh token continúan usando `APP_ENCRYPTION_KEY` antes de persistirse.
- [x] Registrar cuándo fueron actualizadas.
  - La conexión conserva `connected_at` y `updated_at`.
- [x] Registrar revocación.
  - Desconectar Gmail vacía tokens y conserva `revoked_at`.
- [x] Permitir rotación del refresh token.
  - El refresh del scheduler reemplaza el token rotado en la conexión, no en la sesión.
- [x] Mantener sesiones web revocables sin romper el scheduler.
  - Se migra perezosamente la credencial legacy más reciente con refresh token y luego se limpia de las sesiones. Firestore pagina el barrido actual; se requiere consulta indexada por propietario si el escaneo de toda la colección deja de ser aceptable.

## 5.4 Cookies

- [x] Confirmar `HttpOnly`.
- [x] Confirmar `Secure`.
- [x] Confirmar `SameSite` acorde a la arquitectura final.
  - Decisión: `Lax` para sesión y estado OAuth bajo el proxy mismo-origen; los callbacks OAuth son redirects GET de nivel superior.
  - La revisión desplegada aún conserva `APP_COOKIE_SAMESITE=None` y requiere ese cambio de configuración más un smoke test antes de darlo por aplicado en producción.
- [x] Preferir mismo origen mediante proxy para evitar cookies third-party.
- [x] Definir `Domain` solo si es estrictamente necesario.
  - Decisión actual: no se emite atributo `Domain`; web entrega la cookie bajo
    su propio origen al reenviar las callbacks a API.
- [x] Evaluar prefijo `__Host-` para cookie de sesión.
  - Decisión: no migrar en esta revisión. La cookie actual es host-only (sin `Domain`), usa `Path=/` y producción exige `Secure`; cambiar el nombre invalidaría todas las sesiones y no corrige un riesgo P0 adicional. Reconsiderar en una migración planificada de sesiones.
- [x] Evitar exponer tokens a JavaScript.
- [x] Revisar cookies de OAuth temporales.
- [x] Confirmar expiración breve de cookies OAuth.
  - `ghmi_oauth` expira en 600 segundos.

## 5.5 CSRF

- [x] Determinar si las llamadas autenticadas del navegador son same-origin.
  - `ghmi-web` proxya `/auth`, `/gmail`, `/me`, `/analysis-runs`, `/threads` y rutas afines a API bajo el mismo origen. Los webhooks directos no usan cookie de usuario y validan su propio secreto/firma.
- [x] Validar `Origin` en requests mutables.
- [x] Rechazar orígenes desconocidos.
- [ ] Evaluar token CSRF si se conserva `SameSite=None`.
- [x] Probar un POST desde un origen externo.
- [x] Asegurar las mutaciones no relacionadas con pagos:
  - Logout individual y global.
  - Revisión manual.
  - Cambios de configuración y presets.
  - Borrado de datos.
  - Desconexión de Gmail.
  - Creación e inicio de análisis.
  - El middleware global rechaza el origen antes de entrar al handler; una prueba recorre esas rutas. Checkout, cancelación y cambio de plan conservan el mismo middleware, pero sus pruebas funcionales siguen diferidas con pagos.

---

# 6. Errores, mensajes, logs y exposición de información

Prioridad: **P0**

## 6.1 Contrato público de errores

- [x] Definir un formato único:

```json
{
  "error": {
    "code": "PAYMENT_PROVIDER_UNAVAILABLE",
    "message": "No pudimos confirmar el pago. Inténtalo nuevamente.",
    "request_id": "req_..."
  }
}
```

- [x] Definir catálogo de códigos públicos.
- [x] Separar mensaje técnico y mensaje al usuario.
- [x] No devolver `error.to_string()` directamente.
- [x] No devolver bodies de proveedores.
- [x] No devolver URLs internas, nombres de colecciones o stack traces mediante errores inesperados.
- [x] Mantener mensajes 401/403/404 consistentes.
- [x] Evitar confirmar existencia de recursos de otro usuario.
  - Evidencia 2026-07-15: prueba de contrato cubre `401` sin sesión, `403`
    por origen no confiable y el mismo `404` para un análisis ajeno o ausente.

Catálogo inicial: `BAD_REQUEST`, `AUTHENTICATION_REQUIRED`, `FORBIDDEN`,
`SUBSCRIPTION_REQUIRED`, `NOT_FOUND`, `CONFLICT`, `RATE_LIMITED`,
`EXTERNAL_SERVICE_UNAVAILABLE`, `SERVICE_UNAVAILABLE` e `INTERNAL_ERROR`.
El `request_id` siempre se genera en servidor y se devuelve además en header.

## 6.2 Proveedores externos

Revisar y sanitizar errores de:

- [x] WorkOS.
- [x] Google OAuth.
- [x] Gmail API.
- [ ] Mercado Pago.
- [x] Resend.
- [x] Bedrock.
- [x] AI worker.
- [x] Firestore.
- [x] Metadata server de GCP.

Evidencia 2026-07-15: auditoría estática de logs, cuerpos de respuesta y
propagación de errores. Los proveedores no financieros conservan como máximo
status/código seguro o un identificador permitido; no registran bodies ni
excepciones completas. Mercado Pago queda diferido junto con su incidente.

Para cada proveedor:

- [ ] Crear código interno de categoría.
- [ ] Crear mensaje público.
- [ ] Registrar status code externo.
- [ ] Registrar request/provider ID si es seguro.
- [ ] Redactar tokens, cuerpos y datos personales.

## 6.3 Identificadores de correlación

- [x] Generar `request_id` por request.
- [x] Aceptar un request ID entrante solo si se valida o regenerarlo.
- [x] Devolverlo en header y error público.
- [x] Incluirlo en logs.
- [x] Propagarlo al worker.
  - API reenvía el UUID a los endpoints de auditoría y el worker sólo acepta
    valores UUID válidos; de otro modo genera uno propio.
- [x] Asociarlo a `run_id`.
  - `analysis_run_created` registra `run_id` dentro del span HTTP que ya contiene `request_id`; `scheduled_analysis_run_created` registra el mismo identificador para ejecuciones sin request HTTP.
- [ ] Asociarlo a:
  - `checkout_id`.
  - `subscription_id`.
  - `org_id` pseudonimizado.
- [ ] No usar email como identificador principal en logs.

## 6.4 Redacción de logs

### Avance local — 2026-07-15

- [x] Los errores persistidos de análisis y scheduler usan categorías seguras
  (`analysis_failed`, `scheduled_analysis_failed`,
  `report_delivery_failed`) en vez de propagar el texto original.
- [x] Los helpers no relacionados con pagos para Google OAuth, WorkOS, Resend y
  Firestore dejan de registrar o devolver cuerpos completos de proveedores.
- [x] El callback de WorkOS no devuelve `error_description` del proveedor; usa
  un mensaje fijo y registra `workos_login_rejected`.
- [x] El scheduler registra solo totales por resultado, no su objeto completo
  con correo de usuario.
- [x] Un fallo de auditoría IA detallada se persiste como razón fija para
  revisión manual y se registra con `ai_detailed_audit_failed`, sin incluir el
  texto del proveedor.
- [x] Validar la estructura de logs HTTP de API en Cloud Logging.
  - Evidencia 2026-07-16: una petición controlada a
    `ghmi-api-00033-xut` devolvió 200 y un UUID generado por servidor. El
    evento correlacionado contiene `service`, `environment`, método, ruta,
    `request_id`, operación, estado y duración; no expone campos de token,
    secreto, cookie, autorización, cuerpo ni email.
- [ ] Completar revisión extremo a extremo de Bedrock y de errores reales de
  proveedores. Mercado Pago queda expresamente diferido por el incidente de
  pagos actual.

- [ ] No registrar access tokens.
- [ ] No registrar refresh tokens.
- [ ] No registrar cookies.
- [ ] No registrar secretos.
- [ ] No registrar números de tarjeta.
- [ ] No registrar bodies completos de correo.
- [ ] No registrar excerpts salvo necesidad justificada.
- [ ] No registrar payload completo enviado a IA.
- [ ] No registrar headers completos.
- [ ] No registrar respuestas completas de proveedores.
- [ ] Pseudonimizar email y org cuando sea posible.
- [x] Revisar errores persistidos en runs y scheduler.

## 6.5 Frontend y navegador

- [x] Buscar `console.log`.
- [x] Buscar `console.error`.
- [x] Eliminar logs innecesarios de producción.
- [x] No imprimir objetos de error completos.
- [x] Revisar mensajes visibles en banners y modales no financieros.
  - Evidencia 2026-07-16: revisión estática de vistas y componentes de la app
    autenticada; los errores siguen usando el contrato público de API. Los
    mensajes y modales de checkout quedan diferidos junto con pagos.
- [x] Traducir estados técnicos a lenguaje de usuario.
  - Evidencia 2026-07-16: estados de análisis y ejecución programada se
    traducen antes de renderizarse; se eliminaron IDs, categorías de error,
    tokens de IA y conteos de llamadas al proveedor de la UI.
- [ ] Revisar Network:
  - Login fallido.
  - Gmail OAuth fallido.
  - Pago fallido.
  - Análisis fallido.
  - Worker caído.
  - Firestore caído.
- [ ] Confirmar que ninguna respuesta contiene secretos o detalles internos.
- [x] Revisar Source Maps.
- [x] Decidir si se publican source maps.
  - Decisión: no generar ni publicar source maps en el build inicial.
- [-] Si se publican a una herramienta, impedir acceso público.
  - No aplica mientras no se generen; reevaluar al incorporar una herramienta
    de seguimiento de errores.

## 6.6 Pasada de copy

- [x] Unificar nombre “Mira Helpdesk”.
- [x] Eliminar nombres internos o históricos.
- [x] Eliminar referencias técnicas como:
  - `policy_snapshot`.
  - `pending_backend_contract`.
  - `provider_subscription_id`.
  - `invalid_grant`.
  - Evidencia 2026-07-16: búsqueda en `apps/web/src` sin coincidencias; la
    UI autenticada además deja de mostrar proveedores, scopes, IDs de
    ejecución, estados crudos ni categorías internas de error.
- [x] Corregir mensajes en español de la UI autenticada.
  - Evidencia 2026-07-16: Configuración, Resumen, Ayuda, Cuenta, Privacidad y
    estados de análisis usan lenguaje de usuario. Los borradores legales se
    mantienen pendientes de datos del responsable y revisión legal.
- [ ] Corregir textos contradictorios.
- [x] Confirmar que no aparece “beta” en copy público, precios, términos ni onboarding.
- [x] Confirmar que no se promete funcionalidad futura.
  - El nombre interno del runbook de despliegue sigue siendo técnico y no se
    muestra al usuario.

---

# 7. Uso de IA, privacidad y consentimiento informado

Prioridad: **P0**

## 7.1 Posicionamiento coherente

Decisión implementada: la auditoría IA viene activa por defecto en organizaciones
nuevas; la organización puede desactivarla desde Configuración. Reactivarla exige
confirmación explícita y se aplica a análisis futuros.

- [x] Definir formalmente la IA como funcionalidad activa por defecto con opt-out.
- [x] Dejar de describirla como opt-in si no requiere activación.
- [x] Actualizar README.
- [x] Actualizar landing.
- [x] Actualizar onboarding.
- [x] Actualizar Configuración.
- [x] Actualizar Ayuda.
- [ ] Actualizar Política de Privacidad.
- [ ] Actualizar Términos de Uso.

Copy sugerido:

> Mira utiliza IA para clasificar fragmentos minimizados de los correos analizados. Puedes desactivarla cuando quieras desde Configuración. Al desactivarla, algunas clasificaciones requerirán revisión manual y pueden perder precisión.

## 7.2 Información que debe conocer el usuario

- [x] Qué proveedor procesa la IA.
  - Configuración identifica Amazon Bedrock.
- [x] Qué datos se envían.
  - Participantes, fecha, asunto y texto limitado por mensaje; Configuración
    aclara que un mensaje corto puede caber completo dentro del límite.
- [x] Qué datos no se envían.
  - No se procesan adjuntos, imágenes ni headers completos.
- [x] Límite de mensajes por hilo.
  - Configuración muestra el valor vigente de la política.
- [x] Límite de caracteres por mensaje.
  - Configuración muestra el valor vigente de la política.
- [x] Ausencia de adjuntos.
  - La configuración informa la exclusión y el normalizador descarta partes con
    `filename` o `attachmentId`, incluso si son `text/*`.
- [x] Posibilidad de texto sensible en excerpts.
  - Configuración lo advierte junto con el límite de contenido.
- [x] Objetivo del procesamiento.
  - Clasificar casos ambiguos para revisión manual.
- [x] Consecuencia de desactivar IA.
  - Los casos inciertos pasan a revisión manual.
- [x] Cómo desactivarla.
  - Disponible mediante el control de Auditoría IA en Configuración.
- [x] Cuándo aplica el cambio.
  - Configuración indica que se aplica al próximo análisis.
- [ ] Política de retención del proveedor, si corresponde.
- [ ] Región de procesamiento, si es relevante.

## 7.3 Registro de aceptación

- [ ] Guardar versión de términos aceptada.
- [ ] Guardar versión de política de privacidad aceptada.
- [ ] Guardar timestamp.
- [ ] Guardar usuario que aceptó.
- [ ] Guardar versión de política IA aplicable.
- [ ] Pedir nueva aceptación solo cuando exista un cambio material.
- [ ] Permitir consultar las versiones vigentes.

## 7.4 Revisión de minimización

- [x] Confirmar que bodies completos no se persisten.
  - Evidencia 2026-07-15: la única función que prepara mensajes para
    `StorageRepository` borra `body_text`; se usa tanto en el flujo normal como
    en el de override manual y tiene prueba dedicada.
- [x] Confirmar que el worker solo recibe el contenido necesario.
  - Evidencia 2026-07-15: antes de la auditoría detallada, los mensajes se
    limitan al máximo de mensajes y caracteres de la política; hay prueba del
    recorte y del límite de cantidad.
- [x] Confirmar límites reales contra el copy mostrado.
  - Evidencia 2026-07-15: Configuración conserva y muestra los límites de la
    política cargada, en vez de reescribir o anunciar constantes fijas.
- [x] Confirmar que no se envían adjuntos.
  - Evidencia 2026-07-15: prueba de payload mixto conserva el cuerpo del correo
    y excluye un adjunto `text/plain` marcado por Gmail.
- [x] Confirmar que headers almacenados están limitados.
  - Evidencia 2026-07-15: sólo se conserva `auto-submitted`, necesario para
    distinguir correo automático; los demás headers de Gmail se descartan antes
    de persistir.
- [ ] Revisar snippets y subjects como datos personales.
- [x] Revisar datos incluidos en reportes por correo.
  - Los ítems de revisión ocultan asunto y remitente salvo habilitación expresa.
- [x] Confirmar modo métricas-only por defecto.
  - Evidencia 2026-07-15: la política inicial no incorpora ítems de revisión y
    las pruebas cubren ambos modos.

---

# 8. Desconexión, eliminación y ciclo de vida de datos

Prioridad: **P0**

## 8.1 Desconectar Gmail

- [x] Añadir acción disponible en UI.
- [x] Exigir confirmación.
- [x] Revocar el token en Google cuando sea posible.
- [x] Marcar mailbox como revocada.
- [x] Eliminar o inutilizar access token.
- [x] Eliminar o inutilizar refresh token.
- [x] Desactivar scheduler al dejarlo sin credenciales Gmail.
- [x] Bloquear nuevos análisis.
- [x] Mantener datos históricos; la UI lo explica antes de confirmar.
- [x] Permitir reconectar.
- [ ] Probar reconexión con la misma cuenta.
- [ ] Probar reconexión con otra cuenta, si se permite.

## 8.2 Borrar análisis

- [ ] Definir borrado de un run.
- [x] Definir borrado de todos los runs del usuario.
- [x] Eliminar:
  - Analysis run.
  - Threads.
  - Messages derivados.
  - AI audits.
  - Manual reviews asociadas.
  - Overrides cuya semántica dependa del run.
- [x] No recalcular uso: borrar datos no devuelve cuota consumida.
- [x] Definir que borrar datos no devuelve cuota.
  - Recomendación: no devolver cuota consumida.
- [x] Confirmación fuerte: frase escrita `BORRAR MIS ANALISIS`.
- [x] Operación idempotente.
- [x] Registro de auditoría de la eliminación: `auditLogs/{request_id}` guarda
  hash de propietario, estado y marcas de tiempo, sin correo ni contenido de mensajes.
- [x] Evitar dejar documentos huérfanos: Firestore borra primero subcolecciones conocidas.

## 8.3 Borrar cuenta y organización

### Bloqueo de alcance — 2026-07-15

No se implementa todavía la eliminación de cuenta. El propio plan deja sin
definir quién puede solicitarla, el tratamiento de la cuenta WorkOS y los
registros financieros/legalmente necesarios. Además, exige impedir cobros
futuros, mientras el flujo de pagos está expresamente diferido por el incidente
actual. Implementar sólo una parte podría borrar datos y dejar una suscripción
activa o incumplir una obligación de retención.

**Qué sigue:** acordar esas decisiones y resolver el incidente de pagos antes
de diseñar un endpoint destructivo, su reautenticación y sus pruebas.

- [ ] Definir quién puede solicitarlo.
- [ ] Limitarlo al owner.
- [ ] Exigir reautenticación reciente.
- [ ] Exigir confirmación explícita.
- [ ] Cancelar suscripción o impedir cobros futuros.
- [ ] Desconectar Gmail.
- [ ] Desactivar scheduler.
- [ ] Revocar sesiones.
- [ ] Eliminar configuración.
- [ ] Eliminar políticas y versiones según requisitos.
- [ ] Eliminar presets.
- [ ] Eliminar runs y datos derivados.
- [ ] Eliminar credenciales.
- [ ] Eliminar cuenta WorkOS si corresponde o documentar su tratamiento.
- [ ] Mantener solo registros financieros/legalmente necesarios.
- [ ] Documentar esos registros y su retención.
- [ ] Enviar confirmación final.

## 8.4 Retención

- [ ] Decidir si la retención se aplica por plan o por política.
- [ ] Definir qué entidades expiran.
- [ ] Crear job de eliminación.
- [ ] Ejecutarlo con idempotencia.
- [ ] Registrar cantidad eliminada.
- [ ] Alertar por fallos.
- [ ] Probar con datos de prueba.
- [ ] Probar reejecución.
- [x] No prometer retención automática hasta que el job esté activo.
  - Evidencia 2026-07-15: la UI distingue el plazo configurado de la
    eliminación efectiva y ofrece solamente el borrado manual existente.

## 8.5 Procedimiento manual temporal

Mientras los flujos automáticos no estén completos:

- [ ] Publicar un canal para solicitar eliminación.
- [ ] Definir responsable.
- [ ] Definir plazo.
- [ ] Crear checklist manual.
- [ ] Registrar solicitudes.
- [ ] Verificar identidad.
- [ ] Confirmar ejecución.
- [ ] Auditar que no quedan datos.

---

# 9. Firestore: hardening inmediato

Prioridad: **P0**

## 9.1 Recuperación

- [x] Habilitar Point-in-Time Recovery.
  - Evidencia 2026-07-16: `(default)` informa
    `POINT_IN_TIME_RECOVERY_ENABLED`.
- [x] Habilitar protección contra eliminación de la base.
  - Evidencia 2026-07-16: `(default)` informa
    `DELETE_PROTECTION_ENABLED`.
- [x] Confirmar periodo de recuperación.
  - Evidencia 2026-07-16: `versionRetentionPeriod=604800s` (7 días) y
    `earliestVersionTime=2026-07-15T19:07:00Z`. La ventana crecerá hasta los
    siete días desde la habilitación.
- [x] Documentar restauración.
  - `docs/operational-runbooks.md` define un clone a una base nueva para el
    drill; no se restaura `(default)` in-place.
- [ ] Crear un entorno o proyecto para probar restore.
- [ ] Ejecutar una prueba real de restauración.
- [ ] Medir tiempo de recuperación.
- [ ] Definir RPO.
- [ ] Definir RTO.

Sugerencia inicial:

- RPO: 24 horas o mejor.
- RTO: 4 horas para la versión inicial.

### Estado histórico antes de aplicar controles — 2026-07-15

Firestore `(default)` confirma `POINT_IN_TIME_RECOVERY_DISABLED` y
`DELETE_PROTECTION_DISABLED`; los dos controles siguen siendo P0 abiertos.
La última auditoría de solo lectura también confirmó `maxScale=20` en
`ghmi-api`, `ghmi-web` y `ghmi-ai-worker`; no hubo cambios de infraestructura.

### Estado actual — 2026-07-16

El estado anterior fue reemplazado por los controles aplicados: Firestore
`(default)` tiene PITR y delete protection habilitados. El runbook usa un clone
a una base nueva para una recuperación o drill; la prueba real, RPO y RTO
siguen pendientes.

## 9.2 Permisos

- [x] Revisar permisos de la service account.
- [ ] Mantener mínimo privilegio.
- [ ] Evitar roles Owner/Editor.
- [ ] Confirmar que web y worker no acceden directamente a Firestore.
- [ ] Separar service accounts por servicio cuando sea útil.
- [ ] Revisar acceso humano al proyecto.
- [ ] Activar MFA para cuentas administrativas.
- [x] Revisar claves de service account.
- [ ] Evitar claves persistentes en producción.

### Auditoría de identidades — 2026-07-16

- `ghmi-api` y `ghmi-ai-worker` con tráfico usan
  `ghmi-runtime@...`; esa identidad tiene `roles/datastore.user` y
  `roles/secretmanager.secretAccessor`. El código del worker no contiene
  cliente Firestore, pero conserva el permiso por compartir identidad con API.
- `ghmi-web` con tráfico usa la identidad Compute Engine por defecto, que sí
  tiene `roles/editor`; no se considera mínimo privilegio.
- Las candidatas sin tráfico ya separan esos roles: `ghmi-web-00021-qod` usa
  `ghmi-web-runtime` sin roles de proyecto y devuelve HTTP 200;
  `ghmi-ai-worker-00011-fom` usa `ghmi-worker-runtime`, sin roles de proyecto
  y con acceso a nivel de recurso sólo a `bedrock-token`. Ambas están `Ready`
  y con 0% de tráfico.
- Las identidades nuevas no tienen claves `USER_MANAGED`. La invocación
  privada API→worker sigue autorizada para `ghmi-runtime`.
- Existe `ghmi-firestore@...` con `roles/datastore.user` y una clave
  `USER_MANAGED` activa. No está asignada a Cloud Run, pero su credencial está
  en `secrets/gcp-service-account.json` para desarrollo local. No revocarla ni
  reutilizarla en producción hasta decidir su reemplazo local y la rotación.

## 9.3 Integridad y concurrencia

- [x] Identificar operaciones read-modify-write.
  - Evidencia: `docs/firestore-concurrency-audit.md` inventaría scheduler, configuración, runs, Gmail, sesiones y lookup de cuentas sin mezclar el flujo de pagos diferido.
- [x] Proteger contadores de uso contra incrementos perdidos.
  - `add_usage` aplica el delta con `currentDocument.updateTime` o
    `currentDocument.exists=false`, y reintenta hasta tres veces. Cubre los
    análisis creados, hilos analizados y auditorías IA; la prueba concurrente
    de `MemoryStorage` verifica que ambos deltas se conservan.
  - La reserva atómica de cupo junto con la creación del análisis sigue
    pendiente antes de aumentar la concurrencia o el número de instancias.
- [x] Proteger claims del scheduler.
  - `scheduleStates/{email}` se crea con `currentDocument.exists=false` o se reemplaza con la precondición `currentDocument.updateTime`; tras tres conflictos se aborta sin ejecutar el análisis duplicado.
  - Verificado localmente con 159 pruebas Rust, 10 del worker, Clippy y build web; falta el escenario de dos instancias desplegadas.
- [x] Evitar que una configuración tardía reactive scheduler tras revocación Gmail/WorkOS.
  - La sincronización de `scheduleConfigs` exige una `GmailConnection` activa; la preferencia queda guardada para una reconexión válida. La protección de conflictos generales de configuración sigue pendiente.
- [x] Evitar que un refresh Gmail tardío restaure una conexión revocada.
  - El refresh compara la conexión leída con la actual y, en Firestore, su `updateTime`; si cambia, cancela el tick sin reescribir tokens.
- [x] Evitar inicios duplicados del mismo análisis.
  - `claim_pending_analysis_run` cambia `Pending` a `Running` sólo si el
    documento conserva el `currentDocument.updateTime`; quien pierde la
    carrera recibe conflicto y no crea una segunda tarea en segundo plano.
  - La prueba concurrente de `MemoryStorage` demuestra un único claim; falta
    validarlo contra Firestore desplegado.
- [ ] Proteger actualización de suscripciones.
- [ ] Proteger creación de checkout.
- [ ] Usar transacciones/precondiciones donde corresponda.
- [ ] Confirmar comportamiento ante retries.
- [ ] Confirmar comportamiento con dos requests concurrentes en Firestore desplegado.
  - La prueba local demuestra un único claim en `MemoryStorage`; falta ejecutar el mismo escenario con dos instancias Cloud Run y Firestore real.

## 9.4 Rendimiento provisional

- [ ] Eliminar búsquedas que listan colecciones completas en rutas frecuentes.
- [x] Evitar listar todas las cuentas para buscar por email.
  - `accountEmailIndexes/{hash_del_email}` apunta a la cuenta WorkOS y evita el escaneo en lecturas nuevas. Las cuentas legacy se indexan perezosamente en la primera lectura encontrada.
- [ ] Evitar listar todas las sesiones para buscar refresh token.
- [ ] Evitar listar todas las suscripciones para buscar provider ID.
- [ ] Evitar listar todos los checkouts para buscar provider ID.
- [x] Crear documento lookup para cuentas por email.
  - Otros lookups e índices siguen pendientes; no se alteraron los flujos de suscripción o checkout mientras pagos está diferido.
- [ ] Definir limpieza de sesiones históricas.
- [ ] Definir limpieza de checkouts abandonados.
- [ ] Medir lecturas Firestore por flujo.
- [ ] Configurar presupuesto y alerta de costes.

---

# 10. Decisión sobre Supabase/Postgres

Prioridad: **P0**

La migración de persistencia a PostgreSQL/Supabase es un requisito antes de
habilitar cobros. WorkOS sigue siendo el único proveedor de autenticación y la
API Rust seguirá siendo el único acceso de la aplicación a la base: no se
habilitará acceso directo del frontend a tablas de negocio ni Supabase Auth.

## 10.1 Decisión y base disponible

- [x] Decidir la migración antes de producción pagada.
  - Decisión 2026-07-16: mover la persistencia desde Firestore a PostgreSQL
    administrado por Supabase, conservando WorkOS para autenticación y
    sesiones.
- [x] Identificar proyecto Supabase existente y saludable para Mira.
  - Evidencia 2026-07-16: Management API autenticada informa el proyecto
    `Mira`, organización `Ninfa`, región `us-east-2`, estado
    `ACTIVE_HEALTHY`.
- [x] Guardar la URL de conexión de PostgreSQL exclusivamente en Secret
  Manager.
  - Evidencia 2026-07-16: se rotó la contraseña de base y se creó
    `mira-postgres-url` (versión 2 vigente) sin escribir la URL en Git ni
    mostrarla.
  - La revisión candidata PostgreSQL es la única revisión que la referencia.
- [x] Seleccionar un pooler y límite de conexiones compatible con Cloud Run.
  - La API usa el pooler TLS **de sesión** de Supabase (IPv4, puerto 5432) y
    un pool local máximo de cinco conexiones. Con `maxScale=1` evita abrir una
    conexión por request y permite las sentencias preparadas de SQLx.
- [x] Aceptar el plan actual de Supabase para el lanzamiento temprano.
  - Decisión de la persona responsable 2026-07-16: con cero usuarios externos
    y sin cobros, Supabase Free es suficiente por ahora y no bloquea declarar
    lista la infraestructura de esta etapa. Se reevaluará el plan al crecer el
    uso o antes de asumir compromisos de continuidad mayores.
  - Verificación previa: el proyecto está `ACTIVE_HEALTHY` y no tiene add-ons
    seleccionados; no se hizo ningún cambio de facturación.

## 10.2 Implementación del backend PostgreSQL

- [x] Añadir implementación `PostgresStorage` de `StorageRepository` sin
  cambiar contratos HTTP ni WorkOS.
  - `APP_STORAGE=postgres` selecciona el backend nuevo; `memory` y
    `firestore` conservan sus comportamientos actuales para pruebas y rollback.
- [x] Crear migraciones SQL versionadas y reproducibles.
  - Evidencia 2026-07-16: `0001_mira_records` se aplicó a Supabase y quedó
    registrada con checksum en `mira.schema_migrations`; el aplicador rechaza
    archivos modificados tras ser aplicados.
- [x] Crear la base privada con claves e índices para las búsquedas actuales.
  - `mira.records` conserva el payload JSON compatible con el contrato actual
    y separa las claves de consulta (cuenta/email, sesiones WorkOS,
    organización, proveedor, propietario, run y thread) en columnas indexadas.
    Es una primera capa segura de persistencia; la normalización adicional se
    hará sólo cuando una consulta real la requiera.
- [x] Usar operaciones atómicas para usage ledger, claims de scheduler/análisis
  y borrado de datos.
  - El usage ledger usa un UPSERT acumulativo; el claim del scheduler usa una
    transacción con bloqueo de fila; el claim de análisis es `UPDATE ... WHERE
    state='pending'`; y el borrado se realiza en una sentencia atómica.
  - Las futuras operaciones de pagos siguen pendientes y fuera de alcance.
- [x] Mantener tablas de negocio inaccesibles desde el frontend.
  - `mira` no es un schema público y se revocaron privilegios a `anon`,
    `authenticated`, `service_role` y `PUBLIC`.
- [x] Activar RLS como defensa adicional, sin depender de ella para el flujo de
  backend.
  - Evidencia 2026-07-16: `pg_class.relrowsecurity=true` para
    `mira.records` y los roles de frontend no tienen `SELECT`.

## 10.3 Migración y cutover

- [x] Inventariar conteos y entidades de Firestore antes de copiar datos.
  - Evidencia 2026-07-16: la instantánea validada conserva 4 cuentas, 49
    sesiones, 1 conexión Gmail, 2 configuraciones de scheduler, 59 runs,
    1.235 threads, 4.092 mensajes, 762 auditorías y 75 revisiones manuales.
- [x] Crear exportador Firestore e importador PostgreSQL de instantánea.
  - `MIGRATE_FIRESTORE_TO_POSTGRES=true` sólo se permite fuera de
    `APP_ENV=production`, nunca escribe en Firestore y reemplaza el destino
    candidato completo. Conserva las claves de origen y aborta si el total
    PostgreSQL no coincide con el total importado.
- [x] Validar una instantánea completa por conteos, segunda ejecución y
  referencias.
  - Corrección 2026-07-16: los conteos antes anotados (2 runs, 31 threads,
    125 mensajes y 28 auditorías) eran una copia parcial y no constituyen la
    aceptación de la migración.
  - Dos instantáneas consecutivas terminaron con 6.284 registros. Las cuatro
    comprobaciones de relación run/thread/message/audit/manual review
    devolvieron cero huérfanos.
- [x] Verificar que las migraciones de revisión manual v1/v2 estén aplicadas
  en Firestore antes de importar.
  - El importador aborta antes de escribir si falta cualquiera de los dos
    marcadores; PostgreSQL no las reejecuta.
- [x] Cargar y validar la base candidata con una copia de datos, sin tráfico
  de usuarios.
  - La copia reemplazó sólo `mira.records` de Supabase; la revisión Firestore
    `ghmi-api-00031-6lx` conserva 100% de tráfico.
- [x] Definir una ventana breve de escritura congelada para el cutover; no se
  aplicará dual-write, para no introducir dos fuentes de verdad.
  - Decisión 2026-07-16: antes de la promoción, la persona responsable pide
    al único tester pausar el uso de Mira y pausa Cloud Scheduler; se espera
    que terminen las requests en curso, se ejecuta la instantánea final y sólo
    entonces se mueve el tráfico. Scheduler se reanuda después de validar
    PostgreSQL o de volver a Firestore.
- [x] Documentar rollback a Firestore antes de mover tráfico.
  - `docs/postgres-cutover-runbook.md` exige ventana de escritura congelada
    cuando existan usuarios, registra la revisión previa y prohíbe declarar un
    rollback seguro si PostgreSQL recibió escrituras sin reconciliar.
- [x] Mover primero una candidata de API a PostgreSQL sin tráfico.
  - Evidencia 2026-07-16: `ghmi-api-00036-yad`, tag `postgres`, arrancó con
    `APP_STORAGE=postgres`, la identidad dedicada y
    `POSTGRES_DATABASE_URL=mira-postgres-url:2`; `/health` respondió 200 y
    Cloud Logging no registró errores.
  - La revisión vigente del tag `postgres` es `ghmi-api-00037-vov`; conserva
    `APP_STORAGE=postgres` e identidad dedicada, está `Ready=True`, recibe 0%
    de tráfico y usa el callback Google público canónico.
- [ ] Verificar en la candidata login, Gmail, análisis,
  scheduler, borrado y auditoría.
  - Avance 2026-07-16: `GET /auth/workos/login` de la candidata devolvió 307
    con callback público `https://mira.ninfasolutions.com/auth/workos/callback`.
    La persona responsable ya registró en Google el origen JavaScript y la URI
    `https://mira.ninfasolutions.com/gmail/connect/callback`; la candidata
    vigente ya usa esa URI. Falta completar una sesión real y los flujos que
    escriben datos.
- [ ] Retirar Firestore del runtime sólo después de un periodo de observación y
  respaldo exportado.

## 10.4 Modelo relacional inicial esperado

El backend actual usa `mira.records` como capa privada compatible con el
contrato existente. Normalizar tablas no es un gate adicional para este
cutover: se hará por entidad sólo cuando una consulta medida requiera relaciones
o índices que el modelo actual no cubra.

La persona responsable acepta este modelo inicial para producción temprana: no
se reescribirá la persistencia como tablas por entidad hasta que una consulta o
reporte real justifique la migración incremental.

- [ ] `accounts`
- [ ] `organizations`
- [ ] `memberships`
- [ ] `mailboxes`
- [ ] `mailbox_credentials`
- [ ] `policy_drafts`
- [ ] `policy_versions`
- [ ] `subscriptions`
- [ ] `checkout_sessions`
- [ ] `payment_events`
- [ ] `usage_ledgers`
- [ ] `analysis_runs`
- [ ] `threads`
- [ ] `messages`
- [ ] `ai_audits`
- [ ] `manual_reviews`
- [ ] `manual_review_overrides`
- [ ] `schedule_configs`
- [ ] `schedule_states`
- [ ] `filter_presets`
- [ ] `user_sessions`
- [ ] `audit_log`

---

# 11. Observabilidad mínima con GCP

Prioridad: **P0**

Decisión inicial recomendada:

- Cloud Logging para logs.
- Cloud Monitoring para métricas y alertas.
- Uptime Checks para disponibilidad.
- Métricas basadas en logs para eventos de negocio.
- Sin Loki/Prometheus/Grafana autohospedados durante la versión inicial.

## 11.1 Logs estructurados

- [x] Emitir JSON estructurado desde API.
- [x] Emitir JSON estructurado desde worker.
- [ ] Mantener niveles:
  - `INFO`: operación normal relevante.
  - `WARN`: degradación recuperable.
  - `ERROR`: operación fallida.
- [ ] Incluir:
  - [x] `service`.
  - [x] `environment`.
  - [x] `request_id`.
  - [x] `operation`.
  - [x] `status`.
  - Evidencia 2026-07-15: inicio local y un `GET /auth/me` fallido emitieron JSON; el evento incluyó el span con `method`, `path` y `request_id` generado por servidor.
  - [x] `duration_ms`.
  - [x] `run_id`, cuando aplique.
  - `checkout_id`, cuando aplique.
  - `error_code`, cuando aplique.
- Avance 2026-07-15: el worker emite eventos JSON por request con `service`,
  `environment`, `request_id`, `operation`, `status` y `duration_ms`; sus
  errores Bedrock incluyen un `error_code` seguro y el `run_id` cuando existe.
  Falta comprobarlos en Cloud Logging tras despliegue.
- [ ] No incluir datos sensibles.
- [ ] Definir retención de logs.
- [ ] Revisar coste estimado.

## 11.2 Uptime checks

- [ ] Uptime check para web.
- [ ] Uptime check para API `/health`.
- [ ] Considerar un readiness interno que verifique Firestore.
- [ ] No incluir dependencias costosas en cada liveness check.
- [ ] Configurar frecuencia.
- [ ] Configurar múltiples regiones.
- [ ] Crear canal de notificación.
- [ ] Probar alerta provocando una condición controlada.

## 11.3 Alertas técnicas

- [ ] Tasa de 5xx en API.
- [ ] Tasa de 5xx en web/proxy.
- [ ] Errores del worker.
- [ ] Latencia p95 API.
- [ ] Latencia del worker.
- [ ] Instancias reiniciándose.
- [ ] Uso alto de memoria.
- [ ] Uso alto de CPU.
- [ ] Requests 429 anormalmente altos.
- [ ] Errores de autenticación anormalmente altos.
- [ ] Fallos de acceso a Firestore.

## 11.4 Alertas de negocio

Crear métricas basadas en logs para:

- [ ] `payment_webhook_failed`.
- [ ] `payment_reconciliation_mismatch`.
- [ ] `checkout_stuck`.
- [ ] `analysis_failed`.
- [ ] `scheduled_analysis_failed`.
- [ ] `scheduled_analysis_missed`.
- [ ] `gmail_refresh_invalid_grant`.
- [ ] `resend_delivery_failed`.
- [ ] `ai_worker_failed`.
- [ ] `bedrock_failed`.
- [x] `data_deletion_failed`.
  - La API emite el código al fallar un borrado, con hash de propietario y el
    `request_id` del span; falta conectar una alerta en Cloud Monitoring.
- [ ] `retention_job_failed`.

## 11.5 Dashboard operativo en Cloud Monitoring

- [ ] Requests por servicio.
- [ ] Latencia p50/p95/p99.
- [ ] 4xx y 5xx.
- [ ] Análisis creados/completados/fallidos.
- [ ] Duración de análisis.
- [ ] Hilos procesados.
- [ ] Llamadas IA.
- [ ] Tokens IA.
- [ ] Checkouts iniciados.
- [ ] Pagos aprobados/rechazados/pendientes.
- [ ] Scheduler exitoso/fallido.
- [ ] Errores Gmail.
- [ ] Costes o señales de consumo.

## 11.6 On-call mínimo

- [ ] Definir quién recibe alertas.
- [ ] Definir canal:
  - Email.
  - Slack/Discord.
  - SMS solo para incidentes graves.
- [ ] Definir severidades.
- [ ] Definir tiempo de respuesta.
- [ ] Definir cuándo desactivar temporalmente una función.
- [ ] Crear runbook por alerta crítica.

---

# 12. OpenTelemetry y Grafana como siguiente etapa

Prioridad: **P2**

## 12.1 Cuándo incorporarlo

- [ ] Existe más de un servicio difícil de correlacionar.
- [ ] Cloud Logging ya no permite diagnosticar suficientemente.
- [ ] Se necesitan trazas distribuidas.
- [ ] El volumen o coste justifica una plataforma adicional.
- [ ] Existe tiempo para operar o pagar un servicio administrado.

## 12.2 Ruta recomendada

- [ ] Instrumentar API con OpenTelemetry.
- [ ] Instrumentar worker con OpenTelemetry.
- [ ] Propagar contexto de trazas.
- [ ] Añadir spans para:
  - WorkOS.
  - Google OAuth.
  - Gmail.
  - Firestore.
  - Worker IA.
  - Bedrock.
  - Mercado Pago.
  - Resend.
- [ ] Evaluar Grafana Cloud antes de autohospedar.
- [ ] Usar Grafana Alloy como collector si aporta valor.
- [ ] Mantener datos sensibles fuera de atributos y spans.

## 12.3 Evitar durante la versión inicial

- [ ] No desplegar Loki autohospedado.
- [ ] No desplegar Prometheus autohospedado.
- [ ] No desplegar Grafana autohospedado.
- [ ] No desplegar Tempo autohospedado.
- [ ] No administrar almacenamiento, backups y upgrades de observabilidad sin necesidad real.

---

# 13. Panel administrativo

Prioridad: **P1**

El panel admin debe resolver operaciones del negocio. No debe intentar reemplazar Cloud Monitoring o Grafana.

## 13.1 Acceso y autorización

- [ ] Crear rol admin explícito.
- [ ] No depender solamente de una lista de emails privilegiados.
- [ ] Exigir MFA mediante proveedor de identidad.
- [ ] Registrar cada acción administrativa.
- [ ] Mostrar advertencia antes de acciones destructivas.
- [ ] Evitar acceso directo del navegador a Firestore.

## 13.2 Vista de organizaciones

- [ ] Lista de organizaciones.
- [ ] Estado de onboarding.
- [ ] Plan.
- [ ] Estado de suscripción.
- [ ] Gmail conectado/revocado.
- [ ] Scheduler activo.
- [ ] Último análisis.
- [ ] Último análisis exitoso.
- [ ] Uso del período.
- [ ] Estado de trial.
- [ ] Fecha de creación.

## 13.3 Acciones administrativas

- [ ] Ver estado sin mostrar secretos.
- [ ] Revocar sesiones.
- [ ] Desconectar Gmail.
- [ ] Desactivar scheduler.
- [ ] Reintentar reconciliación de pago.
- [ ] Reintentar una operación idempotente.
- [ ] Iniciar borrado solicitado.
- [ ] Bloquear organización.
- [ ] Quitar acceso interno privilegiado.
- [ ] Ver historial de acciones.

## 13.4 Datos que no debe mostrar

- [ ] Access tokens.
- [ ] Refresh tokens.
- [ ] Cookies.
- [ ] Secretos.
- [ ] Bodies completos de correo.
- [ ] Datos de tarjeta.
- [ ] Payload completo enviado a IA.

---

# 14. CI, calidad y proceso de release

Prioridad: **P0**

## 14.1 Check local

- [x] `cargo fmt --check`.
- [x] `cargo test`.
- [x] `cargo clippy --all-targets -- -D warnings`.
- [x] `python3 -m pytest apps/ai-worker/tests`.
- [x] `npm --prefix apps/web run build`.
- [x] `npm audit --omit=dev`.
  - Se ejecuta desde `scripts/check-all.sh`; la última ejecución local informó 0 vulnerabilidades de runtime.
- [x] `cargo build --release`.
  - Se ejecuta desde `scripts/check-all.sh` junto a formato, tests y Clippy.
- [x] Resolver el warning actual de Clippy.
- [x] Mantener `./scripts/check-all.sh` completamente verde.
  - Evidencia 2026-07-15: 147 tests Rust, 10 tests Python, build web y Clippy sin warnings.

## 14.2 CI

- [x] Crear workflow de CI.
- [ ] Ejecutarlo en pull requests.
- [x] Ejecutarlo antes de deploy.
  - `scripts/redeploy-gcp.sh` ejecuta `scripts/check-all.sh` antes de autenticar Docker, construir o publicar imágenes.
- [ ] Bloquear merge si falla.
- [x] Cachear dependencias.
- [x] No exponer secretos en PRs.
- [x] Añadir escaneo de secretos de archivos rastreados.
  - `scripts/check-secrets.sh` usa `git grep` para rechazar formatos de clave privada, AWS, Google, Mercado Pago, GitHub y secretos `sk_live/prod`; se ejecuta desde `scripts/check-all.sh` y por tanto en CI. No es un scanner de entropía ni inspecciona archivos no versionados.
- [ ] Añadir auditoría de dependencias Rust.
- [ ] Añadir auditoría de dependencias Python.
- [x] Mantener `npm audit`.
  - El gate ejecuta `npm --prefix apps/web audit --omit=dev` antes del build; el último resultado fue 0 vulnerabilidades.

## 14.3 Tests críticos faltantes

- [ ] Tests del nuevo checkout embebido.
- [ ] Tests de webhook duplicado.
- [ ] Tests de webhook fuera de orden.
- [x] Tests de expiración de sesión.
- [x] Tests de logout revocable.
- [x] Tests CSRF/origen.
- [x] Tests de desconexión Gmail.
- [x] Tests de borrado de análisis.
- [ ] Tests de borrado de cuenta.
- [ ] Tests de retención.
- [x] Tests de sanitización de errores.
- [x] Tests de concurrencia de usage ledger.
  - `storage::tests::usage_additions_do_not_lose_concurrent_updates` cubre dos
    incrementos concurrentes y verifica los tres contadores resultantes.

## 14.4 E2E mínimo

- [ ] Landing → registro.
- [ ] Login.
- [ ] Selección de plan.
- [ ] Checkout.
- [ ] Activación mediante webhook.
- [ ] Conexión Gmail.
- [ ] Configuración inicial.
- [ ] Creación de análisis.
- [ ] Ejecución.
- [ ] Visualización de resultados.
- [ ] Revisión manual.
- [ ] Logout.
- [ ] Reconexión de sesión.

## 14.5 Artefactos reproducibles

- [x] Etiquetar imágenes con SHA del commit.
  - Evidencia 2026-07-16: `scripts/redeploy-gcp.sh` usa por defecto los 12
    caracteres del commit actual como `IMAGE_TAG`, admite override explícito y
    rechaza un árbol Git con cambios.
- [x] Evitar depender únicamente de `latest`.
- [x] Registrar qué commit corresponde a cada revisión.
  - Evidencia: `docs/gcp-deploy.md` usa `IMAGE_TAG` en la imagen y documenta
    cómo asociarla a la revisión.
- [ ] Mantener imagen anterior disponible.
- [x] Documentar rollback por servicio.
- [x] Eliminar el lockfile duplicado `apps/web/apps/web/package-lock.json`.
- [x] Fijar dependencias Python mediante lockfile.
- [x] Usar `npm ci` en builds.
- [x] No instalar dependencias de test en imagen final del worker.

---

# 15. Seguridad de contenedores y frontend

Prioridad: **P1**

## 15.1 Contenedores

- [x] Ejecutar API como usuario no root.
- [x] Ejecutar worker como usuario no root.
- [x] Ejecutar web como usuario no root.
- [ ] Reducir paquetes del runtime.
- [x] Separar dependencias build/test/runtime.
- [ ] Escanear imágenes.
- [ ] Revisar vulnerabilidades base.
- [ ] Definir frecuencia de rebuild.
- [ ] Limitar filesystem de escritura cuando sea viable.
- [ ] Definir límites de CPU y memoria.
- [ ] Probar comportamiento ante OOM.

## 15.2 Headers HTTP

- [x] `Content-Security-Policy` mínimo.
  - `base-uri 'self'; frame-ancestors 'none'; object-src 'none'` evita objetos, cambios de base y framing sin limitar aún scripts, conexiones o iframes de OAuth/Gmail/Mercado Pago.
- [x] `Strict-Transport-Security`.
- [x] `X-Content-Type-Options: nosniff`.
- [x] `Referrer-Policy`.
- [x] `Permissions-Policy`.
- [x] `frame-ancestors` o `X-Frame-Options`.
- [x] Política de caché para HTML.
- [x] Caché larga e immutable para assets con hash.
- [x] No cachear respuestas autenticadas sensibles.
  - Evidencia 2026-07-15: respuesta local de `server.mjs` revisada con `curl`; HTML `no-cache`, assets `/assets/*` `immutable` y API proxied `no-store`.

## 15.3 CSP

- [ ] Inventariar orígenes necesarios.
- [ ] Revisar Google Fonts.
- [ ] Revisar scripts de Mercado Pago.
- [ ] Revisar frames necesarios para checkout embebido.
- [ ] No usar `unsafe-eval`.
- [ ] Minimizar `unsafe-inline`.
- [ ] Probar checkout bajo CSP.
- [ ] Probar WorkOS y OAuth.
- [ ] Crear reporte CSP inicialmente si es necesario.

## 15.4 Frontend

- [ ] Dividir bundle si afecta carga.
- [ ] Cargar vistas pesadas de forma diferida.
- [ ] Revisar accesibilidad básica.
- [ ] Probar móvil.
- [ ] Probar Chrome, Firefox y Safari.
- [ ] Probar bloqueo de cookies third-party.
- [ ] Probar refresh en rutas internas.
- [ ] Probar expiración de sesión mientras la app está abierta.

---

# 16. Disponibilidad y resiliencia

Prioridad: **P1**

## 16.1 Health y readiness

- [ ] Mantener liveness simple.
- [ ] Crear readiness separado si es necesario.
- [ ] Readiness puede validar Firestore de forma liviana.
- [ ] No llamar Gmail, Bedrock o Mercado Pago en cada healthcheck.
- [ ] Mostrar solo estado general, sin detalles sensibles.

## 16.2 Timeouts y retries

- [ ] Revisar timeout de WorkOS.
- [ ] Revisar timeout de Google OAuth.
- [ ] Revisar timeout de Gmail.
- [ ] Revisar timeout de Firestore.
- [ ] Revisar timeout del worker.
- [ ] Revisar timeout de Bedrock.
- [ ] Revisar timeout de Mercado Pago.
- [ ] Revisar timeout de Resend.
- [ ] Añadir retries solo para operaciones idempotentes.
- [ ] Aplicar backoff con jitter.
- [ ] Evitar retry automático de cobros no idempotentes.

## 16.3 Degradación controlada

- [ ] Si IA falla, definir si el run:
  - Continúa con heurísticas.
  - Queda en revisión manual.
  - Falla completamente.
- [ ] Si Resend falla, no perder el análisis.
- [ ] Si el scheduler falla, permitir reintento.
- [ ] Si Gmail limita requests, informar y reintentar.
- [ ] Si Mercado Pago no responde, no conceder acceso sin confirmación.
- [ ] Si Firestore no responde, evitar estados parciales.

## 16.4 Jobs largos

- [ ] Medir duración máxima de análisis.
- [ ] Confirmar timeout Cloud Run.
- [ ] Evitar que el navegador sea responsable de completar el proceso.
- [ ] Mantener progreso persistido.
- [ ] Recuperar runs atascados.
- [ ] Definir TTL de estado `running`.
- [ ] Alertar por runs atascados.

---

# 17. Legal, confianza y comunicación

Prioridad: **P0**

> Este checklist organiza implementación y producto; no reemplaza asesoría legal.

## 17.1 Política de privacidad

- [ ] Identidad del responsable.
- [ ] Datos recopilados.
- [ ] Finalidad.
- [ ] Base o autorización aplicable.
- [ ] Proveedores/subprocesadores.
- [ ] WorkOS.
- [ ] Google.
- [ ] GCP/Firestore.
- [ ] AWS Bedrock.
- [ ] Mercado Pago.
- [ ] Resend.
- [ ] Retención.
- [ ] Seguridad.
- [ ] Transferencias internacionales, si aplican.
- [ ] Derechos y solicitudes.
- [ ] Canal de contacto.
- [ ] Fecha y versión.

## 17.2 Términos y condiciones

- [ ] Descripción del servicio.
- [ ] Estado del servicio inicial y sus límites reales.
- [ ] Obligaciones del usuario.
- [ ] Autorización sobre la casilla conectada.
- [ ] Uso permitido.
- [ ] Uso de IA.
- [ ] Exactitud y necesidad de revisión humana.
- [ ] Planes y precios.
- [ ] Renovación.
- [ ] Trial.
- [ ] Cancelación.
- [ ] Reembolsos.
- [ ] Disponibilidad.
- [ ] Limitación de responsabilidad.
- [ ] Suspensión.
- [ ] Terminación.
- [ ] Eliminación de datos.
- [ ] Cambios de términos.
- [ ] Jurisdicción y contacto.

## 17.3 Página de seguridad

- [ ] Gmail readonly.
- [ ] Tokens cifrados.
- [ ] Worker privado.
- [ ] Datos enviados a IA.
- [ ] Datos no enviados.
- [ ] Retención.
- [ ] Proceso de reporte de vulnerabilidades.
- [ ] Contacto de seguridad.
- [ ] No prometer certificaciones inexistentes.
- [ ] No usar lenguaje absoluto como “100% seguro”.

## 17.4 Aceptación

- [ ] Links visibles antes de crear cuenta o contratar.
- [ ] Checkbox o acto inequívoco según decisión legal.
- [ ] Registrar versión aceptada.
- [ ] Acceso permanente a documentos vigentes.
- [ ] Historial de versiones.

---

# 18. Operación y soporte

Prioridad: **P0**

## 18.1 Runbooks

- [ ] Pago no activado.
- [ ] Webhook fallido.
- [ ] Suscripción duplicada.
- [x] Gmail desconectado.
- [x] Refresh token revocado.
- [x] Evento WorkOS de revocación o deprovisioning.
- [x] Análisis atascado.
- [x] Worker IA caído.
- [x] Bedrock caído.
- [x] Resend caído.
- [x] Scheduler omitido.
- [x] Firestore no disponible.
- [x] Borrado solicitado.
- [x] Incidente de seguridad.
- [x] Rollback.
- [ ] Restauración de datos.

Evidencia: `docs/operational-runbooks.md`. Los runbooks de pago permanecen
diferidos junto con el incidente actual de Mercado Pago.

Cada runbook debe incluir:

- [ ] Síntoma.
- [ ] Cómo confirmar.
- [ ] Impacto.
- [ ] Mitigación inmediata.
- [ ] Resolución.
- [ ] Verificación.
- [ ] Comunicación al usuario.

## 18.2 Soporte al usuario

- [ ] Canal visible.
- [ ] Mensaje de recepción.
- [ ] Prioridades de tickets.
- [ ] Registro de incidentes.
- [ ] Plantillas para:
  - Pago.
  - Gmail.
  - Datos.
  - Análisis.
  - Disponibilidad.
- [ ] Procedimiento para solicitar información sin pedir secretos.
- [ ] Nunca solicitar tokens o contraseñas por correo.

## 18.3 Incidentes

- [ ] Definir severidades.
- [ ] Definir quién decide desactivar funciones.
- [ ] Mantener timeline.
- [ ] Preservar evidencia.
- [ ] Comunicar a usuarios afectados.
- [ ] Realizar postmortem sin culpas.
- [ ] Convertir acciones correctivas en checklist.

---

# 19. Costes y abuso

Prioridad: **P1**

## 19.1 Presupuestos

- [ ] Budget de GCP.
- [ ] Alertas de presupuesto.
- [ ] Seguimiento de Firestore.
- [ ] Seguimiento de Cloud Run.
- [ ] Seguimiento de Logging.
- [ ] Seguimiento de AWS Bedrock.
- [ ] Seguimiento de Resend.
- [ ] Seguimiento de WorkOS.
- [ ] Seguimiento de Mercado Pago.

## 19.2 Límites

- [ ] Rate limit de login si WorkOS no lo cubre.
- [ ] Rate limit de checkout.
- [ ] Rate limit de consultas de estado.
- [ ] Rate limit de análisis.
- [ ] Límite de hilos por run.
- [ ] Límite mensual.
- [ ] Límite IA.
- [ ] Límite de destinatarios.
- [ ] Límite de retries.
- [ ] Protección contra loops de scheduler.

## 19.3 Cuentas internas

- [ ] Revisar `INTERNAL_FULL_ACCESS_EMAILS`.
- [ ] Mantener lista mínima.
- [ ] Registrar uso del bypass.
- [ ] Evitar que el bypass relaje aislamiento de datos.
- [ ] Evitar usar cuentas privilegiadas para pruebas rutinarias.
- [ ] Crear proceso para retirar privilegios.

---

# 20. Checklist de lanzamiento

## 20.1 48–72 horas antes

- [ ] Congelar nuevas features.
- [ ] Resolver todos los P0.
- [ ] Ejecutar CI completo.
- [ ] Construir imágenes con commit SHA.
- [ ] Confirmar secretos.
- [ ] Confirmar URLs y redirects OAuth.
- [ ] Confirmar Mercado Pago sandbox.
- [ ] Ejecutar smoke test completo.
- [x] Activar PITR.
  - Evidencia 2026-07-15: Firestore `(default)` con PITR habilitado; falta
    prueba de restauración controlada.
- [ ] Activar alertas.
- [ ] Confirmar canal de soporte.
- [ ] Publicar documentos legales.
- [ ] Revisar copy.
- [ ] Confirmar rollback.

## 20.2 Día del lanzamiento

- [ ] Desplegar revisión.
- [ ] Validar health.
- [ ] Validar login.
- [ ] Validar pago real controlado de bajo riesgo.
- [ ] Validar webhook.
- [ ] Validar Gmail.
- [ ] Validar análisis.
- [ ] Validar email de reporte.
- [ ] Revisar logs.
- [ ] Revisar métricas.
- [ ] Incorporar usuarios gradualmente.
- [ ] Mantener monitoreo activo.

## 20.3 Primeras 24 horas

- [ ] Revisar 5xx.
- [ ] Revisar latencia.
- [ ] Revisar checkouts pendientes.
- [ ] Revisar webhooks.
- [ ] Revisar errores Gmail.
- [ ] Revisar análisis fallidos.
- [ ] Revisar scheduler.
- [ ] Revisar consumo Bedrock.
- [ ] Revisar costes.
- [ ] Contactar proactivamente a usuarios con fallos.

## 20.4 Primera semana

- [ ] Revisión diaria de alertas.
- [ ] Revisión diaria de pagos.
- [ ] Revisión diaria de runs fallidos.
- [ ] Revisión de feedback.
- [ ] Clasificar problemas por frecuencia e impacto.
- [ ] Evitar abrir trabajo multiproveedor salvo demanda real.
- [ ] Decidir primeras mejoras P1.
- [ ] Revisar si Firestore sigue siendo suficiente.

---

# 21. Orden recomendado de ejecución

## Fase A — Bloqueadores inmediatos

- [ ] Terminar checkout embebido.
- [ ] Cerrar seguridad e idempotencia del webhook.
- [ ] Configurar modo producción.
- [ ] Limitar API a una instancia.
- [x] Sanitizar errores.
- [x] Corregir sesión/logout.
- [x] Implementar desconexión Gmail.
- [ ] Definir y habilitar borrado.
- [x] Alinear copy público: sin “beta”, sin prometer multiproveedor.
- [ ] Alinear copy y documentos sobre IA.
- [ ] Retocar landing pública para reducir fondos vacíos y reforzar confianza.

## Fase B — Protección operativa

- [x] Activar PITR y delete protection.
- [x] Crear logs estructurados.
- [x] Crear request IDs.
- [ ] Crear uptime checks.
- [ ] Crear alertas técnicas y de negocio.
- [x] Crear runbooks.
- [x] Corregir check local; falta primera corrida oficial de CI.
- [x] Crear CI.

## Fase C — Lanzamiento controlado

- [ ] Smoke test.
- [ ] Prueba real controlada de pago.
- [ ] Onboarding del primer usuario externo.
- [ ] Monitoreo diario.
- [ ] Resolver errores P0/P1 observados.

## Fase D — Después de validar uso

- [ ] Panel admin.
- [ ] Retención automática.
- [ ] OpenTelemetry.
- [ ] Grafana Cloud si se justifica.
- [ ] Normalización selectiva de entidades PostgreSQL si una consulta medida lo
  requiere.
- [ ] Escalado multi-instancia.
- [ ] Rate limiting distribuido.
- [ ] Conexión multiproveedor.

---

# 22. Registro de progreso

Usar esta tabla para mantener una visión ejecutiva:

| Área | Prioridad | Estado | Responsable | Evidencia | Observaciones |
|---|---:|---|---|---|---|
| Checkout embebido | P0 | En progreso |  | `scripts/check-all.sh` — verde; build Docker sandbox con clave pública | Card Payment Brick → token → `/preapproval` `authorized`, sin redirect; la ejecución real espera login WorkOS. |
| Webhook Mercado Pago | P0 | Diferido |  | Incidente de pagos actual; sin cambios en este avance | Se retoma después de resolver el incidente y autorizar la prueba real. |
| Configuración de entornos | P0 | En progreso |  | Cloudflare: DNS delegado; mapping Cloud Run listo; callbacks y webhook WorkOS production registrados (2026-07-16) | Certificado HTTPS provisionado; falta callback Google OAuth, enlazar las credenciales WorkOS production y deploy candidato. |
| Configuración production | P0 | Listo local |  | `scripts/check-all.sh` — 161 Rust, 10 worker | La validación acepta callbacks HTTPS del origen API o web/proxy y exige secreto de webhook WorkOS; `APP_ENV=production` sigue pendiente de autorización, secretos y pagos. |
| Sesiones y logout | P0 | Listo local |  | `scripts/check-all.sh` — 161 Rust, 10 worker | Expiración absoluta 30 días, logout individual/global revocable, atributos seguros de cookies y revocación WorkOS de nuevas sesiones; falta smoke test desplegado. |
| Ciclo de vida WorkOS | P0 | En progreso |  | Webhook production y smoke firmado en `ghmi-api-00036-yad` (2026-07-16) | La revisión con tráfico conserva staging; faltan login/entrega real y rotación de cookie. |
| Sanitización de errores | P0 | En progreso |  | `scripts/check-all.sh` — 161 Rust, 10 worker | API tiene catálogo público, no enumera análisis ajenos y propaga `request_id`; falta validar el flujo desplegado. |
| Desconexión Gmail | P0 | Listo local |  | `scripts/check-all.sh` — 161 Rust, 10 worker | Revocación Google, tokens borrados, scheduler desactivado y confirmación UI; falta prueba desplegada. |
| Borrado de datos | P0 | En progreso |  | `scripts/check-all.sh` — 161 Rust, 10 worker | Borrado de todos los análisis disponible, auditado sin PII y con señal de fallo; falta borrado de cuenta y prueba Firestore real. |
| Privacidad y términos | P0 | En progreso |  | `docs/privacy.md`; Configuración y Ayuda | El copy técnico refleja el comportamiento actual; faltan política y términos aprobados/publicables. |
| Landing/copy público | P1 | Listo local |  | `npm --prefix apps/web run build` | Sin lenguaje de beta; el copy de IA refleja el opt-out real. Onboarding, ayuda y documentos legales continúan aparte. |
| PITR Firestore | P0 | Habilitado |  | Firestore `(default)`: PITR y delete protection habilitados (2026-07-15) | Falta prueba de restauración controlada. |
| Uptime y alertas | P0 | Pendiente |  |  |  |
| Logs API | P0 | En progreso |  | Evento HTTP correlacionado en Cloud Logging de `ghmi-api-00033-xut` | API ya validó campos estructurados y redacción en candidata; faltan Bedrock y errores reales de proveedores. |
| CI verde | P0 | Configurado local |  | `.github/workflows/ci.yml`; `scripts/check-all.sh` — 161 Rust, 10 worker, build web y scan de secretos | Corre en PR y `main`, reutiliza el gate y el lockfile. Falta primera ejecución remota y protección de rama. |
| Panel admin | P1 | Pendiente |  |  |  |
| Hardening contenedores | P1 | En progreso |  | Builds Docker, salud y UID no-root de API/worker/web | Runtime separado, lockfile y usuarios no-root; faltan CSP, escaneo y límites operativos. |
| Retención automática | P1 | Pendiente |  |  |  |
| OpenTelemetry/Grafana | P2 | Pendiente |  |  |  |
| Migración Supabase | P0 | Candidata validada |  | Dos instantáneas de 6.284 registros y `ghmi-api-00036-yad` sin tráfico | Falta sesión real, Gmail y cutover controlado; Firestore sigue activo. |
| Multiproveedor | P3 | Planificado |  | `docs/plan-conexion-multiproveedor.md` |  |

---

# 23. Estado observado al crear este plan

Este bloque es una fotografía inicial y debe actualizarse a medida que cambie el sistema.

## Ya existente o favorable

- [x] API, web y worker desplegados en Cloud Run.
- [x] Worker IA privado mediante IAM.
- [x] Gmail usa scope readonly.
- [x] Tokens Gmail cifrados antes de persistirse.
- [x] OAuth incluye protecciones de state/PKCE.
- [x] Aislamiento de runs y threads probado.
- [x] 147 tests Rust pasan.
- [x] 10 tests Python pasan.
- [x] Frontend compila para producción.
- [x] API compila en release.
- [x] `npm audit --omit=dev` sin vulnerabilidades reportadas al 2026-06-25.
- [x] Cloud Scheduler existe y está habilitado.
- [x] Worker no es público.

## Pendiente o riesgoso

- [ ] `APP_ENV=production` no está configurado en la API desplegada.
- [x] Billing enforcement está configurado explícitamente en la revisión auditada.
- [ ] Mercado Pago no está configurado en la revisión auditada.
- [x] API limitada a una instancia (`ghmi-api-00030-vxc`).
- [x] Firestore PITR habilitado; falta prueba de restauración controlada.
- [x] Firestore delete protection habilitado.
- [ ] No se observaron alertas operativas configuradas.
- [ ] Migrar y rotar `WORKOS_API_KEY` y `WORKOS_COOKIE_SECRET` desde variables
  de texto plano a Secret Manager.
- [x] El check local ya no falla por Clippy; falta su primera corrida remota y
  protección de rama.
- [x] Checkout versionado no utiliza redirección.
- [x] Desconexión Gmail y borrado de análisis disponibles con confirmación; falta borrado de cuenta.
- [ ] Retención declarada no tiene job destructivo.
- [x] Sesiones expiran a los 30 días y se revocan server-side.
- [x] Logout revoca la sesión además de borrar la cookie.
- [x] La persona usuaria puede cerrar todas sus sesiones web sin desconectar Gmail.
- [x] Errores inesperados y de proveedores se sanitizan antes de llegar al navegador.
- [x] Copy de IA consistente con el comportamiento real; política y términos
  continúan pendientes de revisión legal.
- [x] CI visible en el repositorio; falta primera ejecución remota y protección de rama.
- [ ] No hay tests frontend/E2E.
- [x] El servidor web emite headers básicos, políticas de caché y un CSP mínimo no disruptivo; una política restrictiva de scripts, conexiones e iframes queda pendiente de inventario y pruebas de integraciones.

---

# 24. Criterio de “production-ready” para esta etapa

Mira puede considerarse production-ready para una **versión inicial pagada y controlada** cuando:

- [ ] Puede cobrar sin duplicar, perder o inventar estados.
- [ ] Puede bloquear acceso cuando corresponde.
- [ ] Puede recuperar el estado desde Mercado Pago.
- [x] Puede expirar y revocar sesiones.
- [x] Puede desconectar Gmail.
- [x] Puede borrar análisis derivados de forma verificable.
- [ ] Explica honestamente cómo usa IA y datos.
- [x] No filtra errores inesperados ni de proveedores al usuario.
- [ ] Tiene respaldo y recuperación probados.
- [ ] Alguien recibe alertas cuando falla.
- [ ] Existe un procedimiento para operar incidentes.
- [ ] El release puede validarse y revertirse.
- [ ] El producto sigue siendo suficientemente simple para operarlo con pocos usuarios.

No requiere todavía:

- Alta disponibilidad multi-región.
- Kubernetes.
- Microservicios adicionales.
- Loki/Prometheus/Grafana autohospedados.
- SOC 2 o ISO 27001.
- Panel admin sofisticado.
- Multiproveedor.
- Escalado horizontal.

La meta de esta etapa no es construir una plataforma perfecta. Es lograr que los primeros usuarios puedan pagar, conectar su correo, obtener valor y confiar en que los fallos serán detectados, explicados y recuperables.
