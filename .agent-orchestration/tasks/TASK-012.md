# TASK-012 — Scheduler V2 policy-first incremental

## Estado

done

## Owner primario

Pi

## Modelo recomendado

- Codex: n/a
- Claude: n/a

## Objetivo

Acercar el scheduler a producción usando la configuración SaaS/policy implementada en TASK-007, sin abordar todavía borrado, desconexión OAuth ni retención destructiva.

## Contexto

- Scheduler legacy usaba `scheduleConfigs` con dominios/filtros hardcodeables.
- TASK-007 introdujo `PolicyDraft`, `PolicyVersion` y snapshot por run.
- Decisión de producto: disconnect/delete no son prioridad ahora; scheduler sí.

## Puede tocar

- `apps/api/src/scheduler/**`
- `apps/api/src/http/**`
- `.agent-orchestration/**`

## No puede tocar

- `.env`
- `.env.*`
- `secrets/*`

## Criterios de aceptación

- [x] Al guardar `/me/org/config`, scheduler legacy se sincroniza desde policy.
- [x] Runs programados con org config guardan `PolicySnapshot`.
- [x] Se conserva compatibilidad legacy/env seed.
- [x] Reportes respetan `report_content` metrics-only/redacción.
- [x] Tests backend OK.
- [x] Clippy OK.

## Handoff

Siguientes mejoras: ventana/hora configurable por timezone/preset real, observabilidad de scheduler en UI, y job runner persistente si hay más de una instancia.
