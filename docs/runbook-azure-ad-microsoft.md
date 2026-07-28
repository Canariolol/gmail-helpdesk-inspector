# Runbook — Registrar Mira en Azure AD (Microsoft Entra ID)

> Para: habilitar el botón "Conectar con Microsoft" (Outlook y Microsoft 365).
> Requiere: una cuenta Microsoft. **No** requiere suscripción de pago ni tarjeta:
> registrar una aplicación en Entra ID es gratis.
> Tiempo estimado: 15–20 minutos.

Al terminar tendrás tres valores para cargar en la API:

| Variable | De dónde sale |
|---|---|
| `MICROSOFT_CLIENT_ID` | Paso 4 — "Application (client) ID" |
| `MICROSOFT_CLIENT_SECRET` | Paso 5 — el **Value** del secreto |
| `MICROSOFT_REDIRECT_URL` | Lo defines tú en el paso 3 |
| `MICROSOFT_TENANT` | `common` (paso 8) |

Mientras la API no tenga las tres primeras, arranca igual y simplemente no ofrece
el botón de Microsoft. No hay riesgo de romper nada por dejarlo a medias.

---

## Vocabulario mínimo (para no perderse)

Azure llama a las cosas distinto que Google:

| Google Cloud | Azure / Entra ID |
|---|---|
| Proyecto | **Tenant** (o "directorio") |
| Pantalla de consentimiento OAuth | **App registration** |
| ID de cliente | Application (client) ID |
| Secreto de cliente | Client secret |
| Scopes | **API permissions** (delegated) |
| Google Cloud Console | **portal.azure.com** |

"Microsoft Entra ID" es el nombre nuevo de lo que antes se llamaba **Azure Active
Directory (Azure AD)**. En la documentación vieja de internet verás los dos
nombres para lo mismo. En el portal hoy dice Entra ID.

---

## Paso 1 — Entrar al portal

1. Anda a <https://portal.azure.com>.
2. Inicia sesión con una cuenta Microsoft. Si no tienes, créala en
   <https://signup.live.com> — sirve una cuenta personal.
3. Si es tu primera vez, Azure te crea automáticamente un directorio por defecto.
   Eso es suficiente; **no** necesitas activar una suscripción ni ingresar
   tarjeta.

> Si el portal te muestra un banner ofreciendo una "prueba gratuita" o pidiendo
> una suscripción, ignóralo y sigue. Ese flujo es para máquinas virtuales y
> demás; el registro de aplicaciones no lo necesita.

## Paso 2 — Abrir el registro de aplicaciones

1. En la barra de búsqueda de arriba escribe `Microsoft Entra ID` y entra.
2. En el menú de la izquierda busca **App registrations** (Registros de
   aplicaciones).
3. Click en **+ New registration** (Nuevo registro).

## Paso 3 — Registrar la aplicación

Tres campos:

**Name** — el nombre que verán los usuarios en la pantalla de consentimiento.
Pon exactamente:

```
Mira Helpdesk
```

**Supported account types** — elige:

```
Accounts in any organizational directory (Any Microsoft Entra ID tenant -
Multitenant) and personal Microsoft accounts (e.g. Skype, Xbox)
```

Es la tercera opción de la lista. Esto permite conectar tanto casillas de
empresa en Microsoft 365 como cuentas personales de Outlook.com / Hotmail.
Si eliges una opción más restrictiva, las cuentas fuera de tu propio directorio
no van a poder conectarse.

**Redirect URI** — selecciona la plataforma **Web** en el desplegable y pega:

```
https://mira.ninfasolutions.com/mailbox/connect/microsoft/callback
```

> Esta URL tiene que coincidir **carácter por carácter** con lo que configures en
> `MICROSOFT_REDIRECT_URL`. Una barra de más al final y Microsoft rechaza el
> login con un error críptico. Es el error nº1 de este flujo.

Click en **Register**.

## Paso 4 — Copiar el Client ID

Caes en la página **Overview** de la app. Ahí ves:

