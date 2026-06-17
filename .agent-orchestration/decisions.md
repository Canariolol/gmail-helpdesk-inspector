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
