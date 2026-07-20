# Plan — Conexión multiproveedor (Gmail + Microsoft 365 + IMAP)

> Estado: **auditado contra el código y reabierto para implementación**.
> Creado: 2026-06-25 · Auditado y reescrito: 2026-07-20
> Alcance: ampliar Mira más allá de Gmail para soportar Outlook/Microsoft 365 y,
> en una fase posterior, proveedores genéricos vía IMAP (Hostinger, cPanel, Zoho).

---

## 0. Qué cambió respecto de la versión del 2026-06-25

La definición conceptual original sigue siendo correcta en lo grueso, pero cuatro
supuestos quedaron obsoletos tras la migración a PostgreSQL y el trabajo de
"separar sesión y conexión de Gmail" (sección 5.3 del plan de producción):

| Supuesto original | Realidad al 2026-07-20 |
|---|---|
| El discriminante `provider` vive en la sesión web | Las credenciales se mudaron de `UserSession` a un registro propio `GmailConnection` (`apps/api/src/mailbox.rs:7`) para que el scheduler opere sin sesión. **El `provider` va en la conexión.** Los campos `gmail_*` de `UserSession` son legacy y se limpian (`storage/mod.rs:282`). |
| Renombrar `gmail_thread_id` "toca storage" | El storage es una sola tabla `mira.records` con `data JSONB`. **Cero DDL.** La tabla ya expone una columna `provider`. El costo real es `#[serde(alias)]` para leer filas existentes + ~55 ocurrencias entre Rust y TypeScript. |
| El motor de análisis es 100% neutral | Casi. Hay **una** excepción real: `refine_classification_with_gmail_labels` (`analysis/mod.rs:1024`) usa `CATEGORY_PROMOTIONS/SOCIAL/FORUMS` para mutar la clasificación. Además las etiquetas viajan al prompt de Bedrock (`BatchThreadSummary.gmail_labels`, `http/mod.rs:3218`). |
| Neutralizar "labels"→"carpetas" es cosmético en la UI | `gmail_thread_search_query` (`gmail/mod.rs:346`) construye sintaxis de búsqueda **ejecutada por el servidor de Gmail** (`category:`, `is:`, `label:"…"`). IMAP no tiene equivalente: solo `SINCE`/`BEFORE` y selección de carpeta. Todo lo demás habría que filtrarlo cliente-side, descargando más correo. |

Lo que **sí** se confirmó como escrito:

- `GmailClient` está bien aislado: solo **3 call sites** en toda la API
  (`http/mod.rs:648`, `:2753`, `:3065`). Extraer el trait es barato.
- El motor (ventanas, métricas, percentiles, reportes) opera sobre
  `EmailMessage`/`EmailThread` sin conocimiento del proveedor.
- `gmail_connect_login` es `Redirect::temporary` de página completa
  (`http/mod.rs:551`), no un popup → el auto-redirect de la §9 es viable.
- `login_hint` efectivamente **no** se envía hoy (`http/mod.rs:524-536`).

## 0.1 Decisiones tomadas por el responsable — 2026-07-20

1. **Orden:** trait primero, luego **Microsoft Graph**, IMAP después.
   Graph es arquitectónicamente gemelo de Gmail (OAuth, hilos nativos vía
   `conversationId`, búsqueda server-side, carpetas) → valida el trait con un
   segundo proveedor barato antes de enfrentar el caso hostil.
2. **Una casilla por organización**, reemplazable. Se mantiene el modelo actual
   de un registro por `owner_email`; conectar otra casilla reemplaza la anterior.
   Coincide con lo que ya declaran los términos (`legalContent.ts:82`).
3. **Sí al renombre** `gmail_thread_id`→`thread_id`, `gmail_message_id`→`message_id`,
   con `#[serde(alias)]` para leer sin migrar datos vivos.
4. **La detección por MX queda fuera de esta entrega.** En v1 el usuario elige
   proveedor con botones explícitos. Ver §9, congelada.

---

## 1. Punto de partida: el motor ya es (casi) agnóstico

Lo caro de Mira —clasificación, ventanas de tiempo, tiempos de respuesta y
resolución, auditoría con Bedrock, reportes— opera sobre `EmailMessage`
(`apps/api/src/analysis/mod.rs`): `from_email`, `subject`, `headers`, `body_text`,
`is_internal/external/automated`, fechas. **Ese motor no se toca.**

Gmail está aislado en `GmailClient` (`apps/api/src/gmail/mod.rs`), que hace 3 cosas:

1. Listar IDs de hilos (query de búsqueda de Gmail).
2. Traer un hilo y normalizarlo a `EmailMessage`.
3. Leer metadata de la casilla (labels, alias "enviar como", filtros, perfil).