- **Application (client) ID** → este es tu `MICROSOFT_CLIENT_ID`. Cópialo.
- Directory (tenant) ID → **no lo necesitas** si usas `common` (ver paso 8).

El Client ID no es secreto: identifica la app, no autoriza nada por sí solo.

## Paso 5 — Crear el Client Secret

1. Menú izquierdo → **Certificates & secrets**.
2. Pestaña **Client secrets** → **+ New client secret**.
3. Description: `Mira API produccion`.
4. Expires: elige **24 months** (el máximo). Azure no permite secretos sin
   vencimiento.
5. Click **Add**.

Ahora la parte crítica:

> La tabla muestra dos columnas, **Value** y **Secret ID**. Necesitas el
> **Value**, no el Secret ID. Y el Value **solo se muestra en esta pantalla**: si
> recargas o navegas fuera, queda oculto para siempre y hay que crear otro
> secreto. Cópialo ahora.

Ese es tu `MICROSOFT_CLIENT_SECRET`.

**Anota en tu calendario la fecha de vencimiento.** Cuando el secreto expire, el
botón de Microsoft deja de funcionar y el síntoma en los logs es un error de
`invalid_client` en el intercambio de código. Renovarlo es repetir este paso 5.

## Paso 6 — Dar los permisos que Mira usa

1. Menú izquierdo → **API permissions**.
2. Verás que `User.Read` ya está agregado por defecto. Bien, ese lo usamos.
3. Click **+ Add a permission** → **Microsoft Graph** → **Delegated permissions**
   (no "Application permissions" — Mira actúa en nombre de la persona usuaria, no
   como servicio autónomo).
4. Busca y marca:
   - `Mail.Read` — leer el correo de la persona que conecta.
   - `offline_access` — sin esto Microsoft **no entrega refresh token** y la
     conexión se cae en una hora, rompiendo el análisis programado.
5. Click **Add permissions**.

La lista final debe tener exactamente estos tres permisos delegados:
`User.Read`, `Mail.Read`, `offline_access`.

> **No agregues `Mail.ReadWrite` ni `Mail.Send`.** Mira solo lee, y pedir
> permisos que no usa hace que más administradores rechacen la app — además de
> contradecir lo que promete el copy del producto.

## Paso 7 — Consentimiento de administrador

En esa misma pantalla hay un botón **Grant admin consent for [tu directorio]**.

- Si vas a conectar una casilla de **tu propio** directorio, apriétalo. Evita que
  te pida consentimiento individual.
- Para casillas de **otras organizaciones** (clientes con Microsoft 365), este
  botón no aplica: el administrador de esa organización tendrá que autorizar Mira
  la primera vez que alguien de ahí intente conectarse. Es normal y esperado; el
  copy del producto ya lo anticipa.
- Para cuentas personales de Outlook.com no hay administrador: la persona
  consiente sola.

## Paso 8 — El tenant

Deja `MICROSOFT_TENANT=common`.

| Valor | Significa |
|---|---|
| `common` | Cualquier organización **y** cuentas personales. **Es el que corresponde** al tipo de cuenta elegido en el paso 3. |
| `organizations` | Solo cuentas de trabajo/estudio, sin personales. |
| `consumers` | Solo cuentas personales. |
| Un tenant ID concreto | Restringe la app a una sola organización. |

Si en el paso 3 elegiste incluir cuentas personales, `common` es obligatorio:
cualquier otro valor va a rechazar la mitad de los casos.

---

## Paso 9 — Cargar las credenciales

### Desarrollo local

`docker-compose.yml` monta el `.env` completo dentro del contenedor de la API
(`env_file: .env`), así que basta con agregarlo ahí y `scripts/local-up.sh` lo
toma solo. No hay que tocar el script.

```bash
MICROSOFT_CLIENT_ID=<el Application client ID>
MICROSOFT_CLIENT_SECRET=<el Value del secreto>
MICROSOFT_REDIRECT_URL=http://localhost:8080/mailbox/connect/microsoft/callback
MICROSOFT_TENANT=common
```

