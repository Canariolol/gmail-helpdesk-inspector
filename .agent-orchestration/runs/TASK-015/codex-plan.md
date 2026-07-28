Modo plan, sin cambios. No leí `.env` ni archivos de secrets.

**Recomendación**
Implementar rate limiting explícito en los handlers manuales, no como middleware global. El scheduler entra por `/internal/scheduled-analysis` y por `scheduler::run_scheduled_analysis`, así que limitar solo `POST /analysis-runs` y `POST /analysis-runs/{id}/start` evita bloquearlo.

Usaría un token bucket in-memory por instancia como fase 1, con llave:

```text
lowercase(user_session.google_account_email) + action
```

Acciones:

```text
manual_create_analysis_run
manual_start_analysis_run
```

Defaults beta seguros:

```text
RATE_LIMIT_ENABLED=true
RATE_LIMIT_MANUAL_CREATE_BURST=3
RATE_LIMIT_MANUAL_CREATE_REFILL_SECONDS=1200
RATE_LIMIT_MANUAL_START_BURST=3
RATE_LIMIT_MANUAL_START_REFILL_SECONDS=1200
```

Eso permite 3 intentos rápidos por usuario y luego 1 cada 20 min por acción. Scheduler no consume cuota.

**Plan Incremental**
1. Crear `apps/api/src/rate_limit.rs`
   - `RateLimiter`
   - `RateLimitAction`
   - `RateLimitDecision`
   - token bucket con `Instant`
   - pruning de usuarios inactivos para evitar crecimiento infinito
   - sin dependencias nuevas

2. Extender config en [apps/api/src/config/mod.rs](/home/ryagar/Documentos/DevProyects/gmail-helpdesk-inspector/apps/api/src/config/mod.rs:9)
   - `AppConfig.rate_limit: RateLimitConfig`
   - env vars arriba
   - validación: burst > 0, refill_seconds > 0
   - `test_app_config()` configurable para tests

3. Extender `AppState` en [apps/api/src/http/mod.rs](/home/ryagar/Documentos/DevProyects/gmail-helpdesk-inspector/apps/api/src/http/mod.rs:50)
   - agregar `rate_limiter: Arc<RateLimiter>`
   - inicializar desde config

4. Agregar `ApiError::too_many_requests(retry_after)` en [apps/api/src/http/mod.rs](/home/ryagar/Documentos/DevProyects/gmail-helpdesk-inspector/apps/api/src/http/mod.rs:1558)
   - status `429`
   - body: `{ "error": "Demasiadas solicitudes. Intenta nuevamente más tarde." }`
   - header `Retry-After`

5. Aplicar límite en:
   - [create_analysis_run](/home/ryagar/Documentos/DevProyects/gmail-helpdesk-inspector/apps/api/src/http/mod.rs:756): justo después de `require_session`, antes de crear/storage.
   - [start_analysis_run](/home/ryagar/Documentos/DevProyects/gmail-helpdesk-inspector/apps/api/src/http/mod.rs:903): justo después de `require_session`, antes de mutar el run o hacer `tokio::spawn`.

6. Hardening recomendado para `start`
   - Hoy `start_analysis_run` puede disparar workers repetidos sobre el mismo run si entra dos veces dentro del burst.
   - Agregaría guardia: solo `Pending` puede pasar a `Running`; `Running`/`Completed`/`Failed` deberían devolver `409` o el estado actual. El `429` queda exclusivamente para cuota.

7. Web opcional
   - Agregar mensaje `429` en [apps/web/src/api/client.ts](/home/ryagar/Documentos/DevProyects/gmail-helpdesk-inspector/apps/web/src/api/client.ts:3).

**Tests**
Agregar tests unitarios del limiter:
- permite hasta burst.
- siguiente request devuelve `429` con `Retry-After`.
- refill vuelve a permitir.
- usuarios distintos no comparten cuota.
- acciones distintas no comparten cuota.
- pruning elimina buckets viejos.

Agregar tests HTTP en [apps/api/src/http/mod.rs](/home/ryagar/Documentos/DevProyects/gmail-helpdesk-inspector/apps/api/src/http/mod.rs:1620):
- `POST /analysis-runs` excedido devuelve `429`.
- `POST /analysis-runs/{id}/start` excedido devuelve `429`.
- `alice` limitada no limita a `bob`.
- requests sin sesión siguen devolviendo `401`, no `429`.
- scheduler en [apps/api/src/http/internal.rs](/home/ryagar/Documentos/DevProyects/gmail-helpdesk-inspector/apps/api/src/http/internal.rs:22) no queda afectado aunque la cuota manual esté agotada.

**Riesgos**
El mayor riesgo es despliegue multi-instancia: un limiter in-memory es por proceso, se resetea al reiniciar y se puede esquivar si Cloud Run balancea entre instancias. Para beta con bajo tráfico o `max_instances=1`, sirve. Si esto ya protege costo real en producción multi-instancia, iría directo a fase 2 con Firestore/Redis distribuido.

Otro riesgo: limitar dentro del handler ocurre después de parsear JSON, así que no protege abuso de bodies enormes. Si eso preocupa, se debe sumar `DefaultBodyLimit`/límite de payload separado.

Paths principales: `apps/api/src/rate_limit.rs` nuevo, `apps/api/src/config/mod.rs`, `apps/api/src/http/mod.rs`, opcional `apps/web/src/api/client.ts`.
