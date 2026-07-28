Actúa como Claude Code. No uses Codex. No leas .env, .env.* ni secrets/*.

Genera directamente un runbook markdown para beta privada de Gmail Helpdesk Inspector basado en este contexto:
- Monorepo con apps/api Rust, apps/web Vite, apps/ai-worker Python FastAPI.
- Gmail OAuth readonly, Firestore, Resend, scheduler interno/endpoint cron, AI worker stateless, policy/org config, rate limiting in-memory.
- Checks: ./scripts/check-all.sh.
- Producción rechaza secretos default con APP_ENV=production.

Entrega: runbook Cloud Run beta con variables placeholder seguras, pasos predeploy/deploy/postdeploy, OAuth, scheduler, rollback, riesgos y decisiones pendientes. No incluyas secretos reales.