**La excepción a documentar:** `refine_classification_with_gmail_labels` sí
entiende de Gmail. Ver §6.

## 2. Lo que hoy está casado con Google

1. **OAuth Google-specific**: `auth/refresh.rs` pega a `oauth2.googleapis.com`
   con scope `gmail.readonly`; `RefreshError::InvalidGrant` clasifica el
   `error=="invalid_grant"` propio de Google (`auth/refresh.rs:48-53`).
2. **Adaptador por tipo concreto**: `state.gmail.fetch_thread(...)`. No existe
   un `MailboxProvider`.
3. **Conceptos de Gmail filtrados al modelo**: "primary inbox" = `INBOX` +
   `CATEGORY_PERSONAL` (`gmail/mod.rs:15`); operadores de búsqueda; nombres
   `gmail_thread_id`/`gmail_message_id` por storage y frontend; identidad de la
   casilla leída del endpoint de perfil de Gmail, no de un `id_token`
   (`http/mod.rs:596`).
4. **Frontend**: `GmailConnectGate.tsx` es la única pantalla de conexión, con un
   solo botón; `components/filters/labels.ts` codifica el catálogo de etiquetas
   de sistema de Gmail; el copy público y legal nombra Gmail explícitamente.

## 3. La verdad incómoda: no hay una API universal

No existe "la API de todos los proveedores". Hay **dos caminos reales**, y Mira
solo lee correo (no envía), lo que simplifica:

- **OAuth dedicado** → un clic, sin contraseñas:
  - **Google** (Gmail + Google Workspace) — ya implementado.
  - **Microsoft Graph** (Outlook + Microsoft 365) — equivalente funcional.
- **IMAP** → el idioma común para todo lo demás. Un solo adaptador abre decenas
  de proveedores, al costo de perder hilos nativos y búsqueda server-side.

## 4. Estrategia de proveedores

| Proveedor | Mecanismo | UX | Fase |
|---|---|---|---|
| Gmail | Google OAuth | 1 botón | ya existe |
| Google Workspace (dominio propio) | **el mismo** Google OAuth | 1 botón | ya existe |
| Outlook / Microsoft 365 | Microsoft Graph (OAuth) | 1 botón | **esta entrega** |
| Hostinger / cPanel / Zoho / genérico | IMAP (email + app-password) | ~2 campos | fase posterior |

**Importante:** Google Workspace **es Gmail por debajo** (misma OAuth, misma API,
mismo scope). Un dominio propio en Workspace ya queda conectado hoy apretando
"Conectar con Google", sin conector nuevo. Lo mismo aplicará para M365 con el
botón de Microsoft.

## 5. Arquitectura: el trait y su despacho

### 5.1 El trait

```rust
#[async_trait]
pub trait MailboxProvider: Send + Sync {
    fn kind(&self) -> MailboxProviderKind;
    async fn list_thread_ids(&self, token: &str, config: &AnalysisConfig, max: u32)
        -> anyhow::Result<ThreadListPage>;
    async fn fetch_thread(&self, token: &str, thread_id: &str, config: &AnalysisConfig)
        -> anyhow::Result<ProviderThread>;
    async fn fetch_mailbox_metadata(&self, token: &str, now: DateTime<Utc>) -> MailboxMetadata;
}
```

Se usa `#[async_trait]` porque **ya es dependencia del proyecto** y es el patrón
de `StorageRepository` (`storage/mod.rs:96`). No se introduce nada nuevo.

### 5.2 Despacho: por conexión, no por arranque

El plan original decía `AppState.mailbox: Box<dyn MailboxProvider>` elegido una
vez. Es incorrecto: **cada organización puede usar un proveedor distinto**, así
que el despacho es por request, a partir del `provider` guardado en la conexión.

```rust
pub struct MailboxProviders {
    gmail: GmailClient,
    microsoft: Option<GraphClient>,   // None si Azure AD no está configurado
}

impl MailboxProviders {
    pub fn get(&self, kind: MailboxProviderKind)
        -> Result<&dyn MailboxProvider, ProviderUnavailable>;
}
```

`AppState.gmail` → `AppState.mailbox: MailboxProviders`. `microsoft` es `Option`
para que la API arranque sin credenciales de Azure AD (mismo criterio que se usó
con Mercado Pago: la ausencia degrada una función, no impide arrancar).

### 5.3 Tipos neutrales

- `GmailThreadData` → `ProviderThread`. Campo `is_primary_inbox` se conserva
  pero pasa a significar "está en la bandeja principal **según el proveedor**":
  Gmail = `INBOX`+`CATEGORY_PERSONAL`; Graph = carpeta `Inbox`; IMAP = `INBOX`.
