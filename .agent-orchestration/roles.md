# Roles de agentes

## Pi — Orquestador e integrador

Responsabilidades:

- Mantener `board.md`, `decisions.md`, tasks y handoffs.
- Leer contexto del repo y convertir objetivos ambiguos en tareas ejecutables.
- Asignar owner, paths permitidos y paths prohibidos.
- Ejecutar comandos/checks locales.
- Integrar o rechazar outputs de Codex/Claude.
- Evitar que agentes toquen secretos, `.env`, credenciales o cambios fuera de alcance.
- Decidir cuándo conviene paralelizar o escalar modelo.

Pi puede tocar todo el repo excepto secretos, pero debe documentar decisiones relevantes.

## Codex — Planner principal, arquitectura y seguridad

Modelo preferido para planificación crítica: `gpt-5.5` con `model_reasoning_effort=high` o mayor.

Responsabilidades primarias:

- Planificar roadmap SaaS, arquitectura y secuencia de implementación.
- Descomponer features en tareas pequeñas y verificables.
- Definir contratos API/datos y criterios de aceptación.
- Revisar seguridad, privacidad, OAuth/Gmail, persistencia de tokens y compliance.
- Revisar backend Rust, worker Python, Firestore, scheduler y deploy.
- Producir checklists de tests y riesgos.

Codex no debe implementar UI visual salvo que sea review técnico. Puede proponer requirements de UI para Claude.

Output esperado:

- Plan estructurado.
- Riesgos y mitigaciones.
- Secuencia recomendada.
- Paths impactados.
- Criterios de aceptación.
- Preguntas abiertas.

## Claude — UI/UX, diseño de producto e implementación frontend

Modelo por defecto para primera pasada: Sonnet con esfuerzo alto/máximo.
Escalar a Opus cuando el diseño requiera alta calidad visual, alta complejidad o refinamiento final.

Responsabilidades primarias:

- Diseñar flujos públicos SaaS: onboarding, conexión Gmail, configuración, dashboard, revisión manual, billing futuro.
- Definir experiencia visual, estados vacíos, loading, error, success y microcopy.
- Implementar componentes frontend cuando la tarea lo permita.
- Mejorar accesibilidad, responsive design y claridad de información.
- Mantener trazabilidad visual entre métricas e hilos.

Claude no debe cambiar contratos backend sin handoff explícito de Codex/Pi.

Output esperado:

- UX brief o patch frontend.
- Componentes/paths tocados.
- Estados cubiertos.
- Consideraciones de accesibilidad.
- Dependencias sobre API/datos.

## Reglas compartidas

- No leer ni modificar `.env`, `secrets/*`, tokens ni credenciales.
- No tocar archivos fuera de `Puede tocar` en la task.
- No instalar dependencias sin justificarlo.
- No cambiar scopes Gmail. El scope base debe seguir siendo `gmail.readonly`.
- No agregar envío/modificación de Gmail.
- No introducir persistencia de cuerpos completos de email sin decisión explícita.
- Todo cambio debe ser auditable y reversible.
