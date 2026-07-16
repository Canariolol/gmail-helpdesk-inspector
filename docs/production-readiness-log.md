# Bitácora de preparación para producción

Este registro acompaña a `plan-production-ready.md`: solo se anota trabajo terminado con evidencia. Los pagos permanecen diferidos hasta resolver su problema actual.

## 2026-07-16 — Callback de AuthKit de la aplicación Mira

**Estado:** terminado y verificado contra WorkOS; pendiente integración en una
revisión candidata de Cloud Run.

**Qué se hizo:**

- Se validó sin exponerla la nueva `WORKOS_API_KEY` guardada localmente.
- La lista de redirects de AuthKit estaba vacía; se registró
  `https://mira.ninfasolutions.com/auth/workos/callback` y se confirmó la
  respuesta de creación HTTP 201.

**Motivo:** WorkOS sólo permite terminar el login en URLs previamente
autorizadas. El callback bajo el dominio público conserva la sesión en la
arquitectura same-origin de Mira.

**Evidencia:**

- `GET /user_management/redirect_uris` autenticado con la nueva clave: lista
  inicial vacía.
- `POST /user_management/redirect_uris`: HTTP 201 para el callback de Mira.

**Qué sigue:** obtener el `WORKOS_CLIENT_ID` de la nueva aplicación, migrar la
API key a Secret Manager y preparar una revisión sin tráfico. La rotación de
`WORKOS_COOKIE_SECRET` se mantiene separada porque cerrará las sesiones
existentes. Google OAuth y los pagos no se modificaron.

## 2026-07-16 — Nueva API key de WorkOS resguardada

**Estado:** preparado y verificado en GCP; pendiente asociarla a una revisión
candidata.

**Qué se hizo:**

- Se creó `workos-api-key` en Secret Manager con la nueva clave como su versión
  inicial, sin imprimir ni registrar el valor.
- Se concedió `roles/secretmanager.secretAccessor` únicamente a
  `ghmi-runtime`, la cuenta de ejecución de `ghmi-api`.

**Motivo:** sustituir la clave literal actual por una clave nueva almacenada y
entregada por el mecanismo de secretos de GCP, sin afectar todavía el login
que sirve tráfico.

**Evidencia:**

- Secret Manager confirma `workos-api-key` y su versión inicial.
- La política del secreto confirma acceso para `ghmi-runtime`.
- La revisión activa todavía marca `WORKOS_API_KEY` y
  `WORKOS_COOKIE_SECRET` como literales; `WORKOS_WEBHOOK_SECRET` ya es un
  secreto. No se modificó la revisión ni se envió tráfico nuevo.

**Qué sigue:** copiar el `WORKOS_CLIENT_ID` de la nueva aplicación para
configurar la revisión candidata y enlazarle `workos-api-key`. La rotación de
la cookie requiere una autorización separada porque invalida sesiones. Google
OAuth y pagos permanecen sin cambios.

## 2026-07-15 — IDs de correlación HTTP

**Estado:** terminado y verificado localmente.

**Qué se hizo:**

- La API genera un UUID nuevo por request, ignora cualquier `x-request-id` entrante, lo incorpora al span de logs y lo devuelve en todas las respuestas.
- El cliente web añade ese ID a los errores visibles como código de soporte.

**Motivo:** permitir correlacionar un reporte de usuario con los logs sin confiar en valores controlados por el cliente ni exponer detalles internos.

**Evidencia:**

- `cargo fmt --manifest-path apps/api/Cargo.toml -- --check`
- `cargo test --manifest-path apps/api/Cargo.toml responses_get_a_server_generated_request_id` — 1 prueba aprobada.
- `npm --prefix apps/web run build` — aprobado.

**Qué sigue:** revisar el contrato público de errores y la redacción de logs fuera de los flujos de pago; después, avanzar con los controles operativos que no requieren modificar infraestructura productiva.

## 2026-07-15 — CI reproducible

**Estado:** configurado localmente; pendiente su primera ejecución remota al abrir un PR.

**Qué se hizo:**

- Se añadió `.github/workflows/ci.yml`, ejecutado en pull requests y en `main`.
- El workflow fija Rust 1.96.0, instala las dependencias del worker y la web, usa cachés y ejecuta el único gate existente: `scripts/check-all.sh`.

**Motivo:** evitar que la validación local y la del repositorio diverjan, sin crear una segunda lista de comandos que haya que mantener.

**Evidencia:**

- El YAML fue validado localmente.
- `scripts/check-all.sh` — 137 pruebas Rust, 7 pruebas Python, Clippy, formato y build web aprobados.

**Qué sigue:** abrir un PR para comprobar la primera corrida y configurar protección de rama para exigir el check. Luego corregir la contradicción de copy sobre IA detectada entre el comportamiento real y la documentación pública.

## 2026-07-15 — Copy de IA alineado con el producto

**Estado:** terminado y verificado localmente.

**Qué se hizo:**

- README, documentación técnica, landing y borrador de privacidad ya describen la IA como activa por defecto y desactivable desde Configuración.
- Se conservó la explicación existente de que reactivarla exige confirmación y afecta análisis futuros.

**Motivo:** la política real crea organizaciones con IA activa; el copy previo decía que era opt-in u opcional, lo que habría dado información incorrecta a usuarios.

**Evidencia:**

- Defaults y migración de opt-out cubiertos por pruebas API existentes.
- `npm --prefix apps/web run build` — aprobado.

**Qué sigue:** completar los textos legales con responsable, contacto y revisión legal; no se publican como política ni términos finales hasta contar con esa información y aprobación.

## 2026-07-15 — Headers y caché del servidor web

**Estado:** terminado y verificado localmente.

**Qué se hizo:**

- El servidor web emite HSTS, `nosniff`, política de referrer, permisos mínimos y protección contra framing.
- HTML queda en `no-cache`, assets con hash en caché larga `immutable` y respuestas proxied de API en `no-store`.

**Motivo:** reducir exposición del navegador y evitar que datos autenticados se almacenen en cachés intermedias, manteniendo los assets estáticos rápidos.

**Evidencia:**

- `npm --prefix apps/web run build` — aprobado.
- `curl -I` contra el servidor web local confirmó headers y ambas políticas de caché.

**Qué sigue:** inventariar orígenes y probar CSP con WorkOS, Google OAuth y el checkout embebido antes de activarla. Los pagos continúan deliberadamente fuera de este bloque.

## 2026-07-15 — Inspección P0 de infraestructura GCP

**Estado:** observado; sin cambios de infraestructura.

**Qué se hizo:**

- Se revisó en solo lectura Cloud Run y Firestore del proyecto activo.
- `ghmi-api`, `ghmi-web` y `ghmi-ai-worker` siguen con máximo de 20 instancias.
- Firestore `(default)` tiene PITR y delete protection desactivados.

**Motivo:** confirmar el estado real antes de modificar capacidad o recuperación, que no se puede inferir desde el repositorio.

**Qué sigue:** con aprobación explícita, reducir `ghmi-api` a una instancia para la versión inicial y habilitar PITR/delete protection en Firestore. Después se debe probar una restauración, que no queda cubierta solo por activar los controles.

## 2026-07-15 — Logs JSON de la API

**Estado:** terminado y verificado localmente.

**Qué se hizo:**

- La capa `tracing` de la API ahora emite JSON e incluye el span activo y su lista de spans.
- Los logs producidos dentro de una request conservan el `request_id`, método y ruta generados por el middleware existente.

**Motivo:** Cloud Logging puede indexar campos JSON sin introducir otra plataforma de observabilidad; el ID de soporte ya entregado al navegador ahora permite encontrar el evento correspondiente.

**Evidencia:**

- `cargo fmt --manifest-path apps/api/Cargo.toml -- --check` y el test de `request_id` aprobaron.
- Inicio de API local y un `GET /auth/me` sin sesión: evento JSON `WARN` con `method`, `path` y `request_id`.

**Qué sigue:** añadir los campos comunes que faltan y estructurar el worker. La revisión de redacción de logs sigue abierta: los errores de proveedores no deben terminar como texto sensible en Cloud Logging.

## 2026-07-15 — Redacción de errores y resultados operativos

**Estado:** implementado y verificado localmente; pendiente validación en Cloud Logging.

**Qué se hizo:**

- Los fallos de análisis y scheduler se persisten como categorías seguras, sin
  guardar el mensaje original del proveedor.
- Los helpers de Google OAuth, WorkOS, Resend y Firestore dejan de incorporar
  cuerpos de respuesta externos en sus errores. El scheduler informa totales
  de resultados, no objetos que contienen correos de usuarios.
- Se eliminó el registro de errores completos en los caminos revisados de API
  y scheduler.

**Motivo:** evitar que tokens, cuerpos de proveedores o datos personales
lleguen a Cloud Logging o a los estados visibles de operaciones.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml failed_runs_persist_a_safe_error_code` — aprobado.
- `cargo test --manifest-path apps/api/Cargo.toml other_failures_keep_status_without_provider_body` — aprobado.
- `cargo clippy --manifest-path apps/api/Cargo.toml --all-targets -- -D warnings` — aprobado.

**Qué sigue:** desplegar en un entorno no productivo y comprobar los eventos
reales en Cloud Logging. Después completar Bedrock y el worker. El logging de
Mercado Pago no se modifica hasta resolver el problema actual de pagos.

## 2026-07-15 — Worker: observabilidad y errores de Bedrock

**Estado:** terminado y verificado localmente; pendiente despliegue y revisión
en Cloud Logging.

**Qué se hizo:**

- El worker emite JSON para su inicio y cada request, con servicio, entorno,
  operación, estado, duración y un UUID de correlación en `x-request-id`.
- Un error de Bedrock se registra con `bedrock_failed` y, para auditorías
  individuales, su `run_id`; no se registra la excepción ni el payload.
- Las respuestas 502 y los errores de validación son genéricos, por lo que no
  devuelven cuerpos de Bedrock ni datos del correo enviado al worker.

**Motivo:** el worker recibe excerpts y metadatos de correo; tanto sus logs
como sus respuestas deben conservar señales operativas sin reflejar esos datos.

**Evidencia:**

- `python3 -m pytest apps/ai-worker/tests/test_bedrock.py -q` — 9 pruebas aprobadas.
- Ejecución local de Uvicorn y `GET /health`: header `x-request-id` y evento
  JSON con `service`, `environment`, `operation`, `status` y `duration_ms`.

**Qué sigue:** verificar la salida de la revisión desplegada en Cloud Logging.
Luego completar campos comunes de la API y decidir las alertas y la retención.

## 2026-07-15 — Campos comunes en logs de API

**Estado:** terminado y verificado localmente; pendiente comprobación en la
revisión desplegada.

**Qué se hizo:**

- La configuración de API conserva `APP_ENV` y lo incorpora al span de cada
  request junto con `service=api`, método, ruta y `request_id`.
- Al terminar la request, la API emite un evento JSON con operación, estado y
  `duration_ms`. Los eventos de análisis mantienen además el `run_id` cuando
  corresponde.

**Motivo:** un formato común permite filtrar y correlacionar API y worker en
Cloud Logging sin añadir otra plataforma de observabilidad.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml responses_get_a_server_generated_request_id` — aprobado.
- `cargo clippy --manifest-path apps/api/Cargo.toml --all-targets -- -D warnings` — aprobado.
- `GET /health` local con `APP_ENV=local`: evento JSON con `service`,
  `environment`, `request_id`, `operation`, `status` y `duration_ms`.

**Qué sigue:** desplegar y revisar ambos servicios en Cloud Logging; después
definir retención, alertas y uptime checks. Los errores y campos de pagos se
mantienen diferidos hasta resolver el incidente de Mercado Pago.