- `label_ids: Vec<String>` → `folder_ids: Vec<String>`, valores opacos definidos
  por cada adaptador.
- `MailboxMetadata.labels` conserva su nombre en esta entrega (lo consume la UI
  de filtros); Graph mapea sus carpetas a esa forma. La neutralización del
  vocabulario en la UI es trabajo de la fase IMAP.
- `GmailConnection` → `MailboxConnection`, con campo nuevo
  `provider: MailboxProviderKind` (`#[serde(default)]` = `Google`, para que las
  filas existentes se lean sin migración) y `gmail_account_email` →
  `mailbox_email` (con alias).

### 5.4 Renombres con compatibilidad

Todos con `#[serde(alias = "<nombre viejo>")]` para leer el JSONB existente sin
migración de datos. La escritura pasa al nombre nuevo; las filas viejas se
normalizan al reescribirse.

| Antes | Después |
|---|---|
| `gmail_thread_id` | `thread_id` |
| `gmail_message_id` | `message_id` |
| `gmail_account_email` | `mailbox_email` |
| `GmailConnection` | `MailboxConnection` |
| record kind `gmail_connection` | se conserva (cambiarlo sí requeriría migrar datos) |

`gmail_scope_snapshot` se conserva como está: es una instantánea histórica del
scope consentido y renombrarla haría que los runs viejos mientan.

## 6. La deuda de clasificación que abre el multiproveedor

`refine_classification_with_gmail_labels` (`analysis/mod.rs:1024`) usa las
pestañas de Gmail (`CATEGORY_PROMOTIONS/SOCIAL/FORUMS`) para degradar la
clasificación y forzar revisión manual. Esa señal **no existe** en Graph ni en
IMAP.

- **Esta entrega:** la función se renombra a `refine_classification_with_folders`
  y opera sobre `folder_ids`; con Graph la lista viene vacía y la función es un
  no-op. El comportamiento de Gmail no cambia.
- **Consecuencia honesta:** una casilla Outlook tendrá **peor** separación de
  promociones/social que una de Gmail, y por tanto más ruido llegando a la
  auditoría IA. Se acepta como deuda conocida; el mitigante es la heurística
  `is_automated_sender` que ya existe y sí es neutral.
- **No** se inventa una señal equivalente en esta entrega.

## 7. UX de conexión: lista de opciones

En esta entrega el usuario ve **dos** opciones explícitas (la tercera llega con
IMAP). Los subtítulos son clave para que no adivine mal: le avisan que los
botones cubren también dominios propios.

```
┌──────────────────────────────────────────────────┐
│   Conecta tu casilla de correo                     │
│   Mira solo lee. Nunca envía. Revocable cuando     │
│   quieras.                                         │
│                                                    │
│   ┌──────────────────────────────────────────┐    │
│   │  G   Conectar con Google              →   │    │
│   │      Gmail y dominios en Google Workspace │    │
│   └──────────────────────────────────────────┘    │
│   ┌──────────────────────────────────────────┐    │
│   │  ⊞   Conectar con Microsoft           →   │    │
│   │      Outlook y Microsoft 365              │    │
│   └──────────────────────────────────────────┘    │
└──────────────────────────────────────────────────┘
```

Cuando ya hay una casilla conectada, la pantalla de cuenta muestra el proveedor
activo y conectar otra **reemplaza** la conexión previa, con confirmación
explícita (decisión 0.1.2).

## 8. Microsoft Graph — especificidades

- **Registro en Azure AD** (dependencia externa del responsable): app
  multi-tenant, `redirect_uri` propio, scope `Mail.Read` + `offline_access` +
  `User.Read`. Los tenants empresariales pueden exigir **consentimiento de
  administrador**; el copy debe anticiparlo.
- **Hilos nativos:** Graph expone `conversationId` en cada mensaje. Se listan
  mensajes filtrados y se agrupan por `conversationId` → no hace falta JWZ.
- **Búsqueda server-side:** `$filter` sobre `receivedDateTime` cubre la ventana
  de fechas. `$search` existe pero es incompatible con `$orderby`; se prefiere
  `$filter`.
- **Carpetas, no etiquetas:** `mailFolders` mapea a `MailboxMetadata.labels` con
  `label_type: "folder"`. No hay categorías tipo pestaña.
- **Identidad de la casilla:** de `/me` (`mail` o `userPrincipalName`), no de un
  endpoint de perfil de correo.
