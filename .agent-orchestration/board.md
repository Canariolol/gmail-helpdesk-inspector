# Board

## Now

### TASK-001 — Auditoría actual + plan SaaS público

- Estado: done
- Owner: Codex
- Modelo recomendado: `gpt-5.5` con reasoning high o superior
- Output: `.agent-orchestration/runs/TASK-001/codex-plan.md`
- Objetivo: evaluar el estado actual del producto y proponer roadmap SaaS seguro, priorizado y auditable.

### TASK-002 — Brief UX público SaaS

- Estado: done
- Owner: Claude
- Input usado: `.agent-orchestration/runs/TASK-001/codex-plan.md`
- Modelo usado: Sonnet max; no requiere Opus en esta pasada.
- Output: `.agent-orchestration/runs/TASK-002/claude-ui.md`
- Objetivo: proponer experiencia SaaS pública para onboarding, conexión Gmail, configuración, dashboard y revisión manual.

## Next

### TASK-003 — P0 authz/ownership backend

- Estado: done
- Owner: Codex
- Objetivo: planificar implementación de `GHMI-SEC-001`/`GHMI-SEC-002`: ownership en runs, metrics, threads, events y manual review, con tests IDOR de dos usuarios.
- Output: `.agent-orchestration/runs/TASK-003/codex-plan.md`

### TASK-004 — UI quick wins de confianza

- Estado: done
- Owner: Pi/Claude
- Objetivo: implementar cambios frontend sin bloqueantes: LoginView profesional, PrivacyCallout, OAuthExplainModal, Sidebar a11y, AyudaView privacidad.
- Output: `.agent-orchestration/runs/TASK-004/pi-implementation.md`

### TASK-005 — Configuración SaaS parametrizable + política del auditor IA

- Estado: done
- Owner: Codex/Pi
- Objetivo: diseñar modelo robusto de configuración por usuario/organización y eliminar hardcodes de la empresa original, incluyendo política del auditor IA.
- Output: `.agent-orchestration/runs/TASK-005/codex-plan.md`
- Decisiones del dueño: registradas en `.agent-orchestration/owner-questions.md` y D-011.

### TASK-006 — Remover reviewer_label desde frontend

- Estado: done
- Owner: Pi
- Objetivo: limpiar `ReviewForm` para no enviar `reviewer_label`; backend ya usa el email autenticado.
- Output: `.agent-orchestration/runs/TASK-006/pi-implementation.md`

### TASK-007 — Configuración SaaS incremental

- Estado: done — incremento backend policy-first + UI inicial
- Owner: Codex/Pi/Claude
- Objetivo: implementar primer modelo de configuración owner/org-ready para reemplazar hardcodes de dominio, timezone, filtros, IA y scheduler.
- Outputs:
  - `.agent-orchestration/runs/TASK-007/codex-plan.md`
  - `.agent-orchestration/runs/TASK-007/claude-ui.md`
  - `.agent-orchestration/runs/TASK-007/pi-integration.md`
  - `.agent-orchestration/runs/TASK-007/pi-implementation.md`
  - `.agent-orchestration/runs/TASK-007B/claude-implementation.md`
- Checks: API tests 55 OK, clippy OK, web build OK.

### TASK-008 — Centro de privacidad y datos

- Estado: done — backend read-only + UI inicial
- Owner: Claude/Pi
- Objetivo: hacer visible qué datos usa la app y preparar flujos de desconexión/borrado sin ejecutar acciones destructivas aún.
- Outputs:
  - `.agent-orchestration/tasks/TASK-008.md`
  - `.agent-orchestration/runs/TASK-008/pi-backend-implementation.md`
  - `.agent-orchestration/runs/TASK-008/claude-ui-plan.md`
  - `.agent-orchestration/runs/TASK-008/claude-implementation.md`
- Checks: API tests 59 OK, clippy OK, web build OK.

### TASK-011 — Checklist seguridad, privacidad y compliance Gmail

- Estado: backlog
- Owner: Codex
- Objetivo: definir requisitos antes de lanzamiento público: OAuth verification, datos sensibles, retention, logs, scopes, términos, privacidad.

### TASK-012 — Scheduler V2 policy-first incremental

- Estado: done
- Owner: Pi
- Objetivo: conectar scheduler existente con policy SaaS, snapshots y política de reportes sin abordar borrados/desconexión.
- Outputs:
  - `.agent-orchestration/tasks/TASK-012.md`
  - `.agent-orchestration/runs/TASK-012/pi-implementation.md`
- Checks: API tests 62 OK, clippy OK.

### TASK-013 — Scheduler timezone/preset real por tenant

- Estado: done
- Owner: Pi
- Objetivo: scheduler interno evalúa due por timezone de cada tenant/config.
- Outputs:
  - `.agent-orchestration/tasks/TASK-013.md`
  - `.agent-orchestration/runs/TASK-013/pi-implementation.md`
- Checks: API tests 66 OK, clippy OK.

### TASK-014 — Observabilidad scheduler/run

