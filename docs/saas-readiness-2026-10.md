# Preparación SaaS — octubre de 2026

Este documento describe los cambios implementados y las comprobaciones disponibles. El merge y despliegue del 6 de octubre se registran en [la bitácora operativa](production-readiness-log.md). No constituye una aprobación de los proveedores ni una certificación de seguridad.

El producto admite tenants independientes con una cuenta y una casilla por organización. Los planes y la interfaz reflejan esa capacidad. Permite solicitudes externas, internas o ambas; las cuentas que responden y los criterios se configuran por tenant. No se anuncian invitaciones de equipos, múltiples casillas ni un SLA de resolución.

## Cambios aplicados

| Hallazgo | Corrección y comportamiento resultante |
|---|---|
| Creación legacy podía omitir políticas, límites y consentimiento | Toda creación usa la política actual de la organización; no acepta una versión antigua ni filtros que eludan sus restricciones. |
| Roles declarativos y organizaciones deshabilitadas | Las mutaciones verifican rol y membresía activa. Configuración, conexiones, facturación y borrado requieren Owner/Admin; análisis y revisión permiten Analyst; Viewer conserva lectura. |
| Cuotas susceptibles a carreras | PostgreSQL reserva cupos por organización con transacciones y locks. Cada run conserva su período original y libera lo reservado que no utilizó al terminar o fallar normalmente. |
| Promesa de tres usuarios/casillas sin implementación | Catálogo y migración `0004` ajustados a una cuenta y una casilla. La capacidad adicional requiere una implementación futura explícita. |
| Cobros anuales, cancelaciones y renovaciones incoherentes | Montos según intervalo y acceso según períodos adquiridos/trial real; cancelación conserva acceso hasta su fin. Notificaciones consultan el estado actual, identificadores firmados y cobros aprobados. Checkout serializado e idempotente, incluso después de una operación interrumpida. |
| Supuestos exclusivos de una mesa de servicio | Scope de solicitudes, equipo explícito, criterios y zona horaria propios. Para solicitudes externas, una lista vacía de equipo mantiene la compatibilidad histórica por dominio corporativo; dominios públicos no representan equipos. |
| Correos entre colegas contaban como respuesta | Una respuesta debe ser humana y dirigirse al solicitante por To/CC. Destinatarios desconocidos requieren revisión. Los hitos elegidos manualmente deben pertenecer al hilo, estar ordenados y ubicar la solicitud dentro del período. |
| IA incierta modificaba métricas | Consentimiento explícito; propuestas con baja confianza, señales de revisión o hitos inconsistentes quedan separadas del resultado confirmado. Sin IA, los candidatos semánticos requieren revisión. |
| “Resolución” equivalía a último correo | La interfaz describe el último envío al solicitante. No afirma que un ticket esté cerrado ni mide un SLA laboral. |
| Reportes sumaban ejecuciones solapadas | Endpoint consolidado filtra por fechas locales y deduplica por casilla, proveedor, conversación y solicitud foco. Una corrección más reciente puede provenir de un run anterior. |
| Configuración sobrescribía opciones no visibles | Guardado parcial preserva límites, rangos horarios y destinatarios; contenidos de reporte y días/horas se editan explícitamente. |
| Gmail/Microsoft usaban ventanas diferentes | Conversión de fechas locales a epoch/UTC; pruebas de límites de día y cambios de horario. Gmail utiliza preferentemente la fecha del proveedor. |
| Microsoft compartido no completado | Target firmado, permiso `Mail.Read.Shared`, probe previo del buzón y Graph `/users/{correo}`; cookie PKCE ligada a sesión, tenant, proveedor y expiración. |
| Otros proveedores sin soporte | IMAP TLS 993 con certificado validado, DNS público y conexión a IP validada; contraseña cifrada; `EXAMINE`/`BODY.PEEK`; Inbox y Enviados; threading por Message-ID. |
| Paginación y errores ocultaban falta de cobertura | Graph recorre páginas y subcarpetas. Límites, conversaciones truncadas y fallos de hilos aparecen como cobertura incompleta; las métricas no se presentan como exhaustivas. JSON de Gmail/Graph limitado a 16 MiB antes de parsear; lectura MIME acotada. |
| Una conexión antigua podía sobrescribir una nueva | Actualizaciones de refresh y metadata verifican la conexión actual. Autorización revocada solicita reconexión. Cambiar casilla reinicia sus filtros específicos y bloquea runs pendientes de la anterior. |
| Borrado podía resucitar registros | Escrituras dependen de un parent existente y no fallido, con lock compartido en PostgreSQL. El borrado bloquea el parent y elimina sus datos derivados; los workers posteriores no pueden recrearlo. |
| Retención era sólo metadata y no aplicaba el límite del plan | Cada run nuevo usa el menor plazo entre política y plan, con snapshot, hash y expiración coherentes. Al vencer se bloquea lectura; mantenimiento elimina runs y sus hilos, mensajes, auditorías y overrides. Los existentes conservan su expiración; los históricos sin fecha usan su snapshot o 90 días. |
| Scheduler y reportes frágiles | Días y minutos arbitrarios, ventanas por tenant y claim inicial atómico; dos tenants se procesan concurrentemente. Reintentos de análisis espaciados, aviso de fallo configurable y reintento de envío sin repetir análisis. |
| URL malformada terminaba el servidor web | Devuelve 400 y continúa atendiendo. Streams y proxy tienen manejo de errores y timeout. |
| Acumulación de cuerpos y páginas podía agotar memoria | Se preparan extractos de IA y se descartan cuerpos completos antes de encolar. Presupuesto de datos retenidos de 32 MiB por run, 16 MiB agregados por conversación Graph y una recuperación a la vez por run. Exceder capacidad falla explícitamente y pide reducir la ventana. |
| Reintento de análisis fallido intentaba reutilizar un run inmutable | La interfaz crea un run nuevo con la misma ventana y la política vigente; no copia el tenant, la casilla ni filtros antiguos. |
| Presentación pública contenía placeholders e información desactualizada | Textos y unidades de precios/checkout corregidos; renovación automática explícita, demos identificadas y navegación móvil, grilla de planes y scroll entre páginas reparados. Los documentos legales conservan su estado de borrador. |
| Dependencias observadas por auditoría | Actualizados `anyhow` y `event-listener`; eliminado JWT sin uso. Excepción RSA limitada a MySQL opcional de SQLx, ausente del árbol compilado y protegida por un guard en CI. |
| Respaldos sin prueba continua de recuperación | Drill en CI usa el script real y compara datos, estructura, RLS y permisos en una base nueva. Dumps con permisos privados, publicación atómica y rotación posterior a la verificación. |

