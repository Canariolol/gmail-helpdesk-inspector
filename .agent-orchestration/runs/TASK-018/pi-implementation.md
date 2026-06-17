# TASK-018 — Docs/runtime update

Fecha: 2026-06-17

## Implementado

Actualizado `README.md`:

- IA ahora se documenta como opt-in por policy, no auditor obligatorio.
- Se añadió sección de modelo SaaS:
  - org implícita;
  - mailbox;
  - policy draft/version;
  - snapshot por run;
  - endpoints `/me/org/config`, `/me/data-summary`, `/me/operations/status`.
- Scheduler actualizado:
  - `weekdays_08_local` por timezone de tenant;
  - backfill con `as_of_date`;
  - `scheduleConfigs` sincronizado desde policy SaaS.
- Variables nuevas documentadas:
  - `APP_ENV`;
  - `RATE_LIMIT_ANALYSIS_CREATE_PER_HOUR`;
  - `RATE_LIMIT_ANALYSIS_START_PER_HOUR`.
- Caveats operacionales:
  - rate limiter in-memory;
  - AI worker stateless;
  - retention job pendiente.
- Checks apuntan a `./scripts/check-all.sh`.

## Nota

No se leyó ni editó `.env.example` por la regla de no tocar `.env.*`.
