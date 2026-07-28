# TASK-014 — Observabilidad scheduler/run production-ready

## Estado

ready

## Owner primario

Pi + Claude

## Objetivo

Exponer estado operativo útil para beta: último scheduler state, próximo intento aproximado, errores redacted, estado config, policy version, y UI simple para verlo.

## Puede tocar

- Backend: `apps/api/src/http/**`, `apps/api/src/scheduler/**`, `apps/api/src/storage/**`
- Frontend: `apps/web/src/**`
- `.agent-orchestration/runs/TASK-014/**`

## No puede tocar

- `.env`, `.env.*`, `secrets/*`

## Criterios

- [ ] Endpoint read-only requiere sesión.
- [ ] No expone tokens/secrets/errores crudos sensibles.
- [ ] UI visible desde configuración/privacidad o vista dedicada.
- [ ] Tests/build OK.
