# TASK-005 — Configuración SaaS parametrizable + política del auditor IA

## Estado

ready

## Owner primario

Codex

## Modelo recomendado

- Codex: `gpt-5.5`
- Esfuerzo: high

## Objetivo

Diseñar el modelo robusto de configuración por usuario/organización para reemplazar los hardcodes de la empresa original y permitir que el sistema funcione como SaaS. La configuración debe capturar política operacional y comportamiento del auditor IA, no limitarse a fechas, keywords y checkboxes.

## Contexto que debes leer

- `.agent-orchestration/human-in-loop.md`
- `.agent-orchestration/decisions.md`
- `.agent-orchestration/saas-roadmap.md`
- `.agent-orchestration/runs/TASK-001/codex-plan.md`
- `.agent-orchestration/runs/TASK-002/claude-ui.md`
- `specs_prd.md`
- `README.md`
- `apps/api/src/analysis/mod.rs`
- `apps/api/src/scheduler/model.rs`
- `apps/ai-worker/src/ai_worker/bedrock.py`
- `apps/ai-worker/src/ai_worker/schemas.py`
- `apps/web/src/components/filters/FilterBar.tsx`

No leas `.env` ni `secrets/*`.

## Puede tocar

Primera pasada solo output en:

- `.agent-orchestration/runs/TASK-005/codex-plan.md`

## No puede tocar

- `.env`
- `secrets/*`
- código fuente en esta pasada

## Output requerido

Plan en español con:

1. Inventario de hardcodes/assumptions actuales que impiden SaaS.
2. Propuesta de entidades/configuración: organización, mailbox, analysis policy, AI auditor policy, schedule/report policy, retention policy.
3. Qué debe versionarse por `AnalysisRun` para trazabilidad.
4. Defaults seguros para beta privada.
5. Qué debe ser configurable por UI vs avanzado/admin.
6. Cómo evitar que la UI sea solo una lista de checkboxes/keywords.
7. Cambios backend/API/storage necesarios.
8. Cambios worker IA necesarios para eliminar contexto hardcoded.
9. Handoff para Claude: experiencia de configuración guiada.
10. `BLOCKED_QUESTIONS` para Pi/dueño del producto con opciones y recomendación.

## Criterios de aceptación

- [ ] Identifica hardcodes de empresa, dominio, timezone, contexto IA y reportes.
- [ ] Propone modelo extensible pero implementable incrementalmente.
- [ ] Mantiene `gmail.readonly`.
- [ ] No propone persistir cuerpos completos.
- [ ] Incluye preguntas explícitas para el dueño del producto.
- [ ] Diferencia MVP endurecido, beta privada y SaaS público.

## Handoff esperado

Un bloque `Handoff para Pi/Claude` indicando decisiones que deben confirmarse antes de diseñar o implementar la UI de configuración.
