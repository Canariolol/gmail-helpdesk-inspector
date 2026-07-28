# TASK-007 — Plan Codex/Pi para configuración SaaS incremental

Modo: planificación read-only. No se modificó código fuente. No se leyeron `.env`, `.env.*`, `secrets/*`, tokens ni credenciales.

## ASSUMPTIONS

- Las decisiones de producto en `D-011` están aprobadas: beta privada, 1 mailbox Workspace por org, IA opt-in, retención default 30 días configurable, reportes configurables con default solo métricas y scheduler con preset simple.
- El primer incremento puede mantener compatibilidad con `analysisRuns/{run_id}` top-level si cada run nuevo guarda `org_id`, `mailbox_id`, membership verificada y snapshot inmutable. Mover todos los runs a paths tenant-scoped queda como fase posterior.
- Workspace-only en esta iteración se aplica con allowlist/beta gate y validación conservadora del dominio de la cuenta Gmail. Verificación fuerte de dominio Workspace queda fuera porque puede requerir señales adicionales de Google y no debe ampliar scopes sin decisión explícita.

## 1. Recomendación de incremento técnico mínimo

Implementar una capa de configuración SaaS beta, no un modelo multi-org público completo:

1. Provisionar una organización implícita por usuario autenticado en el primer `GET /me/org/config`.
2. Crear una membership `owner` para ese usuario y una mailbox inicial asociada al email de Gmail autorizado.
3. Guardar un `PolicyDraft` mutable por org y emitir un `PolicyVersion` inmutable en cada cambio efectivo.
4. Crear cada `AnalysisRun` desde una ventana de fechas + `policy_version_id`, no desde listas ad hoc enviadas por `FilterBar`.
5. Guardar snapshot completo de políticas dentro del run para trazabilidad y reproducibilidad.
6. Hacer IA opt-in por org: si `ai_policy.enabled=false`, el backend no llama al worker.
7. Cambiar scheduler/reportes para leer `ScheduleReportPolicy`; default `enabled=false` o preset simple desactivado hasta confirmar destinatarios, y contenido default `metrics_only`.
8. Mantener `gmail.readonly` y la política de no persistir cuerpos completos. Los excerpts para IA siguen siendo transitorios y limitados por policy.

Este incremento elimina hardcodes de dominio/timezone/filtros/IA/reportes sin requerir billing, roles complejos, self-serve multi-org ni migración profunda de Firestore.

## 2. Modelo de datos Rust/Firestore

