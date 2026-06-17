# TASK-019 — Check script local/CI

## Estado

done

## Owner primario

Pi

## Objetivo

Agregar script único de validación local que ejecute API fmt/tests/clippy, AI worker tests y web build.

## Criterios

- [x] Script ejecutable en `scripts/check-all.sh`.
- [x] Incluye Rust fmt --check.
- [x] Incluye cargo test + clippy.
- [x] Incluye pytest AI worker.
- [x] Incluye web build.
- [x] Script pasa completo.