Las métricas observan únicamente mensajes presentes en la casilla conectada. «Sin respuesta registrada» no prueba ausencia de atención por otro canal. En Microsoft 365 compartido, el administrador debe habilitar copias de mensajes enviados como el buzón o en su nombre; si quedan exclusivamente en Enviados del delegado, el target no las observa. El [runbook](runbook-azure-ad-microsoft.md) explica ese requisito.

Los cuerpos completos se usan transitoriamente para análisis; se guardan metadata, snippets y extractos acotados. IMAP no ofrece necesariamente credenciales restringidas a lectura: la garantía corresponde a las operaciones de este cliente. El código desactiva logs de protocolo IMAP incluso si `RUST_LOG=trace`.

## Validación disponible

La ejecución local del 4 de octubre de 2026 aprobó 265 tests de Rust, 6
integraciones con PostgreSQL real, 17 tests del worker y 6 regresiones web.
Formato, Clippy con `-D warnings`, builds release/TypeScript/Vite y el drill
de respaldo/restauración pasaron. Los formularios de conexión se revisaron
en navegador local. No se utilizaron casillas reales ni se desplegó esta versión.

El cierre local del 6 de octubre aprobó 267 tests de Rust, 17 del worker y 7
regresiones web, además de Clippy y builds. CI remoto aprobó también las seis
integraciones PostgreSQL de esa versión y desplegó API, worker y web. Se restauró
un respaldo real del entorno desplegado en PostgreSQL 17 aislado antes de aplicar
la migración. El dominio público pasó las comprobaciones de disponibilidad,
proveedores habilitados y rechazo de lectura sin sesión. El worker respondió
correctamente con Bedrock usando un hilo sintético; eso no valida casillas reales.
El ajuste final de los topes de retención aprobó 269 tests de Rust, 17 del worker
y 7 regresiones web, además de formato, Clippy y builds. Sus regresiones cubren
creación manual/programada y conservación de políticas e históricos.

- `scripts/check-all.sh`: detección de secretos versionados, formato Rust, tests, Clippy sin warnings, build release, tests del worker, audit npm, build TypeScript/Vite y regresiones web.
- `scripts/check-postgres.sh`: PostgreSQL 16 descartable; aplica las migraciones y prueba aislamiento, permisos, cuotas concurrentes, checkout locks, primer claim del scheduler, borrado durante escritura, retención y reconexiones. Incluye respaldo y restauración completos de `mira` y `billing` en una base nueva, con comparación de datos y estructura y rechazo de roles públicos.
- Tests de proveedores: paginación Graph, carpetas anidadas, origen de `nextLink`, fechas y direcciones Gmail, SSRF IMAP y un servidor IMAP local para verificar operaciones de lectura y threading.
- Pruebas locales de navegador de los formularios Microsoft compartido e IMAP, con aviso de reconexión y campos de credenciales ocultos.

Los fixtures de distintos perfiles demuestran reglas y coherencia de métricas. La calidad semántica para un nuevo cliente necesita una muestra etiquetada por ese cliente: revisar solicitudes válidas, solicitudes fuera de alcance, intercambios internos, automatizaciones y casos sin respuesta antes de confiar en agregados.

Las auditorías npm y Python no encontraron vulnerabilidades conocidas. `cargo audit` pasa con la excepción RSA documentada, pero conserva avisos de mantenimiento: `fxhash` sin mantenimiento y versiones retiradas de `chacha20` y `spin`. No equivalen a vulnerabilidades demostradas en este producto; corresponde seguir sus actualizaciones.

