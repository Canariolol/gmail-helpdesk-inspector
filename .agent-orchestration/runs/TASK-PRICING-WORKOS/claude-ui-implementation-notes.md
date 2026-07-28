# Claude UI Implementation Notes — WorkOS + Pricing + Gmail Connect

Implementación **solo UI/UX**. No se crearon endpoints nuevos ni se cambió lógica backend.
Build web verificado: `npm run build` (tsc -b + vite build) ✅ verde.

## Qué cambié

### Estados de acceso (nuevo, `apps/web/src/views/access/`)
Antes el gating vivía inline en `App.tsx` con dos estados (sin plan → pricing, sin gmail → gmail).
Ahora hay una máquina de estados explícita y componentes dedicados:

- `accessState.ts` — `deriveAccessState(account, hasPendingCheckout)` → `pricing | checkout_pending | blocked | connect_gmail | ready`.
- `AccessShell.tsx` — layout centrado con marca, fondo cálido.
- `PricingPlans.tsx` — grilla de 3 planes (Pro destacado), CLP prominente + USD referencial + badge de trial.
- `PaymentNotes.tsx` — copy obligatorio (CLP por Mercado Pago / USD referencia / Gmail readonly post-pago).
- `PricingGate.tsx` — **sesión sin plan**: "Elige un plan para activar tu auditoría".
- `CheckoutPendingGate.tsx` — **checkout pendiente**: espera con auto-refresh + reintentar pago + volver a planes.
- `BlockedGate.tsx` — **plan vencido/bloqueado**: copy por estado (`past_due` / `cancelled` / `expired`), reactivación con la misma grilla, reasegura que los datos se conservan.
- `GmailConnectGate.tsx` — **trial/plan activo sin Gmail**: diferencia clara "cuenta ≠ Gmail", Google aparece SOLO aquí, garantías de solo lectura.
- `LoadingGate.tsx` — estado de carga.
- `UsageLimitBanner.tsx` — **límite de uso alcanzado**: banner accionable dentro de la app con CTA "Subir de plan".
- `PlansModal.tsx` — modal de planes para el flujo "subir de plan".
- `pendingCheckout.ts` — persistencia en `localStorage` del checkout en curso (id, url, plan) para detectar el estado "pendiente" al volver de Mercado Pago.

### `App.tsx`
- Reescrito el bloque de gating para usar `deriveAccessState`.
- Nueva query `GET /me/usage` (solo cuando la app está desbloqueada) → banner de límite cuando `runs_created >= runs_per_month`.
- Polling de `GET /me/account` cada 5s **solo** mientras hay checkout pendiente y el plan aún no está activo; se detiene y limpia solo al activarse o bloquearse.
- `checkout.onSuccess` guarda el checkout pendiente antes de redirigir a Mercado Pago.
- `createRun.onSuccess` invalida también `usage`.

### Copy de landing (`views/landing/content.ts`)
- CTA del hero: **"Entrar con Google" → "Crear cuenta"** (el destino real es WorkOS; el brief lo pide explícito).
- Pasos "Cómo funciona" ahora account-first: 1) Crea tu cuenta (WorkOS + plan/trial), 2) Conecta tu Gmail (recién aquí, solo lectura), 3) Lee tus métricas.
- `betaTag`: ya no dice "cualquier cuenta de Google" (confundía cuenta WorkOS con Gmail).

### Estilos
- Nuevo `styles/access.css` (importado en `main.tsx`) con la estética naranja/crema de `tokens.css`. No se duplicaron componentes existentes; las gates usan el design system de la app (`.btn-primary`, `.btn-ghost`, `.card`).

## Verificaciones
- Sin "Entrar con Google" ni "Paddle" en UI pública. "Google" solo queda en: paso de conexión Gmail, disclaimer legal del footer, y vistas internas de privacidad/config (correctos).
- No se tocó la lógica de runs/threads/reportes ni sus contratos.

## BLOCKED_QUESTIONS (requieren decisión/backend)

1. **Upgrade/downgrade y cancelación**: no hay endpoint de billing-management. "Subir de plan" hoy crea un *nuevo* checkout vía `POST /checkout/subscriptions` con otro `plan_id`. ¿El backend maneja cambio de plan / prorrateo sobre una suscripción existente, o se necesita un portal de billing? Cancelación no tiene UI/endpoint.

2. **Datos visibles con plan bloqueado**: hoy `/me/org/config` y `/analysis-runs` están detrás de `entitlement.allowed` (`appUnlocked` / `require_active_entitlement`). Un usuario bloqueado NO puede ver su historial. El `BlockedGate` solo reasegura que los datos se conservan. Si se quiere visibilidad read-only de runs/reportes anteriores estando bloqueado, el backend debe permitir esos GET con `allowed = false`.

3. **Detección de "checkout pendiente"**: la `Subscription` se crea recién en el webhook de Mercado Pago, así que entre el redirect y el webhook `/me/account` devuelve `subscription_status: null`. Lo detecto con un flag en `localStorage`. ¿Aceptable, o conviene exponer el estado del checkout en `/me/account` (p. ej. `pending_checkout_session_id`)?

