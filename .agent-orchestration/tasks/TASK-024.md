# TASK-024 — Paginación básica de runs/threads

## Estado

done

## Owner primario

Claude Code / Pi orchestration

## Objetivo

Diseñar e implementar paginación incremental para runs y threads, suficiente para beta privada, evitando cargas ilimitadas en UI/API.

## Resultado

- Plan Claude: `.agent-orchestration/runs/TASK-024/claude-plan-v2.md`
- Implementación: `.agent-orchestration/runs/TASK-024/pi-implementation.md`

## Criterios

- [x] No usar Codex.
- [x] No leer `.env`, `.env.*`, `secrets/*`.
- [x] Contrato paginado opt-in.
- [x] Compatibilidad con arrays legacy sin query params.
- [x] Authz/ownership preservado.
- [x] Tests OK.
