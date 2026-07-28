# TASK-023 — Observability v2 / historial operativo

Fecha: 2026-06-17

## Delegación Claude Code

- Plan read-only inicial con herramientas quedó colgado y fue cerrado.
- Se relanzó Claude sin herramientas con contexto resumido.
- Output: `.agent-orchestration/runs/TASK-023/claude-plan-v2.md`.
- No se usó Codex.

## Implementado

Backend:

- Nuevo endpoint autenticado `GET /me/operations/history`.
- Devuelve historial corto derivado de runs recientes y último `ScheduleState` si no está representado.
- No almacena datos nuevos sensibles.
- No expone cuerpos de correo ni error crudo; usa `redact_public_error` y `error_category`.
- Soporta `limit` y `page_token` usando la misma semántica básica de paginación.

Frontend:

- Nuevos tipos `OperationsHistory` / `OperationsHistoryEntry`.
- `ConfiguracionView` consulta `/me/operations/history?limit=8`.
- Nuevo panel “Historial operativo” bajo “Operación automática”.

Tests:

- `operations_history_lists_recent_runs_without_sensitive_details`.

## Archivos

- `apps/api/src/http/mod.rs`
- `apps/web/src/api/types.ts`
- `apps/web/src/views/ConfiguracionView.tsx`
- `apps/web/src/styles/components.css`
