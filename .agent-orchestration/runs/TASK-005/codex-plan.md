# TASK-005 — Configuración SaaS parametrizable + política del auditor IA

Nota de ejecución: Codex trabajó en modo read-only y no leyó `.env`, `.env.*`, `secrets/*` ni credenciales. El CLI no pudo escribir desde dentro del sandbox, pero Pi guardó el output final en este archivo.

Modo: planificación read-only. No se modificó código.

## ASSUMPTIONS

- El scope Gmail se mantiene en `gmail.readonly`; no se propone enviar, etiquetar, borrar ni modificar Gmail.
- No se deben persistir cuerpos completos. El body puede usarse transitoriamente y minimizado para auditoría IA, pero Firestore debe guardar solo metadata/snippets/headers/decisiones.
- MVP endurecido = mono/owner-safe; beta privada = tenants limitados con UI guiada; SaaS público = self-serve + compliance/OAuth verification/billing.

## 1. Inventario de hardcodes/assumptions actuales

- Empresa/dominio/personas: `apps/ai-worker/src/ai_worker/bedrock.py` y `settings.py` codifican West Ingeniería, N1, `soporte@west-ingenieria.cl`, `catherine.trivino@...`, `west-ingenieria.cl` y miembros del equipo.
- UI: `FilterBar.tsx` hardcodea fechas `2026-06-01..2026-06-12`, dominio `@west-ingenieria.cl`, ignorados `google.com,calendar.google.com`, keywords y timezone `America/Santiago`.
- Timezone/reportes: Rust usa `America/Santiago` como default/fallback en análisis, scheduler, reportes y formateo frontend; scheduler corre lunes-viernes 08:00 SCL.
- Política de scheduler: ventana fija: lunes cubre viernes-domingo; martes-viernes cubren día anterior; fines de semana no corren.
- Reportes: copy en español e informal, asunto “Reporte Helpdesk”, footer fijo con 08:00 SCL, Resend como canal único, destinatarios por env/documento manual.
- IA: README dice auditor IA obligatorio; código tiene umbral auto-apply `0.92`, máximo 14 mensajes y 280 chars/body como política global no versionada por tenant.
- Storage/tenancy: `users`, `scheduleConfigs/{email}`, `scheduleStates/{email}`, `analysisRuns/{id}` top-level; ownership por `user_email`, sin org/tenant/roles formal.
- Retención/lifecycle: no hay política de retención, export, delete por tenant/run ni snapshots versionados de políticas.
- Modelo operacional: configuración actual cubre dominios/listas/fechas, pero no define quién cuenta como equipo, qué mailbox se audita, qué solicitudes cuentan, qué evidencia requiere IA ni qué hacer ante ambigüedad.

## 2. Entidades/configuración propuestas

- `Organization`: `id`, `name`, `slug`, `timezone`, `locale`, `status`, `plan`, `created_by`, `data_region?`, `created_at`.
- `Membership`: `org_id`, `user_id`, `role` (`owner|admin|analyst|viewer`), `email`, `status`.
- `Mailbox`: `id`, `org_id`, `google_account_email`, `display_name`, `purpose`, `authorized_by_user_id`, OAuth token ref, `gmail_scope_snapshot`, `connected_at`, `revoked_at`.
- `AnalysisPolicy`: dominios internos, dominios cliente permitidos/bloqueados, direcciones/equipos internos, mailbox objetivo, reglas de solicitud válida, exclusiones, automatismos, horarios, ventana default, límites de threads, reglas de reapertura, thresholds de revisión.
- `AiAuditorPolicy`: enabled/required, provider/model, prompt template/version, idioma, contexto operacional, criterios de validez, criterios de respuesta, auto-apply threshold, manual-review threshold, campos permitidos para IA, límites de mensajes/chars, prompt-injection handling.
- `ScheduleReportPolicy`: enabled, timezone, días/horas, window strategy, backfill policy, recipients, report language/tone, include subjects/senders toggle, failure notices, provider config ref.
- `RetentionPolicy`: retention days por runs/messages/audits, deletion mode, exportability, AI consent, audit-log retention, legal hold flag.
- `PolicyVersion`: snapshot inmutable de cada policy con `version`, `hash`, `effective_from`, `created_by`, `change_reason`.

## 3. Qué debe versionarse por `AnalysisRun`

- `org_id`, `mailbox_id`, `trigger_type` (`manual|scheduled|backfill`), `created_by`.
- Snapshot completo o hashes resolubles de `analysis_policy`, `ai_auditor_policy`, `schedule_report_policy`, `retention_policy`.
- Gmail scope autorizado, mailbox analizado, query/window real, timezone y límites efectivos.
- Reglas determinísticas efectivas: dominios internos, exclusiones, automatismos, horarios, thresholds.
- IA efectiva: provider, model id, prompt version/hash, temperatura, token limits, auto-apply threshold, campos enviados y minimización aplicada.
- Versiones de código/config schema y migration version.
- Consentimiento IA vigente y `data_minimization_mode`.
- Métricas de coste/uso IA y conteo de mensajes enviados a IA, sin cuerpos completos.

## 4. Defaults seguros

- MVP endurecido: owner-scoped, IA opt-in visible, `gmail.readonly`, retención 30 días, no scheduler por defecto, no reportes externos si no hay destinatarios confirmados.
- Beta privada: org por invitación, 1 mailbox/org inicial, IA opt-in por org, reportes deshabilitados hasta configurar destinatarios, retención default 30 o 90 días pendiente de owner, límites bajos de threads/run.
- SaaS público: self-serve solo después de OAuth verification, delete/export UI, cuotas por plan, rate limits, tenant isolation, DPA/subprocessors, incident/audit logs.

## 5. UI vs avanzado/admin

