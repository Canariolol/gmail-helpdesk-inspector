# TASK-003 — P0 authz/ownership backend

## Estado

ready

## Owner primario

Codex

## Modelo recomendado

- Codex: `gpt-5.5`
- Esfuerzo: high

## Objetivo

Planificar la implementación segura de `GHMI-SEC-001` y `GHMI-SEC-002`: todas las rutas que exponen runs, metrics, threads, events y manual review deben validar sesión y ownership. Deben existir tests IDOR con dos usuarios.

## Contexto que debes leer

- `.agent-orchestration/runs/TASK-001/codex-plan.md`
- `.agent-orchestration/runs/TASK-002/claude-ui.md`
- `apps/api/src/http/mod.rs`
- `apps/api/src/firestore/mod.rs`
- `apps/api/src/storage/mod.rs`
- `apps/api/src/analysis/mod.rs`

No leas `.env` ni `secrets/*`.

## Puede tocar

Primera pasada solo output en:

- `.agent-orchestration/runs/TASK-003/codex-plan.md`

## No puede tocar

- `.env`
- `secrets/*`
- código fuente en esta pasada

## Output requerido

Plan en español con:

1. Inventario exacto de endpoints afectados.
2. Modelo de ownership mínimo para MVP mono-usuario endurecido.
3. Cambios de StorageRepository necesarios.
4. Tests unit/integration propuestos con dos usuarios.
5. Orden de implementación de menor riesgo.
6. Riesgos de breaking changes frontend.
7. Diff plan por archivo.
8. Criterios de aceptación.

## Criterios de aceptación

- [ ] Cubre `GET /analysis-runs/:id`, metrics, threads, events, `GET /threads/:thread_id`, `PATCH /threads/:thread_id/manual-review`.
- [ ] Incluye pruebas IDOR.
- [ ] No requiere multi-tenant completo todavía, pero no bloquea evolución a tenant.
- [ ] No cambia scopes Gmail.

## Handoff esperado

Un bloque `Handoff para Pi` indicando si conviene implementar directamente o pedir una segunda revisión.
