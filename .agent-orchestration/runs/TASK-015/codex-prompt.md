Planifica rate limiting incremental para apps/api Rust/axum de este repo.
No leas .env ni secrets. Modo plan.
Objetivo: limitar abuso de POST /analysis-runs y /analysis-runs/:id/start por usuario autenticado, sin bloquear scheduler. Respuestas 429. Defaults beta seguros. Tests.
Entrega riesgos, recomendación, paths.
