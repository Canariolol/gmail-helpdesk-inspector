# TASK-013 — Scheduler timezone/preset real por tenant

Fecha: 2026-06-17

## Implementado

- `scheduler::window` ahora calcula ventanas due por timezone IANA usando `due_window_for(now_utc, timezone)`.
- El scheduler interno ya no depende de un único `America/Santiago`; evalúa cada `ScheduleConfig.timezone` en cada tick.
- `run_due_scheduled_analysis` corre solo configs cuyo timezone local ya llegó a weekdays 08:00.
- `/internal/scheduled-analysis` mantiene backfill con `as_of_date`; sin `as_of_date` usa modo due multi-timezone.
- Se preserva compatibilidad legacy/env seed.

## Tests

Agregados tests de timezone/DST:

- `due_window_uses_configured_timezone`
- `next_fire_time_label_skips_weekends`

## Checks

- `cargo test --manifest-path apps/api/Cargo.toml` → 66 OK.
- `cargo clippy --manifest-path apps/api/Cargo.toml --all-targets -- -D warnings` → OK.

## Límites

- El único preset funcional sigue siendo `weekdays_08_local`.
- Multi-instancia aún depende de idempotencia por `ScheduleState`; claim transaccional Firestore queda siguiente hardening.
