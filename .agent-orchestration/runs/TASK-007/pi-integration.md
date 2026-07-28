# TASK-007 — Integración Pi de planes Codex + Claude

Fecha: 2026-06-17

## Ejecución de agentes

- Codex planner ejecutado con `gpt-5.5` y reasoning high.
- Claude UI ejecutado con Sonnet en pasada enfocada. La primera ejecución pesada con `--effort max` quedó colgada; se reemplazó por una ejecución directa más acotada que produjo output completo.
- No se leyó `.env`, `.env.*`, `secrets/*`, tokens ni credenciales.

Outputs:

- `.agent-orchestration/runs/TASK-007/codex-plan.md`
- `.agent-orchestration/runs/TASK-007/claude-ui.md`

## Síntesis

Ambos agentes convergen en un incremento beta, no big-bang SaaS público:

- Organización implícita por usuario autenticado.
- 1 mailbox Google Workspace por organización en beta.
- `PolicyDraft` mutable + `PolicyVersion` inmutable.
- `AnalysisRun` creado desde policy vigente y con snapshot inmutable.
- IA opt-in explícita; off por defecto.
- Retención default 30 días configurable.
- Scheduler configurable con preset simple.
- Reportes default solo métricas.
- UI guiada por intención operacional, no checklist de filtros.

## Alternativas de implementación

### Opción A — Big-bang SaaS completo

Implementar orgs, memberships multi-rol, tenant-scoped Firestore completo, wizard, scheduler, IA y retention jobs en un solo bloque.

- Ventaja: arquitectura ideal más limpia.
- Riesgo: alto blast radius, más probabilidad de romper análisis actual, más difícil revisar seguridad.
- Recomendación: no usar ahora.

### Opción B — Backend policy-first incremental

Primero implementar modelos/policies/snapshots y endpoints `/me/org/config`; mantener compatibilidad legacy y después adaptar UI.

- Ventaja: crea base auditable y segura para SaaS; reduce hardcodes desde la fuente de verdad.
- Riesgo: UI temporal seguirá usando `FilterBar` hasta el siguiente paso.
- Recomendación: sí, como siguiente implementación.

### Opción C — UI wizard-first con mocks/contrato parcial

Diseñar wizard y componentes usando datos simulados mientras backend se implementa.

- Ventaja: feedback visual rápido.
- Riesgo: puede prometer capacidades no implementadas; requiere retrabajo si contrato cambia.
- Recomendación: no como prioridad; hacer después de B o en paralelo solo si contrato queda congelado.

### Opción D — Híbrida acotada

Implementar backend mínimo `/me/org/config` + adaptar UI a leer policy para `FilterBar` sin wizard completo todavía.

- Ventaja: elimina hardcodes visibles rápido y permite runs con snapshot.
- Riesgo: experiencia aún no será la UI guiada ideal.
- Recomendación: alternativa aceptable si queremos valor rápido con menos frontend.

## Recomendación Pi

Avanzar con **Opción B**, en subtask `TASK-007A`:

1. Agregar modelos de policies/org config en API.
2. Agregar storage memory/firestore para org implícita, membership, mailbox, policy draft/version.
3. Agregar `GET/PUT /me/org/config`.
4. Cambiar `POST /analysis-runs` para aceptar ventana + policy vigente y guardar snapshot.
5. Mantener compatibilidad legacy para runs existentes.
6. Tests backend P0.

Después abrir `TASK-007B` para UI wizard o adaptación incremental del `FilterBar` contra policy.

## Decisión sugerida

No hay preguntas bloqueantes de producto nuevas. La única decisión operativa es el camino de implementación.

Recomendación: aprobar **Opción B** como siguiente paso, con alcance backend-first y sin tocar IA worker/scheduler completo hasta que policy snapshot esté estable.
