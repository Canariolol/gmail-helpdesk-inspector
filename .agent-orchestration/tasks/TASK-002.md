# TASK-002 — Brief UX público SaaS

## Estado

ready

## Owner primario

Claude

## Modelo recomendado

- Primera pasada: Sonnet con esfuerzo max/high.
- Escalar a Opus max solo si se requiere refinamiento visual avanzado o si Pi detecta falta de calidad.

## Objetivo

Diseñar la experiencia inicial de SaaS público para usuarios que conectan Gmail y necesitan métricas confiables de helpdesk sin convertirse en un sistema de tickets.

## Contexto que debes leer

- `README.md`
- `specs_prd.md`
- `.agent-orchestration/roles.md`
- `.agent-orchestration/path-ownership.md`
- `.agent-orchestration/runs/TASK-001/codex-plan.md` si existe; si no existe, trabajar con PRD/README y marcar supuestos.
- `apps/web/src/views/**`
- `apps/web/src/components/**`
- `apps/web/src/styles/**`

No leas `.env` ni `secrets/*`.

## Puede tocar

Solo output en:

- `.agent-orchestration/runs/TASK-002/claude-ui.md`

Esta task es read-only sobre el código.

## No puede tocar

- `.env`
- `secrets/*`
- código fuente en esta primera pasada
- contratos API/backend

## Output requerido

Genera un brief UX en español con estas secciones:

1. Posicionamiento del producto en una frase.
2. Usuario objetivo y jobs-to-be-done.
3. Flujo ideal: landing/login → autorización Google → configuración → análisis → dashboard → revisión manual → reporte.
4. Mapa de pantallas y componentes.
5. Estados vacíos/loading/error/success.
6. Microcopy crítico, especialmente privacidad y Gmail readonly.
7. Recomendaciones visuales para dashboard auditable.
8. Mejoras concretas sobre la UI existente.
9. Accesibilidad y responsive.
10. Dependencias/preguntas para Codex/Pi.
11. Si conviene o no escalar a Opus, y por qué.

## Criterios de aceptación

- [ ] Refuerza confianza, privacidad y trazabilidad.
- [ ] No promete funciones que el backend no soporta.
- [ ] Explica cómo navegar desde métricas a hilos.
- [ ] Incluye estados operativos reales.
- [ ] Puede convertirse luego en tareas de implementación.

## Handoff esperado

Un bloque final `Handoff para Pi/Codex` con cambios sugeridos, riesgos y dependencias técnicas.
