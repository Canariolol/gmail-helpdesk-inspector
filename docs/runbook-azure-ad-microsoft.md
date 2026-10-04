# Runbook — Registrar Mira en Azure AD (Microsoft Entra ID)

La revisión del 4 de octubre de 2026 encontró Microsoft habilitado en el
despliegue documentado del 29 de septiembre. Este runbook también sirve para
registrar aplicaciones nuevas o revisar esa configuración. La disponibilidad
de `/mailbox/providers` confirma las credenciales configuradas; no sustituye
una prueba de autorización y análisis contra cada tipo de casilla.

Mira usa Graph para Outlook.com, Hotmail y Microsoft 365. La conexión propia
pide `offline_access User.Read Mail.Read`. Para una casilla compartida o
delegada, pide además `Mail.Read.Shared` y consulta
`/users/{correo-de-la-casilla}/mailFolders` y `/messages`. El usuario que
autoriza debe tener acceso a la casilla. Esta opción corresponde a Microsoft
365 empresarial; las cuentas personales conectan su propia casilla.

Antes de aceptar la conexión, el backend realiza una lectura mínima de Inbox
para comprobar acceso. No pide permisos de escritura ni permisos de aplicación
para leer toda la organización. Consulta [el modelo de permisos compartidos de
Microsoft](https://learn.microsoft.com/en-us/graph/outlook-share-messages-folders).

Para clientes de otras organizaciones, comprobar que el registro admite
cuentas de cualquier directorio y cuentas Microsoft personales. Verificar
también el publisher y preparar el consentimiento de administrador cuando la
política del cliente lo exija. Un secreto válido no garantiza que ese tenant
permita la conexión. [Microsoft explica las restricciones para publishers sin
verificar](https://learn.microsoft.com/entra/identity-platform/publisher-verification-overview).

Las fechas de análisis usan el timestamp del proveedor: `internalDate` en
Gmail, `receivedDateTime` en Graph e `INTERNALDATE` en IMAP. La cabecera `Date`
es sólo un respaldo en Gmail/IMAP si falta un timestamp válido del proveedor.
No se inventa una fecha actual para mensajes con fechas inválidas.

La recuperación convierte las fechas del tenant a UTC, sigue páginas de
mensajes hasta el cupo de conversaciones, y recorre carpetas y subcarpetas.
Cada respuesta JSON de Gmail/Graph tiene un límite de 16 MiB antes de
deserializar, aplicado también a respuestas sin `Content-Length`. Sobrepasarlo
produce un error de recuperación y cobertura incompleta. El parser JSON mantiene
su límite de 128 niveles; el parser MIME IMAP limita la recursión a 100 niveles
y la extracción de texto a 20. Estas condiciones se verifican con payloads
anidados y una respuesta HTTP por chunks que supera el límite.

Los límites son 50 páginas de búsqueda, 2.000 mensajes por conversación,
1.000 carpetas y 20 niveles de profundidad. Al alcanzar un límite, informa
truncación. Las lecturas reintentan hasta tres veces los errores temporales;
respetan `Retry-After` hasta 10 segundos y devuelven un fallo recuperable cuando
el proveedor exige una espera mayor.

Microsoft devuelve refresh tokens nuevos, pero emitir uno no revoca
automáticamente el anterior. Mira exige un refresh token nuevo al conectar,
para no mezclar credenciales de dos usuarios delegados de una misma casilla.
Guarda ese refresh y renueva sin restringir el scope, conservando el
consentimiento original de casilla propia o compartida. [Comportamiento de refresh tokens](https://learn.microsoft.com/en-us/entra/identity-platform/refresh-tokens).

Los callbacks Google y Microsoft verifican una cookie PKCE firmada, con payload
JSON en base64url, ligada al usuario, sesión local, organización y proveedor
que iniciaron la conexión. El estado vence en diez minutos. Cambiar sesión o
organización obliga a reiniciar la conexión. Los payloads anteriores sin esta
vinculación se rechazan; una conexión iniciada antes del cambio debe repetirse.

Para cerrar la validación del entorno, ejecutar estas pruebas con cuentas
autorizadas y datos de prueba. No se han ejecutado desde este cambio local.

1. Conectar Outlook.com o Hotmail y ejecutar un análisis con una solicitud y
   su respuesta conocidas. Confirmar que los permisos siguen siendo de lectura.
2. Repetir en una casilla propia Microsoft 365 de otro tenant. Probar el caso
   en que su administrador bloquea el consentimiento y comprobar el aviso.
3. Conectar una casilla compartida con un usuario delegado y confirmar que el
   informe contiene el correo compartido, no su casilla personal. Denegar acceso
   a otra casilla y comprobar que el probe impide guardarla.
4. Incluir una subcarpeta y suficientes mensajes para forzar paginación.
   Confirmar la selección y el aviso si se supera un límite de recuperación.
5. Incluir solicitudes al inicio y final del día en la zona del tenant y una
   fecha de cambio de horario de verano.
6. Renovar la conexión, cerrar sesión y ejecutar el scheduler. Revocar luego el
   consentimiento y comprobar que se solicita reconexión.

Los proveedores externos usan una conexión IMAP separada, con hostname público,
puerto 993, TLS con certificado validado, usuario y contraseña de aplicación.
El backend rechaza direcciones privadas, locales, reservadas o mezcladas en DNS
y conecta directamente a las IP públicas verificadas para impedir DNS rebinding.
Usa `EXAMINE` y `BODY.PEEK`, sin marcar mensajes como leídos. Agrupa mediante
`Message-ID`, `References` e `In-Reply-To`; no une mensajes sólo por asunto.
Las operaciones de lectura siguen [RFC 9051](https://www.rfc-editor.org/rfc/rfc9051.html).
Puede autodetectar la carpeta Enviados mediante SPECIAL-USE o nombres frecuentes;
si no existe una detección fiable, seleccionar su nombre exacto.

Los límites iniciales IMAP son 20 carpetas por análisis, 5.000 cabeceras,
200 mensajes por conversación, 128 KiB recuperados por mensaje y 32 MiB por
sesión. La falta de carpeta Enviados o los recortes de cabeceras/cuerpo dejan
la cobertura incompleta explícita. No se descargan ni persisten adjuntos como
archivos. Un proveedor que sólo ofrece OAuth u otro protocolo requiere su
adaptador específico; tener un cliente Outlook instalado no determina el
proveedor real.

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
   - `Mail.Read.Shared` — agregar si se ofrecerán casillas compartidas o delegadas
     de Microsoft 365. El backend lo solicita sólo al elegir una casilla destino.
   - `offline_access` — sin esto Microsoft **no entrega refresh token** y la
     conexión se cae en una hora, rompiendo el análisis programado.
5. Click **Add permissions**.

La conexión propia usa estos tres permisos delegados:
`User.Read`, `Mail.Read`, `offline_access`. Para una casilla compartida o delegada,
se agrega `Mail.Read.Shared`.

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
2. La pantalla de conexión debe ofrecer Microsoft entre los proveedores disponibles.
3. Aprieta "Conectar con Microsoft".
4. Microsoft pide login y muestra la pantalla de consentimiento con el nombre
   "Mira Helpdesk" y los tres permisos.
5. Al aceptar vuelves a Mira con la casilla conectada.
6. Crea un análisis sobre un rango de fechas con correos reales y confirma que
   trae hilos.

---

## Requisito para métricas de un buzón compartido: copias de enviados

Mira sólo puede medir las respuestas almacenadas en la casilla auditada. En
Exchange, los envíos de un delegado pueden quedar únicamente en su propia
carpeta de enviados. Antes de usar las métricas de un buzón compartido, su
administrador debe comprobar que también se guardan copias en ese buzón.

Desde Exchange Online PowerShell, una persona administradora puede habilitar
las copias para ambas modalidades de envío:

```powershell
Set-Mailbox "soporte@empresa.cl" -MessageCopyForSentAsEnabled $true -MessageCopyForSendOnBehalfEnabled $true
Get-Mailbox "soporte@empresa.cl" | Format-List MessageCopyForSentAsEnabled,MessageCopyForSendOnBehalfEnabled
```

Después, enviar una respuesta de prueba desde una cuenta delegada y verificar
que la copia aparece en los enviados del buzón compartido. Este ajuste sólo
permite observar las copias; Mira sigue usando permisos de lectura. La etiqueta
"Sin respuesta registrada" significa que no se encontró una respuesta en la
casilla conectada, y no demuestra ausencia de atención por otros canales.

Referencia: [documentación oficial de Microsoft sobre enviados de buzones
compartidos](https://learn.microsoft.com/en-us/troubleshoot/exchange/user-and-shared-mailboxes/sent-mail-is-not-saved).

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

## Excepción de auditoría de dependencias

La revisión local del 4 de octubre retiró `jsonwebtoken`, que no tenía ningún
uso en el backend. La sesión se valida mediante WorkOS en servidor y la cookie
local firmada; retirar esa dependencia no elimina una verificación JWT activa.
Se actualizaron `anyhow` a 1.0.103 y `event-listener` a 5.4.2.

`cargo audit` todavía encuentra RUSTSEC-2023-0071 en el lockfile por
`sqlx-mysql`, dependencia opcional que Cargo conserva aunque SQLx está
configurado con `default-features=false` y sólo PostgreSQL. `rsa` no se compila:

```sh
cargo tree --manifest-path apps/api/Cargo.toml --invert rsa --target all --prefix none
```

Ese comando devuelve un árbol vacío. La excepción en `.cargo/audit.toml` aplica
exclusivamente a esta dependencia inactiva; CI debe comprobar que el árbol sigue
vacío. Si cambia la configuración de SQLx o se introduce RSA, hay que retirar
la excepción y evaluar [el advisory de RustSec](https://rustsec.org/advisories/RUSTSEC-2023-0071.html).
No se ignoran vulnerabilidades nuevas ni los avisos de mantenimiento o yanked.
La configuración se encuentra al ejecutar `cargo audit --file apps/api/Cargo.lock`
desde la raíz del repo; ejecutar el comando desde otro directorio requiere
comprobar que cargó la misma configuración.