### Rust propuesto

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub status: OrganizationStatus, // private_beta | disabled
    pub default_timezone: String,
    pub locale: String, // es-CL default
    pub created_by_user_email: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    pub org_id: String,
    pub user_email: String,
    pub role: OrgRole, // owner | admin | analyst | viewer
    pub status: MembershipStatus, // active | disabled
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mailbox {
    pub id: String,
    pub org_id: String,
    pub google_account_email: String,
    pub workspace_domain: String,
    pub display_name: String,
    pub purpose: MailboxPurpose, // support_shared | supervisor_inbox | other
    pub authorized_by_user_email: String,
    pub gmail_scope_snapshot: Vec<String>,
    pub connected_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDraft {
    pub org_id: String,
    pub mailbox_id: String,
    pub analysis_policy: AnalysisPolicy,
    pub ai_policy: AiPolicy,
    pub schedule_report_policy: ScheduleReportPolicy,
    pub retention_policy: RetentionPolicy,
    pub updated_by_user_email: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyVersion {
    pub id: String,
    pub org_id: String,
    pub version: u64,
    pub schema_version: u32,
    pub policy_hash: String,
    pub snapshot: PolicySnapshot,
    pub created_by_user_email: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySnapshot {
    pub mailbox: MailboxSnapshot,
    pub analysis: AnalysisPolicy,
    pub ai: AiPolicy,
    pub schedule_report: ScheduleReportPolicy,
    pub retention: RetentionPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisPolicy {
    pub timezone: String,
    pub internal_domains: Vec<String>,
    pub responder_emails: Vec<String>,
    pub mailbox_aliases: Vec<String>,
    pub valid_request_criteria: Vec<String>,
    pub non_responsibility_rules: Vec<String>,
    pub ignored_senders: Vec<String>,
    pub ignored_domains: Vec<String>,
    pub ignored_keywords: Vec<String>,
    pub default_time_from: String,
    pub default_time_to: String,
    pub max_threads_per_run: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiPolicy {
    pub enabled: bool,
    pub consent_granted_at: Option<DateTime<Utc>>,
    pub provider: String, // bedrock
    pub model_id: String,
    pub prompt_version: String,
    pub auto_apply_threshold: f64,
    pub manual_review_threshold: f64,
    pub max_audit_messages: u32,
    pub max_body_chars_per_message: u32,
    pub allowed_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleReportPolicy {
    pub scheduler_enabled: bool,
    pub preset: SchedulePreset, // weekdays_08_local | disabled
    pub timezone: String,
    pub report_recipients: Vec<String>,
    pub report_content: ReportContentPolicy,
    pub failure_notice_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportContentPolicy {
    pub mode: ReportMode, // metrics_only | metrics_and_review_items
    pub include_subjects: bool,
    pub include_senders: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub retention_days: u32, // default 30
    pub delete_threads_and_messages: bool,
    pub delete_ai_audits: bool,
}
```

### Firestore propuesto

Incremental y owner-safe:

```text
ownerProfiles/{sha256(normalized_email)}
  user_email
  default_org_id
  created_at
  updated_at

organizations/{org_id}
  Organization

organizations/{org_id}/memberships/{sha256(normalized_email)}
  Membership

organizations/{org_id}/mailboxes/{mailbox_id}
  Mailbox

organizations/{org_id}/policyDraft/current
  PolicyDraft

organizations/{org_id}/policyVersions/{policy_version_id}
  PolicyVersion

organizations/{org_id}/scheduleStates/{mailbox_id}
  ScheduleStateV2

analysisRuns/{run_id}
  AnalysisRun V2 con org_id, mailbox_id y policy_snapshot
analysisRuns/{run_id}/threads/{thread_id}
analysisRuns/{run_id}/threads/{thread_id}/messages/{message_id}
analysisRuns/{run_id}/threads/{thread_id}/aiAudits/{audit_id}
analysisRuns/{run_id}/manualReviews/{review_id}
```

No usar emails crudos como document id nuevos. Para beta, `analysisRuns` puede seguir top-level para reducir blast radius, pero todos los accesos deben pasar por `require_org_member` y responder `404` si el recurso no pertenece a la org del usuario.

## 3. API endpoints mínimos y contratos JSON

### `GET /me/org/config`

Provisiona org implícita si no existe y devuelve estado de setup.

```json
{
  "org": {
    "id": "org_01...",
    "name": "Acme",
    "default_timezone": "America/Santiago",
    "locale": "es-CL"
  },
  "membership": { "role": "owner" },
  "mailbox": {
    "id": "mbx_01...",
    "google_account_email": "soporte@acme.cl",
    "workspace_domain": "acme.cl",
    "gmail_scope_snapshot": ["https://www.googleapis.com/auth/gmail.readonly"]
  },
  "policy_version": {
    "id": "pol_01...",
    "version": 3,
    "policy_hash": "sha256:..."
  },
  "draft": {
    "analysis_policy": {
      "timezone": "America/Santiago",
      "internal_domains": ["acme.cl"],
      "responder_emails": [],
      "mailbox_aliases": ["soporte@acme.cl"],
      "valid_request_criteria": [],
      "non_responsibility_rules": [],
      "ignored_senders": [],
      "ignored_domains": [],
      "ignored_keywords": [],
      "default_time_from": "00:00",
      "default_time_to": "23:59",
      "max_threads_per_run": 50
    },
    "ai_policy": {
      "enabled": false,
      "consent_granted_at": null,
      "provider": "bedrock",
      "model_id": "configured-server-side",
      "prompt_version": "helpdesk-auditor-v1",
      "auto_apply_threshold": 0.92,
      "manual_review_threshold": 0.72,
      "max_audit_messages": 14,
      "max_body_chars_per_message": 280,
      "allowed_fields": ["headers", "participants", "snippet", "body_excerpt"]
    },
    "schedule_report_policy": {
      "scheduler_enabled": false,
      "preset": "weekdays_08_local",
      "timezone": "America/Santiago",
      "report_recipients": [],
      "report_content": {
        "mode": "metrics_only",
        "include_subjects": false,
        "include_senders": false
      },
      "failure_notice_enabled": true
    },
    "retention_policy": {
      "retention_days": 30,
      "delete_threads_and_messages": true,
      "delete_ai_audits": true
    }
  },
  "setup_state": {
    "ready_for_analysis": false,
    "missing": ["internal_domains", "valid_request_criteria"]
  }
}
```

### `PUT /me/org/config`

Valida y guarda un draft completo o por secciones. Si el hash cambia, crea `PolicyVersion`.

```json
{
  "org": { "name": "Acme", "default_timezone": "America/Santiago", "locale": "es-CL" },
  "mailbox": { "display_name": "Mesa de ayuda", "purpose": "support_shared" },
  "analysis_policy": {
    "timezone": "America/Santiago",
    "internal_domains": ["acme.cl"],
    "responder_emails": ["soporte@acme.cl"],
    "mailbox_aliases": ["soporte@acme.cl"],
    "valid_request_criteria": ["Clientes externos piden soporte tecnico u operativo"],
    "non_responsibility_rules": ["Ventas, facturacion y marketing no cuentan como mesa de ayuda"],
    "ignored_senders": [],
    "ignored_domains": ["calendar.google.com"],
    "ignored_keywords": ["newsletter"],
    "default_time_from": "00:00",
    "default_time_to": "23:59",
    "max_threads_per_run": 50
  },
  "ai_policy": {
    "enabled": true,
    "consent_confirmed": true,
    "auto_apply_threshold": 0.92,
    "manual_review_threshold": 0.72,
    "max_audit_messages": 14,
    "max_body_chars_per_message": 280
  },
  "schedule_report_policy": {
    "scheduler_enabled": true,
    "preset": "weekdays_08_local",
    "timezone": "America/Santiago",
    "report_recipients": ["operaciones@acme.cl"],
    "report_content": {
      "mode": "metrics_only",
      "include_subjects": false,
      "include_senders": false
    },
    "failure_notice_enabled": true
  },
  "retention_policy": { "retention_days": 30 }
}
```

Respuesta:

```json
{
  "policy_version": {
    "id": "pol_01...",
    "version": 4,
    "policy_hash": "sha256:...",
    "created_at": "2026-06-17T06:30:00Z"
  },
  "setup_state": { "ready_for_analysis": true, "missing": [] }
}
```

Validaciones mínimas: emails válidos, dominios sin `@` normalizados, timezone IANA parseable, `retention_days` entre 7 y 365 para beta, thresholds `0..1`, `max_threads_per_run` acotado por server, report recipients requeridos si scheduler/reportes están enabled.

### `POST /analysis-runs`

El cliente ya no envía dominios/filtros como fuente de verdad. Envía ventana y opcionalmente `policy_version_id`; si se omite, usar la versión vigente.

```json
{
  "date_from": "2026-06-01",
  "date_to": "2026-06-12",
  "time_from": "00:00",
  "time_to": "23:59",
  "policy_version_id": "pol_01..."
}
```

Respuesta: `AnalysisRun` V2 con snapshot incluido o resumido:

```json
{
  "id": "run_01...",
  "org_id": "org_01...",
  "mailbox_id": "mbx_01...",
  "user_email": "owner@acme.cl",
  "trigger_type": "manual",
  "policy_version_id": "pol_01...",
  "policy_hash": "sha256:...",
  "gmail_scope_snapshot": ["https://www.googleapis.com/auth/gmail.readonly"],
  "retention_expires_at": "2026-07-17T06:30:00Z",
  "data_minimization_mode": "metadata_snippets_excerpts_only",
  "config": {
    "date_from": "2026-06-01",
    "date_to": "2026-06-12",
    "time_from": "00:00",
    "time_to": "23:59",
    "timezone": "America/Santiago",
    "internal_domains": ["acme.cl"],
    "ignored_senders": [],
    "ignored_domains": ["calendar.google.com"],
    "ignored_keywords": ["newsletter"]
  },
  "policy_snapshot": { "...": "snapshot completo inmutable" },
  "status": "pending",
  "metrics": { "...": "igual que hoy" }
}
```

### Endpoints existentes

`GET /analysis-runs`, `GET /analysis-runs/{id}`, `GET /analysis-runs/{id}/threads`, `GET /threads/{thread_id}`, `PATCH /threads/{thread_id}/manual-review` se mantienen, pero su authz debe cambiar de `run.user_email == session.email` a:

1. legacy fallback: si `org_id` falta, validar `user_email`;
2. V2: validar membership activa para `run.org_id`;
3. responder `404` ante recurso ajeno.

## 4. Migración desde configuración owner-scoped actual

1. Agregar modelos V2 y repositorio sin borrar campos actuales.
2. En `GET /me/org/config`, resolver `ownerProfiles/{email_hash}`. Si no existe:
   - crear `organizations/{uuid}`;
   - crear membership owner;
   - crear mailbox con `google_account_email=session.google_account_email`;
   - inferir `workspace_domain` del email y rechazar dominios consumer conocidos para beta (`gmail.com`, `googlemail.com`) con error de setup, no con scope nuevo;
   - construir `PolicyDraft` desde `scheduleConfigs/{email}` si existe; si no, desde el ultimo run del usuario; si no, defaults seguros.
3. Crear `PolicyVersion` inicial con `ai.enabled=false`, retención 30 y reportes `metrics_only`.
4. Los runs legacy siguen visibles por `user_email` hasta que una migración opcional los anote con `org_id`, `mailbox_id` y un `legacy_policy_snapshot`.
5. Nuevos runs siempre nacen desde `PolicyVersion`.
6. `scheduleConfigs/{email}` queda read-only/legacy. La fuente SaaS pasa a `organizations/{org_id}/policyDraft/current.schedule_report_policy`.

No copiar, leer ni re-encriptar tokens en esta migración. La sesión actual sigue siendo la fuente para OAuth; mailbox solo referencia el email autorizado.

## 5. Cambios necesarios en `AnalysisRun`

Agregar campos V2 con `serde(default)` para compatibilidad:

```rust
pub struct AnalysisRun {
    pub id: String,
    pub user_email: String, // legacy + actor visible
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub mailbox_id: Option<String>,
    #[serde(default)]
    pub trigger_type: Option<TriggerType>, // manual | scheduled | backfill
    #[serde(default)]
    pub policy_version_id: Option<String>,
    #[serde(default)]
    pub policy_hash: Option<String>,
    #[serde(default)]
    pub policy_snapshot: Option<PolicySnapshot>,
    #[serde(default)]
    pub gmail_scope_snapshot: Vec<String>,
    #[serde(default)]
    pub retention_expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub data_minimization_mode: Option<String>,
    pub config: AnalysisConfig,
    // resto igual
}
```

`AnalysisConfig` puede mantenerse como vista derivada para Gmail/query y compatibilidad frontend. La fuente auditable debe ser `policy_snapshot`.

Reglas:

- `policy_snapshot` se copia al crear el run y nunca se recalcula desde el draft.
- `retention_expires_at = created_at + retention_days`.
- `config.internal_domains`, `ignored_*`, `timezone` y limites se derivan de snapshot.
- `AiAuditResult` debe sumar metadata: `policy_version_id`, `prompt_version`, `model_id`, `auto_apply_threshold`, `max_audit_messages`, `max_body_chars_per_message`.

## 6. Scheduler/reportes y worker IA

### Scheduler/reportes

- Reemplazar `list_schedule_configs()` para SaaS por `list_enabled_schedule_policies()` que devuelve org/mailbox/policy vigente.
- `ScheduleState` V2 debe keyed por `org_id + mailbox_id + window`, no por `user_email`.
- El preset mínimo `weekdays_08_local` usa timezone de policy, no `America/Santiago` global.
- `create_scheduled_run` debe crear run con `trigger_type=scheduled` y snapshot.
- `fresh_access_token` busca sesión con refresh token del `mailbox.google_account_email`.
- Default reportes:
  - `mode=metrics_only`;
  - `include_subjects=false`;
  - `include_senders=false`;
  - si no hay recipients confirmados, no enviar reporte externo.
- El template actual debe dejar de incluir saludo informal, footer fijo `America/Santiago` y asuntos/remitentes salvo que la policy lo habilite.

### Worker IA

- Si `policy_snapshot.ai.enabled=false`, no llamar al worker.
- `AuditThreadRequest` debe incluir `policy_snapshot.ai` y contexto operacional minimizado:
  - mailbox purpose y aliases;
  - internal domains/responder emails;
  - criterios de solicitud valida;
  - reglas de no responsabilidad;
  - idioma/locale.
- `bedrock.py` debe eliminar referencias a West Ingenieria, N1 hardcoded, `settings.analyzed_mailbox`, `settings.desk_mailbox`, `settings.internal_domain` como comportamiento de producto. `Settings` queda solo para provider/credenciales/model default.
- Limites `MAX_AUDIT_MESSAGES`, `MAX_AUDIT_BODY_CHARS` y threshold auto-apply salen de policy snapshot.
- El prompt debe registrar `prompt_version` y prohibir inventar IDs igual que hoy.
- No devolver ni persistir cuerpos completos. `body_text` solo puede ser excerpt transitorio ya truncado.

## 7. Orden de implementación con archivos tocados

Fase 1 — modelos y storage:

- `apps/api/src/analysis/mod.rs`: campos V2 en `AnalysisRun`, `AiAuditResult`, defaults serde.
- `apps/api/src/storage/mod.rs`: trait para org/config/policy versions/membership y memory storage.
- `apps/api/src/firestore/mod.rs`: paths Firestore nuevos, email hash helper, sanitizacion existente de `body_text` se conserva.
- Nuevo modulo sugerido `apps/api/src/org_config/mod.rs` o `apps/api/src/policies/mod.rs`.

Fase 2 — API config y authz:

- `apps/api/src/http/mod.rs`: rutas `GET/PUT /me/org/config`, create run desde policy, `require_org_member`, fallback legacy.
- `apps/api/src/http/internal.rs`: scheduler interno debe resolver policy V2 si aplica.
- `apps/api/src/config/mod.rs`: defaults server-side no sensibles para limites, sin hardcodes de empresa.

Fase 3 — scheduler/reportes:

- `apps/api/src/scheduler/model.rs`: `ScheduleStateV2`, `SchedulePreset`, `ReportContentPolicy`.
- `apps/api/src/scheduler/mod.rs`: loop por timezone/policy y estado org/mailbox.
- `apps/api/src/scheduler/window.rs`: generalizar SCL a timezone recibido.
- `apps/api/src/report/template.rs`: contenido metrics-only default y copy neutral.

Fase 4 — IA:

- `apps/ai-worker/src/ai_worker/schemas.py`: policy/context en request/response.
- `apps/ai-worker/src/ai_worker/bedrock.py`: prompt desde policy snapshot, sin hardcodes de empresa.
- `apps/ai-worker/src/ai_worker/settings.py`: remover defaults de producto; mantener provider config.
- `apps/ai-worker/tests/*`: actualizar tests de prompt/schema.

Fase 5 — frontend contrato minimo:

- `apps/web/src/api/types.ts`: tipos `OrgConfig`, `PolicyDraft`, `PolicyVersion`.
- `apps/web/src/components/filters/FilterBar.tsx`: reemplazar hardcodes por policy vigente o wizard.
- `apps/web/src/views/*`: nueva configuracion guiada segun plan Claude.
- `apps/web/src/lib/format.ts`: timezone desde run/config, no global fijo.

## 8. Tests requeridos

Backend Rust:

- Provisioning: primer `GET /me/org/config` crea org, membership owner, mailbox y policy version inicial.
- Idempotencia: segundo `GET` no duplica org ni versiones.
- Validacion: timezone invalido, email invalido, retention fuera de rango, threshold fuera de rango y recipient faltante para scheduler enabled devuelven `400`.
- IDOR: usuario B recibe `404` para runs/threads/config de org A.
- Legacy fallback: runs sin `org_id` siguen accesibles solo para `user_email` dueño.
- Snapshot: editar config despues de crear run no cambia `run.policy_snapshot`.
- IA opt-in: con `ai.enabled=false`, `audit_thread` no se invoca y tokens quedan cero.
- No persistencia: Firestore y memory storage no guardan `body_text` completo; agregar test especifico contra mensajes con body.
- Scheduler: estado V2 por org/mailbox/window, no por email; preset timezone-local; no envia reportes si recipients vacios.
- Reportes: default `metrics_only` no contiene subject/from de hilos.
- Anti-enumeracion: resource ajeno con ID opaco responde `404`, sesion ausente `401`.

AI worker Python:

- Prompt no contiene `West`, `west-ingenieria`, personas ni mailbox hardcoded.
- Request con policy genera prompt con contexto operacional recibido.
- Respuesta sigue schema estricto y rechaza IDs inexistentes desde backend.
- Body excerpts respetan `max_body_chars_per_message`.

Frontend:

- `npm --prefix apps/web run build`.
- Tests/component smoke del wizard cuando existan.
- Confirmar que IA aparece off por defecto y requiere confirmacion explicita.

Checks finales:

- `cargo test --manifest-path apps/api/Cargo.toml`
- `cargo clippy --manifest-path apps/api/Cargo.toml --all-targets -- -D warnings`
- `python3 -m pytest apps/ai-worker/tests`
- `npm --prefix apps/web run build`

## 9. Riesgos de seguridad, privacidad e IDOR

- IDOR por `org_id`: cualquier endpoint que acepte ID debe verificar membership activa y devolver `404` para ajenos.
- Emails como IDs: no usar emails crudos en nuevos document IDs; usar hash normalizado o UUID. Los emails pueden seguir como campos internos.
- Firestore top-level scans: mantener `analysisRuns` top-level es aceptable solo incrementalmente; requiere filtros en backend y tests IDOR. Tenant-scoped real queda como P2.
- Workspace-only debil: con `gmail.readonly` no hay verificacion fuerte de dominio Workspace. Mitigacion beta: invite/allowlist + bloquear consumer domains + copy honesto "cuenta Workspace conectada", no "dominio verificado" salvo que se implemente verificacion.
- IA y privacidad: opt-in debe guardarse con timestamp; el worker solo recibe campos permitidos y excerpts truncados; no loggear prompts con cuerpos.
- Reportes por email: default metrics-only evita exfiltrar asuntos/remitentes; cualquier detalle debe ser opt-in por admin.
- Retencion: guardar `retention_expires_at` sin job de borrado real puede crear falsa promesa. Si la UI promete borrado automatico, implementar job o mostrar "se eliminara automaticamente cuando el proceso de retencion este activo" no es aceptable para beta.
- Scheduler: idempotencia sin transaccion puede duplicar runs en multi-instancia. Para beta de una instancia es tolerable; antes de multi-instancia usar precondiciones Firestore o lock transaccional.
- Errores publicos: evitar devolver errores crudos de Google/Bedrock/Firestore al cliente porque pueden filtrar detalles.
- CSRF/rate limit: mutaciones `PUT /me/org/config`, `POST /analysis-runs`, manual review y scheduler interno necesitan proteccion antes de beta amplia.

No hay `BLOCKED_QUESTIONS` para este incremento: las decisiones necesarias ya estan aprobadas. Las dudas restantes son handoffs de implementacion/validacion, no bloquean el plan.
