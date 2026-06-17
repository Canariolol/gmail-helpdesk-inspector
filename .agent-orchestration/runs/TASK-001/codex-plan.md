Nota de ejecución: Codex trabajó en modo read-only y no leyó `.env`, `.env.*`, `secrets/*` ni credenciales. El CLI no pudo escribir desde dentro del sandbox, pero Pi guardó el output final en este archivo.

# TASK-001 — Auditoría actual + plan SaaS público

Fecha: 2026-06-17  
Modo: auditoría read-only. Tests no ejecutados porque el sandbox no permite escribir artefactos de build/cache.

## 1. Resumen ejecutivo del estado actual

`gmail-helpdesk-inspector` está bien encaminado como MVP open-source/local o despliegue mono-usuario: usa Gmail `readonly`, no modifica Gmail, clasifica hilos, calcula métricas auditables, permite revisión manual, persiste en Firestore y usa un worker Python/Bedrock como auditor IA.

Para SaaS público todavía no está launch-ready. El mayor gap no es UI: es seguridad multiusuario, privacidad, aislamiento tenant, lifecycle OAuth, compliance Google restricted scopes y operación confiable de jobs.

## 2. Gaps críticos para SaaS público

- Varias rutas no validan sesión/ownership: `GET /analysis-runs/{id}`, metrics, threads, events, `GET /threads/{thread_id}` y `PATCH /threads/{thread_id}/manual-review`.
- No hay modelo formal de tenant/org/roles.
- No hay desconexión Gmail con revocación OAuth ni borrado verificable de datos.
- Retención, exportación, eliminación y consentimiento IA no están definidos.
- Worker IA tiene contexto hardcoded de una empresa/personas concretas.
- Scheduler/idempotencia sirve para instancia única, no para SaaS multi-tenant.
- Firestore usa scans/listados con `pageSize=300` y paths top-level poco tenant-safe.
- Google OAuth verification para `gmail.readonly` está pendiente.

## 3. Riesgos de seguridad/privacidad/compliance

Críticos:
- IDOR/cross-user exposure por endpoints sin authz.
- Modificación de manual review sin ownership check.
- Exposición de metadata/snippets/headers de otros usuarios si se conoce un ID.

Altos:
- Defaults de desarrollo para `APP_ENCRYPTION_KEY` y `APP_SESSION_SECRET`.
- Errores públicos con `error.to_string()` pueden filtrar detalles internos.
- Falta CSRF para mutaciones con cookie auth.
- Falta rate limiting/quotas para Gmail API, Bedrock y análisis.
- Bedrock recibe extractos de emails; requiere consentimiento, subprocessor disclosure y minimización.
- No hay política SaaS completa de privacidad, DPA, retención, eliminación y subprocessors.

## 4. Riesgos técnicos/arquitectura

- `tokio::spawn` desde request no es suficiente para jobs SaaS confiables.
- StorageRepository no fuerza acceso scoped por tenant/owner.
- `get_thread` en Firestore escanea runs.
- Falta paginación real en runs/threads/messages.
- Locks del scheduler no son transaccionales.
- IA no versiona prompt/model/provider en resultados auditables.
- No hay eval suite/regression dataset para calidad de clasificación.

## 5. Roadmap por fases

Hardening MVP:
- Corregir authz/ownership en todas las rutas.
- Fail-fast de secretos en producción.
- CSRF/rate limiting/errores redacted.
- Desconexión Gmail, revocación y borrado de datos.
- Parametrizar worker IA por tenant/run.
- Tests IDOR, CSRF y no persistencia de body.

Beta privada:
- Modelo `Tenant`, `Membership`, roles.
- Firestore tenant-scoped + migración.
- Job runner persistente.
- Retención configurable.
- Observabilidad por tenant/run/coste IA.
- Onboarding privado con consentimiento claro.

Beta pública:
- Google OAuth verification para `gmail.readonly`; no ampliar scopes.
- Privacy/terms/DPA/subprocessors/deletion flow.
- Límites beta: threads/run, runs/día, reportes, miembros.
- E2E completos y security review ligero.

