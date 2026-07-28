# TASK-012 — Scheduler V2 policy-first incremental

Fecha: 2026-06-17

## Implementado

### Sincronización policy → scheduler

`PUT /me/org/config` ahora sincroniza `scheduleConfigs` desde la policy SaaS:

- `enabled` ← `draft.schedule_report_policy.scheduler_enabled`
- `recipients` ← `draft.schedule_report_policy.report_recipients`
- `timezone` ← `draft.schedule_report_policy.timezone`
- dominios/filtros ← `draft.analysis_policy`
- `gmail_max_threads` ← `draft.analysis_policy.max_threads_per_run`

Esto permite que el scheduler existente siga usando `scheduleConfigs`, pero alimentado por configuración SaaS en vez de hardcodes.

### Runs programados con snapshot

`create_scheduled_run` ahora:

- busca org config por `user_email`;
- si existe y scheduler está enabled:
  - valida `setup_state.ready_for_analysis`;
  - crea run con `org_id`, `mailbox_id`, `policy_version_id`, `policy_hash`, `policy_snapshot`, `gmail_scope_snapshot`, `retention_expires_at`;
  - usa `default_time_from/default_time_to`, timezone, dominios y filtros desde la snapshot.
- si no hay org config, conserva fallback legacy/env seed.
- si existe org config pero scheduler está disabled, no cae a legacy accidentalmente.

### Reportes respetan privacidad de policy

Para scheduled runs con policy snapshot:

- `metrics_only` no incluye review items con asunto/remitente.
- `metrics_and_review_items` incluye items, pero redacted si:
  - `include_subjects=false` → "Asunto oculto por política de reporte";
  - `include_senders=false` → "Remitente oculto por política de reporte".

Legacy scheduled runs conservan el comportamiento anterior.

## Tests agregados

- `scheduled_policy_run_uses_current_snapshot`
- `scheduled_policy_report_respects_metrics_only_mode`
- `scheduled_policy_report_redacts_subjects_and_senders_when_disabled`

## Checks

```bash
cargo fmt --manifest-path apps/api/Cargo.toml
cargo test --manifest-path apps/api/Cargo.toml
cargo clippy --manifest-path apps/api/Cargo.toml --all-targets -- -D warnings
```

Resultado:

- 62 tests OK.
- Clippy OK.

## Límites / siguientes pasos

- El fire time global sigue siendo lunes-viernes 08:00 America/Santiago en `window.rs`; la policy sincroniza timezone/preset, pero el loop interno todavía no calcula fire time por tenant/timezone.
- `SchedulePreset` aún solo tiene `Weekdays08Local`/disabled; no hay UI/API para presets avanzados.
- Idempotencia sigue siendo suficiente para instancia única; producción multi-instancia requerirá claim transaccional/precondición Firestore.
- Retención destructiva sigue pendiente, no abordada por esta tarea.
