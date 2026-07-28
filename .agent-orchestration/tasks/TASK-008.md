# TASK-008 — UI centro de privacidad y datos

## Estado

ready

## Owner primario

Claude

## Modelo recomendado

- Claude: `sonnet`
- Esfuerzo: max

## Objetivo

Implementar una primera versión UI/UX de un centro de privacidad/datos dentro de la app, orientado a beta privada Workspace-only, sin asumir endpoints destructivos todavía.

Debe mejorar confianza y claridad sobre:

- Scope Gmail `gmail.readonly`.
- Qué datos usa la app.
- IA opt-in y minimización.
- Retención configurada.
- Futuras acciones: desconectar Gmail, borrar análisis, borrar datos/cuenta.

Importante: las acciones destructivas deben quedar como placeholders seguros o estados "próximamente / requiere confirmación" si no hay contrato backend confirmado. No inventar endpoints.

## Contexto

Leer:

- `.agent-orchestration/roles.md`
- `.agent-orchestration/path-ownership.md`
- `.agent-orchestration/human-in-loop.md`
- `.agent-orchestration/decisions.md`
- `.agent-orchestration/runs/TASK-007/pi-implementation.md`
- `.agent-orchestration/runs/TASK-007B/claude-implementation.md`
- `apps/web/src/App.tsx`
- `apps/web/src/api/types.ts`
- `apps/web/src/views/ConfiguracionView.tsx`
- `apps/web/src/views/AyudaView.tsx`
- `apps/web/src/components/layout/Sidebar.tsx`
- `apps/web/src/styles/components.css`
- `apps/web/src/styles/layout.css`

## Puede tocar

- `apps/web/src/**`
- `.agent-orchestration/runs/TASK-008/**`

## No puede tocar

- `.env`
- `.env.*`
- `secrets/*`
- `apps/api/**`
- `apps/ai-worker/**`
- paths fuera de alcance

## Inputs requeridos

- TASK-007 backend contract ya implementado:
  - `GET /me/org/config`
  - `PUT /me/org/config`
- No existen todavía endpoints backend confirmados para revoke/delete.

## Output requerido

Guardar o actualizar `.agent-orchestration/runs/TASK-008/claude-implementation.md` con:

- resumen;
- archivos tocados;
- UX decisions;
- riesgos;
- checks;
- `BLOCKED_QUESTIONS` si hay decisiones destructivas o legales;
- handoff backend requerido.

## Criterios de aceptación

- [ ] No se prometen capacidades que el backend no tenga.
- [ ] No se inventan endpoints destructivos.
- [ ] UI deja claro que Gmail es readonly.
- [ ] UI deja claro que desconectar/borrar datos requiere confirmación.
- [ ] Accesible, consistente con diseño actual.
- [ ] `npm --prefix apps/web run build` pasa.

## Preguntas / decision gates

Si una decisión requiere dueño, documentar opciones + recomendación + impacto, especialmente:

- ¿Desconectar Gmail conserva o elimina análisis históricos?
- ¿Borrar análisis borra manual reviews y AI audits?
- ¿Borrar cuenta/org en beta debe requerir confirmación por email?

## Handoff

El backend necesitará implementar endpoints reales en una tarea posterior, después de aprobar semántica exacta de desconexión/borrado.