Para que ese redirect local funcione, vuelve a Azure → **Authentication** →
**Add URI** y agrega también
`http://localhost:8080/mailbox/connect/microsoft/callback`.
Azure acepta `http://` **solo** para `localhost`; para cualquier otro host exige
HTTPS.

### Producción

**Decisión 2026-07-20: este secreto NO va a Secret Manager.** Se despliega como
variable de entorno.

No hay que declarar nada al invocar el deploy: `redeploy-gcp.sh` lee las
credenciales de Microsoft desde `.keys` (o `.env` como respaldo) y las inyecta
solo. El comando de siempre:

```bash
scripts/redeploy-gcp.sh --no-traffic api   # candidata sin tráfico de usuarios
scripts/redeploy-gcp.sh api                # despliega y mueve el tráfico
```

Antes de desplegar valida que el client id sea un GUID, que el secreto **no** lo
sea (así detecta el error de copiar el Secret ID en vez del Value) y que no
contenga comas ni espacios que `gcloud` truncaría en silencio. Después del
deploy consulta `/mailbox/providers` y avisa si Microsoft no quedó habilitado.

El `MICROSOFT_REDIRECT_URL` de producción **no** se lee de `.env` —ahí vive el de
localhost, y desplegarlo rompería el login con `AADSTS50011`. Se deriva del
`WEB_BASE_URL` que ya tiene el servicio desplegado. Para forzar otro valor,
exportá `MICROSOFT_REDIRECT_URL_PROD`.

Si no hay credenciales locales, el deploy funciona igual y la API simplemente no
ofrece el proveedor.

**Lo que estás aceptando al no usar Secret Manager:** el valor queda dentro del
spec de la revisión de Cloud Run, legible por cualquiera con `roles/run.viewer`
en el proyecto, y persiste en las revisiones viejas aunque después lo rotes.
Registrado como deuda aceptada en `plan-production-ready.md` §4.1, junto a
`WORKOS_API_KEY`.

---

## Paso 10 — Probar

1. Entra a Mira con una cuenta que tenga plan o trial activo y sin casilla
   conectada.
2. La pantalla de conexión debe mostrar **dos** botones.
3. Aprieta "Conectar con Microsoft".
4. Microsoft pide login y muestra la pantalla de consentimiento con el nombre
   "Mira Helpdesk" y los tres permisos.
5. Al aceptar vuelves a Mira con la casilla conectada.
6. Crea un análisis sobre un rango de fechas con correos reales y confirma que
   trae hilos.

---

## Errores frecuentes y qué significan

| Lo que ves | Causa | Arreglo |
|---|---|---|
| `AADSTS50011: The redirect URI specified in the request does not match` | La URL del paso 3 no coincide con `MICROSOFT_REDIRECT_URL` | Compáralas carácter por carácter, incluida la barra final |
| `AADSTS7000215: Invalid client secret provided` | Copiaste el **Secret ID** en vez del **Value**, o el secreto expiró | Crea un secreto nuevo (paso 5) y copia la columna Value |
| `AADSTS650057: Invalid resource` | Falta algún permiso delegado | Revisa el paso 6 |
| `AADSTS65001: The user or administrator has not consented` | Falta consentimiento de admin en ese tenant | El administrador de esa organización debe autorizar Mira |
| El botón de Microsoft no aparece | Falta alguna de las tres variables | `curl /mailbox/providers` y revisa el despliegue |
| Conecta, pero al día siguiente falla el análisis programado | Falta `offline_access` → no hay refresh token | Agrégalo (paso 6) y reconecta la casilla |

## Rotación del secreto

`MICROSOFT_CLIENT_SECRET` vence a los 24 meses. Para rotarlo sin caída:

1. Crear un secreto nuevo en Azure (paso 5) **sin borrar el viejo** — Azure
   admite varios secretos activos a la vez.
2. Cargar la versión nueva en Secret Manager y desplegar.
3. Verificar un connect real.
4. Recién entonces borrar el secreto viejo en Azure.
