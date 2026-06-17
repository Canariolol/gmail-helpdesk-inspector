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
