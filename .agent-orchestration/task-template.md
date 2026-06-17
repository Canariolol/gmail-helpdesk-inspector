# TASK-XXX — Título

## Estado

backlog | ready | running | blocked | review | done

## Owner primario

Pi | Codex | Claude

## Modelo recomendado

- Codex: `gpt-5.5-high` / cheaper / n/a
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

## Criterios de aceptación

- [ ] Resultado cubre el objetivo.
- [ ] Lista paths impactados.
- [ ] Identifica riesgos.
- [ ] No toca secretos.
- [ ] Incluye próximos pasos verificables.

## Handoff

Qué necesita saber el siguiente agente.
