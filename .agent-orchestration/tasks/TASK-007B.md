# TASK-007B — UI/UX configuración guiada incremental

## Estado

approved

## Owner primario

Claude

## Objetivo

Implementar una primera UI de configuración guiada para beta privada, basada en el contrato planificado de TASK-007A:

- `GET /me/org/config`
- `PUT /me/org/config`
- `POST /analysis-runs` con ventana + `policy_version_id` cuando exista

Debe ser incremental y segura: si el backend aún no responde, la app debe fallar de forma clara sin romper login/análisis legacy.

## Contexto que debes leer

- `.agent-orchestration/human-in-loop.md`
- `.agent-orchestration/decisions.md`
- `.agent-orchestration/runs/TASK-007/codex-plan.md`
- `.agent-orchestration/runs/TASK-007/claude-ui.md`
- `apps/web/src/App.tsx`
- `apps/web/src/api/client.ts`
- `apps/web/src/api/types.ts`
- `apps/web/src/components/filters/FilterBar.tsx`
- `apps/web/src/views/ResumenView.tsx`
- `apps/web/src/views/AyudaView.tsx`
- `apps/web/src/components/layout/Sidebar.tsx`
- CSS en `apps/web/src/styles/*.css`

No leas `.env`, `.env.*` reales ni `secrets/*`.

## Puede tocar

- `apps/web/src/**`
- `apps/web/package.json` solo si estrictamente necesario, pero evita nuevas dependencias.
- `.agent-orchestration/runs/TASK-007B/claude-implementation.md`

## No puede tocar

- `.env`, `.env.*`, `secrets/*`
- `apps/api/**`
- `apps/ai-worker/**`
- archivos de orquestación salvo output indicado

## Requisitos UI

1. Agregar navegación/configuración visible: `Configuración` o `Setup`.
2. Crear una vista incremental `SetupWizardView` o equivalente.
3. Consultar `GET /me/org/config` cuando haya sesión.
4. Mostrar estado de setup y policy summary.
5. Permitir editar al menos:
   - organización: nombre, timezone;
   - dominios internos;
   - criterios de solicitud válida;
   - IA opt-in con confirmación explícita;
   - retención 30/60/90;
   - reportes metrics-only por defecto.
6. Si `setup_state.ready_for_analysis=false`, mostrar banner claro en Resumen.
7. Mantener legacy `FilterBar` funcional si no hay config o si backend falla.
8. No prometer borrado irreversible, dominio Workspace verificado ni que nunca se leen cuerpos completos.
9. Mantener copy de `gmail.readonly`.
10. Ejecutar `npm --prefix apps/web run build` si tienes permiso de Bash; si no, documenta que queda pendiente.

## Output requerido

- Código frontend implementado.
- `.agent-orchestration/runs/TASK-007B/claude-implementation.md` con resumen, archivos tocados, checks y handoffs.
