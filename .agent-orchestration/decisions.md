# Decisiones

## D-001 — Orquestación por archivos + CLI

Fecha: 2026-06-17

Se usará `.agent-orchestration/` como fuente de verdad para coordinar Pi, Codex y Claude. Las CLIs pueden ejecutarse de forma no interactiva, pero sus outputs deben guardarse en `runs/` antes de integrar cambios.

## D-002 — Codex planifica; Claude lidera UI/UX

Fecha: 2026-06-17

Codex será el planner principal para producto, arquitectura, seguridad y backend. Claude será el owner de UI/UX. Pi integra y arbitra.

## D-003 — Model routing por costo/calidad

Fecha: 2026-06-17

No todo requiere el modelo más caro. Se reserva razonamiento alto/xhigh para decisiones críticas, seguridad y UX premium. Sonnet puede hacer la primera pasada UI/UX; Opus se usa para refinamiento complejo o final.

## D-004 — Paralelismo conservador

Fecha: 2026-06-17

Inicialmente se trabajará con Pi + una instancia Codex + una instancia Claude. Se abrirán más instancias solo con paths independientes y beneficio claro.

## D-005 — Secretos fuera de alcance

Fecha: 2026-06-17

`.env` y `secrets/*` quedan fuera de alcance para agentes. Solo se pueden usar plantillas/documentación segura.

## D-006 — Modelo Codex CLI válido

Fecha: 2026-06-17

La CLI de Codex rechazó `gpt-5.5-high` con cuenta ChatGPT. El modelo válido detectado en `~/.codex/config.toml` es `gpt-5.5` con `model_reasoning_effort = "high"`. Los scripts usan ese formato por defecto.

## D-007 — UI no requiere Opus todavía

Fecha: 2026-06-17

Claude Sonnet con esfuerzo máximo produjo un brief suficiente para la primera iteración UI/UX. Opus se reserva para sistema visual avanzado, billing o refinamiento final.

## D-008 — Política anti-enumeración

Fecha: 2026-06-17

Para recursos ajenos con IDs opacos se preferirá responder `404` en vez de `403`, salvo endpoints donde el usuario ya conoce explícitamente el recurso. Objetivo: reducir enumeración/IDOR. `401` se mantiene para sesión ausente/inválida.

## D-009 — Dueño del producto in-the-loop

Fecha: 2026-06-17

Codex y Claude deben elevar dudas a Pi usando `BLOCKED_QUESTIONS` y no resolver ambigüedades críticas inventando supuestos. Pi debe consolidar opciones, sugerir una recomendación y preguntar al dueño del producto cuando la decisión afecte producto, seguridad, privacidad, SaaS, legal, costos o promesas de UX.

## D-010 — Generalizar hardcodes de la empresa original

Fecha: 2026-06-17

El sistema nació para una persona/empresa concreta, pero para SaaS ningún dominio, contexto del auditor IA, timezone, destinatario de reporte, umbral, filtro o comportamiento operacional debe quedar predefinido para esa empresa. Debe migrarse a configuración por usuario/organización con defaults seguros y auditables. La configuración debe ser más robusta que checkboxes simples: debe capturar política operacional y criterios de clasificación.

## D-011 — Decisiones iniciales de beta/configuración SaaS

Fecha: 2026-06-17

Aprobado por el dueño del producto:

- Beta privada para organizaciones con 1 mailbox inicial.
- IA opt-in explícita por organización.
- Retención default 30 días en beta, configurable por admin.
- Reportes por email configurables, default solo métricas.
- Scheduler configurable con preset simple.
- Google Workspace only para beta.

## D-012 — Protocolo de preguntas al dueño

Fecha: 2026-06-17

Pi debe mostrar siempre alternativas, recomendación e impacto antes de pedir decisiones al dueño del producto. No basta con preguntar aprobación de una recomendación resumida.

## D-013 — P0 ownership backend implementado

Fecha: 2026-06-17

Se implementó validación de sesión/ownership para runs, status, metrics, threads, events, thread detail y manual review. Recursos ajenos responden `404`. Manual review usa `reviewer_label` desde el email autenticado, no desde el cliente. Checks: `cargo test` y `cargo clippy --all-targets -- -D warnings` OK.

