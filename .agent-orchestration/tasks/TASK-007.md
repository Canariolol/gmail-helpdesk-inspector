# TASK-007 — Configuración SaaS incremental

## Estado

ready

## Owner primario

Codex/Pi para arquitectura e implementación backend. Claude para UX de configuración guiada.

## Modelo recomendado

- Codex: `gpt-5.5` con `model_reasoning_effort=high`
- Claude: Sonnet con effort max; escalar a Opus solo si justifica necesidad visual/sistémica.

## Objetivo

Diseñar un primer incremento implementable de configuración SaaS para reemplazar hardcodes de dominio, timezone, filtros, IA y scheduler/reportes, sin intentar resolver todo el modelo multi-org público aún.

El resultado debe permitir avanzar hacia beta privada con:

- 1 organización/membership implícito por usuario o owner-safe migration path.
- 1 mailbox Workspace por org en beta.
- Configuración versionable/snapshot por `AnalysisRun`.
- Políticas explícitas de análisis, IA, scheduler/reportes y retención.
- UI guiada por intención/política operacional, no checklist de keywords.
- Mantener Gmail scope `gmail.readonly`.
- No persistir cuerpos completos.

## Decisiones de producto ya aprobadas

- Beta privada para organizaciones con 1 mailbox inicial.
- IA opt-in explícita por organización.
- Retención default 30 días, configurable por admin.
- Reportes configurables, default solo métricas.
- Scheduler configurable con preset simple.
- Workspace only para beta.
- Pi debe mostrar alternativas + recomendación + impacto antes de pedir nuevas decisiones.

## Contexto que debes leer

- `.agent-orchestration/human-in-loop.md`
- `.agent-orchestration/decisions.md`
- `.agent-orchestration/saas-roadmap.md`
- `.agent-orchestration/runs/TASK-005/codex-plan.md`
- `.agent-orchestration/runs/TASK-004/pi-implementation.md`
- `README.md`
- `specs_prd.md`
- `apps/api/src/http/mod.rs`
- `apps/api/src/storage/mod.rs`
- `apps/api/src/firestore/mod.rs`
- `apps/api/src/analysis/mod.rs`
- `apps/api/src/scheduler/model.rs`
- `apps/api/src/scheduler/mod.rs`
- `apps/ai-worker/src/ai_worker/bedrock.py`
- `apps/ai-worker/src/ai_worker/schemas.py`
- `apps/web/src/components/filters/FilterBar.tsx`
- `apps/web/src/views/LoginView.tsx`
- `apps/web/src/views/AyudaView.tsx`

No leas `.env`, `.env.*` reales ni `secrets/*`.

## Puede tocar en esta pasada

Solo outputs de agente:

- `.agent-orchestration/runs/TASK-007/codex-plan.md`
- `.agent-orchestration/runs/TASK-007/claude-ui.md`

## No puede tocar en esta pasada

- `.env`
- `.env.*` reales
- `secrets/*`
- código fuente

## Output requerido para Codex

Plan en español con:

1. Recomendación de incremento técnico mínimo para TASK-007.
2. Modelo de datos concreto Rust/Firestore para policies y snapshots.
3. API endpoints mínimos y contratos JSON.
4. Estrategia de migración desde configuración actual owner-scoped.
5. Cambios necesarios en `AnalysisRun` para snapshot/versioning.
6. Cambios necesarios en scheduler/reportes y worker IA.
7. Orden de implementación con archivos tocados.
8. Tests requeridos.
9. Riesgos de seguridad/privacidad/IDOR.
10. `BLOCKED_QUESTIONS` solo si hay decisiones realmente necesarias; cada pregunta debe traer opciones, recomendación e impacto.

## Output requerido para Claude

Plan UX en español con:

1. Flujo de configuración guiada para beta privada.
2. Pantallas/componentes mínimos para el primer incremento.
3. Microcopy de privacidad/IA/retención/reportes.
4. Cómo evitar que sea una lista plana de checkboxes/keywords.
5. Estados vacíos, errores y confirmaciones.
6. Dependencias de contrato/API que debe entregar backend.
7. Riesgos de promesas UX que requieran confirmación legal/producto.
8. `BLOCKED_QUESTIONS` solo si hay decisiones realmente necesarias; cada pregunta debe traer opciones, recomendación e impacto.

## Criterios de aceptación

- [ ] Mantiene `gmail.readonly`.
- [ ] IA queda opt-in explícita.
- [ ] Retención 30 días default configurable.
- [ ] Reportes default solo métricas.
- [ ] Scheduler configurable con preset simple.
- [ ] Workspace only para beta.
- [ ] Propone implementación incremental, no big-bang SaaS público.
- [ ] Incluye versionado/snapshot por run.
- [ ] No propone persistir cuerpos completos.
- [ ] Respeta anti-enumeración `404` para recursos ajenos.