4. **Return URL de Mercado Pago**: asumo que el preapproval de MP redirige de vuelta a la web app. Confirmar que `back_url` apunta a la app para que el usuario aterrice en el estado "checkout pendiente" y el polling resuelva.

5. **Límites distintos a runs**: solo `runs_per_month` se enforce server-side (`enforce_usage_allows_run`). `candidate_threads` / `ai_audited_threads` se trackean pero no se bloquean; el banner solo refleja el límite de análisis. ¿Se quiere avisar/bloquear por esos otros límites?

6. **Copy menor (no tocado)**: el heading de landing "Tres pasos. Sin servidores." quedó intacto, pero "Sin servidores" es discutible ahora que hay backend SaaS/WorkOS/MP. Lo dejo a criterio del dueño del copy.

---

## Resolución de bloqueos (2da iteración — full-stack)

Decisiones del usuario: cambio de plan = actualizar preapproval (PUT, sin reautorizar); cancelación = al fin del período; gestión en nueva vista "Plan y cuenta".

### Backend (Rust)
- **Gating de historial (BLOCKED_QUESTION #2 → resuelto)**: nuevo helper `require_entitlement` aplicado a `list_analysis_runs`, `get_analysis_run`, `analysis_events`, `get_metrics`, `list_threads`, `get_thread`, `manual_review`. Un plan bloqueado ahora recibe 402 en la lectura del historial (defensa en backend, no solo UI). Test nuevo: `history_reads_require_active_subscription`. Se mantienen accesibles `/me/account`, `/me/usage`, `/me/data-summary`, `/me/org/config` (GET/PUT), `/me/operations/*` para que el bloqueado siga usando configuración/control de cuenta.
- **Gestión de plan (BLOCKED_QUESTION #1 → resuelto)**: `POST /me/subscription/change-plan` (PUT al preapproval de MP, sin reautorizar) y `POST /me/subscription/cancel` (cancela en MP + `cancel_at_period_end`). Nuevo helper `update_mercadopago_preapproval`.
- **Cancelación al fin del período**: `Subscription.cancel_at_period_end` (con `#[serde(default)]`); `subscription_allows_access` mantiene acceso de un `Active` cancelado hasta `current_period_end`; `entitlement_snapshot` reporta `Cancelled` cuando ya venció. Tests nuevos en `billing.rs`.
- **Retorno MP (BLOCKED_QUESTION #4 → resuelto)**: `back_url` ahora `WEB_BASE_URL/checkout-return?session_id=...` (antes `/checkout/return`, que `server.mjs` **proxyaba** a la API). Se agregó `notification_url = API_BASE_URL/billing/mercadopago/webhook`. Ambos derivan de env (`WEB_BASE_URL` = web, `API_BASE_URL` = api).
- `EntitlementSnapshot` enriquecido: `cancel_at_period_end`, `current_period_end`, `trial_ends_at`.
- Verificación: `cargo test` 80/80 OK, `cargo clippy --all-targets` sin warnings.

### Frontend (React)
- **Shell restringido para bloqueados**: el usuario bloqueado entra a la app pero el sidebar solo muestra `Plan y cuenta`, `Configuración`, `Privacidad y datos`, `Ayuda` (filtro `availableViews`). Las vistas de datos se fuerzan a `cuenta` (`effectiveView`). Banner superior `BlockedBanner` con CTA a reactivar.
- **Vista nueva `CuentaView`** ("Plan y cuenta"): plan/estado/fechas, "Cambiar de plan" (modal en modo change → `change-plan`), "Cancelar suscripción" (confirm → `cancel`, avisa acceso hasta la fecha), "Reanudar" tras cancelar (checkout nuevo), y reactivación inline para bloqueados.
- **Retorno MP robusto (BLOCKED_QUESTION #3)**: `App.tsx` lee `?session_id=` al cargar, llama `GET /checkout/sessions/:id`, fija el estado "checkout pendiente" y limpia la URL. El `localStorage` sigue como respaldo.
- `PlansModal` con modo `change` (CTA "Cambiar a este plan", marca el plan actual). `UsageLimitBanner` "Subir de plan" usa change-plan.
- Verificación: `npm run build` (tsc + vite) OK.

### Config
- `.env.example`: `WEB_BASE_URL`/`API_BASE_URL` con los valores de prod (web/api Cloud Run) y notas de back_url (web) vs notification_url (api).

### Notas que persisten
- **"Reanudar" tras cancelar = checkout nuevo** (el preapproval de MP ya quedó cancelado; no se des-cancela). Usa el flujo existente.
- **Doble cobro al reactivar**: riesgo bajo (al cancelar ya cortamos en MP; bloqueado/expired ⇒ preapproval inactivo). Posible mejora futura: best-effort cancel del preapproval previo al activar uno nuevo.
- **`operations/history`** queda accesible para bloqueados (metadata operacional, no contenido analizado). Fácil de bloquear si se prefiere.
- **BLOCKED_QUESTION #5** (límites distintos a runs) sigue abierta: solo `runs_per_month` se enforce/avisa.
