# TASK-015 — Rate limiting para análisis manuales

## Estado

ready

## Owner primario

Pi / Codex review

## Objetivo

Agregar rate limiting incremental para prevenir abuso accidental en beta: creación/inicio de análisis manuales por usuario, con respuesta 429 clara. Mantener tests y compatibilidad local.

## Puede tocar

- `apps/api/src/http/**`
- `apps/api/src/config/**`
- `.agent-orchestration/runs/TASK-015/**`

## No puede tocar

- `.env`, `.env.*`, `secrets/*`

## Criterios

- [ ] Limita creación/start manual por usuario.
- [ ] No bloquea scheduler interno.
- [ ] Configurable por env con defaults seguros para beta.
- [ ] Tests OK.
