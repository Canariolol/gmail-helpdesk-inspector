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

- Estado: ready
- Owner: Claude
- Objetivo: implementar cambios frontend sin bloqueantes: LoginView profesional, PrivacyCallout, OAuthExplainModal, Sidebar a11y, AyudaView privacidad.

### TASK-005 — Checklist seguridad, privacidad y compliance Gmail

- Estado: backlog
- Owner: Codex
- Objetivo: definir requisitos antes de lanzamiento público: OAuth verification, datos sensibles, retention, logs, scopes, términos, privacidad.

### TASK-006 — Backlog técnico por épicas

- Estado: backlog
- Owner: Pi + Codex
- Objetivo: convertir el plan aprobado en épicas y tareas implementables con path ownership.

### TASK-007 — Design system SaaS liviano

- Estado: backlog
- Owner: Claude
- Objetivo: definir tokens visuales, layout, navegación y componentes base para una experiencia pública consistente.

## Done

- D-001 a D-005: protocolo inicial de orquestación definido.