- Estado: done
- Owner: Claude/Pi
- Objetivo: endpoint/UI read-only con estado operativo, próximo intento y último scheduler state.
- Outputs:
  - `.agent-orchestration/tasks/TASK-014.md`
  - `.agent-orchestration/runs/TASK-014/claude-ui-plan.md`
  - `.agent-orchestration/runs/TASK-014/pi-implementation.md`
- Checks: API tests 66 OK, clippy OK, web build OK.

### TASK-015 — Rate limiting análisis manuales

- Estado: done — fase incremental in-memory
- Owner: Codex/Pi
- Objetivo: limitar abuso accidental de análisis manuales sin bloquear scheduler.
- Outputs:
  - `.agent-orchestration/tasks/TASK-015.md`
  - `.agent-orchestration/runs/TASK-015/codex-plan.md`
  - `.agent-orchestration/runs/TASK-015/pi-implementation.md`
- Checks: API tests 66 OK, clippy OK, web build OK.

### TASK-016 — Roadmap SaaS production-ready extendido

- Estado: done — draft v2 creado
- Owner: Pi/Codex/Claude
- Objetivo: revisar brechas y consolidar roadmap extendido production-ready.
- Outputs:
  - `.agent-orchestration/tasks/TASK-016.md`
  - `.agent-orchestration/production-roadmap-v2.md`

### TASK-017 — AI worker policy injection

- Estado: done
- Owner: Pi
- Objetivo: eliminar hardcodes de empresa original y enviar policy context por run al worker IA.
- Outputs:
  - `.agent-orchestration/tasks/TASK-017.md`
  - `.agent-orchestration/runs/TASK-017/pi-implementation.md`
- Checks: API 66 OK, clippy OK, AI worker 5 OK.

### TASK-018 — Docs/runtime update

- Estado: done
- Owner: Pi
- Objetivo: alinear README con SaaS actual, IA opt-in, scheduler timezone, endpoints y checks.
- Outputs:
  - `.agent-orchestration/tasks/TASK-018.md`
  - `.agent-orchestration/runs/TASK-018/pi-implementation.md`

### TASK-019 — Check script local/CI

- Estado: done
- Owner: Pi
- Objetivo: script único de validación local.
- Outputs:
  - `.agent-orchestration/tasks/TASK-019.md`
  - `.agent-orchestration/runs/TASK-019/pi-implementation.md`
  - `scripts/check-all.sh`
- Checks: `./scripts/check-all.sh` OK.

### TASK-020 — Landing SaaS beta privada / waitlist

- Estado: done
- Owner: Claude/Pi
- Objetivo: landing pública mínima para beta privada.
- Outputs:
  - `.agent-orchestration/tasks/TASK-020.md`
  - `.agent-orchestration/runs/TASK-020/claude-landing-plan.md`
  - `.agent-orchestration/runs/TASK-020/pi-implementation.md`

### TASK-021 — Onboarding copy Workspace-only + IA opt-in

- Estado: done
- Owner: Pi/Claude
- Objetivo: alinear onboarding con beta Workspace-only, Gmail readonly, IA opt-in y retención prudente.
- Outputs:
  - `.agent-orchestration/tasks/TASK-021.md`
  - `.agent-orchestration/runs/TASK-021/pi-implementation.md`

### TASK-022 — Empty/error states y UX 429

- Estado: done
- Owner: Pi
- Objetivo: feedback accionable en errores de análisis manual, especialmente rate limit 429.
- Outputs:
  - `.agent-orchestration/tasks/TASK-022.md`
  - `.agent-orchestration/runs/TASK-022/pi-implementation.md`

### TASK-023 — Observability v2 / historial operativo

- Estado: done
- Owner: Claude Code / Pi orchestration
- Objetivo: historial operativo reciente y errores categorizados/redactados.
- Outputs:
  - `.agent-orchestration/tasks/TASK-023.md`
  - `.agent-orchestration/runs/TASK-023/claude-plan-v2.md`
  - `.agent-orchestration/runs/TASK-023/pi-implementation.md`

### TASK-024 — Paginación básica runs/threads

- Estado: done
- Owner: Claude Code / Pi orchestration
- Objetivo: paginación opt-in compatible para runs y threads.
- Outputs:
  - `.agent-orchestration/tasks/TASK-024.md`
  - `.agent-orchestration/runs/TASK-024/claude-plan-v2.md`
  - `.agent-orchestration/runs/TASK-024/pi-implementation.md`

### TASK-025 — Deployment runbook + Cloud checklist

- Estado: done
- Owner: Claude Code / Pi orchestration
- Objetivo: runbook beta privada sin secretos reales.
- Outputs:
  - `.agent-orchestration/tasks/TASK-025.md`
  - `.agent-orchestration/runs/TASK-025/claude-runbook.md`
  - `docs/deployment-beta-runbook.md`

### TASK-009 — Backlog técnico por épicas

- Estado: backlog
- Owner: Pi + Codex
- Objetivo: convertir el plan aprobado en épicas y tareas implementables con path ownership.

### TASK-010 — Design system SaaS liviano

- Estado: backlog
- Owner: Claude
- Objetivo: definir tokens visuales, layout, navegación y componentes base para una experiencia pública consistente.

## Done

- D-001 a D-005: protocolo inicial de orquestación definido.
