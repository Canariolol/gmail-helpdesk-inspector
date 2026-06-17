# TASK-008A — Backend read-only data summary

Fecha: 2026-06-17

## Implementado

Nuevo endpoint seguro/read-only:

- `GET /me/data-summary`

Requiere sesión y no ejecuta acciones destructivas.

## Contrato

```json
{
  "account": {
    "google_account_email": "user@example.com",
    "gmail_scope_snapshot": ["https://www.googleapis.com/auth/gmail.readonly"],
    "mailbox_connected": true,
    "mailbox_revoked_at": null
  },
  "org": {
    "id": "...",
    "name": "Example",
    "role": "owner",
    "policy_version": 1,
    "setup_ready": false,
    "setup_missing": ["valid_request_criteria"]
  },
  "privacy": {
    "data_minimization_mode": "metadata_snippets_excerpts_only",
    "ai_enabled": false,
    "ai_consent_granted_at": null,
    "retention_days": 30,
    "report_mode": "metrics_only"
  },
  "stored_data": {
    "analysis_runs_count": 1,
    "threads_count": 1,
    "messages_count": 1,
    "ai_audit_records_count": null
  },
  "actions": {
    "disconnect_gmail": { "available": false, "reason": "pending_backend_contract" },
    "delete_analysis_data": { "available": false, "reason": "pending_backend_contract" },
    "delete_account_data": { "available": false, "reason": "pending_backend_contract" }
  }
}
```

## Seguridad

- No borra datos.
- No revoca OAuth.
- No expone subjects, snippets, remitentes ni cuerpos.
- Cuenta solo datos propios vía sesión actual.
- Provisiona org config si falta, igual que `/me/org/config`.

## Tests

Agregado test:

- `data_summary_is_read_only_and_counts_owned_data`

Checks ejecutados:

```bash
cargo fmt --manifest-path apps/api/Cargo.toml
cargo test --manifest-path apps/api/Cargo.toml
cargo clippy --manifest-path apps/api/Cargo.toml --all-targets -- -D warnings
```

Resultado:

- 56 tests OK.
- Clippy OK.

## Pendiente / decision gate

Antes de implementar endpoints destructivos, el dueño debe aprobar semántica exacta:

1. Desconectar Gmail:
   - solo revocar token y preservar análisis;
   - revocar token + pausar scheduler;
   - revocar token + borrar datos derivados.
2. Borrar análisis:
   - por run;
   - todos los runs del usuario/mailbox;
   - también manual reviews y AI audits.
3. Borrar cuenta/org:
   - inmediato con confirmación textual;
   - solicitud asincrónica con período de gracia;
   - confirmación por email.
