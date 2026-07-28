# TASK-017 — AI worker policy injection

Fecha: 2026-06-17

## Implementado

### API

`audit_thread` ahora envía `policy_context` al AI worker cuando el run tiene `PolicySnapshot`.

Campos enviados:

- mailbox email/display/domain;
- internal domains;
- responder emails;
- aliases;
- valid request criteria;
- non-responsibility rules;
- ignored senders/domains/keywords;
- prompt version;
- allowed fields.

### AI worker

- `Settings` ya no tiene defaults con empresa/mailbox/personas reales.
- Nuevo modelo `AuditPolicyContext`.
- `build_system_prompt(settings, policy_context)` genera prompt genérico y tenant-aware.
- Mantiene fallback legacy genérico si un backend viejo no envía policy.

### Tests

Agregados/actualizados:

- Prompt default no contiene hardcodes de tenant original.
- Prompt usa campos del request `policy_context`.
- Tests existentes de Bedrock siguen OK.

## Checks

```bash
python3 -m pytest apps/ai-worker/tests
cargo test --manifest-path apps/api/Cargo.toml
cargo clippy --manifest-path apps/api/Cargo.toml --all-targets -- -D warnings
```

Resultado:

- AI worker: 5 tests OK.
- API: 66 tests OK.
- Clippy OK.

## Verificación de hardcodes

`rg` sobre código fuente productivo ya no encuentra dominio/personas de la empresa original fuera de tests deliberados.