- **Refresh token:** Graph rota el refresh token en cada uso. El código de
  refresco **debe** persistir el nuevo valor; el de Google no lo necesitaba.
  Esto ya está cubierto por el CAS de `refresh_gmail_connection`.
- **Clasificación de error de refresh:** el equivalente de `invalid_grant` es
  `AADSTS700082`/`invalid_grant` en el cuerpo. Se mapea a la misma
  `RefreshError::InvalidGrant`.

## 9. [CONGELADO] Detección por MX y auto-redirect

> No entra en esta entrega (decisión 0.1.4). Se conserva el diseño para cuando
> llegue IMAP y la tercera opción "No estoy seguro / otro" lo justifique.

Cuando el usuario escribe su correo en la opción 3, Mira detectaría el proveedor
por los registros MX del dominio (no por el sufijo — el sufijo miente:
`west-ingenieria.cl` es Google, `ninfasolutions.com` es Hostinger).

| Dominio | MX apunta a… | Proveedor | Camino |
|---|---|---|---|
| `west-ingenieria.cl` | `aspmx.l.google.com` | Google Workspace | OAuth Google |
| `ninfasolutions.com` | `mx1.hostinger.com` | Hostinger | IMAP |
| (M365) | `*.mail.protection.outlook.com` | Microsoft 365 | OAuth Microsoft |
| (Zoho) | `mx.zoho.com` | Zoho | IMAP |

Cascada: **MX** → autodiscover (Microsoft) / autoconfig (Mozilla ISPDB) → SRV →
heurística (`imap.dominio`/`mail.dominio`:993).

Auto-redirigir es seguro porque el connect es navegación de página completa
(`Redirect::temporary`), no un popup: no aplica el bloqueo de popups sin gesto.
Para que se sienta idéntico al botón hace falta `login_hint=<correo>` (Google) y
`login_hint`+`domain_hint` (Microsoft).

**Riesgos a resolver antes de construirlo** (no estaban en la versión original):

- El endpoint recibe un dominio arbitrario y hace DNS saliente → necesita rate
  limiting y un timeout corto.
- Gateways de seguridad (Proofpoint `*.pphosted.com`, Mimecast `*.mimecast.com`)
  enmascaran el MX real → no fiarse solo del MX.
- Definir el umbral de "alta confianza" que habilita el auto-redirect.

`login_hint` sí se agrega en esta entrega: es una línea y mejora el flujo de los
dos botones explícitos.

## 10. IMAP — fase posterior, riesgos ya identificados

- **Sin hilos nativos.** Hay que reconstruir el hilo desde `Message-ID` /
  `In-Reply-To` / `References` (algoritmo JWZ). Es el trabajo más fino.
- **Sin búsqueda server-side equivalente.** IMAP da `SINCE`/`BEFORE` y selección
  de carpeta. El filtrado por etiqueta desaparece; el resto se filtra
  cliente-side descargando más correo.
- **SSRF:** el host IMAP lo aporta el usuario → la API se conectaría a
  `host:puerto` arbitrarios desde Cloud Run. Necesita validación de host,
  bloqueo de rangos privados y puertos permitidos acotados.
- **Rendimiento:** descargar bodies completos para JWZ es mucho más lento que la
  API de Gmail. Cloud Run corta a 1800s y la API corre con `max-instances=1`.
  Hay que medir antes de prometer.
- **App-passwords.** Muchos proveedores con 2FA los exigen. No se puede eliminar
  la fricción; se suaviza con guía por proveedor. Ventaja de confianza: es de
  solo lectura, revocable y no es la clave real.
- **Postura legal distinta:** guardar una contraseña no es lo mismo que guardar
  un token revocable. Los borradores legales (`legalContent.ts:143`) hablan de
  tokens; hay que reescribirlos antes de habilitar IMAP.
- **Microsoft mata basic-auth IMAP** en M365 empresarial → para Outlook serio el
  camino es Graph, no contraseña. Confirma el orden elegido.

## 11. Superficie de copy y documentos a corregir

El multiproveedor no es solo backend. Estos textos afirman hoy que Mira es
solo-Gmail y quedarían mintiendo:

- `apps/web/src/views/landing/pages/legalContent.ts:82` — *"La versión actual
  soporta únicamente Gmail y Google Workspace"*. También `:32`, `:49`, `:138`, `:143`.
- `apps/web/src/views/landing/content.ts:25,49,61` — copy de la landing.
- `apps/web/src/views/ConfiguracionView.tsx:463,754`, `AyudaView.tsx:73`,
  `PrivacidadDatosView.tsx:190-196`, `CuentaView.tsx:192,200`,
  `PaymentNotes.tsx:14`, `PublicFooter.tsx:15`.