## D-014 — TASK-007 opción B implementada

Fecha: 2026-06-17

Se aprobó e implementó la opción B: backend policy-first incremental, con avance UI/UX en paralelo por Claude. El sistema ahora tiene organización implícita, mailbox inicial, `PolicyDraft`, `PolicyVersion`, snapshot por `AnalysisRun`, IA opt-in por policy, retención default 30 días y UI inicial de configuración guiada. Se mantiene compatibilidad legacy para análisis con filtros antiguos. Scheduler V2, retention job real y worker IA con contexto operacional completo quedan para tareas posteriores.

## D-015 — Acciones destructivas visibles pero deshabilitadas

Fecha: 2026-06-17

Se implementó un centro de privacidad y datos con resumen read-only (`GET /me/data-summary`). Las acciones de desconectar Gmail, borrar análisis y borrar cuenta/datos quedan visibles pero deshabilitadas hasta aprobar su semántica exacta, confirmaciones requeridas, efectos sobre scheduler y cascadas de borrado. Esto evita prometer capacidades destructivas o ejecutar borrados/revocaciones sin decisión humana explícita.

## D-016 — Scheduler V2 incremental priorizado sobre borrado/desconexión

Fecha: 2026-06-17

El dueño priorizó acercar la app a production-ready funcional y confirmó que borrado/desconexión no son prioridad por ahora. Se implementó scheduler policy-first incremental: `/me/org/config` sincroniza `scheduleConfigs`, los scheduled runs usan `PolicySnapshot` cuando existe org config, y los reportes respetan `report_content` (`metrics_only` o redacción de asuntos/remitentes). Se mantiene compatibilidad legacy/env seed.

## D-017 — Tres frentes production-ready en paralelo

Fecha: 2026-06-17

Se avanzó en paralelo en scheduler timezone por tenant, observabilidad operativa y rate limiting. El scheduler interno ahora evalúa `weekdays_08_local` por timezone IANA de cada config. Se agregó `GET /me/operations/status` y panel UI en Configuración. Se agregó rate limiting in-memory por usuario para creación/inicio manual de análisis y guardia para evitar iniciar dos veces un run no pendiente. Borrado/desconexión sigue explícitamente fuera de prioridad.

## D-018 — Roadmap v2 y estabilización base

Fecha: 2026-06-17

Se aprobó la opción B: crear una base de roadmap production-ready antes de seguir acumulando features sueltas. Se creó `.agent-orchestration/production-roadmap-v2.md`. Como primer sprint de estabilización se implementó: AI worker stateless con `policy_context` desde `PolicySnapshot` y sin hardcodes de empresa original; README/runtime docs actualizadas; y `scripts/check-all.sh` como check local unificado. Landing/waitlist pasa a TASK-020 como siguiente foco product/UX.

## D-019 — Sprint B beta shell en misma app Vite

Fecha: 2026-06-17

Para la beta privada se implementó la landing/waitlist dentro de la misma app Vite, en el estado público no autenticado, en vez de crear una app marketing separada. Alternativas consideradas: app separada/marketing site o ruta pública dentro de la app. Recomendación aplicada: misma app por menor complejidad de despliegue, reutilización del modal OAuth y velocidad de validación. Impacto: suficiente para beta privada; para SaaS público/pricing puede separarse más adelante. El formulario waitlist usa `mailto:` y no introduce un nuevo backend ni almacenamiento de leads.

## D-020 — Sprint C delegado a Claude Code, sin Codex

Fecha: 2026-06-17

Por límite de uso disponible, el dueño pidió no usar Codex y delegar solo a Claude Code. Se intentó planificación read-only con herramientas para TASK-023/024/025; los procesos quedaron sin output y se cerraron. Se relanzó Claude sin herramientas con contexto resumido para TASK-023/024 y generó planes útiles. Claude generó el runbook inicial de TASK-025; Pi lo corrigió con nombres reales verificados en código permitido. Se implementó Sprint C: `/me/operations/history`, panel de historial operativo, paginación opt-in compatible para runs/threads, y `docs/deployment-beta-runbook.md`. No se leyó contenido de `.env` ni `.env.*`; no se usó Codex.
