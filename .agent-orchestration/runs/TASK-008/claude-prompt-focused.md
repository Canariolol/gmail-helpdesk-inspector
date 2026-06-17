Actúa como Claude UI/UX implementador frontend.

Reglas:
- No leas .env, .env.*, secrets/*, tokens ni credenciales.
- Modifica SOLO apps/web/src/** y .agent-orchestration/runs/TASK-008/**.
- No toques backend.
- No inventes endpoints destructivos.
- Usa el endpoint read-only existente: GET /me/data-summary.
- Acciones desconectar/borrar deben quedar deshabilitadas con copy claro: requiere confirmación y contrato backend pendiente.
- Mantén estilo actual, accesible, español.

Implementa:
1) Tipos TS para DataSummary en apps/web/src/api/types.ts.
2) Vista nueva PrivacidadDatosView que muestra:
   - cuenta conectada, gmail readonly scope, estado mailbox;
   - minimización de datos, IA on/off, consentimiento, retención, modo reporte;
   - conteos analysis_runs/threads/messages;
   - tarjetas de acciones: desconectar Gmail, borrar análisis, borrar datos/cuenta deshabilitadas.
3) Integración en App + Sidebar como vista "Privacidad y datos" o reusar navegación ayuda si prefieres, sin romper Ayuda.
4) Documenta en .agent-orchestration/runs/TASK-008/claude-implementation.md: archivos, checks, BLOCKED_QUESTIONS para semántica destructiva.
5) Ejecuta npm --prefix apps/web run build si es posible.

Contrato GET /me/data-summary devuelve:
{
  account: { google_account_email, gmail_scope_snapshot, mailbox_connected, mailbox_revoked_at },
  org: { id, name, role, policy_version, setup_ready, setup_missing },
  privacy: { data_minimization_mode, ai_enabled, ai_consent_granted_at, retention_days, report_mode },
  stored_data: { analysis_runs_count, threads_count, messages_count, ai_audit_records_count },
  actions: {
    disconnect_gmail: { available, reason },
    delete_analysis_data: { available, reason },
    delete_account_data: { available, reason }
  }
}
