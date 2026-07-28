# TASK-019 — Check script local/CI

Fecha: 2026-06-17

## Implementado

Nuevo script:

- `scripts/check-all.sh`

Ejecuta:

```bash
cargo fmt --manifest-path apps/api/Cargo.toml -- --check
cargo test --manifest-path apps/api/Cargo.toml
cargo clippy --manifest-path apps/api/Cargo.toml --all-targets -- -D warnings
python3 -m pytest apps/ai-worker/tests
npm --prefix apps/web run build
```

## Resultado

`./scripts/check-all.sh` pasó completo:

- API: 66 tests OK.
- Clippy OK.
- AI worker: 5 tests OK.
- Web build OK, con warning conocido de chunk grande.
