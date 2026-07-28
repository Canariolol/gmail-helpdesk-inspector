Actúa como Claude Code. Tarea TASK-024. Modo read-only planning.
No uses Codex. No leas .env, .env.* ni secrets/*.

Objetivo: diseñar paginación básica de analysis-runs y threads para beta privada, manteniendo authz/ownership y compatibilidad razonable.
Lee solo archivos necesarios del repo. Entrega markdown con: contrato API, cambios storage memory/firestore, cambios frontend, migración compatible, tests, riesgos, BLOCKED_QUESTIONS si hay decisiones humanas.
