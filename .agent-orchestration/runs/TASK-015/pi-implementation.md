# TASK-015 — Rate limiting para análisis manuales

Fecha: 2026-06-17

## Agentes

- Codex produjo plan en `.agent-orchestration/runs/TASK-015/codex-plan.md`.
- Pi implementó fase incremental conservadora.

## Implementado

- Config nueva:
  - `RATE_LIMIT_ANALYSIS_CREATE_PER_HOUR` default `12`.
  - `RATE_LIMIT_ANALYSIS_START_PER_HOUR` default `12`.
- `AppState` incluye rate limiter in-memory por instancia.
- `POST /analysis-runs` consume cuota `analysis_create:{user}`.
- `POST /analysis-runs/:id/start` consume cuota `analysis_start:{user}`.
- Scheduler interno no consume cuota.
- Respuesta al exceder: `429 Too Many Requests` con mensaje claro.
- Frontend agrega copy genérico para HTTP 429.
- Hardening adicional: `start_analysis_run` solo permite iniciar runs `pending`, evitando workers duplicados.

## Tests

- `create_analysis_run_is_rate_limited_per_user`.

## Checks

- API tests 66 OK.
- Clippy OK.
- Web build OK.

## Límites / fase 2

- Limiter es in-memory por instancia. Para Cloud Run multi-instancia se recomienda Firestore/Redis/token bucket distribuido o max_instances=1 durante beta.
- Implementación actual es ventana por hora, no token bucket con `Retry-After`.
- Falta test explícito de start limit y usuarios distintos.
