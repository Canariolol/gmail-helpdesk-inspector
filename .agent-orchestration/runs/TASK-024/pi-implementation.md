# TASK-024 — Paginación básica de runs/threads

Fecha: 2026-06-17

## Delegación Claude Code

- Plan read-only inicial con herramientas quedó colgado y fue cerrado.
- Se relanzó Claude sin herramientas con contexto resumido.
- Output: `.agent-orchestration/runs/TASK-024/claude-plan-v2.md`.
- No se usó Codex.

## Implementado

Backend compatible:

- `GET /analysis-runs` mantiene respuesta legacy array si no hay query de paginación.
- `GET /analysis-runs?limit=N&page_token=T` devuelve wrapper:
  - `items`
  - `next_page_token`
  - `total_count`
- `GET /analysis-runs/:id/threads` mantiene respuesta legacy array si no hay query de paginación.
- `GET /analysis-runs/:id/threads?limit=N&page_token=T` devuelve el mismo wrapper.
- `limit` se limita a rango `1..=200`.
- `page_token` es opaco base64-url de offset, suficiente para beta.
- Ownership/authz existentes se preservan antes de listar threads.

Tests:

- `list_analysis_runs_legacy_returns_array_without_pagination_query`.
- `paginated_analysis_runs_and_threads_return_wrapper`.

## Nota

La UI actual sigue consumiendo arrays legacy. Botón “Cargar más” queda como mejora posterior si el volumen beta lo requiere.
