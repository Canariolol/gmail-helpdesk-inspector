# Agent Orchestration

Este directorio es el control plane local para coordinar Pi, Codex CLI y Claude Code sin depender de las extensiones visuales de VSCode.

## Principio operativo

- **Pi** mantiene el tablero, crea tareas, integra resultados y protege el repo.
- **Codex** planifica primero cuando hay decisiones de producto, arquitectura, seguridad, datos o backend.
- **Claude** diseña e implementa UI/UX cuando la tarea afecta experiencia visual, flujos, componentes o microcopy.
- Los agentes no compiten por los mismos archivos. Cada tarea declara ownership explícito.
- Ningún agente debe leer, copiar, resumir ni modificar `.env`, `secrets/*`, credenciales o tokens.
- Todos los agentes deben seguir `.agent-orchestration/human-in-loop.md`: ante dudas, deben preguntar a Pi; Pi pregunta al dueño del producto cuando la decisión afecte producto, seguridad, privacidad o SaaS.

## Flujo estándar

1. Pi crea o actualiza `.agent-orchestration/tasks/TASK-XXX.md`.
2. Codex/Claude producen plan/review en `.agent-orchestration/runs/TASK-XXX/`.
3. Si el output contiene `BLOCKED_QUESTIONS`, Pi consolida opciones y pregunta al dueño del producto antes de implementar.
4. Pi valida el plan y decide si se requiere UI/UX, backend o revisión cruzada.
5. Claude/Codex implementan solo con paths y criterios aprobados.
6. Codex revisa seguridad/integración si aplica.
7. Pi aplica cambios, ejecuta checks y actualiza `board.md`/`decisions.md`.

## Comandos locales

Plan Codex read-only:

```bash
CODEX_PLANNER_MODEL="gpt-5.5" CODEX_REASONING_EFFORT="high" ./scripts/agents/dispatch-codex-planner.sh TASK-001
```

Diseño Claude read-only:

```bash
CLAUDE_UI_MODEL="sonnet" CLAUDE_UI_EFFORT="max" ./scripts/agents/dispatch-claude-ui.sh TASK-002
```

Para escalar UI a Opus solo cuando haga falta:

```bash
CLAUDE_UI_MODEL="opus" CLAUDE_UI_EFFORT="max" ./scripts/agents/dispatch-claude-ui.sh TASK-002
```

## Política de paralelismo

Por defecto usamos máximo:

- 1 sesión Codex planner/reviewer.
- 1 sesión Claude UI/UX.
- Pi como integrador.

Se permite abrir más instancias solo si:

- Las tareas son independientes.
- Los paths permitidos no se solapan.
- Hay criterios de aceptación cerrados.
- Existe un handoff claro.
- El beneficio supera el riesgo de divergencia.

Ejemplos donde sí puede valer paralelizar:

- Codex audita seguridad backend mientras Claude rediseña dashboard.
- Codex prepara estrategia SaaS mientras Claude propone flujos de onboarding.
- Un agente genera tests sobre un módulo estable mientras otro trabaja docs.

Ejemplos donde no:

- Dos agentes editando `apps/web/src/App.tsx`.
- Dos agentes cambiando contratos API/frontend a la vez.
- Refactors transversales sin plan aprobado.

## Routing por modelo/costo

- **Codex GPT-5.5 con reasoning high/xhigh**: planificación, arquitectura, seguridad, migraciones, launch readiness, reviews finales.
- **Codex cheaper/lower reasoning**: tareas mecánicas, docs simples, generación inicial de tests sobre plan aprobado.
- **Claude Sonnet max/xhigh**: primera propuesta UI/UX, wireframes textuales, componentes frontend regulares, microcopy.
- **Claude Opus max/xhigh**: diseño visual complejo, dashboard crítico, accesibilidad avanzada, refinamiento premium, revisión final de UX pública.

La regla es gastar caro en decisiones irreversibles o altamente sensibles; gastar barato en ejecución acotada y reversible.
