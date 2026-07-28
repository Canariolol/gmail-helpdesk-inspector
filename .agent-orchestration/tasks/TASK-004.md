# TASK-004 — UI quick wins de confianza

## Estado

ready

## Owner primario

Claude

## Modelo recomendado

- Primera implementación: Sonnet con esfuerzo high/max.
- Escalar a Opus solo si Pi considera insuficiente la calidad visual.

## Objetivo

Preparar cambios frontend de bajo riesgo que aumenten confianza antes de pedir acceso Gmail: LoginView profesional, PrivacyCallout, OAuthExplainModal, mejoras de accesibilidad en Sidebar y sección privacidad en AyudaView.

## Contexto que debes leer

- `.agent-orchestration/runs/TASK-002/claude-ui.md`
- `apps/web/src/views/LoginView.tsx`
- `apps/web/src/views/AyudaView.tsx`
- `apps/web/src/components/layout/Sidebar.tsx`
- `apps/web/src/styles/**`
- `apps/web/src/App.tsx`

No leas `.env` ni `secrets/*`.

## Puede tocar

En implementación aprobada por Pi:

- `apps/web/src/views/LoginView.tsx`
- `apps/web/src/views/AyudaView.tsx`
- `apps/web/src/components/layout/Sidebar.tsx`
- `apps/web/src/components/common/**`
- `apps/web/src/styles/**`

## No puede tocar

- `.env`
- `secrets/*`
- `apps/api/**`
- `apps/ai-worker/**`
- contratos API
- dependencias npm sin aprobación explícita

## Output requerido

Primera pasada debe producir plan/patch summary en:

- `.agent-orchestration/runs/TASK-004/claude-ui.md`

Si se autoriza implementación después, incluir:

1. Archivos modificados.
2. Estados UI cubiertos.
3. Riesgos.
4. Check `npm --prefix apps/web run build`.

## Criterios de aceptación

- [ ] Login comunica `gmail.readonly`, no modificación de Gmail, tokens cifrados y revocabilidad.
- [ ] El CTA es profesional.
- [ ] Modal previo OAuth no cambia el endpoint de login.
- [ ] Sidebar tiene `aria-current`.
- [ ] AyudaView incluye privacidad/datos.
- [ ] No se agregan dependencias.

## Handoff esperado

Un bloque `Handoff para Pi/Codex` con cualquier dependencia backend detectada.