## 2026-07-15 — Gate integral tras hardening de observabilidad

**Estado:** terminado y verificado localmente.

**Qué se hizo:**

- Se ejecutó el gate único del repositorio después de los cambios de API,
  scheduler y worker.
- Una prueba de scheduler que esperaba el mensaje interno de refresh token se
  actualizó para comprobar la categoría segura persistida.

**Motivo:** mantener la prueba alineada con la regla de no guardar causas
internas y comprobar que los cambios no introdujeron regresiones entre servicios.

**Evidencia:**

- `./scripts/check-all.sh` — formato Rust, 138 pruebas Rust, Clippy, 9 pruebas
  del worker y build web aprobados.

**Qué sigue:** revisar logs del navegador y luego validar los cambios desplegados
en Cloud Logging. La configuración GCP y los pagos no se han modificado.

## 2026-07-15 — Logs del frontend y proxy web

**Estado:** terminado y verificado localmente.

**Qué se hizo:**

- Se revisó `apps/web/src`: no contiene llamadas `console.*`.
- El proxy conserva sus dos eventos operativos necesarios (inicio y fallo de
  conexión) como JSON estructurado; el fallo ya no incluye el objeto de error
  de `fetch`.

**Motivo:** impedir que detalles de red terminen en logs y conservar eventos
operativos mínimos para diagnosticar disponibilidad del proxy.

**Evidencia:**

- Búsqueda estática de `console.log`, `console.error` y variantes en frontend.
- Proxy local contra una API inaccesible: respuesta pública 502, headers de
  seguridad y evento JSON `api_proxy_failed` sin detalle de excepción.
- `npm --prefix apps/web run build` — aprobado (permanece advertencia conocida
  de chunk JavaScript mayor a 500 kB).

**Qué sigue:** probar los flujos Network de autenticación, Gmail, análisis,
worker y Firestore en un entorno desplegado. Pagos queda fuera de esa prueba
hasta resolver el incidente actual.

## 2026-07-15 — Source maps de la web

**Estado:** terminado y verificado localmente.

**Qué se hizo:**

- Se inspeccionó la configuración y el artefacto de build de Vite.
- No se generan archivos `.map` ni referencias `sourceMappingURL`; se decidió
  no publicarlos para la versión inicial.

**Motivo:** no existe aún una herramienta de seguimiento de errores que se
beneficie de ellos y no se deben exponer por defecto.

**Evidencia:**

- Búsqueda de configuración de source maps y de archivos/referencias `.map` en
  `apps/web/dist` después del build, sin resultados.

**Qué sigue:** si se incorpora una herramienta de errores, generar los source
maps sólo para subirlos a ella y evitar servirlos públicamente.

## 2026-07-15 — Imágenes de contenedor sin privilegios

**Estado:** terminado y verificado localmente.

**Qué se hizo:**

- API, worker y web ahora ejecutan con usuarios no root en sus imágenes finales.
- La API compila con `Cargo.lock` y `--locked`; el build web usa `npm ci`.
- El worker instala sólo dependencias de runtime, no el extra de test.

**Motivo:** reducir el impacto de una vulnerabilidad en tiempo de ejecución y
hacer reproducibles los artefactos sin enviar herramientas de test a producción.

**Evidencia:**

- Builds Docker locales exitosos de API, worker y web.
- Inspección de usuarios finales: UID `999` (API), `10001` (worker) y `1000`
  (web), todos distintos de root.
- `/health` de API y worker, y `/` de web respondieron desde los contenedores.
- `import pytest` falla dentro de la imagen final del worker, como corresponde.

**Qué sigue:** fijar dependencias Python con lockfile y decidir límites de
CPU/memoria en Cloud Run. Esos límites requieren revisión de coste y una
aplicación explícita en infraestructura.

## 2026-07-15 — Auditoría y build de producción locales

**Estado:** terminado y verificado localmente.

**Qué se hizo:**

- Se auditó el árbol de dependencias de runtime de la web.
- Se compiló la API en perfil release respetando el lockfile.

**Motivo:** detectar vulnerabilidades conocidas antes del deploy y comprobar
que el binario de producción se construye con las versiones fijadas.

**Evidencia:**

- `npm --prefix apps/web audit --omit=dev` — 0 vulnerabilidades.
- `cargo build --manifest-path apps/api/Cargo.toml --release --locked` — aprobado.

**Qué sigue:** fijar dependencias Python mediante lockfile y decidir cómo se
auditarán Rust y Python en CI, sin introducir dependencias de producción.

## 2026-07-15 — Lockfile reproducible del worker

**Estado:** terminado y verificado localmente.

**Qué se hizo:**

- Se añadió `apps/ai-worker/uv.lock` con las 32 dependencias resueltas.
- El Dockerfile usa `uv sync --frozen --no-dev`, por lo que no vuelve a resolver
  versiones durante el build ni instala extras de test.

**Motivo:** los rangos de `pyproject.toml` dejaban que cada imagen resolviera
versiones distintas; el lock hace repetible el entorno de runtime.

**Evidencia:**

- `uv lock --check --directory apps/ai-worker` — aprobado.
- Build y `/health` de la imagen worker aprobados después de consumir el lock.
- `pytest` continúa ausente de la imagen final.

**Qué sigue:** incorporar una auditoría de dependencias Python/Rust en CI. No
se añade aún otro servicio ni dependencia de runtime para hacerlo.

## 2026-07-15 — CI alineada con el lockfile Python

**Estado:** configurado localmente; pendiente primera ejecución remota en PR.

**Qué se hizo:**

- `scripts/check-all.sh` ejecuta las pruebas del worker mediante
  `uv run --frozen --all-extras`.
- El workflow instala la versión fijada de `uv`, restaura su caché desde
  `uv.lock` y deja de resolver el extra de test con `pip` en cada ejecución.

**Motivo:** el gate local y CI deben probar exactamente el conjunto de
dependencias bloqueado, no versiones nuevas que aparezcan entre ejecuciones.

**Evidencia:**

- YAML de CI validado localmente.
- `./scripts/check-all.sh` — 138 pruebas Rust, 9 pruebas worker con lockfile,
  Clippy y build web aprobados.

**Qué sigue:** abrir un PR y confirmar la primera corrida remota. Después
añadir auditorías de dependencias Rust/Python al mismo workflow.

## 2026-07-15 — Contrato público único de errores

**Estado:** terminado y verificado localmente.

**Qué se hizo:**

- Toda respuesta `ApiError` usa `error.code`, `error.message` y
  `error.request_id`; los detalles seguros de configuración quedan bajo
  `error.details`.
- El rechazo CSRF usa el mismo serializador y el cliente web entiende el nuevo
  formato, conservando compatibilidad con una respuesta heredada de texto.
- El stream de eventos de análisis dejó de reflejar el texto de errores de
  storage.

**Motivo:** dar a usuarios y soporte un contrato estable y correlacionable,
sin exponer errores técnicos o cuerpos de proveedores.

**Evidencia:**

- Nueva prueba verifica que un 404 contiene código y el mismo UUID del header.
- `cargo test --manifest-path apps/api/Cargo.toml` — 139 pruebas aprobadas.
- Clippy y build web aprobados.

**Qué sigue:** validar el contrato en una revisión desplegada y definir sólo
los códigos de proveedor que hagan falta al resolver pagos. Mercado Pago sigue
fuera de alcance en esta etapa.

## 2026-07-15 — Pasada de copy público de lanzamiento

**Estado:** terminado y verificado localmente.

**Qué se hizo:**

- Se revisaron landing, precios, ayuda, onboarding y páginas legales públicas
  contra nombres históricos, “beta” y promesas de Microsoft, IMAP o múltiples
  proveedores.
- El copy público usa “Mira Helpdesk” o “Mira” y no ofrece esas funciones fuera
  de alcance.

**Motivo:** el lanzamiento debe describir el producto que existe y no reducir
confianza con lenguaje de prueba o promesas no entregadas.

**Evidencia:**

- Búsqueda estática sobre `apps/web/src` y documentos públicos sin resultados
  de beta, proveedores futuros ni nombres históricos.
- Build web aprobado.

**Qué sigue:** completar identidad, contacto y revisión legal antes de publicar
la política y los términos definitivos. Los borradores actuales no se declaran
documentos legales finales.

## 2026-07-15 — Validación de producción y callbacks same-origin

**Estado:** corrección local terminada; cambio de entorno pendiente de
autorización y de resolver pagos.

**Qué se hizo:**

- Se verificó en solo lectura que la API desplegada usa callbacks bajo el origen
  web y el proxy web hacia API, para entregar la cookie en el mismo origen de
  navegación.
- La validación de `APP_ENV=production` ahora acepta callbacks HTTPS tanto del
  origen API como del origen web, sin aceptar terceros.

**Motivo:** la regla anterior habría rechazado la arquitectura same-origin que
el despliegue ya utiliza, bloqueando activar el modo producción.

**Evidencia:**

- Lectura selectiva de Cloud Run: callbacks web y proxy API configurados; no se
  leyeron secretos.
- Prueba `production_accepts_web_origin_oauth_callbacks_for_the_same_origin_proxy` — aprobada.

**Qué sigue:** con autorización, configurar `APP_ENV=production` sólo junto
con la revisión completa de secretos y pagos; luego ejecutar smoke test OAuth.

## 2026-07-15 — Atributos de cookies y OAuth temporal

**Estado:** terminado y verificado localmente; smoke test OAuth desplegado
pendiente.

**Qué se hizo:**

- Se comprobó y cubrió con test que las cookies de sesión y OAuth incluyen
  `Path=/`, `HttpOnly`, `SameSite`, `Secure` en HTTPS y ningún `Domain`.
- La cookie de OAuth usa 600 segundos; la sesión sigue con expiración absoluta
  server-side de 30 días.
- Se confirmó que las callbacks pasan por el origen web/proxy, evitando una
  cookie third-party en los flujos normales.

**Motivo:** reducir exposición de tokens a JavaScript y limitar la ventana de
estado OAuth sin alterar la arquitectura de proxy actual.

**Evidencia:**

- Test `session_and_oauth_cookies_keep_the_required_security_attributes` — aprobado.
- Configuración desplegada y código del proxy revisados en solo lectura.

**Qué sigue:** probar login y conexión Gmail en una revisión desplegada antes
de cerrar `SameSite` y evaluar el prefijo `__Host-` con la URL final.

## 2026-07-15 — Runbooks operativos sin pagos

**Estado:** terminado en documentación; pendiente simulacro y responsables de
on-call.

**Qué se hizo:**

- Se creó `docs/operational-runbooks.md` para API, worker/Bedrock, Gmail,
  scheduler/Resend, análisis atascados, Firestore, borrado de análisis e
  incidentes de seguridad.
- Los comandos de diagnóstico usan los servicios Cloud Run y el formato de
  logs reales del proyecto; no incluyen secretos ni payloads sensibles.

**Motivo:** permitir una primera respuesta repetible sin improvisar cambios
de infraestructura ni pedir tokens a usuarios.

**Evidencia:**

- Comando local `gcloud run services logs read --help` validó las opciones de
  diagnóstico documentadas.
- Revisión cruzada contra `docs/gcp-deploy.md` y el estado actual de servicios.

**Qué sigue:** definir responsables, canal y tiempos de respuesta; ejecutar un
simulacro cuando exista una revisión no productiva. Rollback, restauración y
pagos continúan abiertos porque requieren decisiones y recursos externos.

## 2026-07-15 — Gate local final de esta tanda

**Estado:** terminado localmente; sin despliegue, commit ni cambios en GCP.

