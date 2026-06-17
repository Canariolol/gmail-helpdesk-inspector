Actúa como Claude Code. No uses Codex. No leas archivos. Planifica con este contexto:
- API Rust expone GET /analysis-runs -> AnalysisRun[] y GET /analysis-runs/:id/threads -> EmailThread[]. Ambos protegidos por sesión/ownership.
- Frontend React Query consume arrays directos en App.tsx y selecciona runs/threads.
- Storage tiene MemoryStorage y FirestoreStorage. Queremos paginación básica beta sin romper UI.

Entrega plan markdown para implementar TASK-024: contrato API compatible, query params limit/page_token, respuesta paginada, compatibilidad frontend, storage memory/firestore, tests, aceptación, riesgos, sin BLOCKED si no hay decisión imprescindible.
