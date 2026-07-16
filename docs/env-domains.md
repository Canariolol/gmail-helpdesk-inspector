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
- WorkOS webhook: `https://mira-dev.ninfasolutions.com/auth/workos/webhook`
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

`WORKOS_WEBHOOK_SECRET` se inyecta como secreto (no en este bloque). Registra
en WorkOS el endpoint directo de API y únicamente los eventos `user.deleted` y
`session.revoked`:

```text
https://<ghmi-api-run-app-url>/auth/workos/webhook
```

Tras desplegar, usa «Send test event» de WorkOS para ambos eventos y confirma
en logs `operation=workos_webhook`; no envíes payloads ni secretos a logs.

Webhook live de Mercado Pago:

```text
https://<ghmi-api-run-app-url>/billing/mercadopago/webhook
```

No uses `mira.ninfasolutions.com/billing/...` para el webhook live si quieres
que Mercado Pago llegue directo a Cloud Run API.

Credenciales:

- Obtén las credenciales exactas para cada entorno desde la aplicación de Mercado Pago; el prefijo depende del producto.
- No mezclar la `VITE_MERCADOPAGO_PUBLIC_KEY` y el `MERCADOPAGO_ACCESS_TOKEN` de entornos distintos.

## Conectar `mira.ninfasolutions.com` con Cloudflare y Cloud Run

Cloudflare por sí solo no hace que Cloud Run acepte el host
`mira.ninfasolutions.com`: primero hay que asociar el host al servicio de web
en Google Cloud. No apuntar un CNAME directo a `*.run.app` ni activar el proxy
naranja como sustituto de esa asociación.

### Beta o prueba privada: Cloud Run Domain Mapping

Cloud Run Domain Mapping es la ruta más corta para una prueba privada en
`us-central1`, pero Google la clasifica como Preview y no la recomienda para el
lanzamiento productivo. Para cobrar a usuarios externos, usar un Global
External Application Load Balancer delante de `ghmi-web` en lugar de este
mapping.

1. En Cloudflare, añadir la zona `ninfasolutions.com` y reemplazar en el
   registrador los nameservers actuales por los que Cloudflare entregue.
   Conservar todos los registros existentes de correo (MX, SPF, DKIM y DMARC)
   al revisar la importación. Esperar a que la zona diga `Active`.

   **Estado 2026-07-15:** la zona fue creada por API y se importaron 23
   registros existentes como `DNS only`, incluyendo MX, SPF, DKIM, DMARC,
   autodiscover y los hosts web. Los dos NS de Hostinger no se importaron porque
   Cloudflare será el DNS autoritativo. El ALIAS de Hostinger del ápice se
   normalizó a CNAME flattening de Cloudflare; ambos destinos raíz existentes
   resolvían al mismo host Firebase. Se validó que no quedaron nombres con el
   sufijo duplicado. Los nameservers fueron delegados y se confirmó la
   resolución pública mediante Cloudflare y Google DNS. Ya es seguro continuar
   con el mapping de Cloud Run.
2. En Google Cloud Console, abrir **Cloud Run > Domain mappings > Add mapping**.
   Elegir `ghmi-web`, la opción **Cloud Run Domain Mappings** y el dominio
   `mira.ninfasolutions.com`. Si Google pide verificar propiedad, verificar el
   dominio base `ninfasolutions.com` mediante el TXT que muestra Search Console.
3. Al finalizar el mapping, abrir **DNS Records** dentro de ese mapping y copiar
   todos los `resourceRecords` que Google entregue.
4. En Cloudflare > **DNS > Records**, crear exactamente esos registros para
   `mira`. Mientras Google valida el dominio y emite el certificado, dejarlos
   como **DNS only** (nube gris) y no dejar un A/AAAA/CNAME anterior con el
   mismo nombre.
5. En Cloudflare > **SSL/TLS > Edge Certificates**, mantener desactivado
   **Always Use HTTPS** durante la validación y la emisión o renovación del
   certificado de Google. Esperar hasta 24 horas si el certificado no aparece
   de inmediato.
6. Verificar sin iniciar sesión:

   ```bash
   curl -fsSI https://mira.ninfasolutions.com/
   ```

   El certificado debe ser válido y la respuesta debe venir de `ghmi-web`.

   **Estado 2026-07-16:** Cloud Run informa `Ready=True` y
   `CertificateProvisioned=True` para `mira.ninfasolutions.com`; el CNAME
   `mira -> ghs.googlehosted.com` resuelve por DNS público. El resolver local
   aún puede conservar cache antiguo; no cambiar OAuth desde una respuesta DNS
   local desactualizada.

En el deploy candidato siguiente se actualizarán, como un único cambio
verificable:

```text
WEB_BASE_URL=https://mira.ninfasolutions.com
GOOGLE_REDIRECT_URL=https://mira.ninfasolutions.com/gmail/connect/callback
WORKOS_REDIRECT_URI=https://mira.ninfasolutions.com/auth/workos/callback
```

También se deben registrar esas dos callbacks en Google OAuth y WorkOS. Los
handlers actuales aceptan esas rutas a través del proxy same-origin de
`ghmi-web`.

### Lanzamiento real: External Application Load Balancer

Antes de un lanzamiento pagado, reemplazar el Domain Mapping Preview por un
Global External Application Load Balancer con un serverless NEG que apunte a
`ghmi-web`. Cloudflare puede seguir siendo el DNS/proxy del subdominio, pero
el certificado, el host y el enrutamiento quedan gestionados por el balanceador
GA de Google. Documentar el coste antes de crearlo.