**Qué se hizo:**

- Se ejecutó el gate unificado con el lockfile del worker, formato, Clippy y
  build web.
- Se sincronizó el resumen ejecutivo del plan con los cambios locales de
  errores públicos, correlación, logs, CI, contenedores y runbooks.

**Motivo:** dejar una evidencia única y actualizada antes de pasar a cambios
que afectan la infraestructura productiva.

**Evidencia:**

- `./scripts/check-all.sh` — 141 pruebas Rust, 9 pruebas del worker, formato,
  Clippy y build web aprobados.
- `git diff --check` — aprobado.

**Qué sigue:** abrir un PR para comprobar CI remoto y, con autorización
explícita, decidir los cambios GCP pendientes: `APP_ENV=production` junto a la
revisión de secretos/pagos, límite de instancias, PITR/delete protection y
alertas. Los pagos siguen expresamente diferidos.

## 2026-07-15 — Evaluación de borrado de cuenta

**Estado:** pausado de forma intencional; no se implementó una eliminación
destructiva incompleta.

**Qué se hizo:** se contrastó el flujo ya disponible de borrar análisis con el
alcance de borrado de cuenta. El segundo exige determinar solicitante/owner,
reautenticación, tratamiento WorkOS, retención financiera/legal y el bloqueo de
cobros futuros.

**Motivo:** pagos están diferidos y esas decisiones no están definidas. Borrar
datos de cuenta antes de resolverlas puede dejar una suscripción activa o
incumplir retención necesaria.

**Qué sigue:** acordar la política de borrado y resolver el incidente de pagos;
recién entonces implementar endpoint, confirmación, reautenticación, efectos
en WorkOS y pruebas de idempotencia.

## 2026-07-15 — Contrato de errores y aislamiento de recursos

**Estado:** terminado y verificado localmente.

**Qué se hizo:** se añadió una prueba de contrato para `401` sin sesión, `403`
por origen no confiable y `404` de un análisis ajeno. El último caso compara el
mensaje con un ID realmente inexistente para impedir enumerar recursos de otra
persona.

**Motivo:** cerrar la parte P0 de errores públicos que podía revelar si un ID
existía fuera de la organización solicitante.

**Evidencia:**

- `protected_errors_keep_the_public_contract_and_hide_foreign_runs` — aprobada.
- `./scripts/check-all.sh` — 142 pruebas Rust, 9 del worker, formato, Clippy y
  build web aprobados.

**Qué sigue:** propagar `request_id` al worker y verificar el contrato en una
revisión desplegada; pagos no se tocaron.

## 2026-07-15 — Correlación API → worker

**Estado:** terminado y verificado localmente.

**Qué se hizo:** las llamadas API a `/audit/thread` y `/audit/batch` reenvían
el `x-request-id` generado por servidor. El worker conserva únicamente un UUID
válido y regenera cualquier valor inválido antes de registrarlo o devolverlo.

**Motivo:** unir los logs de una solicitud con sus auditorías IA sin permitir
que un cliente inyecte identificadores arbitrarios.

**Evidencia:**

- Prueba API de disponibilidad del UUID en el contexto de request y prueba del
  worker de validación/regeneración — aprobadas.
- `./scripts/check-all.sh` — 143 pruebas Rust, 10 del worker, formato, Clippy
  y build web aprobados.

**Qué sigue:** desplegar una revisión y comprobar en Cloud Logging un mismo
`request_id` a ambos lados. Pagos siguen diferidos.

## 2026-07-15 — Ayuda sobre auditoría IA

**Estado:** terminado localmente.

**Qué se hizo:** la ayuda explica que la auditoría IA viene activa, dónde se
puede desactivar y que los casos inciertos pasan a revisión manual en los
análisis futuros.

**Motivo:** alinear la información de ayuda con Configuración, la landing y el
comportamiento real del opt-out.

**Evidencia:** `npm --prefix apps/web run build` — aprobado.

**Qué sigue:** onboarding, política de privacidad y términos requieren revisión
de contenido/legal antes de declararlos publicados.

## 2026-07-15 — Rollback de Cloud Run documentado

**Estado:** terminado en documentación; no se cambió tráfico.

**Qué se hizo:** se añadió al runbook operativo el procedimiento para elegir
una revisión sana y mover 100% del tráfico de API, web o worker, con una
verificación específica por servicio.

**Motivo:** el gate P0 exige un rollback probado o documentado; ejecutar un
rollback sólo para probarlo cambiaría producción sin necesidad.

**Evidencia:** comandos contrastados con el flujo ya existente en
`docs/deployment-beta-runbook.md`; `gcloud run services update-traffic --help`
confirma la operación documentada.

**Qué sigue:** asignar responsable operativo y ejercitarlo en una revisión no
productiva. Restauración de Firestore sigue pendiente de PITR y una prueba real.

## 2026-07-15 — Onboarding de auditoría IA alineado

**Estado:** terminado localmente.

**Qué se hizo:** el paso «Auditoría con IA» del asistente de Configuración
explica que viene activo, permite el opt-out y aclara que reactivarlo confirma
el uso para análisis futuros.

**Motivo:** ese asistente es el onboarding real de la organización; así no
queda una versión del producto que describa la IA como opt-in.

**Evidencia:** `npm --prefix apps/web run build` — aprobado.

**Qué sigue:** política de privacidad y términos siguen pendientes de revisión
legal y datos de responsable/contacto antes de publicarse.

## 2026-07-15 — Mensajes de acciones de datos corregidos

**Estado:** terminado localmente.

**Qué se hizo:** el resumen de privacidad ya no describe como pendiente la
desconexión de Gmail, que existe en Cuenta. También se eliminaron referencias
técnicas al endpoint de configuración y al contrato backend en la interfaz.

**Motivo:** evitar que la aplicación contradiga funciones ya disponibles o
muestre términos internos a la persona usuaria.

**Qué sigue:** el borrado de cuenta sigue deshabilitado hasta definir su
política, retención y la interacción con pagos; no se habilitó ese flujo.

## 2026-07-15 — Billing enforcement verificado en GCP

**Estado:** confirmado en solo lectura; no se modificó infraestructura.

**Qué se hizo:** se comprobó que la revisión actual de `ghmi-api` tiene
`BILLING_ENFORCEMENT_ENABLED=true`, junto con Firestore y cookies seguras.

**Motivo:** el gate maestro no debe mantener como pendiente un control ya
activo, aunque `APP_ENV=production` y pagos aún no estén listos.

**Evidencia:** consulta selectiva de Cloud Run sin leer secretos. También se
confirmó `maxScale=20` y que Firestore continúa sin PITR/delete protection.

**Qué sigue:** con autorización explícita, reducir instancias y habilitar los
controles de recuperación; no se realizaron esos cambios.

## 2026-07-15 — Lockfile web vestigial eliminado

**Estado:** terminado localmente.

**Qué se hizo:** se eliminó `apps/web/apps/web/package-lock.json`, un lockfile
vacío sin `package.json` asociado. El lockfile efectivo permanece en
`apps/web/package-lock.json`.

**Motivo:** evitar que herramientas o mantenedores infieran un segundo proyecto
web o resuelvan dependencias contra un archivo vacío.

**Qué sigue:** mantener `npm ci` contra el lockfile real en CI; no se añadieron
dependencias.

## 2026-07-15 — Imágenes versionadas por commit

**Estado:** procedimiento de despliegue actualizado; no se desplegó una imagen.

**Qué se hizo:** `docs/gcp-deploy.md` etiqueta API, web y worker con el SHA
corto del commit y documenta cómo asociar la imagen versionada a su revisión.

**Motivo:** una imagen mutable `latest` no permite saber qué código se está
revirtiendo ni reproducir una revisión anterior.

**Evidencia:** no quedan referencias a `:latest` para imágenes en el runbook;
la lista de revisiones muestra la imagen etiquetada que recibió cada una.

**Qué sigue:** en el primer despliegue con este procedimiento, comprobar que la
revisión y la imagen apuntan al mismo SHA antes de recibir tráfico.

## 2026-07-15 — Pruebas de sesión verificadas

**Estado:** terminado y verificado localmente.

**Qué se hizo:** se ejecutaron las pruebas de expiración server-side y de
logout revocable; ambas rechazan la sesión después del evento correspondiente.

**Motivo:** el checklist de tests críticos estaba desactualizado respecto de la
cobertura ya implementada.

**Evidencia:**

- `logout_revokes_the_server_side_session` — aprobada.
- `expired_sessions_are_rejected_server_side` — aprobada.

**Qué sigue:** falta smoke test de login/logout en una revisión desplegada;
pagos no se tocaron.

## 2026-07-15 — Auditoría del borrado de análisis

**Estado:** terminado y verificado localmente.

**Qué se hizo:** cada borrado confirmado crea o actualiza
`auditLogs/{request_id}` con hash del propietario, estado (`requested`,
`completed` o `failed`) y marcas de tiempo. No guarda correo ni contenido de
mensajes. El estado sólo pasa a `completed` tras borrar los datos derivados.

**Motivo:** dejar evidencia operativa de una solicitud de eliminación sin
retener PII adicional ni afirmar una operación que falló a mitad de camino.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml delete_analysis_data_requires_confirmation_and_only_deletes_the_owner_data` — aprobada; prueba confirmación, aislamiento de otra cuenta y la auditoría correlacionada por `x-request-id`.
- `./scripts/check-all.sh` — 143 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.

**Qué sigue:** al desplegar una revisión autorizada, probar el borrado contra
Firestore real. El borrado de cuenta y los pagos siguen fuera de este avance.

## 2026-07-15 — Minimización de cuerpos antes de persistir

**Estado:** terminado y verificado localmente.

**Qué se hizo:** se centralizó la preparación de mensajes para almacenamiento.
El helper elimina `body_text` y se usa tanto al completar un análisis como al
guardar un override manual; conserva sólo los metadatos ya definidos para el
producto.

**Motivo:** convertir una regla de privacidad que estaba repetida en dos
llamadas en un único límite verificable contra regresiones.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml persistence_discards_full_email_bodies` — aprobada.
- `./scripts/check-all.sh` — 144 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.

**Qué sigue:** revisar con una ejecución desplegada que Firestore tampoco
contiene cuerpos completos. No se publicaron documentos legales ni se tocaron
los pagos.

## 2026-07-15 — Límite de datos para auditoría IA detallada

**Estado:** terminado y verificado localmente.

**Qué se hizo:** se añadió una prueba que verifica que, antes de llamar al
worker de auditoría detallada, el backend limita el número de mensajes y recorta
cada cuerpo al máximo configurado por la política.

**Motivo:** el worker necesita contexto para clasificar, no el historial
completo de un hilo; el límite evita que una futura modificación vuelva a
enviar más contenido del necesario.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml detailed_ai_audit_limits_messages_and_body_length` — aprobada.
- `./scripts/check-all.sh` — 145 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.

**Qué sigue:** comprobar en una ejecución desplegada que los límites
configurados coinciden con los valores enviados. Los documentos legales y los
pagos continúan pendientes.

## 2026-07-15 — Retención automática no prometida

**Estado:** terminado localmente.

**Qué se hizo:** la pantalla de Privacidad ahora llama «plazo configurado» al
valor de retención y aclara que no activa borrado automático. Ofrece sólo el
borrado manual de análisis que realmente existe.

**Motivo:** una cifra de días sin esa aclaración podía hacer creer que ya existe
un job de retención, cuando sigue pendiente de implementación y validación.

**Evidencia:** `npm --prefix apps/web run build` — aprobado.

**Qué sigue:** decidir entidades, responsables y política antes de crear un job
de retención. Los pagos siguen diferidos.

## 2026-07-15 — Headers de Gmail minimizados

**Estado:** terminado y verificado localmente.

**Qué se hizo:** la normalización de Gmail descarta todos los headers antes de
persistir salvo `Auto-Submitted`, que se necesita para reconocer correo
automático.

**Motivo:** headers arbitrarios pueden contener identificadores o contexto que
el producto no necesita conservar; la clasificación sigue teniendo su única
señal requerida.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml normalize_message_persists_only_the_automation_header` — aprobada.
- `./scripts/check-all.sh` — 146 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.

