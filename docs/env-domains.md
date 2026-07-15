# Entornos, dominios y webhooks

## Desarrollo sandbox

Objetivo: probar OAuth y Mercado Pago sandbox con una URL publica estable.

Dominio recomendado:

- `https://mira-dev.ninfasolutions.com`

Flujo:

1. Copia `.env.sandbox.example` a `.env` y completa secretos sandbox.
2. Crea un Cloudflare Tunnel para `mira-dev.ninfasolutions.com`.
3. Guarda el token en `CLOUDFLARED_TUNNEL_TOKEN`.
4. Levanta:

```bash
docker compose -f docker-compose.yml -f docker-compose.tunnel.yml up --build
```

Caddy recibe el trafico del tunnel y enruta:

- `/auth`, `/gmail`, `/checkout`, `/billing`, `/me`, `/public`, `/health` -> API local.
- Todo lo demas -> web local.

URLs sandbox a registrar:

- WorkOS callback: `https://mira-dev.ninfasolutions.com/auth/workos/callback`
- Google OAuth redirect: `https://mira-dev.ninfasolutions.com/gmail/connect/callback`
- Mercado Pago webhook sandbox: `https://mira-dev.ninfasolutions.com/billing/mercadopago/webhook`
- Mercado Pago public key/access token: credenciales `TEST-*`.

## Produccion live

Dominio publico:

- `https://mira.ninfasolutions.com`

Cloud Run:

- `ghmi-web` recibe el dominio publico.
- `ghmi-web` proxya rutas API a `ghmi-api` usando `API_PROXY_TARGET`.
- `ghmi-api` conserva su URL `run.app` para webhooks.

Variables clave:

```env
WEB_BASE_URL=https://mira.ninfasolutions.com
API_BASE_URL=https://<ghmi-api-run-app-url>
GOOGLE_REDIRECT_URL=https://mira.ninfasolutions.com/gmail/connect/callback
WORKOS_REDIRECT_URI=https://mira.ninfasolutions.com/auth/workos/callback
VITE_API_BASE_URL=
VITE_MERCADOPAGO_PUBLIC_KEY=<public-key-de-mercado-pago>
API_PROXY_TARGET=https://<ghmi-api-run-app-url>
BILLING_ENFORCEMENT_ENABLED=true
```

Webhook live de Mercado Pago:

```text
https://<ghmi-api-run-app-url>/billing/mercadopago/webhook
```

No uses `mira.ninfasolutions.com/billing/...` para el webhook live si quieres
que Mercado Pago llegue directo a Cloud Run API.

Credenciales:

- Obtén las credenciales exactas para cada entorno desde la aplicación de Mercado Pago; el prefijo depende del producto.
- No mezclar la `VITE_MERCADOPAGO_PUBLIC_KEY` y el `MERCADOPAGO_ACCESS_TOKEN` de entornos distintos.
