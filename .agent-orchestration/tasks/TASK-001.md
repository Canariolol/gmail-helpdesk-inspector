# TASK-001 — Auditoría actual + plan SaaS público

## Estado

ready

## Owner primario

Codex

## Modelo recomendado

- Codex: `gpt-5.5` o superior
- Esfuerzo: high/xhigh según disponibilidad

## Objetivo

Auditar el estado actual de `gmail-helpdesk-inspector` y proponer un roadmap para convertirlo en SaaS público con foco en calidad, seguridad, privacidad y diferenciación producto.

## Contexto que debes leer

- `README.md`
- `specs_prd.md`
- `docs/privacy.md`
- `docs/google-oauth.md`
- `docs/firestore.md`
- `docs/gcp-deploy.md`
- `apps/api/src/**`
- `apps/ai-worker/src/**`
- `apps/web/src/api/**`
- `apps/web/src/lib/**`

No leas `.env` ni `secrets/*`.

## Puede tocar

Solo output en:

- `.agent-orchestration/runs/TASK-001/codex-plan.md`

Esta task es read-only sobre el código.

## No puede tocar

- `.env`
- `.env.*` con valores reales
- `secrets/*`
- código fuente
- configs de deploy

## Output requerido

Genera un plan en español con estas secciones:

1. Resumen ejecutivo del estado actual.
2. Gaps críticos para SaaS público.
3. Riesgos de seguridad/privacidad/compliance.
4. Riesgos técnicos/arquitectura.
5. Roadmap por fases: hardening MVP, beta privada, beta pública, SaaS público.
6. Tareas priorizadas con IDs sugeridos.
7. Qué debe hacer Claude UI/UX después.
8. Qué no conviene hacer todavía.
9. Checks/tests recomendados.
10. Preguntas abiertas para el dueño del producto.

## Criterios de aceptación

- [ ] No propone ampliar scopes Gmail salvo justificación extrema.
- [ ] Prioriza seguridad y trazabilidad antes que features cosméticas.
- [ ] Incluye tareas concretas y ordenadas.
- [ ] Distingue MVP open-source actual vs SaaS público.
- [ ] Explicita dependencias para UI/UX.

## Handoff esperado

Un bloque final `Handoff para Claude` con límites claros para el brief UI/UX de TASK-002.
