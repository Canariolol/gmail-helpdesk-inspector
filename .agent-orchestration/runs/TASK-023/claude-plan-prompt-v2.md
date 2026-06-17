Actúa como Claude Code. No uses Codex. No leas archivos. Planifica con este contexto:
- API Rust con GET /me/operations/status ya existe; scheduler guarda scheduleStates/{email}; retorna scheduler enabled/timezone/preset/recipients_count/next run/last state/error redacted/policy info.
- UI ConfiguracionView ya muestra panel Operación automática.
- Necesitamos beta incremental: historial corto de operaciones/scheduler/runs y categorías de error no sensibles.
- Storage tiene MemoryStorage y FirestoreStorage. Debe preservar Gmail readonly y no guardar cuerpos completos.

Entrega plan markdown para implementar TASK-023: contrato API, modelo de datos, categorías error, storage, UI, tests, aceptación, riesgos, sin BLOCKED si no hay decisión imprescindible.