SaaS público:
- Billing solo después de seguridad, privacidad y OAuth verification.
- Self-serve onboarding, roles, cuotas y alertas.
- Backups/restore, incident response, vulnerability disclosure.
- Diferenciación: métricas auditables, confianza del reporte, revisión manual guiada, comparativas temporales.

## 6. Tareas priorizadas con IDs sugeridos

- `GHMI-SEC-001`: authz/ownership en runs, metrics, threads, events y manual review.
- `GHMI-SEC-002`: tests IDOR con dos usuarios.
- `GHMI-SEC-003`: prohibir secretos default en producción.
- `GHMI-SEC-004`: CSRF para mutaciones con cookie auth.
- `GHMI-SEC-005`: redacción de errores/logs.
- `GHMI-SEC-006`: rate limits y quotas iniciales.
- `GHMI-SEC-007`: desconectar Gmail y revocar token.
- `GHMI-SEC-008`: borrar run/cuenta/tenant.
- `GHMI-ARCH-001`: modelo tenant/org/roles.
- `GHMI-ARCH-002`: Firestore tenant-scoped.
- `GHMI-ARCH-003`: repositorio owner-scoped por defecto.
- `GHMI-ARCH-004`: job runner persistente.
- `GHMI-PRIV-001`: data map y retención.
- `GHMI-PRIV-002`: privacy policy, DPA, subprocessors.
- `GHMI-AI-001`: eliminar prompt/defaults hardcoded.
- `GHMI-AI-002`: versionar prompt/model/audit.
- `GHMI-OAUTH-001`: paquete Google OAuth verification.
- `GHMI-UX-001`: brief UI/UX beta privada.

## 7. Qué debe hacer Claude UI/UX después

Claude debe diseñar beta privada SaaS, no landing pública ni billing completo. Debe cubrir login, crear org, conectar Gmail, explicar `gmail.readonly`, consentimiento IA, configuración de dominio interno, análisis, progreso, dashboard, revisión manual, reportes, privacidad, desconexión y borrado. No debe cambiar contratos backend ni asumir endpoints inexistentes.

## 8. Qué no conviene hacer todavía

- No ampliar scopes Gmail.
- No enviar/modificar Gmail.
- No persistir cuerpos completos.
- No abrir SaaS público con authz actual.
- No construir billing antes de P0/P1 y OAuth verification.
- No convertirlo en ticketing completo.
- No hacer UI cosmética grande antes de privacidad/seguridad/consentimiento.

## 9. Checks/tests recomendados

- Rust: `cargo fmt`, `cargo clippy`, `cargo test`.
- Security tests: IDOR, CSRF, cookie flags, error redaction.
- Persistence tests: `body_text` nunca en Firestore.
- Python: `pytest`, schema tests, prompt-injection evals.
- Frontend: `npm --prefix apps/web run build`, E2E Playwright.
- Supply chain: `cargo audit`, `npm audit`, `pip-audit`, secret scan, container scan.
- Ops: DAST staging, IAM review, backup/restore, scheduler concurrency test.
- Google: OAuth verification checklist, privacy/deletion evidence.

## 10. Preguntas abiertas para el dueño del producto

- ¿SaaS para empresas con casilla compartida o usuarios individuales?
- ¿Multiusuario desde beta privada?
- ¿Bedrock obligatorio o IA opcional por tenant?
- ¿Retención default: 30, 90, 180 días?
- ¿Reportes por email deben incluir subjects/remitentes o solo métricas?
- ¿Quién financiará/gestionará Google OAuth verification?
- ¿Core open-source y hosted SaaS tendrán feature split?
- ¿Google Workspace solamente o también Gmail personal?
- ¿Billing en lanzamiento o beta manual?
- ¿Quién aprueba textos legales?

## Handoff para Claude

Claude queda limitado a TASK-002 UI/UX beta privada. No debe modificar backend, deploy, scopes OAuth ni contratos. Debe priorizar onboarding seguro, consentimiento, trazabilidad, estados de error, revisión manual, privacidad, desconexión y borrado. Cualquier API faltante debe quedar como dependencia para Codex/Pi.