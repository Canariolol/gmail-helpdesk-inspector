# TASK-007A — Implementación backend policy-first incremental

Fecha: 2026-06-17

## Alcance implementado

### Modelos/policies

Nuevo módulo:

- `apps/api/src/policies.rs`

Incluye:

- `Organization`
- `Membership`
- `Mailbox`
- `PolicyDraft`
- `PolicyVersion`
- `PolicySnapshot`
- `AnalysisPolicy`
- `AiPolicy`
- `ScheduleReportPolicy`
- `RetentionPolicy`
- `SetupState`
- helpers de provisioning, normalización, hash y expiración.

### API

Nuevos endpoints:

- `GET /me/org/config`
  - requiere sesión;
  - provisiona organización implícita si no existe;
  - crea membership owner, mailbox y policy version inicial;
  - IA queda off por defecto;
  - retención default 30 días;
  - reportes metrics-only por defecto.

- `PUT /me/org/config`
  - requiere sesión;
  - actualiza draft;
  - valida timezone IANA, retención 7..365, thresholds 0..1;
  - exige consentimiento explícito para activar IA;
  - crea nueva `PolicyVersion` si cambia el hash.

`POST /analysis-runs` ahora soporta dos modos:

- legacy: si el cliente envía dominios/filtros, conserva comportamiento anterior;
- policy mode: si el cliente envía solo ventana o `policy_version_id`, usa policy vigente/snapshot.

### AnalysisRun snapshot

`AnalysisRun` ahora incluye campos V2 opcionales y compatibles con legacy:

- `org_id`
- `mailbox_id`
- `trigger_type`
- `policy_version_id`
- `policy_hash`
- `policy_snapshot`
- `gmail_scope_snapshot`
- `retention_expires_at`
- `data_minimization_mode`

### Storage

`StorageRepository` ahora soporta:

- `get_org_config_for_user`
- `upsert_org_config`
- `get_policy_version`
- `user_is_org_member`

Implementado en:

- `MemoryStorage`
- `FirestoreStorage`

Firestore guarda paths incrementales bajo:

- `ownerProfiles/{email_hash}/config/current`
- `organizations/{org_id}`
- `organizations/{org_id}/memberships/{email_hash}`
- `organizations/{org_id}/mailboxes/{mailbox_id}`
- `organizations/{org_id}/policyDraft/current`
- `organizations/{org_id}/policyVersions/{policy_version_id}`

### Authz

`require_owned_run` ahora:

- valida membership si el run tiene `org_id`;
- conserva fallback legacy por `user_email`;
- mantiene `404` para recursos ajenos.

### IA opt-in

- Runs legacy conservan comportamiento anterior.
- Runs con policy snapshot solo llaman IA si `policy_snapshot.ai_policy.enabled=true`.
- Límites de auditoría y threshold auto-apply pueden salir del snapshot.
- `AiAuditResult` agrega metadata opcional de policy/prompt/model/threshold/límites.

## Tests agregados

- Provisioning de `/me/org/config`.
- PUT requiere consentimiento IA.
- PUT crea nueva versión de policy.
- Run policy-mode guarda snapshot y retención.
- Compatibilidad legacy en `POST /analysis-runs`.

## Checks ejecutados

```bash
cargo test --manifest-path apps/api/Cargo.toml
cargo clippy --manifest-path apps/api/Cargo.toml --all-targets -- -D warnings
npm --prefix apps/web run build
```

Resultado:

- API: 55 tests OK.
- Clippy: OK.
- Web build: OK, con warning existente de chunk grande.

## Handoffs / límites

- Scheduler V2 tenant/policy aún no está migrado; scheduler legacy sigue funcionando.
- Worker IA todavía no recibe el contexto operacional completo de `PolicySnapshot`; esta implementación sí bloquea llamadas IA cuando la policy está off.
- Retención guarda `retention_expires_at`, pero aún falta job real de borrado.
- Workspace-only se aplica conservadoramente en update config para dominios consumer; verificación fuerte de Workspace queda P2.