**Qué sigue:** verificar la estructura de un documento nuevo en Firestore al
desplegar una revisión autorizada. Pagos siguen fuera de este avance.

## 2026-07-15 — Adjuntos excluidos de la auditoría

**Estado:** terminado y verificado localmente.

**Qué se hizo:** el extractor de Gmail ahora descarta cualquier parte con
`filename` o `attachmentId`, aunque use MIME `text/*`. Sólo el cuerpo textual
del mensaje sigue disponible de forma transitoria para clasificarlo.

**Motivo:** el filtro anterior ignoraba adjuntos binarios, pero podía aceptar un
adjunto de texto inline. El control nuevo cubre ambos marcadores de Gmail.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml extract_text_excludes_parts_marked_as_attachments` — aprobada.
- `./scripts/check-all.sh` — 147 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.

**Qué sigue:** validar contra un correo de prueba en una revisión desplegada
autorizada. Pagos siguen diferidos.

## 2026-07-15 — Límites de IA coherentes con Configuración

**Estado:** terminado y verificado localmente.

**Qué se hizo:** Configuración conserva los límites de mensajes y caracteres de
la política existente al guardar, y los muestra en el texto de consentimiento.

**Motivo:** el formulario antes anunciaba y enviaba siempre `14/280`, por lo
que una política personalizada podía ser sobrescrita y el copy dejar de ser
cierto.

**Evidencia:** `npm --prefix apps/web run build` — aprobado.

`./scripts/check-all.sh` — 147 pruebas Rust, 10 del worker, formato, Clippy y
build web aprobados.

**Qué sigue:** validar el texto contra una política personalizada en una
revisión desplegada autorizada. Pagos siguen diferidos.

## 2026-07-15 — Reportes iniciales minimizados

**Estado:** terminado y verificado localmente.

**Qué se hizo:** se confirmó que una organización nueva usa reportes sólo de
métricas. Si se habilitan ítems de revisión, asunto y remitente permanecen
ocultos a menos que se activen explícitamente.

**Motivo:** reducir datos personales en correo saliente desde el comportamiento
por defecto, sin impedir una decisión explícita de la organización.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml scheduled_policy_report_` — 2 pruebas aprobadas.

**Qué sigue:** validar el correo generado en una revisión desplegada antes de
enviar reportes reales. Pagos siguen diferidos.

## 2026-07-15 — Transparencia de proveedor y uso de IA

**Estado:** terminado localmente.

**Qué se hizo:** Configuración ahora identifica Amazon Bedrock y explica
finalidad, límites, posibilidad de texto sensible, cómo desactivar IA y cuándo
aplica el cambio. El documento técnico de privacidad dejó de presentarse como
un MVP local o una política publicada.

**Motivo:** el usuario debe saber qué proveedor procesa los fragmentos antes
de decidir, sin confundir el borrador técnico con una política legal final.

**Evidencia:** `npm --prefix apps/web run build` — aprobado.

**Qué sigue:** completar responsable, contacto, retención del proveedor,
región y revisión legal antes de publicar una política. Pagos siguen diferidos.

## 2026-07-15 — Consentimiento IA ajustado al dato real

**Estado:** terminado localmente.

**Qué se hizo:** el consentimiento enumera participantes, fecha, asunto y
texto limitado que recibe la auditoría. También aclara que un mensaje breve
puede caber completo en el límite, y que adjuntos, imágenes y headers completos
no se procesan.

**Motivo:** el copy anterior prometía que nunca se enviaba un cuerpo completo,
algo falso para mensajes menores al máximo configurado.

**Evidencia:** `npm --prefix apps/web run build` — aprobado.

**Qué sigue:** revisión legal del consentimiento y validación desplegada con
una política personalizada. Pagos siguen diferidos.

## 2026-07-15 — Señal segura de fallo al borrar datos

**Estado:** terminado y verificado localmente.

**Qué se hizo:** cuando un borrado de análisis falla, la API emite
`data_deletion_failed` con hash del propietario. El `request_id` ya forma parte
del contexto de la solicitud; no se registra el error crudo ni el correo.

**Motivo:** permitir detectar y correlacionar una eliminación incompleta sin
agregar PII a los logs.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml delete_analysis_data_requires_confirmation_and_only_deletes_the_owner_data` — aprobada.
- `./scripts/check-all.sh` — 147 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.

**Qué sigue:** crear una alerta de Cloud Monitoring sobre esa señal al contar
con autorización de infraestructura. Pagos siguen diferidos.

## 2026-07-15 — Tablero ejecutivo sincronizado

**Estado:** terminado en documentación.

**Qué se hizo:** el gate local, la tabla de progreso y el estado de riesgos del
plan ahora reflejan 147 pruebas Rust y el copy IA realmente vigente. Las
entradas anteriores conservan su evidencia histórica.

**Motivo:** el resumen ejecutivo debe mostrar el estado actual y no inducir a
priorizar fallos ya corregidos.

**Evidencia:** `./scripts/check-all.sh` — 147 pruebas Rust, 10 del worker,
formato, Clippy y build web aprobados.

**Qué sigue:** los bloqueos que quedan son pagos, producción/GCP, respaldo,
alertas, documentos legales y pruebas desplegadas; no se modificó ninguno en
esta sincronización.

## 2026-07-15 — Estado GCP revalidado en solo lectura

**Estado:** confirmado; sin cambios de infraestructura.

**Qué se hizo:** se reconsultaron Cloud Run y Firestore. `ghmi-api-00028-7lj`
sigue con escala máxima 20 y sin `APP_ENV`; Firestore continúa sin PITR ni
delete protection. Se confirmó que Firestore, cookies seguras, `SameSite=None`
y billing enforcement siguen configurados.

**Motivo:** separar el estado desplegado actual de las mejoras locales antes de
proponer operaciones que cambien disponibilidad, recuperación o coste.

**Evidencia:** consultas `gcloud run services describe` y
`gcloud firestore databases describe`, filtradas para no leer secretos.

**Qué sigue:** con autorización explícita, desplegar una revisión con
`APP_ENV=production`, reducir escala y habilitar recuperación. Pagos continúan
diferidos.

## 2026-07-15 — Cierre global de sesiones

**Estado:** terminado y verificado localmente.

**Qué se hizo:** se agregó `POST /auth/logout-all` y el control confirmado
«Cerrar todas las sesiones» en Cuenta. La operación revoca server-side todas
las sesiones web de la misma cuenta, borra la cookie actual y recarga al
acceso. No borra tokens de Gmail ni detiene el scheduler.

**Motivo:** permitir a la persona usuaria responder a una sesión sospechosa
sin requerir soporte y sin interrumpir los análisis programados autorizados.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml logout_all_revokes_every_session_for_the_owner_only` — aprobada.
- `cargo test --manifest-path apps/api/Cargo.toml revoke_user_sessions_only_revokes_the_owner_sessions` — aprobada; comprueba que la credencial Gmail para scheduler se conserva.
- `npm --prefix apps/web run build` — aprobado.
- `./scripts/check-all.sh` — 149 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.

**Qué sigue:** documentar la reacción a cambio de contraseña o suspensión en
WorkOS y ejecutar el smoke test de sesiones una vez desplegada la revisión.
Pagos siguen diferidos.

## 2026-07-15 — Cookie same-site endurecida

**Estado:** terminado localmente; pendiente de aplicar y probar en la revisión desplegada.

**Qué se hizo:** el valor predeterminado de `APP_COOKIE_SAMESITE` pasó a `Lax`
para sesiones y estado OAuth. La guía de despliegue ahora reserva `None` para
una API llamada directamente desde otro sitio y exige revisar CSRF en ese caso.

**Motivo:** la web ya proxy las rutas autenticadas bajo su propio origen; no
requiere cookies cross-site. `Lax` conserva los callbacks OAuth, que regresan
como navegación GET de nivel superior.

**Evidencia:** pruebas de configuración de producción y de atributos de cookies
actualizadas para exigir `SameSite=Lax` con `Secure` y `HttpOnly`.

- `./scripts/check-all.sh` — 149 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.

**Qué sigue:** con autorización de infraestructura, cambiar
`APP_COOKIE_SAMESITE` de `None` a `Lax` en Cloud Run y probar login WorkOS y
conexión Gmail. Pagos siguen diferidos.

## 2026-07-15 — Fallo de IA detallada sanitizado

**Estado:** terminado y verificado localmente.

**Qué se hizo:** cuando la auditoría IA detallada falla, el hilo queda para
revisión manual con una razón fija. El error original ya no se persiste en el
hilo; los logs emiten solamente `ai_detailed_audit_failed`.

**Motivo:** una respuesta del proveedor podría contener detalles internos o
texto recibido; no debe terminar en datos persistidos ni en la interfaz.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml detailed_ai_failure_does_not_persist_provider_detail` — aprobada.
- `./scripts/check-all.sh` — 150 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.

**Qué sigue:** validar la señal y el flujo de revisión manual en Cloud Logging
y una revisión desplegada. La revisión de Mercado Pago sigue diferida.

## 2026-07-15 — Error de callback WorkOS sanitizado

**Estado:** terminado y verificado localmente.

**Qué se hizo:** los rechazos enviados por WorkOS ya no devuelven
`error_description` al navegador. La API responde un mensaje fijo y registra
la categoría segura `workos_login_rejected`.

**Motivo:** el texto proporcionado por el identity provider no es un contrato
público y puede contener detalles que no deben exponerse.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml workos_callback_hides_provider_error_description` — aprobada.
- `./scripts/check-all.sh` — 151 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.

**Qué sigue:** validar el login rechazado en la revisión desplegada y continuar
la revisión no relacionada con pagos de errores de proveedores.

## 2026-07-15 — Ciclo de vida WorkOS evaluado

**Estado:** decisión documentada; reemplazada por la implementación local del
webhook firmado registrada más abajo.

**Qué se hizo:** se documentó que Mira mantiene una sesión local independiente
de AuthKit y hoy no conserva `sid` ni procesa eventos WorkOS. Un cambio de
contraseña, suspensión o revocación en WorkOS no invalida automáticamente una
sesión local existente. Se definió el mínimo futuro: webhook firmado de
`user.deleted`, `user.updated`, `organization_membership.updated` y
`session.revoked`, además de persistir `sid` para este último.

**Motivo:** evitar que «cerrar todas las sesiones» se interprete como cierre de
la sesión hospedada por WorkOS o como una integración de deprovisioning que aún
no existe.

