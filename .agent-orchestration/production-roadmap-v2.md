# Production Roadmap v2 — Gmail Helpdesk Inspector SaaS

Fecha: 2026-06-17
Estado: aprobado como base de planificación; Sprint A, B y C completados en alcance beta privada

## Por qué existe este documento

El roadmap original (`saas-roadmap.md`) fue útil para desbloquear P0/P1 iniciales, pero ya quedó desactualizado: varias piezas fueron implementadas y aparecieron brechas nuevas importantes. Este documento propone una base de planificación extendida para llevar el producto a beta privada production-ready y luego a SaaS publicable.

## Estado actual resumido

### Completado / avanzado

- Authz/ownership para endpoints principales de runs, threads, metrics, events y manual review.
- Anti-IDOR con tests de dos usuarios.
- `reviewer_label` controlado por backend.
- Login/ayuda/privacy copy con `gmail.readonly`.
- Configuración SaaS incremental:
  - org implícita;
  - mailbox inicial;
  - policy draft/version;
  - snapshot por run;
  - IA opt-in por policy;
  - retención default 30 días;
  - reportes metrics-only default.
- Centro de privacidad/datos read-only.
- Scheduler policy-first incremental.
- Scheduler por timezone IANA para `weekdays_08_local`.
- Observabilidad read-only básica en Configuración.
- Rate limiting in-memory por usuario para análisis manuales.
- Fail-fast de secretos default en `APP_ENV=production`.

### Brechas críticas detectadas

1. **AI worker con contexto por policy — resuelto en TASK-017**
   - El worker ya no contiene defaults tenant/company específicos.
   - La API envía `policy_context` desde `PolicySnapshot`.
   - Mantener vigilancia para que no vuelvan hardcodes.

2. **README/documentación runtime — actualizado en TASK-018**
   - Scheduler docs siguen hablando de America/Santiago global.
   - README dice IA auditor obligatoria, pero producto aprobó IA opt-in.
   - Falta documentación de org/policy/versioning y endpoints nuevos.
   - Prioridad: P0/P1 para operar sin errores.

3. **Rate limiting no es distribuido**
   - In-memory por instancia.
   - Aceptable para beta con `max_instances=1`, insuficiente para SaaS multi-instancia.
   - Prioridad: P1 para beta controlada; P0 si se despliega multi-instancia.

4. **Retención no ejecuta borrado real**
   - Se calcula `retention_expires_at`, pero no hay job.
   - El dueño indicó que borrado/desconexión no son prioridad inmediata.
   - Prioridad: P2 para SaaS público/compliance; no bloquear quick wins funcionales.

5. **Landing pública inexistente**
   - No hay página pública de marketing/waitlist/trust.
   - Login actual funciona como pantalla de app, no como sitio SaaS publicable.
   - Prioridad: P1 si queremos captar beta/mostrar producto.

6. **OAuth verification y legales pendientes**
   - Privacy Policy, Terms, DPA/subprocessors, Google OAuth verification.
   - Prioridad: P2/P3; no se puede lanzar SaaS público sin esto.

7. **Persistencia/operación production todavía parcial**
   - No hay migración/índices documentados Firestore.
   - No hay backup/restore/runbook.
   - No hay structured operational runbook.
   - No hay distributed job claim.

8. **Paginación y escalabilidad de listados pendientes**
   - Runs/threads/messages aún pueden crecer.
   - Prioridad: P1/P2 según tamaño de beta.

## Roadmap por fases

### Fase 0 — Estabilización de base antes de seguir acumulando features

Objetivo: dejar el repo consistente, documentado y sin hardcodes obvios.

Tareas recomendadas:

1. AI worker policy-injection / eliminar hardcodes. ✅ TASK-017
2. README/runtime docs update. ✅ TASK-018
3. `.env.example`/docs seguros alineados con SaaS. Pendiente por protocolo `.env.*`
4. CI/check script único para API + web + AI worker. ✅ TASK-019
5. Limpieza de roadmap/board: mover tareas done y crear backlog claro. ✅ parcial

Criterio de salida:

- No quedan dominios/personas/empresa original fuera de tests deliberados.
- `cargo test`, `clippy`, `npm build`, `pytest ai-worker` pasan.
- README describe el comportamiento actual.

### Fase 1 — Beta privada funcional production-ready

