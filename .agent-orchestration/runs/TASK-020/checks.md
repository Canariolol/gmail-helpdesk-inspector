# TASK-020/021/022 — Checks

Fecha: 2026-06-17

Comando:

```bash
./scripts/check-all.sh
```

Resultado:

- Rust fmt --check OK.
- API tests: 66 OK.
- API clippy OK.
- AI worker tests: 5 OK.
- Web build OK.
- Warning conocido: Vite chunk >500kB.