## Activación del cambio

1. Respaldar la base y probar restauración en una base separada. Revisar qué runs históricos vencerían antes de desplegar: el mantenimiento inicia aproximadamente un minuto después del arranque y luego corre cada hora.
2. Aplicar `apps/api/migrations/0004_supported_account_limits.sql` mediante `scripts/apply-supabase-migrations.sh` con el proceso habitual. Las pruebas de integración aplican esto sólo en una base descartable.
3. Desplegar API, worker y web como una versión coherente. Reiniciar obliga a volver a iniciar conexiones OAuth que estaban a mitad del flujo, por el nuevo formato de cookie; las conexiones ya guardadas se conservan.
4. Configurar un disparo horario de `POST /internal/maintenance` con `x-cron-secret` cuando el hosting suspenda CPU. Sin `CRON_SECRET`, las rutas internas devuelven 404. Para horarios arbitrarios, el disparo externo de `/internal/scheduled-analysis` debe ser frecuente y no limitarse a cuatro horas de Santiago.
5. Comprobar `/health`, `/health/ready`, `/mailbox/providers`, inicio de sesión, una ejecución por tenant, revisión, reporte consolidado y reconexión. Revisar conteos del mantenimiento y errores sanitizados.
6. Antes de revertir una versión, considerar que el borrado por retención es definitivo en la base activa; volver al código anterior no restaura datos. Un respaldo tiene su propia política de conservación.

Las reservas sin confirmación después de una caída abrupta del proceso pueden contabilizar consumo conservador; requieren conciliación operativa si corresponde. Cuatro análisis ejecutan simultáneamente por instancia, cada uno con una recuperación de conversación a la vez y hasta 32 MiB de datos preparados. La espera máxima para un slot es de cinco minutos; el rate limiter sigue siendo local. Estos límites reducen acumulación de memoria, pero no reemplazan medir carga con casillas reales. Mantener una instancia para la primera etapa o implementar un limiter distribuido y pruebas de carga antes de escalar réplicas. El scheduler no hace backfill automático de varios días en que el servicio estuvo detenido; el endpoint permite backfill explícito.

## Pendientes que requieren servicios o cuentas externas

| Validación | Criterio de cierre |
|---|---|
| Google público | `gmail.readonly` es restringido: para el SaaS público que procesa correo en servidores, completar verificación del scope y evaluación de seguridad vigente con renovación al menos anual, salvo excepción documentada aplicable. La revisión no observó el estado de aprobación/CASA ni la audiencia en consola. [Requisitos oficiales](https://developers.google.com/identity/protocols/oauth2/production-readiness/restricted-scope-verification), [comprobaciones](google-oauth.md). |
| Microsoft | Probar Outlook/Hotmail, Microsoft 365 de otro directorio y un buzón compartido con permisos delegados reales; verificar audiencia multitenant, publisher y consentimiento del administrador. [Runbook](runbook-azure-ad-microsoft.md). |
| IMAP | Probar un servidor externo con TLS real, carpeta Enviados, límites y revocación de contraseña. Las pruebas locales no verifican compatibilidad con cada hosting. |
| Mercado Pago | Ejecutar sandbox con compra mensual/anual, primer cobro, renovación fallida, recuperación, cancelación y replay de webhooks. Los tests actuales simulan respuestas HTTP del proveedor. |
| Operación | El respaldo real se restauró en una base separada y el mantenimiento quedó habilitado y comprobado. Falta establecer respaldo recurrente fuera del servidor con objetivos de recuperación, comprobar recepción de alertas y consumo, y revisar rotación de credenciales según el entorno. No se modificaron secretos durante este trabajo. |
| Calidad por tenant | Comparar una muestra etiquetada con los resultados; ajustar criterios y umbrales. No prometer precisión universal a partir de la usuaria inicial ni de datos sintéticos. |

El cierre completo de cuenta sigue usando el procedimiento operativo publicado; no se añade un borrado autoservicio que pueda dejar una suscripción cobrando o perder registros financieros. Múltiples miembros/casillas y cierre de tickets no forman parte de la capacidad anunciada de esta versión.

La revisión del 6 de octubre añadió smoke de la landing, precios mensual/anual,
demos, navegación legal y recuperación de error al cargar planes, en escritorio
y pantallas de 320/390 px. Sigue pendiente completar los datos del operador,
contacto, regiones de procesamiento y documentos legales, además de confirmar
documentación tributaria. El precio mostrado corresponde al importe final en CLP;
no se afirma un tratamiento de IVA no verificado. El preflight con respaldo real,
restauración y migración se registra en [la bitácora operativa](production-readiness-log.md).

Resend conserva las claves de idempotencia durante 24 horas; los reintentos automáticos de entrega se limitan a 23 horas desde el inicio de la ventana. Fuera de ese plazo se revisa el incidente para evitar duplicados. [Documentación de Resend](https://resend.com/docs/dashboard/emails/idempotency-keys).
