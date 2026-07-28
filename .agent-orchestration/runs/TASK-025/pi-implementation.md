# TASK-025 — Deployment runbook + Cloud checklist

Fecha: 2026-06-17

## Delegación Claude Code

- Claude generó el primer runbook en `.agent-orchestration/runs/TASK-025/claude-runbook.md`.
- La segunda pasada con lectura acotada no produjo salida, por lo que Pi revisó únicamente archivos permitidos (`README.md`, config fuente, settings worker) y corrigió nombres reales de variables.
- No se usó Codex.
- No se leyó contenido de `.env` ni `.env.*`; solo se observó que existen al listar raíz.

## Implementado

Nuevo documento:

- `docs/deployment-beta-runbook.md`

Incluye:

- arquitectura beta;
- variables verificadas API/worker/web;
- predeploy;
- orden de deploy;
- Cloud Run recomendaciones;
- scheduler interno vs externo;
- smoke tests;
- observabilidad mínima;
- rollback;
- riesgos conocidos;
- decisiones pendientes;
- checklist final.
