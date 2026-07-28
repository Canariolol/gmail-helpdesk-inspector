# Claude UI Brief — WorkOS + Pricing + Gmail Connect

## Contexto

El backend cambió el flujo de acceso:

`landing -> pricing -> crear cuenta con WorkOS -> checkout/trial con Mercado Pago -> conectar Gmail readonly -> configurar -> analizar`

WorkOS AuthKit es la autenticación principal de cuenta. Google/Gmail ya no es "login de la app"; ahora es solo la conexión de la casilla que se auditará.

## Pricing

Mostrar 3 planes:

| Plan | USD ref | CLP mensual | Trial |
|---|---:|---:|---|
| Inicial | USD 9 | CLP 9.990 | No |
| Pro | USD 29 | CLP 29.990 | Sí, 30 días |
| Equipo | USD 99 | CLP 99.990 | No |

El plan recomendado/destacado es `Pro`.

Copy obligatorio:

- "Los pagos se procesan en CLP por Mercado Pago."
- "USD es referencia internacional."
- "Gmail se conecta después del pago/trial y solo con permiso de lectura."

No prometer Paddle en UI pública. Paddle queda como decisión interna/futura para Merchant of Record.

## Estados Que Debe Cubrir La UI

- Sin sesión: landing pública + CTA a crear cuenta con WorkOS.
- Sesión sin plan: pricing/checkout.
- Checkout pendiente: estado de espera y opción de reintentar/volver a pricing.
- Trial activo sin Gmail: conectar Gmail readonly.
- Gmail conectado: app normal.
- Plan vencido/bloqueado: bloquear nuevos análisis, explicar pago requerido, mantener datos visibles si backend lo permite.
- Límite de uso alcanzado: mensaje accionable y CTA para subir plan/contactar.

## Diferenciación Visual Importante

- "Crear cuenta" = WorkOS/AuthKit.
- "Conectar Gmail" = permiso Gmail readonly para auditar una casilla.

Evitar textos como "Entrar con Google" en landing si el destino real es WorkOS. Google solo debe aparecer en el paso de conexión de Gmail.

## Backend Disponible

- `GET /public/plans`
- `GET /auth/workos/login`
- `GET /auth/workos/callback`
- `GET /me/account`
- `POST /checkout/subscriptions`
- `GET /checkout/sessions/:id`
- `GET /gmail/connect/login`
- `GET /gmail/connect/callback`
- `GET /me/usage`

`/me/account` entrega:

- `account_email`
- `workos_user_id`
- `org_id`
- `gmail_connected`
- `gmail_account_email`
- `entitlement.allowed`
- `entitlement.subscription_status`
- `entitlement.plan`

## Alcance UI

Puedes mejorar layout, copy, responsive, estados vacíos y jerarquía visual. No inventar endpoints nuevos ni flujos destructivos. Si necesitas billing management, cancelación, upgrade/downgrade o Paddle, déjalo como `BLOCKED_QUESTIONS`.
