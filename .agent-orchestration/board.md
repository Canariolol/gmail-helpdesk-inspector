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

### TASK-008 — Checklist seguridad, privacidad y compliance Gmail

- Estado: backlog
- Owner: Codex
- Objetivo: definir requisitos antes de lanzamiento público: OAuth verification, datos sensibles, retention, logs, scopes, términos, privacidad.

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