- `apps/web/src/components/runs/AnalysisFunnelPanel.tsx:52,60,80` — *"Gmail
  encontró…"*, *"Encontrados en Gmail"*.
- `docs/plan-production-ready.md` §7.1 (política de privacidad y términos, que
  siguen en borrador) y la lista de subprocesadores §17.1: **entra Microsoft**.
- La alerta de negocio `gmail_refresh_invalid_grant` (§11.4 del plan de
  producción) pasa a ser por proveedor.

**Aplicado el 2026-07-20.** Quedan deliberadamente sin tocar, para la fase IMAP:
el vocabulario "etiquetas" del selector de filtros (`LabelPickerModal`,
`labels.ts`), que sigue siendo jerga de Gmail; los identificadores internos
(`disconnect_gmail`, `gmail_connected`, `onDisconnectGmail`), que no son
visibles; y los defaults de dominios ignorados sesgados a `google.com`.

## 12. Tamaño honesto del esfuerzo

| Pieza | Esfuerzo | Nota |
|---|---|---|
| Motor de análisis/reportes | **0** | ya es neutral |
| Trait + `GmailClient` como impl + `MailboxProviders` | Bajo (1 d) | mecánico, 3 call sites |
| Renombres con `serde(alias)` | Bajo (0,5 d) | ~55 ocurrencias, sin DDL |
| `provider` en la conexión + config Microsoft opcional | Bajo (0,5 d) | |
| Adaptador Microsoft Graph | **Medio (3–5 d)** | agrupación por `conversationId`, refresh rotativo |
| Rutas de connect por proveedor + `login_hint` | Bajo (0,5 d) | espejo de `gmail_connect_login` |
| Frontend: 2 botones, provider en el estado, copy | Medio (2–3 d) | |
| Corrección de copy legal y público | Bajo (0,5 d) | requiere tu revisión |
| — fase posterior — | | |
| Adaptador IMAP + JWZ + guardas SSRF | Alto (5–8 d) | |
| Detección MX + embudo opción 3 | Bajo-Medio (1–2 d) | |
| Neutralizar vocabulario "etiquetas"→"carpetas" en UI | Medio (2–3 d) | |

## 13. Orden de ejecución

**Etapa A — neutralización — HECHA 2026-07-20**
1. [x] Renombres con `serde(alias)`: `thread_id`, `message_id`, `mailbox_email`.
2. [x] `MailboxProvider` trait; `GmailClient` pasa a ser impl.
3. [x] `MailboxConnection` con campo `provider`.
4. [x] `AppState.gmail` → `AppState.mailbox: MailboxProviders`.
5. [x] `refine_classification_with_gmail_labels` → `_with_folders`.

**Etapa B — Microsoft Graph — HECHA 2026-07-20 (falta la app en Azure AD)**
6. [x] `MicrosoftConfig` opcional en `config/mod.rs`, desde Secret Manager.
7. [x] `GraphClient` implementando el trait.
8. [x] Rutas `/mailbox/connect/microsoft/login|callback` + `/mailbox/providers`,
   con `/gmail/*` conservadas como alias. `login_hint` agregado a ambos connects.
9. [x] Refresh por proveedor con persistencia del refresh token rotado.
10. [ ] **Pendiente del responsable:** registrar la app en Azure AD y cargar
    `MICROSOFT_CLIENT_ID`, `MICROSOFT_CLIENT_SECRET`, `MICROSOFT_REDIRECT_URL`.
11. [ ] Probar un connect real y un análisis contra una casilla Outlook.

**Etapa C — frontend y copy**
12. [x] `GmailConnectGate` → `MailboxConnectGate` con la lista de proveedores.
13. [x] `mailbox_provider` en `AccountStatus` y en la vista de Cuenta.
14. [x] Corrección del copy público y legal (§11) — 2026-07-20. **Los textos
    legales necesitan tu lectura antes de publicarse**: cambió el alcance
    declarado del servicio y la lista de subprocesadores.

**Fase posterior — IMAP**, solo cuando un cliente de pago lo bloquee.

---

## 14. Cabos sueltos a resolver al implementar

- Qué pasa con los runs históricos de una casilla Gmail si la organización
  conecta después una de Microsoft: se conservan (los datos ya son del run, no
  de la conexión), pero la UI debe evitar dar a entender que son de la casilla
  activa.
- Consentimiento de admin en Workspace/M365: qué mensaje ve el usuario cuando el
  tenant bloquea la app.
- Si Graph no está configurado (`microsoft: None`), el botón no debe aparecer en
  el frontend en vez de fallar al apretarlo.
