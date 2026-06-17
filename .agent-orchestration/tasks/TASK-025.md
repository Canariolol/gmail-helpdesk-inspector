# TASK-025 — Deployment runbook + Cloud checklist

## Estado

done

## Owner primario

Claude Code / Pi orchestration

## Objetivo

Crear runbook de despliegue beta privada para API/web/AI worker, Firestore, Resend, scheduler, OAuth, secretos y validaciones post-deploy.

## Resultado

- Runbook Claude inicial: `.agent-orchestration/runs/TASK-025/claude-runbook.md`
- Runbook final: `docs/deployment-beta-runbook.md`
- Implementación doc: `.agent-orchestration/runs/TASK-025/pi-implementation.md`

## Criterios

- [x] No usar Codex.
- [x] No leer `.env`, `.env.*`, `secrets/*`.
- [x] No incluir secretos reales.
- [x] Variables con placeholders seguros.
- [x] Checklist predeploy/postdeploy.