**Evidencia:** [sesiones de AuthKit](https://workos.com/docs/authkit/sessions),
[eventos WorkOS](https://workos.com/docs/events) y
[provisionamiento de directorio](https://workos.com/docs/authkit/directory-provisioning).

**Qué sigue:** configurar el endpoint y secreto del webhook, desplegar y enviar
eventos de prueba antes de abrir el producto a usuarios externos. Pagos siguen
diferidos.

## 2026-07-15 — Credencial Gmail separada de sesiones web

**Estado:** terminado y verificado localmente; pendiente de comprobar la
migración en Firestore desplegado.

**Qué se hizo:** se creó `GmailConnection` fuera de `users/{session_id}`. La
conexión conserva tokens cifrados, cuenta Gmail, fechas y revocación; las rutas
de cuenta, análisis, desconexión y el scheduler la usan directamente. Las
credenciales de sesiones antiguas se migran en la primera lectura, se copian a
`gmailConnections/{hash_del_propietario}` y luego se eliminan de las sesiones.

**Motivo:** cerrar una sesión web no debe dejar al scheduler sin acceso, ni el
scheduler debe rotar tokens dentro de una sesión que puede expirar o revocarse.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml storage::tests::migrates_legacy_gmail_credentials_without_keeping_them_in_sessions` — aprobada.
- `cargo test --manifest-path apps/api/Cargo.toml storage::tests::revoke_user_sessions_only_revokes_the_owner_sessions` — aprobada.
- `cargo test --manifest-path apps/api/Cargo.toml scheduler::tests::failure_notice_uses_config_recipients_over_fallback` — aprobada.
- `cargo test --manifest-path apps/api/Cargo.toml` — 151 pruebas aprobadas.
- `cargo clippy --manifest-path apps/api/Cargo.toml -- -D warnings` — aprobado.

**Qué sigue:** validar una conexión existente tras desplegar, comprobar que el
documento `gmailConnections` aparece y que logout, scheduler y desconexión se
comportan como corresponde. El barrido legacy Firestore ya pagina; si escanear
la colección completa deja de ser aceptable, debe reemplazarse por una consulta
indexada por propietario. Pagos siguen diferidos.

## 2026-07-15 — Ciclo de vida WorkOS con webhook firmado

**Estado:** terminado y verificado localmente; pendiente de secreto, registro
del endpoint y prueba de evento en el entorno desplegado.

**Qué se hizo:** se persiste el `sid` de AuthKit únicamente para relacionar una
nueva sesión local con WorkOS. Se agregó `POST /auth/workos/webhook`, que valida
el cuerpo crudo mediante `workos-signature` (`t`, `v1`, HMAC-SHA256) y tolera un
desfase máximo de cinco minutos. `session.revoked` invalida solo la sesión local
correspondiente; `user.deleted` revoca las sesiones locales del propietario,
desconecta sus credenciales Gmail locales y desactiva mailbox/scheduler. No se
almacenan access/refresh tokens de WorkOS ni se llama a Google para revocar el
grant remoto.

**Motivo:** una revocación o deprovisión en el identity provider debe cortar el
acceso local y evitar análisis futuros, sin mezclar la duración de la sesión web
con la credencial Gmail del scheduler.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml workos_` — 6 pruebas aprobadas.
- `cargo test --manifest-path apps/api/Cargo.toml invalid_workos_signature_cannot_revoke_a_local_session` — aprobada.
- `./scripts/check-all.sh` — 156 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.
- `git diff --check` — aprobado.

**Qué sigue:** cargar `WORKOS_WEBHOOK_SECRET` en Secret Manager, registrar
`https://<ghmi-api-run-app-url>/auth/workos/webhook` con solo `user.deleted` y
`session.revoked`, desplegar y enviar ambos eventos de prueba desde WorkOS.
`user.updated` y eventos de membresía permanecen pendientes de una política de
estado explícita. Pagos siguen diferidos.

## 2026-07-15 — Cobertura CSRF de mutaciones no financieras

**Estado:** terminado y verificado localmente; smoke test desplegado pendiente.

**Qué se hizo:** se amplió la prueba del middleware global de `Origin` para
confirmar que un origen ajeno recibe 403 antes de llegar a logout individual o
global, desconexión Gmail, borrado de datos, configuración, presets, creación e
inicio de análisis y revisión manual.

**Motivo:** una única prueba de logout no demostraba que las demás mutaciones
del navegador siguieran protegidas al agregar rutas nuevas. Los webhooks no
dependen de cookies de usuario y tienen validación de firma propia.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml mutations_reject_an_untrusted_origin` — aprobada para 12 rutas.
- `./scripts/check-all.sh` — 156 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.
- `git diff --check` — aprobado.

**Qué sigue:** aplicar `APP_COOKIE_SAMESITE=Lax` en la revisión desplegada y
probar OAuth y las mutaciones same-origin. Si se mantiene `SameSite=None` para
una arquitectura distinta, reevaluar una defensa CSRF adicional. Pagos siguen
diferidos.

## 2026-07-15 — Runbook de revocación WorkOS

**Estado:** terminado localmente; requiere usarlo en el primer evento desplegado.

**Qué se hizo:** se añadió a `docs/operational-runbooks.md` el procedimiento
para investigar una revocación de sesión o baja de usuario WorkOS. Indica qué
metadatos mínimos revisar, cómo confirmar endpoint/configuración sin revelar el
secreto y qué efectos locales verificar después de reenviar un evento.

**Motivo:** el webhook puede cortar sesiones y Gmail de forma correcta, pero
una incidencia operativa no debe resolverse editando tokens o sesiones en
Firestore ni copiando payloads de identidad a logs o tickets.

**Evidencia:** el runbook corresponde exactamente a los dos eventos y efectos
que cubren las pruebas `workos_session_revoked_only_invalidates_the_matching_local_session`
y `workos_user_deleted_revokes_access_gmail_and_scheduler`.

**Qué sigue:** configurar el endpoint y secreto WorkOS, desplegar y ejecutar
ambos eventos de prueba usando el procedimiento. Pagos siguen diferidos.

## 2026-07-15 — Lookup de cuenta por email en Firestore

**Estado:** terminado y verificado localmente; pendiente de observar la
migración perezosa en Firestore desplegado.

**Qué se hizo:** `get_account_by_email` ahora consulta
`accountEmailIndexes/{hash_del_email}` y después lee la cuenta por su ID WorkOS.
Cada upsert mantiene ese lookup. Si una cuenta antigua aún no tiene índice, se
usa el escaneo legado una sola vez y se escribe el índice; un índice obsoleto
por cambio de correo se limpia al detectarlo.

**Motivo:** aprovisionar la configuración de una persona no debe listar toda la
colección `accounts` a medida que crece la base. El ID del documento usa el hash
normalizado del correo, sin exponerlo como ruta.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml account_email_index_normalizes_and_does_not_expose_the_email` — aprobada.
- `./scripts/check-all.sh` — 157 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.
- `git diff --check` — aprobado.

**Qué sigue:** comprobar en Firestore que el primer login de una cuenta antigua
crea su lookup y medir sus lecturas. Las búsquedas completas de sesiones y los
lookups de pagos permanecen pendientes; pagos siguen diferidos.

## 2026-07-15 — Auditoría de concurrencia Firestore

**Estado:** inventario terminado; las correcciones de concurrencia siguen
pendientes y no se declaran cubiertas.

**Qué se hizo:** se creó `docs/firestore-concurrency-audit.md` con los flujos
read-modify-write no financieros. Identifica el claim del scheduler como riesgo
P0: dos instancias o reintentos podrían reclamar la misma ventana. También deja
explícitos los conflictos posibles entre configuración, baja WorkOS, estado de
análisis y refresh/desconexión Gmail.

**Motivo:** el límite actual de una instancia mitiga el síntoma, pero no prueba
que las escrituras sean seguras si Cloud Run o un reintento ejecutan en paralelo.

**Evidencia:** recorridos de `scheduler::claim_schedule_window`,
`get_or_provision_org_config`, `update_org_config`, `update_analysis_run` y
las escrituras de Gmail en `StorageRepository`/`FirestoreStorage`.

**Qué sigue:** implementar claim condicional del scheduler y después probar dos
disparos concurrentes; conservar pagos fuera de esa modificación hasta resolver
su incidente.

## 2026-07-15 — Claim condicional del scheduler

**Estado:** terminado y verificado localmente; falta prueba contra Firestore
desplegado con dos instancias.

**Qué se hizo:** se incorporó `claim_schedule_window` al repositorio de storage.
En memoria toma un lock de escritura; en Firestore lee `updateTime` y hace un
`PATCH` con `currentDocument.exists=false` para un documento nuevo o
`currentDocument.updateTime` para uno existente. Si otro escritor gana, relee y
decide de nuevo; después de tres conflictos falla el tick sin iniciar un análisis
duplicado.

**Motivo:** `get` seguido de `upsert` permitía que dos instancias o reintentos
simultáneos marcaran la misma ventana como `Running` y enviaran dos análisis o
reportes.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml concurrent_claims_only_allow_one_scheduled_run` — aprobada.
- `cargo test --manifest-path apps/api/Cargo.toml recognizes_firestore_precondition_conflicts_without_exposing_error_bodies` — aprobada.
- `./scripts/check-all.sh` — 159 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.
- `git diff --check` — aprobado.
- La implementación usa las precondiciones documentadas de Firestore; no se
  efectuó ninguna escritura ni prueba en el proyecto desplegado.

**Qué sigue:** desplegar una revisión autorizada y provocar dos invocaciones
simultáneas del scheduler contra Firestore real, verificando un solo análisis y
el resultado del claim perdedor. Configuración de organización, estado de
análisis y refresh/desconexión Gmail siguen en la auditoría de concurrencia.
Pagos continúa diferido.

## 2026-07-15 — Scheduler bloqueado tras revocación Gmail

**Estado:** terminado y verificado localmente; falta observar la carrera en el
entorno desplegado.

**Qué se hizo:** `sync_schedule_config_from_policy` consulta la conexión Gmail
actual antes de escribir `scheduleConfigs`. Aunque una copia antigua de la
política indique `scheduler_enabled=true`, el documento operativo queda
desactivado si la conexión fue revocada o borrada.

**Motivo:** una actualización iniciada antes de una desconexión Gmail o de
`user.deleted` podía terminar después y volver a activar el scheduler, aun sin
credencial válida.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml stale_policy_sync_cannot_reenable_scheduler_after_gmail_revocation` — aprobada.
- `./scripts/check-all.sh` — 160 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.
- `git diff --check` — aprobado.

**Qué sigue:** probar la secuencia concurrente en Cloud Run/Firestore. Las
escrituras de otros campos de configuración aún no usan control de versión;
agregarlo sólo será necesario si se permiten ediciones concurrentes reales.
Pagos continúa diferido.

## 2026-07-15 — Refresh Gmail condicional ante desconexión

**Estado:** terminado y verificado localmente; pendiente de una carrera real
contra Firestore desplegado.

**Qué se hizo:** el scheduler ya no termina un refresh con un `upsert` ciego.
`refresh_gmail_connection` compara la versión que leyó con la conexión vigente,
requiere que no esté revocada y, en Firestore, usa `currentDocument.updateTime`.
Si una desconexión, baja WorkOS o reconexión gana la carrera, no persiste el
token renovado y cancela ese tick.

**Motivo:** un refresh que recibía su respuesta después de una desconexión podía
sobrescribir los tokens borrados y dejar la conexión activa otra vez.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml refresh_does_not_restore_a_gmail_connection_after_disconnect` — aprobada.
- `cargo test --manifest-path apps/api/Cargo.toml fresh_running_claim_is_skipped_but_stale_claim_retries` — aprobada.
- `./scripts/check-all.sh` — 161 pruebas Rust, 10 del worker, formato, Clippy y build web aprobados.
- `git diff --check` — aprobado.

**Qué sigue:** desplegar una revisión autorizada y forzar refresh/desconexión
concurrentes para confirmar que la precondición Firestore conserva el documento
revocado. Pagos continúa diferido.

## 2026-07-15 — Evaluación de prefijo `__Host-` para sesión

**Estado:** decisión local documentada; no requiere cambio de código.

**Qué se hizo:** se verificó que `ghmi_session` no emite `Domain`, usa
`Path=/` y `HttpOnly`; el arranque de producción exige cookie `Secure`. Se
decidió mantener su nombre actual.

**Motivo:** `__Host-` aportaría una aserción adicional del navegador, pero
renombrar la cookie ahora invalida todas las sesiones y obliga a modificar cada
lector y prueba. La configuración actual ya satisface las restricciones que el
prefijo impondría, sin introducir una migración de sesión no necesaria.

**Evidencia:**

- `apps/api/src/auth/mod.rs` construye la cookie sin `Domain`, con `Path=/` y
  tiene una prueba de sus atributos.
- `apps/api/src/config/mod.rs` rechaza producción si `APP_COOKIE_SECURE` no es
  verdadero.

**Qué sigue:** si se planifica una invalidación global de sesiones, evaluar el
renombre a `__Host-ghmi_session` junto con pruebas de login, logout y OAuth.
Pagos continúa diferido.

## 2026-07-15 — Scan de secretos en el gate local y CI

**Estado:** terminado y verificado localmente; pendiente de su primera corrida
remota en GitHub Actions.

**Qué se hizo:** se agregó `scripts/check-secrets.sh` y se incorporó al inicio
de `scripts/check-all.sh`. Rechaza en archivos rastreados patrones de claves
privadas y formatos reconocibles de credenciales AWS, Google, Mercado Pago,
GitHub y `sk_live/prod`.

**Motivo:** el CI ya reutiliza el gate local, pero no tenía una barrera explícita
contra credenciales incorporadas accidentalmente al repositorio.

**Evidencia:**

- `./scripts/check-secrets.sh` — aprobado sobre los archivos rastreados.
- `./scripts/check-all.sh` — 161 pruebas Rust, 10 del worker, formato, Clippy,
  `npm audit --omit=dev` sin vulnerabilidades, build web y scan de secretos aprobados.
- `git diff --check` — aprobado.

**Qué sigue:** verificar la primera ejecución remota del workflow. Este control
no busca secretos por entropía ni examina archivos no rastreados; incorporar un
scanner dedicado sólo si el repositorio o el riesgo justifican esa complejidad.
Pagos continúa diferido.

## 2026-07-15 — Auditoría de dependencias web dentro del gate

**Estado:** terminado y verificado localmente; pendiente de la primera corrida
remota de CI.

**Qué se hizo:** `scripts/check-all.sh` ejecuta ahora
`npm --prefix apps/web audit --omit=dev` antes del build web.

**Motivo:** el plan ya exigía auditar dependencias de runtime, pero la
comprobación no formaba parte del gate que reutiliza CI.

**Evidencia:**

- `npm --prefix apps/web audit --omit=dev` — 0 vulnerabilidades.
- `./scripts/check-all.sh` — 161 pruebas Rust, 10 del worker, formato, Clippy,
  scan de secretos, auditoría web y build aprobados.
- `git diff --check` — aprobado.

**Qué sigue:** observar el primer workflow remoto y resolver cualquier alerta
de dependencia antes de promover una revisión. Pagos continúa diferido.

## 2026-07-15 — Estado desplegado revalidado en solo lectura

**Estado:** confirmado; no se modificó infraestructura.

**Qué se hizo:** se consultaron Cloud Run y Firestore del proyecto activo.
Las revisiones listas siguen siendo `ghmi-api-00028-7lj`,
`ghmi-web-00020-kdd` y `ghmi-ai-worker-00010-456`; los tres servicios mantienen
`maxScale=20`. Firestore `(default)` conserva PITR y delete protection
desactivados.

**Motivo:** separar las mejoras locales de los bloqueadores realmente
desplegados y confirmar que este avance no alteró tráfico, secretos, pagos ni
capacidad.

**Evidencia:** consultas `gcloud run services list` y
`gcloud firestore databases describe --database='(default)'`, ambas de solo
lectura.

**Qué sigue:** con autorización explícita, limitar temporalmente la API a una
instancia, habilitar protección/recuperación de Firestore y probar restauración.
Después, desplegar una revisión autorizada para los smoke tests y las carreras
Firestore pendientes. Pagos continúa diferido.

## 2026-07-15 — Auditoría de errores de proveedores no financieros

**Estado:** terminado localmente; pendiente de comprobación en Cloud Logging.

**Qué se hizo:** se revisaron logs `tracing`, cuerpos de respuesta y conversión
de errores de WorkOS, Google OAuth/Gmail, Resend, Firestore, metadata de GCP,
Bedrock y el worker. Los fallos no financieros usan mensajes genéricos, códigos
seguros o status; no se interpolan bodies de proveedores ni excepciones
completas en logs o respuestas públicas.

**Motivo:** el contrato público de errores no basta si una ruta interna vuelve
a enviar una respuesta externa o un token a Cloud Logging.

**Evidencia:** recorrido estático de `apps/api/src` y
`apps/ai-worker/src` sobre macros de logging, `response.text()` y
`error.to_string()`. Las pruebas existentes cubren errores Google refresh,
WorkOS inválido, API inesperada y fallos Bedrock sin exponer el detalle.

**Qué sigue:** verificar eventos reales en Cloud Logging después del despliegue.
Mercado Pago no se revisó ni modificó por el diferimiento explícito de pagos.

## 2026-07-15 — Build release dentro del gate

**Estado:** terminado y verificado localmente; CI remoto pendiente.

**Qué se hizo:** se añadió `cargo build --release` a
`scripts/check-all.sh` después de Clippy.

**Motivo:** el plan exigía validar el binario optimizado, pero el gate sólo
compilaba el perfil de test y el de desarrollo de Clippy.

**Evidencia:**

- `cargo build --release --manifest-path apps/api/Cargo.toml` — aprobado.
- `./scripts/check-all.sh` — 161 pruebas Rust, 10 del worker, formato, Clippy,
  build release, scan de secretos, auditoría web y build web aprobados.
- `git diff --check` — aprobado.

**Qué sigue:** validar la primera ejecución de CI remoto y después una revisión
Cloud Run autorizada. Pagos continúa diferido.

## 2026-07-15 — Correlación de creación de análisis

**Estado:** terminado y verificado localmente; Cloud Logging pendiente.

**Qué se hizo:** al persistir un análisis manual se emite
`analysis_run_created` con `run_id`; al crear uno programado se emite
`scheduled_analysis_run_created` con el mismo campo. El primero hereda el
`request_id` del span HTTP ya activo.

**Motivo:** el `request_id` permite seguir la petición y el `run_id` permite
seguir el trabajo asíncrono; sin un evento común, relacionarlos requería buscar
por hora o correo.

**Evidencia:**

- `./scripts/check-all.sh` — 161 pruebas Rust, 10 del worker, formato, Clippy,
  build release, scan de secretos, auditoría web y build web aprobados.
- `git diff --check` — aprobado.

**Qué sigue:** comprobar ambos campos en Cloud Logging tras desplegar. La
correlación de checkout y suscripción queda diferida con pagos; la de
organización requiere definir su hash operativo. Pagos continúa diferido.

## 2026-07-15 — Gate obligatorio antes de redeploy

**Estado:** terminado y verificado localmente; falta ejecutarlo en una
publicación autorizada.

**Qué se hizo:** `scripts/redeploy-gcp.sh` ejecuta ahora
`scripts/check-all.sh` antes de autenticar Docker, construir imágenes o llamar
a Cloud Run. Aplica igual a API, worker, web y `all`.

**Motivo:** el runbook pedía validar antes de desplegar, pero el comando que
publica podía omitirse ese paso manualmente.

**Evidencia:**

- `bash -n scripts/redeploy-gcp.sh` — aprobado.
- `./scripts/check-all.sh` — 161 pruebas Rust, 10 del worker, formato, Clippy,
  build release, scan de secretos, auditoría web y build web aprobados.
- `git diff --check` — aprobado.

**Qué sigue:** usarlo en el próximo deploy autorizado y ejecutar el smoke test
desplegado. Pagos continúa diferido.

## 2026-07-15 — Auditorías de dependencias pendientes delimitadas

**Estado:** auditoría npm cubierta; Rust y Python pendientes de una decisión de
herramienta.

**Qué se hizo:** se confirmó que `cargo-audit`, `cargo-deny` y `pip-audit` no
están instalados ni forman parte del CI. Se marcó como cubierto el `npm audit`
que ya corre dentro del gate.

**Motivo:** añadir instaladores o acciones nuevas de seguridad sin decidir
tiempo de CI, base de vulnerabilidades y responsable produce un control frágil
o que nadie mantiene.

**Evidencia:** inventario local de binarios y de configuración del repositorio;
`npm --prefix apps/web audit --omit=dev` continúa en 0 vulnerabilidades.

**Qué sigue:** elegir `cargo-audit`/`cargo-deny` y `pip-audit` con su política
de excepciones antes de incorporarlos a CI. Pagos continúa diferido.

## 2026-07-15 — CSP mínimo del proxy web

**Estado:** terminado y verificado localmente; validar integraciones tras
despliegue.

**Qué se hizo:** el proxy web añade
`Content-Security-Policy: base-uri 'self'; frame-ancestors 'none'; object-src 'none'`.

**Motivo:** bloquea vectores independientes de las integraciones externas sin
arriesgar el checkout embebido, OAuth o llamadas Gmail con una allowlist CSP
incompleta.

**Evidencia:**

- `curl -fsSI http://127.0.0.1:5174/` confirmó el CSP y los headers de
  seguridad del servidor local.
- `./scripts/check-all.sh` — 161 pruebas Rust, 10 del worker, formato, Clippy,
  build release, scan de secretos, auditoría web y build web aprobados.
- `git diff --check` — aprobado.

**Qué sigue:** inventariar orígenes de scripts, conexiones e iframes en un
entorno desplegado antes de imponer `default-src`, `script-src`, `connect-src`
o `frame-src`. Pagos continúa diferido.

## 2026-07-15 — Controles GCP aplicados y verificados

**Estado:** aplicado por la persona operadora y verificado en solo lectura.

**Qué se hizo:**

- Se creó un commit del trabajo local antes de cambiar infraestructura.
- `ghmi-api` quedó limitado a una instancia (`maxScale=1`).
- Firestore `(default)` tiene PITR y delete protection habilitados.
- El endpoint WorkOS quedó registrado y `WORKOS_WEBHOOK_SECRET` está enlazado
  desde Secret Manager en `ghmi-api`.

**Motivo:** reducir carreras conocidas de la primera versión, proteger datos
ante borrados o escrituras accidentales y permitir revocación de sesiones desde
WorkOS sin registrar secretos en el repositorio.

**Evidencia:** consulta GCP de solo lectura confirmó la revisión
`ghmi-api-00030-vxc`, `maxScale=1`,
`POINT_IN_TIME_RECOVERY_ENABLED`, `DELETE_PROTECTION_ENABLED` y la referencia
de Secret Manager para `WORKOS_WEBHOOK_SECRET`.

**Qué sigue:**

- Migrar y rotar `WORKOS_API_KEY` y `WORKOS_COOKIE_SECRET`, detectados como
  variables de texto plano; no registrar sus valores.
- Configurar el dominio y hacer un deploy candidato antes de probar el webhook
  con eventos firmados de una cuenta desechable.
- Uptime checks y alertas quedan diferidos hasta el lanzamiento real por la
  decisión de no despertar instancias sin tráfico.
- Hacer una restauración Firestore controlada; activar PITR no prueba aún la
  recuperación.

## 2026-07-15 — Zona Cloudflare iniciada para el subdominio

**Estado:** zona activa y delegada; pendiente mapping de Cloud Run.

**Qué se hizo:** se creó mediante API la zona `ninfasolutions.com` en
Cloudflare con un token temporal de alcance DNS. La llave local quedó fuera de
Git y con permisos de lectura solo para su propietario.

**Motivo:** preparar la delegación de `mira.ninfasolutions.com` hacia
`ghmi-web` sin desplegar código ni cambiar pagos, OAuth o WorkOS.

**Evidencia:** el token fue validado, la zona no existía antes de crearla y el
inventario posterior confirmó 23 registros visibles: A, CNAME, MX y TXT. Se
preservaron los ocho registros de correo relevantes. No quedaron nombres con
el sufijo duplicado.

**Qué sigue:** crear el Domain Mapping de Cloud Run para
`mira.ninfasolutions.com`; luego agregar solo los registros DNS que Google
entregue y verificar HTTPS, correo y web antes de cambiar OAuth o desplegar.

## 2026-07-16 — Mapping Cloud Run para Mira creado

**Estado:** CNAME público creado; certificado HTTPS pendiente.

**Qué se hizo:** se creó el Cloud Run Domain Mapping
`mira.ninfasolutions.com -> ghmi-web` en `us-central1`. Se agregó únicamente
el CNAME indicado por Google (`mira -> ghs.googlehosted.com`) como `DNS only`.

**Motivo:** permitir que Google valide el host y emita su certificado antes de
exponer la aplicación con su subdominio, sin modificar OAuth, WorkOS, pagos ni
las imágenes desplegadas.

**Evidencia:** el CNAME resuelve desde DNS público; Cloud Run informa
`Ready=Unknown` con espera de provisioning del certificado.

**Qué sigue:** esperar `Ready=True`, comprobar `https://mira.ninfasolutions.com`
sin iniciar sesión y, solo entonces, actualizar los callbacks y URLs durante
un deploy candidato.

## 2026-07-16 — Revalidación de URL de dominio y runtime

**Estado:** certificado pendiente; sin cambios de configuración.

**Qué se verificó:** el mapping aún espera el certificado HTTPS. La web ya usa
como proxy la URL actual de `ghmi-api`, mientras que `API_BASE_URL` dentro de
la API conserva un URL `run.app` anterior.

**Motivo:** evitar cambiar redirects antes de que el nuevo host tenga TLS y
evitar declarar producción con URLs API inconsistentes.

**Qué sigue:** cuando Cloud Run informe `Ready=True`, desplegar una revisión
candidata que actualice juntas las URLs de web/OAuth/WorkOS y `API_BASE_URL`.
El webhook WorkOS y los pagos no forman parte de este cambio.

## 2026-07-16 — Scheduler externo confirmado sin duplicación interna

**Estado:** terminado y verificado en GCP, con un timeout histórico pendiente
de revisión.

**Qué se hizo:**

- Se confirmó que `ghmi-daily-report` está `ENABLED`, agenda de lunes a viernes
  a las 08:00 `America/Santiago` y apunta al endpoint directo de
  `ghmi-api`.
- Se confirmó que la revisión activa declara `SCHEDULER_ENABLED=false`; Cloud
  Scheduler es la única fuente de ejecuciones programadas.
- Se revisaron ejecuciones recientes: la del 2026-07-15 llegó a la API con
  `200` en 6,2 segundos.

**Motivo:** con la API limitada a una instancia, ejecutar el scheduler interno
y Cloud Scheduler a la vez podría duplicar análisis y consumo. La evidencia
actual prueba que el job externo opera y que no existe esa duplicación.

**Evidencia:**

- `gcloud scheduler jobs describe ghmi-daily-report` — `state=ENABLED`,
  agenda y URI esperados.
- Configuración efectiva de Cloud Run — `SCHEDULER_ENABLED=false` y timeout
  de request de 300 segundos.
- Cloud Logging — `POST /internal/scheduled-analysis` con `200` el
  2026-07-15.

**Qué sigue:** investigar el `504` observado el 2026-07-14 tras 300 segundos
antes de depender del job para usuarios externos. No se modificaron timeout,
tráfico, dominio ni pagos en esta verificación.

## 2026-07-16 — Timeout del scheduler ampliado y verificado

**Estado:** terminado y verificado en GCP.

**Qué se hizo:**

- Se actualizó el timeout de request de `ghmi-api` de 300 a 1.800 segundos.
  Cloud Run creó la revisión de configuración `ghmi-api-00031-6lx` y la dejó
  lista con 100% del tráfico; no se cambió imagen, dominio, variables ni
  secretos.
- Se configuró `attemptDeadline=1800s` en `ghmi-daily-report` para que Cloud
  Scheduler no abandone antes que la API.
- Se comprobó `GET /health` contra la URL directa de API tras la revisión.

**Motivo:** el `504` del 2026-07-14 coincidió exactamente con el límite de 300
segundos de Cloud Run. Los análisis programados ya tienen un job dedicado y
pueden superar ese límite; ambos timeouts deben permitir la misma ventana.

**Evidencia:**

- `ghmi-api-00031-6lx` informa `Ready=True`, tráfico 100% y
  `spec.template.spec.timeoutSeconds=1800`.
- `ghmi-daily-report` informa `state=ENABLED` y `attemptDeadline=1800s`.
- `curl https://ghmi-api-io54uhmrxa-uc.a.run.app/health` devolvió
  `{"ok":true}`.

**Qué sigue:** observar la próxima ejecución programada y revisar su duración
en Cloud Logging. El mapping de `mira.ninfasolutions.com` sigue esperando el
certificado HTTPS; callbacks, URLs, WorkOS y pagos permanecen sin cambios.

## 2026-07-16 — Límites reales para el escalado horizontal documentados

**Estado:** terminado y verificado estáticamente; no habilita todavía más de
una instancia.

**Qué se hizo:**

- Se revisaron los caminos que dependen de estado compartido. El rate limiter
  es un `Mutex<HashMap<...>>` en memoria; los contadores de uso hacen
  lectura-incremento-upsert sin precondición; y checkout/suscripción/webhook
  encadenan operaciones independientes.
- Se corrigió una afirmación desactualizada del plan: el scheduler no hace un
  claim ingenuo. Firestore escribe el claim con la precondición `update_time`
  y reintenta ante una carrera.
- Se ejecutó la prueba
  `scheduler::tests::concurrent_claims_only_allow_one_scheduled_run` con éxito.

**Motivo:** saber qué limita realmente el escalado evita tanto mantener una
restricción innecesaria como ampliar instancias y perder límites de uso o
duplicar efectos de billing.

**Evidencia:**

- `apps/api/src/http/mod.rs` contiene el rate limiter por proceso y los
  incrementos read-modify-write de `UsageLedger`.
- `apps/api/src/firestore/mod.rs` usa `put_if_current(..., update_time)` para
  `claim_schedule_window`.
- `cargo test --manifest-path apps/api/Cargo.toml
  concurrent_claims_only_allow_one_scheduled_run` — 1 prueba aprobada.

**Qué sigue:** conservar `maxScale=1`. Cuando se retomen pagos, reemplazar los
contadores y las mutaciones de billing por operaciones condicionales o
transaccionales y probar concurrencia en una revisión sin tráfico.

## 2026-07-16 — Inventario de secretos de la API confirmado

**Estado:** terminado y verificado en GCP; dos secretos WorkOS siguen
pendientes de migración y rotación.

**Qué se hizo:** se inventariaron los nombres de variables de `ghmi-api` y su
modo de inyección, sin consultar ni imprimir valores. Cifrado, sesión, Google,
cron, Resend, Mercado Pago sandbox y webhook WorkOS provienen de Secret
Manager. `WORKOS_API_KEY` y `WORKOS_COOKIE_SECRET` son los únicos secretos
restantes como valores literales.

**Motivo:** el próximo deploy candidato no debe perpetuar secretos de texto
plano. Separar el hallazgo de sus valores permite preparar la migración sin
exponer credenciales.

**Evidencia:**

- Configuración de `ghmi-api-00031-6lx`: inventario de nombres y referencias
  de Secret Manager.
- `gcloud secrets list`: secretos esperados disponibles.
- `./scripts/check-secrets.sh` — sin patrones de credenciales en archivos
  versionados; su cobertura es deliberadamente parcial.

**Qué sigue:** generar una nueva API key en WorkOS y una nueva cookie secret,
guardarlas como versiones nuevas de Secret Manager y actualizar la API en una
ventana que advierta el cierre de sesiones. Pagos, callbacks y dominio no se
modificaron.

## 2026-07-16 — Deploy candidato sin tráfico preparado

**Estado:** terminado y verificado localmente; ninguna imagen nueva fue
publicada.

**Qué se hizo:** `scripts/redeploy-gcp.sh` acepta ahora `--no-traffic`. Al
usarlo, Cloud Run crea una revisión etiquetada `candidate`, conserva el tráfico
de usuarios en la revisión anterior e imprime la URL directa de la candidata.
Con `all`, la web candidata proxya a la API candidata. El flujo conserva el
gate `scripts/check-all.sh` antes de Docker y Cloud Run.

**Motivo:** el plan exige probar una revisión antes de promoverla. El script
anterior enviaba inmediatamente el tráfico a la nueva revisión, por lo que no
permitía hacer ese smoke test de forma segura.

**Evidencia:**

- `bash -n scripts/redeploy-gcp.sh` — aprobado.
- Ejecución simulada de `scripts/redeploy-gcp.sh --no-traffic api` — ejecutó
  el gate completo (161 pruebas Rust, 10 de worker, auditoría npm y build web)
  y confirmó que `gcloud run deploy` recibiría `--no-traffic --tag=candidate`.
- Ejecución simulada de `scripts/redeploy-gcp.sh --no-traffic all` — confirmó
  que `ghmi-web` recibe `API_PROXY_TARGET` de la API etiquetada `candidate`.
- Se verificó la semántica contra la documentación oficial actual de Cloud Run:
  un tag permite probar una revisión sin enviarle tráfico de usuarios.

**Qué sigue:** cuando el certificado de `mira.ninfasolutions.com` esté listo y
las credenciales WorkOS estén rotadas, publicar la candidata real, probar su
URL etiquetada y recién entonces promover tráfico. Pagos siguen diferidos.

## 2026-07-16 — Recuperación Firestore alineada con el estado real

**Estado:** controles verificados y runbook actualizado; drill real pendiente.

**Qué se hizo:** se confirmó en GCP que Firestore `(default)` tiene PITR,
delete protection y una retención configurada de 7 días. Se corrigieron las
referencias operativas que aún indicaban que los controles estaban pendientes.
El runbook ahora prescribe un clone PITR a una base nueva para recuperación o
drill, en vez de borrar o restaurar `(default)` in-place.

**Motivo:** el estado operativo debe reflejar los controles realmente activos.
Un restore in-place requiere downtime y puede sobrescribir cambios; un clone
permite validar la recuperación sin tocar los datos que usa Mira.

**Evidencia:**

- `gcloud firestore databases describe --database='(default)'` —
  `POINT_IN_TIME_RECOVERY_ENABLED`, `DELETE_PROTECTION_ENABLED`,
  `versionRetentionPeriod=604800s` y
  `earliestVersionTime=2026-07-15T19:07:00Z`.
- Se contrastó el procedimiento con la documentación oficial de Firestore para
  PITR y clones.

**Qué sigue:** con autorización y presupuesto acotado, crear una base de drill
desde un minuto válido de PITR, medir duración y coste, y recién entonces
definir RPO/RTO. No se clonaron ni restauraron datos durante este avance.

## 2026-07-16 — Imágenes de redeploy trazables por commit

**Estado:** terminado y verificado localmente; no se publicaron imágenes.

**Qué se hizo:** `scripts/redeploy-gcp.sh` dejó de construir y desplegar con
`:latest`. Ahora obtiene `IMAGE_TAG` desde los primeros 12 caracteres del
commit actual, o respeta un tag explícito entregado por el operador. Rechaza
un árbol Git con cambios rastreados o archivos sin trackear para no etiquetar
código distinto como si fuera un commit conocido.

**Motivo:** `latest` es mutable y no identifica con qué código se creó una
revisión de Cloud Run. Un tag por commit permite asociar build, candidata,
promoción y rollback al mismo artefacto.

**Evidencia:**

- El registro de Artifact Registry actual sólo mostraba `latest` para los tres
  servicios, lo que confirmaba la discrepancia con el runbook.
- Simulación con `IMAGE_TAG=release-test` confirmó que Docker y Cloud Run usan
  `:release-test`.
- Simulación sin override confirmó el tag `cb5b80436e3d`, obtenido desde Git,
  junto con `--no-traffic --tag=candidate`.
- Con el árbol de trabajo modificado actual, el script abortó antes del gate
  con `Refusing to deploy a dirty Git worktree`.

**Qué sigue:** el próximo deploy candidato publicará imágenes etiquetadas por
commit y dejará la revisión exacta como evidencia. Las imágenes históricas
`latest` no se modificaron; pagos, dominio y WorkOS permanecen fuera de este
cambio.

## 2026-07-16 — Auditoría de privilegios Firestore y claves de servicio

**Estado:** auditoría terminada; separación de identidades y rotación de clave
pendientes.

**Qué se hizo:** se revisaron las identidades de los tres servicios Cloud Run,
los roles de las service accounts `ghmi-*`, las referencias de código y los
selectores de credenciales del runtime. API y worker comparten
`ghmi-runtime`, que posee Firestore y Secret Manager; la web usa la identidad
Compute Engine por defecto. El worker no contiene código Firestore, pero su
identidad actual sí podría acceder a la base. Se encontró una cuenta
`ghmi-firestore` no asignada a Cloud Run con una clave `USER_MANAGED` activa.

**Motivo:** el worker no necesita Firestore. Compartir la identidad amplía el
impacto de una vulneración del worker, y una clave persistente requiere un
propietario, rotación y propósito verificable.

**Evidencia:**

- `ghmi-api` y `ghmi-ai-worker` declaran la misma service account; `ghmi-web`
  declara otra.
- IAM asigna `roles/datastore.user` a `ghmi-runtime` y a `ghmi-firestore`.
- `apps/ai-worker` no contiene referencias Firestore; `apps/api` construye
  `FirestoreStorage`.
- `ghmi-firestore` tiene una clave `USER_MANAGED` sin vencimiento; la API
  desplegada no define `GOOGLE_APPLICATION_CREDENTIALS` ni un selector de
  credenciales Firestore.

**Qué sigue:** en una candidata autorizada, mover API a `ghmi-firestore`,
concederle sólo los secretos necesarios y retirar Firestore de `ghmi-runtime`.
Antes de revocar la clave, confirmar y migrar cualquier uso local que dependa
de ella. No se cambiaron roles, identidades ni claves durante esta auditoría.

## 2026-07-16 — Certificado de `mira.ninfasolutions.com` provisionado

**Estado:** terminado y verificado en Cloud Run; callbacks y deploy candidato
pendientes.

**Qué se hizo:** se reconsultó el Domain Mapping de `mira.ninfasolutions.com`.
Cloud Run informa `Ready=True`, `CertificateProvisioned=True` y
`DomainRoutable=True`. El CNAME requerido sigue respondiendo públicamente como
`ghs.googlehosted.com`.

**Motivo:** el dominio no debía incorporarse a OAuth, WorkOS ni a una imagen
antes de tener TLS provisionado. Este estado confirma que Google ya reconoce y
certifica el host.

**Evidencia:**

- Domain Mapping: transición a `Ready=True` y `CertificateProvisioned=True` a
  las 04:47 UTC.
- `dig @1.1.1.1 CNAME mira.ninfasolutions.com` devuelve
  `ghs.googlehosted.com`.

**Qué sigue:** registrar las callbacks de Mira en Google OAuth y WorkOS,
rotar las credenciales WorkOS pendientes y publicar la revisión candidata con
las URLs alineadas. El resolver local aún puede responder desde caché anterior;
pagos no se modificaron.

## 2026-07-16 — Preparación de release versionada localmente

**Estado:** terminado y verificado localmente; sin push ni despliegue.

**Qué se hizo:** se versionaron las actualizaciones de dominio, runbooks,
recuperación Firestore, trazabilidad de imágenes y despliegue candidato. El
árbol de trabajo quedó limpio, condición exigida por `redeploy-gcp.sh` antes de
construir una imagen etiquetada por commit.

**Motivo:** una revisión candidata debe ser reproducible desde un commit
conocido. Mantener cambios locales sin versionar impediría el deploy a propósito
y haría imposible asociar la imagen al código revisado.

**Evidencia:**

- `./scripts/check-all.sh` — 161 pruebas Rust, 10 del worker, formato, Clippy,
  build release, scan de secretos, auditoría npm y build web aprobados.
- `bash -n scripts/redeploy-gcp.sh` y `git diff --check` — aprobados antes de
  versionar.

**Qué sigue:** completar las callbacks Google/WorkOS y la rotación WorkOS;
después se podrá construir la candidata sin tráfico desde este commit. Pagos no
se modificaron.

## 2026-07-16 — Pasada P0 de copy en la aplicación autenticada

**Estado:** terminado y verificado localmente; pendiente la revisión humana de
landing y textos legales.

**Qué se hizo:** se reemplazaron referencias visibles a WorkOS, Firestore,
scopes de Gmail, scheduler, mailbox, estados de ejecución, IDs de ejecución,
categorías de error y conteos de tokens por texto claro para el usuario. Los
ítems de configuración faltantes y los estados operativos ahora se traducen
antes de mostrarse. También se eliminó la exposición del identificador de cada
ejecución y de su categoría interna de error.

**Motivo:** los nombres de proveedores, campos internos e identificadores no
ayudan a completar una tarea y pueden confundir o filtrar detalles operativos.
La app debe explicar el permiso, el estado y la acción siguiente sin requerir
conocimiento técnico.

**Evidencia:**

- `rg -n -i 'policy_snapshot|pending_backend_contract|provider_subscription_id|invalid_grant' apps/web/src` — sin coincidencias.
- `npm --prefix apps/web run build` — aprobado (TypeScript y build de Vite).
- `git diff --check` — aprobado.

**Qué sigue:** revisar visualmente la candidata con una organización real y
terminar la revisión editorial de landing, precios, términos y privacidad con
la información legal del responsable. No se modificaron checkout, Mercado Pago
ni sus webhooks.

## 2026-07-16 — Cierre de estados y métricas técnicas visibles

**Estado:** terminado y verificado localmente; los flujos de pago permanecen
fuera de esta revisión.

**Qué se hizo:** se completó la revisión estática de banners, modales y
pantallas de la app autenticada. Se retiraron los tokens de IA de Ayuda y del
reporte consolidado, y el número de llamadas al proveedor de IA del embudo.
El embudo conserva la información útil: cuántos hilos se analizaron y cuántos
requirieron revisión con IA.

**Motivo:** esos valores describen coste o implementación interna, no una
acción ni un resultado que el cliente pueda interpretar. Suprimirlos deja las
pantallas enfocadas en cobertura y resultados del análisis.

**Evidencia:**

- Búsqueda estática de los campos y textos visibles en `apps/web/src`.
- `npm --prefix apps/web run build` — aprobado.
- `git diff --check` — aprobado.

**Qué sigue:** validar login, OAuth, errores y análisis en la URL candidata
una vez que se registren sus callbacks. Checkout y sus mensajes no se
revisaron ni cambiaron debido al incidente de pagos diferido.

## 2026-07-16 — Incrementos de usage ledger protegidos contra carreras

**Estado:** terminado y verificado localmente; validación de cupo atómica y
prueba desplegada todavía pendientes.

**Qué se hizo:** se reemplazó la secuencia separada de leer, incrementar y
guardar el `usage ledger` por una única operación de almacenamiento. En
Firestore, cada escritura exige que el documento conserve su `updateTime`, o
que aún no exista al crearlo; ante una carrera relee y reintenta hasta tres
veces. El almacenamiento en memoria ejecuta la misma operación bajo su lock.

**Motivo:** un análisis se ejecuta en segundo plano y puede actualizar los
contadores al mismo tiempo que otra solicitud crea un análisis. Sin una
precondición, un guardado tardío podía eliminar el incremento anterior y dejar
los límites de uso subcontados.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml usage_additions_do_not_lose_concurrent_updates` — aprobado.
- La prueba lanza dos incrementos concurrentes y verifica los totales de
  análisis creados, hilos analizados y auditorías IA.
- `cargo fmt --manifest-path apps/api/Cargo.toml -- --check`, Clippy y
  `git diff --check` — aprobados.

**Qué sigue:** probar la carrera contra Firestore desde dos instancias reales.
La validación de cupo y la creación de un análisis todavía no son una
transacción única, por lo que `maxScale=1` se mantiene. Checkout,
suscripciones y webhooks de Mercado Pago no se modificaron.

## 2026-07-16 — Concurrencia efectiva de la API inventariada

**Estado:** inspección terminada; sin cambios en Cloud Run.

**Qué se hizo:** se consultó la configuración de servicio y de la revisión
activa de `ghmi-api`. Ambas muestran `maxScale=1` y
`containerConcurrency=80`.

**Motivo:** limitar la cantidad de instancias no equivale a procesar una sola
solicitud a la vez. Esta diferencia define qué carreras todavía deben probarse
contra Firestore antes de autorizar escalado.

**Evidencia:**

- `gcloud run services describe ghmi-api --region=us-central1` —
  `containerConcurrency: 80`, `autoscaling.knative.dev/maxScale: '1'` y
  revisión activa `ghmi-api-00031-6lx`.

**Qué sigue:** en la candidata, ejecutar dos requests concurrentes que creen o
actualicen análisis y observar los contadores y estados en Firestore. No se
reduce la concurrencia a 1 de forma automática: el scheduler externo puede
mantener una request de análisis activa por un tiempo prolongado. Pagos no se
modificaron.

## 2026-07-16 — Inicio de análisis protegido contra doble ejecución

**Estado:** terminado y verificado localmente; prueba Firestore desplegada
pendiente.

**Qué se hizo:** se añadió un claim atómico para iniciar un análisis. El
repositorio sólo cambia un run de `Pending` a `Running` si la versión de
Firestore que leyó sigue vigente; al perder la carrera, la segunda solicitud
recibe un conflicto y no inicia otra tarea en segundo plano. El almacenamiento
en memoria aplica la misma transición bajo su lock.

**Motivo:** Cloud Run permite 80 requests simultáneas por instancia. Dos clics
o reintentos casi simultáneos podían observar el estado pendiente y lanzar dos
análisis para la misma solicitud.

**Evidencia:**

- `cargo test --manifest-path apps/api/Cargo.toml concurrent_start_claims_only_allow_one_analysis` — aprobado.
- La prueba dispara dos claims concurrentes y verifica un único ganador y el
  estado final `Running`.
- Formato, Clippy y `git diff --check` — aprobados.

**Qué sigue:** ejecutar el mismo caso contra Firestore desde la revisión
candidata y conservar `maxScale=1` hasta medirlo. Las actualizaciones de
progreso/estado terminal aún requieren revisión antes de escalar. No se
modificaron checkout, suscripciones ni webhooks de Mercado Pago.
