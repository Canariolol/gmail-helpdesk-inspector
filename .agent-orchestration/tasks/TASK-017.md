# TASK-017 — AI worker policy injection y eliminación de hardcodes

## Estado

done

## Owner primario

Pi

## Objetivo

Eliminar contexto de empresa original del AI worker y hacer que el auditor IA use contexto operacional enviado por la API desde el `PolicySnapshot` del run.

## Criterios

- [x] No hay defaults tenant/company específicos en worker.
- [x] API envía `policy_context` por audit request cuando hay policy snapshot.
- [x] Worker mantiene fallback genérico para compatibilidad.
- [x] Tests AI worker OK.
- [x] API tests/clippy OK.
