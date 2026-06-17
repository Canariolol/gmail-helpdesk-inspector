# TASK-013 — Scheduler timezone/preset real por tenant

## Estado

ready

## Owner primario

Pi

## Objetivo

Hacer que el scheduler use timezone/preset por tenant/policy en vez de depender solo de America/Santiago global. Mantener compatibilidad con endpoint interno y tests existentes.

## Puede tocar

- `apps/api/src/scheduler/**`
- `apps/api/src/http/internal.rs`
- `.agent-orchestration/runs/TASK-013/**`

## No puede tocar

- `.env`, `.env.*`, `secrets/*`

## Criterios

- [ ] Scheduler interno calcula due por config timezone.
- [ ] Preset `weekdays_08_local` funciona con DST/timezone IANA.
- [ ] Compat legacy/external se mantiene.
- [ ] Tests OK.