Objetivo: poder usarlo con 1–3 organizaciones beta de forma controlada.

Tareas recomendadas:

1. Landing/waitlist beta privada.
2. Onboarding más claro: Workspace-only, IA opt-in, reportes, retención.
3. Observabilidad más útil: historial corto de scheduler/runs, errores categorizados.
4. Rate limiting fase 2 o decisión explícita `max_instances=1`.
5. Paginación básica de runs/threads.
6. Cloud deployment runbook:
   - Cloud Run/API/web/worker;
   - Firestore;
   - Resend;
   - scheduler;
   - secrets;
   - OAuth test users.
7. Error redaction/logging hardening.

Criterio de salida:

- Un beta user puede entender el producto, conectarse, configurar, analizar y recibir reporte sin intervención manual.
- Operador puede diagnosticar fallos sin mirar datos sensibles.

### Fase 2 — Trust/compliance para beta ampliada

Objetivo: reducir riesgos legales/privacidad y preparar OAuth verification.

Tareas recomendadas:

1. Privacy Policy / Terms / subprocessors draft.
2. OAuth verification checklist.
3. Data retention job real.
4. Disconnect/revoke Gmail y delete flows aprobados.
5. Export/account data summary.
6. Firestore rules/tenant-scoping review.
7. Backup/restore + incident response.
8. Security checklist final: CSRF, headers, CORS, cookies, logs, error bodies.

Criterio de salida:

- Paquete listo para revisión OAuth/privacidad.
- Flujos de datos documentados end-to-end.

### Fase 3 — SaaS público / publicable

Objetivo: salir de beta privada con sitio público y operación SaaS.

Tareas recomendadas:

1. Landing final + pricing/waitlist/demo.
2. OAuth app production/verified.
3. Billing solo cuando seguridad/compliance estén listos.
4. Multi-user/org roles reales.
5. Distributed rate limit/job claims.
6. Cost observability por tenant (Gmail/API/IA/reportes).
7. SLA/support/runbook.

## Próximos 3 sprints sugeridos

### Sprint A — Base sana y hardcodes fuera

Estado: completado en su parte de código/docs permitida.

1. TASK-017: AI worker policy injection y eliminación de hardcodes. ✅
2. TASK-018: Docs/runtime update. ✅
3. TASK-019: Check script/CI local unificado. ✅

Pendiente opcional: `.env.example` seguro, requiere autorización explícita por estar bajo `.env.*`.

### Sprint B — Public beta shell

Estado: completado.

1. TASK-020: Landing/waitlist beta privada. ✅
2. TASK-021: Onboarding copy/UX final para Workspace-only + IA opt-in. ✅
3. TASK-022: Empty/error states y 429 UX. ✅

### Sprint C — Operación beta

Estado: completado.

1. TASK-023: Observability v2/historial corto + error categories. ✅
2. TASK-024: Paginación runs/threads. ✅
3. TASK-025: Deployment runbook + Cloud checklist. ✅

## Quick wins recomendados

- Actualizar README para no mentir sobre IA obligatoria ni scheduler SCL global. ✅
- Eliminar hardcodes AI worker. ✅
- Agregar `scripts/check-all.sh`. ✅
- Agregar página landing mínima con waitlist deshabilitada/local o mailto.
- Agregar banner beta privada/Workspace-only en login/landing.
- Mejorar UX de errores 429 y scheduler disabled.

## Decisiones pendientes

1. Landing dentro de `apps/web` o app separada
   - Opción A: misma Vite app con modo público/login.
   - Opción B: app separada `apps/landing`.
   - Recomendación: A por velocidad; B si se quiere marketing muy distinto.
   - Impacto: A menos overhead, B más limpio a largo plazo.

2. Rate limit distribuido ahora o beta con `max_instances=1`
   - Opción A: documentar `max_instances=1` para beta.
   - Opción B: Firestore distributed limiter.
   - Recomendación: A para sprint inmediato, B antes de beta ampliada.
   - Impacto: A rápido pero limitado; B más robusto.

3. AI worker contextualizado por API o por payload directo
   - Opción A: API envía policy context en cada audit request.
   - Opción B: worker consulta backend/storage.
   - Recomendación: A; mantiene worker stateless y minimiza permisos.
   - Impacto: hay que versionar schema de audit request.
