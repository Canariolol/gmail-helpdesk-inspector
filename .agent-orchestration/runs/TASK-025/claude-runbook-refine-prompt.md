Actúa como Claude Code. No uses Codex. No leas .env, .env.* ni secrets/*.

Refina el runbook de TASK-025 usando SOLO estos archivos permitidos si necesitas verificar nombres reales:
- README.md
- apps/api/src/config/mod.rs
- apps/ai-worker/src/ai_worker/settings.py
- apps/web/package.json
- apps/ai-worker/pyproject.toml

Toma como base .agent-orchestration/runs/TASK-025/claude-runbook.md y corrige nombres de variables/env a los reales cuando estén en esos archivos. Si no puedes verificar un nombre, márcalo como "verificar en código/config" y NO lo inventes.

Entrega markdown final sin fence de código envolvente, listo para guardar como docs/deployment-beta-runbook.md.