- UI guiada: nombre org, conectar Gmail, mailbox auditado, dominios internos, equipo que responde, qué cuenta como solicitud válida, exclusiones comunes, IA on/off, horario/ventana, destinatarios de reporte, retención simple.
- Admin avanzado: prompt/policy versions, thresholds IA, max tokens/mensajes, query Gmail avanzada, backfill, report templates, data region, legal hold, export/delete, roles.
- No exponer controles peligrosos como checkboxes sueltas sin explicar impacto en métricas y privacidad.

## 6. Evitar UI de checkboxes/keywords

- Usar wizard por intención: “¿Quién responde?”, “¿Qué debe contar como solicitud?”, “¿Qué no es responsabilidad del equipo?”, “¿Qué evidencia necesitas?”.
- Ofrecer presets editables: soporte TI, soporte general, operaciones, mailbox compartido, supervisor recibe copia.
- Mostrar preview con ejemplos: “estos hilos se contarían / se ignorarían / irían a revisión”.
- Convertir keywords en reglas explicables con categorías, prioridad, simulación y trazabilidad.
- Incluir “confianza esperada” y “riesgo de falsos negativos” al cambiar reglas.

## 7. Cambios backend/API/storage

- Crear contratos `GET/PUT /orgs/:id/config`, `GET/POST /mailboxes`, `POST /analysis-runs` usando `policy_version_id` o snapshot.
- Migrar storage a paths tenant-scoped: `orgs/{orgId}/mailboxes`, `policyVersions`, `analysisRuns`, `scheduleConfigs`, `scheduleStates`.
- Repositorios deben requerir `org_id`/membership y responder 404 ante recursos ajenos.
- Validación schema server-side para dominios, emails, timezone IANA, límites y horarios.
- Reemplazar seeds `SCHEDULE_*` como bootstrap dev/admin, no fuente SaaS.
- Añadir retention jobs, delete/export, audit log de cambios de policy.
- Tests: IDOR dos usuarios/orgs, snapshot de policy por run, no persistencia `body_text`, validación timezone/domains, scheduler por tenant.

## 8. Cambios worker IA

- Eliminar contexto hardcoded de `settings.py`/prompt; recibir `AiAuditorPolicySnapshot` en cada request.
- Request IA debe incluir `org_context` minimizado: mailbox purpose, desk aliases, internal domains, responder team, valid-request criteria, non-responsibility rules, language.
- Prompt debe ser template versionado, con campos permitidos y salida JSON estricta.
- Registrar `prompt_version`, `model_id`, thresholds y minimización en `AiAuditResult`.
- Mantener body excerpt transitorio con límites configurables; nunca devolver ni persistir cuerpo completo.
- Añadir tests contra prompt hardcoded y evals de no inventar IDs, no inferir clientes sin evidencia y marcar ambiguous cuando falte contexto.

## 9. Handoff para Claude: configuración guiada

Claude debe diseñar una experiencia de configuración por política operacional, no un formulario plano. Flujo recomendado:
1. Crear organización y confirmar timezone/idioma.
2. Conectar Gmail con explicación `gmail.readonly`.
3. Definir mailbox: casilla principal, alias de soporte, si es supervisor/superset.
4. Definir equipo interno: dominios, miembros/aliases que cuentan como respuesta.
5. Definir qué cuenta como solicitud válida y qué queda fuera.
6. Configurar IA con consentimiento y detalle de datos enviados.
7. Configurar reportes/retención.
8. Pantalla de preview con ejemplos y riesgos.

## Handoff para Pi/Claude

Antes de diseñar o implementar UI de configuración deben confirmarse: público beta, modelo org/roles, IA opt-in/mandatory, retención default, nivel de detalle en reportes, report language/tone, Workspace vs Gmail personal y si el mailbox inicial es casilla compartida, alias o casilla personal supervisora.

## BLOCKED_QUESTIONS

1. ¿El producto beta será para organizaciones con casilla compartida/alias o para usuarios individuales?
   - Opción A: organizaciones con 1 mailbox inicial.
   - Opción B: usuarios individuales self-serve.
   - Recomendación: A para beta privada.
   - Impacto si se decide mal: modelo de ownership, UI y OAuth quedan mal diseñados.

2. ¿La IA será obligatoria, opt-in u opt-out?
   - Opción A: opt-in explícito por org.
   - Opción B: obligatoria.
   - Opción C: opt-out.
   - Recomendación: A.
   - Impacto: privacidad, subprocessors, confianza y coste.

3. ¿Retención default?
   - Opción A: 30 días.
   - Opción B: 90 días.
   - Opción C: configurable antes del primer run.
   - Recomendación: 30 días beta, configurable por admin.
   - Impacto: riesgo privacy/compliance y utilidad histórica.

4. ¿Reportes por email pueden incluir asuntos/remitentes?
   - Opción A: solo métricas.
   - Opción B: métricas + asuntos/remitentes de revisión.
   - Opción C: configurable.
   - Recomendación: C con default A.
   - Impacto: exposición de datos sensibles fuera de la app.

5. ¿Scheduler debe ser laboral simple o policy avanzada?
   - Opción A: lunes-viernes horario fijo.
   - Opción B: días/horas/ventanas configurables.
   - Recomendación: B para beta privada, con preset simple.
   - Impacto: SaaS fuera de Chile rompe ventanas y reportes.

6. ¿Se soportará Gmail personal o solo Google Workspace?
   - Opción A: Workspace only.
   - Opción B: ambos.
   - Recomendación: Workspace only para beta.
   - Impacto: confianza, dominios internos, OAuth verification y soporte.

Verificación realizada: lectura estática de los archivos solicitados y búsquedas puntuales de hardcodes; no ejecuté tests porque la task pedía solo planificación read-only.
