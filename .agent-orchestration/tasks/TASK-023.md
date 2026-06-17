# TASK-023 — Observability v2 / historial corto + categorías de error

## Estado

done

## Owner primario

Claude Code / Pi orchestration

## Objetivo

Diseñar e implementar una mejora incremental de operación beta: historial corto de scheduler/runs, categorías de error no sensibles y UI para diagnosticar fallos sin inspeccionar datos sensibles.

## Resultado

- Plan Claude: `.agent-orchestration/runs/TASK-023/claude-plan-v2.md`
- Implementación: `.agent-orchestration/runs/TASK-023/pi-implementation.md`

## Criterios

- [x] No usar Codex.
- [x] No leer `.env`, `.env.*`, `secrets/*`.
- [x] Nuevo endpoint seguro `/me/operations/history`.
- [x] UI de historial en Configuración.
- [x] Categorías de error redacted.
- [x] Tests OK.
