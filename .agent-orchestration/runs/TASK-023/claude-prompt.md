Actúa como Claude Code. Tarea TASK-023. Modo read-only planning.
No uses Codex. No leas .env, .env.* ni secrets/*.

Objetivo: diseñar observability v2 para beta: historial corto de scheduler/runs, categorías de error redacted/no sensibles y UI de diagnóstico operativo.
Lee solo archivos necesarios del repo. Entrega markdown con: arquitectura incremental, archivos a tocar, contrato API/storage, categorías de error, UI, tests, riesgos, BLOCKED_QUESTIONS si hay decisiones humanas.
