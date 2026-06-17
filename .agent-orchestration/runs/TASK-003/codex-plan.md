Nota de ejecución: Codex trabajó en modo read-only y no leyó `.env`, `.env.*`, `secrets/*`, tokens ni credenciales. El CLI no pudo escribir desde dentro del sandbox, pero Pi guardó el output final en este archivo.

# TASK-003 — Plan P0 authz/ownership backend

Fecha: 2026-06-17  
Modo: read-only. No leí `.env`, `.env.*`, `secrets/*`, tokens ni credenciales.

## 1. Inventario exacto de endpoints afectados

| Endpoint | Estado actual | Acción |
|---|---|---|
| `POST /analysis-runs` | Requiere sesión; crea `user_email` desde sesión | Mantener + test owner desde sesión |
| `GET /analysis-runs` | Requiere sesión; filtra por `user_email` | Mantener + regresión multiusuario |
| `GET /analysis-runs/{id}` | Sin sesión/ownership | Proteger |
| `GET /analysis-runs/{id}/status` | Reusa handler vulnerable | Proteger |
| `POST /analysis-runs/{id}/start` | Ya valida owner | Mantener + regresión |
| `GET /analysis-runs/{id}/events` | SSE sin sesión/ownership | Validar antes de abrir stream |
| `GET /analysis-runs/{id}/metrics` | Sin sesión/ownership | Proteger |
| `GET /analysis-runs/{id}/threads` | Sin sesión/ownership | Proteger |
| `GET /threads/{thread_id}` | Lookup global sin sesión | Proteger por run owner |
| `PATCH /threads/{thread_id}/manual-review` | Mutación sin sesión; `reviewer_label` spoofeable | Proteger y setear reviewer desde sesión |

Recomiendo `404` para recurso inexistente o ajeno en IDs opacos, para reducir enumeración. Pi debe confirmar si prefiere mantener `403` por compatibilidad.

## 2. Modelo de ownership mínimo

- Identidad: `UserSession.google_account_email`.
- Owner canónico: `AnalysisRun.user_email`.
- `EmailThread`, `EmailMessage` y `ManualReview` heredan owner vía `analysis_run_id`.
- Ningún endpoint público debe confiar en `user_email`, `owner`, `tenant_id` o `reviewer_label` enviados por cliente.
- No introducir tenant completo todavía, pero nombrar helpers como `require_owned_run` / `require_owned_thread` para migrar luego a `tenant_id + membership`.
- No cambiar scopes Gmail; mantener `gmail.readonly`.

## 3. Cambios de `StorageRepository`

Agregar métodos owner-scoped para uso HTTP:

```rust
get_analysis_run_for_user(id, user_email) -> Option<AnalysisRun>
list_threads_for_user(run_id, user_email) -> Option<Vec<EmailThread>>
get_thread_for_user(thread_id, user_email) -> Option<EmailThread>
add_manual_review_for_user(run_id, user_email, review) -> Result<()>
```

Semántica: retornar `None` si no existe o no pertenece al usuario. En `MemoryStorage`, corregir `add_manual_review`: hoy muta thread antes de comprobar que el run existe. En `FirestoreStorage`, se puede conservar el scan actual de `get_thread` para MVP, pero filtrando owner antes de retornar.

## 4. Tests propuestos con dos usuarios

Fixtures: `alice@example.com` y `bob@example.com`, dos sesiones firmadas, `run-a`, `run-b`, `thread-a`, `thread-b`.

Storage:
- owned run retorna para owner.
- foreign run/thread retorna `None`.
- foreign manual review no muta thread ni metrics.
- mismatch `run_id/thread.analysis_run_id` falla.

HTTP IDOR:
- Sin cookie: `401` en endpoints protegidos.
- Bob no puede leer `GET /analysis-runs/run-a`, `/status`, `/metrics`, `/threads`, `/events`.
- Bob no puede leer `GET /threads/thread-a`.
- Bob no puede mutar `PATCH /threads/thread-a/manual-review`.
- Alice sí puede leer/mutar recursos propios.
- `GET /analysis-runs` solo lista runs propios.
- `POST /analysis-runs/{id}/start` con usuario ajeno falla antes de decrypt/spawn.
- Si cliente envía `reviewer_label = "spoofed-admin"`, backend persiste email autenticado.

## 5. Orden de implementación

1. Escribir tests rojos IDOR.
2. Agregar métodos owner-scoped a `StorageRepository` y `MemoryStorage`.
3. Implementar en `FirestoreStorage`.
4. Agregar helpers HTTP `require_owned_run` y `require_owned_thread`.
5. Wirear handlers vulnerables.
6. En manual review, setear `reviewer_label` desde sesión.
7. Agregar regresión para `start`.
8. Ejecutar `cargo fmt`, `cargo test`, `cargo clippy --all-targets -D warnings`.

## 6. Riesgos frontend

- `GET /threads/:id` y manual review podrán devolver `401/404/403`; UI actual no muestra error específico.
- `reviewer_label` dejará de reflejar `"local-user"` y pasará a backend-controlado.
- `events` no parece usado hoy; si se usa con `EventSource` cross-origin, debe ir con credenciales.
- `404` para foreign resources es correcto en seguridad, pero requiere microcopy genérico.

## 7. Diff plan por archivo

- `apps/api/src/storage/mod.rs`: trait owner-scoped, MemoryStorage, tests.
- `apps/api/src/firestore/mod.rs`: implementaciones owner-scoped sin migración Firestore.
- `apps/api/src/http/mod.rs`: sesión/ownership en handlers, SSE con preflight, manual review backend-controlled, tests IDOR.
- `apps/api/src/analysis/mod.rs`: sin cambios.
- `apps/api/src/main.rs`: sin cambios.
- `apps/web/src/**`: sin cambios en TASK-003; solo handoff UX posterior.

## 8. Criterios de aceptación

- Protege `GET /analysis-runs/:id`, `/status`, `/metrics`, `/threads`, `/events`, `GET /threads/:thread_id`, `PATCH /threads/:thread_id/manual-review`.
- Incluye pruebas IDOR con dos usuarios.
- Usuario ajeno no recibe datos ni muta estado.
- Manual review ajena no cambia clasificación, flags, timestamps ni métricas.
- `reviewer_label` no es spoofeable.
- No requiere multi-tenant completo ni bloquea evolución a tenant.
- No cambia scopes Gmail.
- Checks Rust pasan.

## Handoff para Pi

Conviene implementar directamente después de que Pi confirme la política `404` vs `403` para recursos ajenos. No hace falta segunda revisión del plan; sí recomiendo revisión de seguridad sobre el diff antes de merge por ser P0 authz.