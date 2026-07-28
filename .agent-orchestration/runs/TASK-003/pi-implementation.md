# TASK-003 — Implementación Pi P0 authz/ownership backend

Fecha: 2026-06-17

## Decisiones aplicadas

- `401` para sesión ausente/inválida.
- `404` para recursos ajenos con IDs opacos, siguiendo D-008 anti-enumeración.
- `reviewer_label` ya no se acepta como fuente de verdad del cliente; backend usa el email autenticado.

## Cambios realizados

### `apps/api/src/http/mod.rs`

- `GET /analysis-runs/:id` requiere sesión y ownership.
- `GET /analysis-runs/:id/status` queda protegido porque reutiliza el mismo handler.
- `POST /analysis-runs/:id/start` usa helper de ownership y retorna `404` para runs ajenos antes de decrypt/spawn.
- `GET /analysis-runs/:id/events` valida sesión/ownership antes de abrir SSE.
- `GET /analysis-runs/:id/metrics` requiere sesión/ownership.
- `GET /analysis-runs/:id/threads` requiere sesión/ownership.
- `GET /threads/:thread_id` requiere sesión y valida ownership vía `analysis_run_id`.
- `PATCH /threads/:thread_id/manual-review` requiere sesión/ownership y setea `reviewer_label` desde `session.google_account_email`.
- Se agregaron tests HTTP IDOR con dos usuarios.

### `apps/api/src/storage/mod.rs`

- `MemoryStorage::add_manual_review` valida que el run exista antes de mutar el thread.
- Valida que el thread pertenezca al run.

### `apps/api/src/firestore/mod.rs`

- `FirestoreStorage::add_manual_review` valida que el run y thread existan antes de escribir review.
- Evita escribir manual reviews huérfanas.
- Se corrigió un warning clippy existente.

### `apps/api/src/gmail/mod.rs`

- Se corrigieron warnings clippy existentes sin cambiar comportamiento.

## Tests/checks ejecutados

```bash
cargo fmt --manifest-path apps/api/Cargo.toml
cargo test --manifest-path apps/api/Cargo.toml
cargo clippy --manifest-path apps/api/Cargo.toml --all-targets -- -D warnings
```

Resultado:

- `cargo test`: 52 tests OK.
- `cargo clippy`: OK.

## Riesgos / handoff

- La UI puede recibir `404` en recursos ajenos o stale selections. El cliente ya tiene mensaje genérico para 404.
- `reviewer_label` enviado por frontend queda ignorado; en una task posterior se puede remover de `ReviewForm` para evitar confusión.
- Aún falta CSRF/rate limiting/error redaction de P0/P1; no estaba en el alcance de esta implementación.
