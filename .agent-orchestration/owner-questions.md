# Preguntas pendientes para dueño del producto

Fuente: `.agent-orchestration/runs/TASK-005/codex-plan.md`  
Estado: aprobadas por el dueño del producto el 2026-06-17. Nota de proceso: para próximas decisiones Pi debe mostrar siempre alternativas + recomendación + impacto antes de pedir aprobación.

## Q1 — Público beta

¿La beta será para organizaciones con casilla compartida/alias o para usuarios individuales?

- A: organizaciones con 1 mailbox inicial.
- B: usuarios individuales self-serve.
- Recomendación Pi/Codex: **A para beta privada**.

## Q2 — Auditoría IA

¿La IA será obligatoria, opt-in u opt-out?

- A: opt-in explícito por organización.
- B: obligatoria.
- C: opt-out.
- Recomendación Pi/Codex: **A**.

## Q3 — Retención default

¿Cuánto tiempo se guardan análisis/metadatos por defecto?

- A: 30 días.
- B: 90 días.
- C: configurable antes del primer run.
- Recomendación Pi/Codex: **30 días en beta, configurable por admin**.

## Q4 — Contenido de reportes por email

¿Los reportes por email pueden incluir asuntos/remitentes?

- A: solo métricas.
- B: métricas + asuntos/remitentes de hilos por revisar.
- C: configurable.
- Recomendación Pi/Codex: **C con default A**.

## Q5 — Scheduler

¿El scheduler debe ser laboral simple o policy avanzada?

- A: lunes-viernes horario fijo.
- B: días/horas/ventanas configurables.
- Recomendación Pi/Codex: **B con preset simple**.

## Q6 — Tipo de Gmail

¿Soportar Gmail personal o solo Google Workspace?

- A: Workspace only.
- B: Workspace + Gmail personal.
- Recomendación Pi/Codex: **Workspace only para beta**.

## Decisión aprobada

- Q1: A — beta privada para organizaciones con 1 mailbox inicial.
- Q2: A — IA opt-in explícita por organización.
- Q3: 30 días en beta, configurable por admin.
- Q4: C con default A — reportes configurables, default solo métricas.
- Q5: B con preset simple — scheduler configurable.
- Q6: Workspace only para beta.
