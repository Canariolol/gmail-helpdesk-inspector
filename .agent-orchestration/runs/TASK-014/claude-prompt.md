Actúa como Claude UI/UX senior. No leas .env ni secrets. Modo read-only/plan.

Task: Diseña UI mínima production-ready para observabilidad scheduler/run en este repo.
Contexto:
- Existe ConfiguracionView, PrivacidadDatosView, Sidebar.
- Backend planea endpoint read-only GET /me/operations/status con config scheduler, last_state, next_run_estimate, policy info, errores redacted.

Entrega en markdown:
- ubicación recomendada UI;
- componentes/estados;
- copy español;
- tipos TS sugeridos;
- criterios accesibilidad;
- riesgos y BLOCKED_QUESTIONS si aplica.
