# TASK-XXX — Título

## Estado

backlog | ready | running | blocked | review | done

## Owner primario

Pi | Codex | Claude

## Modelo recomendado

- Codex: `gpt-5.5` high reasoning / cheaper / n/a
- Claude: `sonnet` / `opus` / n/a
- Esfuerzo: low | medium | high | max

## Objetivo

Qué resultado concreto debe producirse.

## Contexto

Archivos/documentos que puede leer el agente.

## Puede tocar

- `path/**`

## No puede tocar

- `.env`
- `secrets/*`
- paths fuera de alcance

## Inputs requeridos

- docs
- decisiones previas
- outputs de otros agentes

## Output requerido

Guardar en `.agent-orchestration/runs/TASK-XXX/`:

- `codex-plan.md` o `claude-ui.md`
- riesgos
- criterios de aceptación
- handoff
- `BLOCKED_QUESTIONS` si hay ambigüedades que requieran decisión de Pi/dueño
- `ASSUMPTIONS` si se avanza con supuestos no críticos

## Criterios de aceptación

- [ ] Resultado cubre el objetivo.
- [ ] Lista paths impactados.
- [ ] Identifica riesgos.
- [ ] No toca secretos.
- [ ] Incluye próximos pasos verificables.

## Preguntas / decision gates

- ¿Qué debe decidir Pi antes de implementar?
- ¿Qué debe preguntarse al dueño del producto?
- ¿Qué supuestos serían peligrosos?

## Handoff

Qué necesita saber el siguiente agente.
