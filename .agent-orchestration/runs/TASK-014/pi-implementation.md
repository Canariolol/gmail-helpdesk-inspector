# TASK-014 — Observabilidad scheduler/run production-ready

Fecha: 2026-06-17

## Agentes

- Claude produjo plan UI en `.agent-orchestration/runs/TASK-014/claude-ui-plan.md`.
- Pi implementó backend + UI ligera.

## Backend

Nuevo endpoint read-only:

- `GET /me/operations/status`

Requiere sesión y devuelve:

- scheduler enabled/timezone/preset/recipients_count;
- next_run_estimate;
- last ScheduleState;
- last_error_redacted;
- policy org/version/hash/setup.

No expone tokens ni secretos. Error visible se redacted/trunca.

## Frontend

- `ConfiguracionView` consulta `/me/operations/status`.
- Muestra panel "Operación automática" con:
  - activo/desactivado;
  - timezone;
  - próximo intento;
  - destinatarios;
  - policy version;
  - última ventana/estado/run parcial;
  - último error redacted.

## Checks

- API tests 66 OK.
- Clippy OK.
- Web build OK.

## Límites

- Observabilidad agregada es por usuario/mailbox actual.
- No hay aún historial completo de ejecuciones scheduler, solo último `ScheduleState`.
