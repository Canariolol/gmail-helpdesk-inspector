# Plan conceptual — Conexión multiproveedor (Gmail + Microsoft + IMAP)

> Estado: **definición conceptual cerrada**. Pendiente de implementación.
> Fecha: 2026-06-25
> Alcance: ampliar Mira más allá de Gmail para soportar casillas de Outlook/Microsoft 365
> y proveedores genéricos (Hostinger, cPanel, Zoho, etc.), manteniendo la conexión lo
> más amigable posible para el usuario.

---

## 1. Punto de partida: el motor ya es agnóstico al proveedor

Lo más caro de Mira —clasificación, ventanas de tiempo, tiempos de respuesta/resolución,
auditoría con Bedrock, reportes— **no sabe nada de Gmail**. Todo opera sobre una struct
neutral, `EmailMessage` (`apps/api/src/analysis/mod.rs`): `from_email`, `subject`,
`headers`, `body_text`, `is_internal/external/automated`, fechas. **Ese motor no se toca.**

Gmail está aislado en un solo adaptador, `GmailClient` (`apps/api/src/gmail/mod.rs`), que
hace 3 cosas:
1. Listar IDs de hilos (vía query de búsqueda de Gmail).
2. Traer un hilo y normalizarlo a `EmailMessage`.
3. Leer metadata de la casilla (labels, alias "enviar como", filtros, perfil).

→ **Consecuencia clave:** ampliar el scope es factible porque la parte cara ya es neutral.
El trabajo nuevo se concentra en (a) un trait fino, (b) uno o dos adaptadores nuevos,
(c) plomería de auth/credenciales, (d) algo de neutralización en el frontend.

## 2. Lo que hoy está casado con Google

1. **OAuth Google-specific**: `apps/api/src/auth/refresh.rs` pega a `oauth2.googleapis.com`
   con scope `gmail.readonly`; la sesión tiene campos `gmail_*`.
2. **Adaptador por tipo concreto, no por trait**: `state.gmail.fetch_thread(...)` en
   `apps/api/src/http/mod.rs`. No existe aún un `MailboxProvider`.
3. **Conceptos de Gmail filtrados al modelo**: "primary inbox" = `INBOX` +
   `CATEGORY_PERSONAL`; operadores de búsqueda (`label:`, `category:`); nombres
   `gmail_thread_id` / `gmail_message_id` por todo el storage y el frontend.

## 3. La verdad incómoda: no hay una API universal

No existe "la API de todos los proveedores". En la práctica hay **dos caminos reales**, y
Mira solo lee correo (no envía), lo que simplifica:

- **OAuth dedicado** → un clic, sin contraseñas:
  - **Google** (Gmail + Google Workspace) — ya implementado.
  - **Microsoft Graph** (Outlook + Microsoft 365) — equivalente a la API de Gmail.
- **IMAP** → el idioma común para todo lo demás (Hostinger, cPanel, Zoho, Fastmail…).
  Un solo adaptador abre decenas de proveedores.

## 4. Estrategia de proveedores (enfoque híbrido)

| Proveedor | Mecanismo | UX |
|---|---|---|
| Gmail | Google OAuth (ya existe) | 1 botón |
| Google Workspace (dominio propio) | **El mismo** Google OAuth + Gmail API | 1 botón |
| Outlook / Microsoft 365 | Microsoft Graph (OAuth) | 1 botón |
| Hostinger / cPanel / Zoho / genérico | IMAP (email + contraseña/app-password) | ~2 campos, guiado |

**Importante:** Google Workspace **es Gmail por debajo** (misma OAuth, misma Gmail API,
mismo scope). Un dominio propio en Workspace (ej. `@west-ingenieria.cl`) **ya quedaría
conectado hoy** apretando "Conectar con Google", sin conector nuevo. Lo mismo para M365 con
el botón de Microsoft. El conector IMAP nuevo solo hace falta para hosting genérico
(ej. `@ninfasolutions.com` en Hostinger).

## 5. UX de conexión: lista de 3 opciones

El usuario ve una lista con tres opciones. Los **subtítulos** son clave para que no adivine
mal (le avisan que los botones cubren también dominios propios).

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
│   ┌──────────────────────────────────────────┐    │
│   │  ?   No estoy seguro / otro           →   │    │
│   │      Hostinger, cPanel, otros proveedores │    │
│   └──────────────────────────────────────────┘    │
└──────────────────────────────────────────────────┘
```

- **Opción 1 — Conectar con Google**: OAuth. Cubre `@gmail.com` y cualquier dominio en
  Google Workspace.
- **Opción 2 — Conectar con Microsoft**: OAuth (Graph). Cubre Outlook/Hotmail/Live y
  cualquier dominio en Microsoft 365.
- **Opción 3 — No estoy seguro / otro**: revela un campo de correo y actúa como **embudo
  inteligente** (ver §6). Es la red de seguridad para quien no sabe su proveedor o usa un
  proveedor IMAP.

## 6. Opción 3: detectar y reencauzar (NO es la puerta del IMAP)

Cuando el usuario escribe su correo en la opción 3, Mira **detecta el proveedor por los
registros MX del dominio** (no por el texto del dominio — el sufijo miente:
`west-ingenieria.cl` es Google, `ninfasolutions.com` es Hostinger).

### Detección por MX (señal primaria)

| Dominio | MX apunta a… | Proveedor | Camino |
|---|---|---|---|
| `west-ingenieria.cl` | `aspmx.l.google.com` | Google Workspace | OAuth Google |
| `ninfasolutions.com` | `mx1.hostinger.com` | Hostinger | IMAP |
| (M365) | `*.mail.protection.outlook.com` | Microsoft 365 | OAuth Microsoft |
| (Zoho) | `mx.zoho.com` | Zoho | IMAP |

Cascada de detección (de más fuerte a más débil): **MX** → *autodiscover* (Microsoft) /
autoconfig (Mozilla ISPDB) → registros SRV → heurística (`imap.dominio`/`mail.dominio`:993).

### Comportamiento según resultado

```
[ No estoy seguro / otro ] → escribe correo → [Continuar]
        │
        ▼
   "Detectando tu proveedor…"   (~1s, estado transitorio, no freeze)
        │
   ┌────┴───────────────┬──────────────────────┐
   ▼                    ▼                       ▼
 Google/Microsoft   Hostinger/cPanel        ambiguo / nada
 (alta confianza)        │                       │
   ▼                     ▼                       ▼
 redirige solo      pide contraseña         muestra opciones
 (login_hint)       (app-password)          o IMAP manual
```

- **Google/Microsoft detectado (alta confianza)** → **auto-redirige al login del proveedor**,
  como si hubiera apretado el botón en primera instancia. **No** se muestra un paso
  intermedio del tipo "tu correo es de Google, pincha acá". Ver §7.
- **Proveedor IMAP detectado** → autodetecta host/puerto y pide **solo la contraseña**
  (o app-password), con guía específica del proveedor. Prueba la conexión en vivo
  (login + leer INBOX) antes de guardar.
- **Ambiguo / sin detección** → **no** auto-redirige; muestra las opciones o cae a un
  formulario IMAP manual mínimo. Nunca a un error seco.

## 7. Auto-redirect a OAuth en la opción 3

Cuando la detección encuentra Google o Microsoft con alta confianza, el usuario es enviado
**directo** al login del proveedor (sin paso intermedio).

### Por qué es seguro

El connect de Google actual (`apps/api/src/http/mod.rs`, `gmail_connect_login`) es un
**redirect de página completa del servidor** (`Redirect::temporary` a `accounts.google.com`),
**no un popup**. Por eso auto-redirigir tras una detección asíncrona es válido y no lo
bloquea el navegador:

> El problema clásico de auto-redirigir a OAuth es que los navegadores bloquean **popups**
> abiertos sin clic directo, y la detección MX es asíncrona (ya se perdió el "gesto"). Con
> navegación de página completa (`window.location` → endpoint del server → 302) eso **no
> aplica**.

### El detalle que lo hace idéntico al botón: `login_hint`

Hoy la URL de OAuth **no pasa el correo**. Para que el provider preseleccione la cuenta y no
muestre el selector (que se siente raro tras un redirect automático), añadir:

- **Google** → `login_hint=<correo>`.
- **Microsoft** → `login_hint=<correo>` + `domain_hint=<dominio>` (preselecciona tenant).

La pantalla de consentimiento igual aparece (`prompt=consent` es necesario para obtener el
refresh token), pero eso pasa **también** por el botón normal → no hay regresión.

### Guardas

1. **Estado transitorio breve** durante la detección ("Detectando… → Te llevamos a Google de
   forma segura"). Feedback, no un clic extra.
2. **Auto-redirige solo con alta confianza.** Ambiguo (gateway tipo Proofpoint/Mimecast
   enmascarando el MX, o detección dudosa) → no auto-redirige.
3. **Retorno elegante** si el usuario cancela en el provider: vuelve a la pantalla de
   conexión limpia, no a un error.

## 8. Especificidades y "gotchas" del adaptador IMAP

- **Sin hilos nativos.** Gmail entrega `threads`; IMAP genérico **no**. Hay que reconstruir
  el hilo desde `Message-ID` / `In-Reply-To` / `References` (algoritmo JWZ). Es el trabajo
  más fino del adaptador.
- **Carpetas, no labels.** IMAP tiene carpetas, no labels ni "categorías". El concepto
  "primary inbox" (`INBOX` + `CATEGORY_PERSONAL`) no traduce; hay que neutralizar la UI de
  filtros a "carpetas" y volver el "primary inbox" específico de cada proveedor.
- **App-passwords.** Muchos proveedores con 2FA exigen una "contraseña de aplicación" en vez
  de la clave real. No se puede eliminar esa fricción; se suaviza con guía específica por
  proveedor y deep-links cuando existan. Ventaja de confianza: el app-password es de **solo
  lectura, revocable y no es la clave real** del usuario.
- **Microsoft mata basic-auth IMAP** en M365 empresarial → para Outlook serio el camino es
  Graph (u OAuth-IMAP/XOAUTH2), no contraseña.
- **Autodetección de servidor** para que el formulario IMAP colapse de 5 campos
  (email, contraseña, host, puerto, seguridad) a ~1 (contraseña): Mozilla ISPDB → SRV DNS →
  autoconfig/autodiscover → heurística.

## 9. Tono / copy (sin jerga)

- **Nunca** mencionar "MX" ni "IMAP" en pantalla. Internamente sí; al usuario:
  *"Detectamos que tu correo es de Google ✓"*. La detección se siente como adivinanza
  amable, no técnica.
- Reforzar confianza junto al campo de contraseña (encaja con `PrivacidadDatosView`):
  "solo lectura, encriptado, revocable".
- Anticipar el **consentimiento de administrador** en Workspace/M365 en el copy
  ("puede que tu administrador deba autorizar Mira").

## 10. Movimiento de arquitectura que habilita todo

Antes de cualquier adaptador nuevo: extraer un trait `MailboxProvider` con lo que hoy hace
`GmailClient`:

```text
trait MailboxProvider {
    list_thread_ids(...) -> Vec<ThreadId>
    fetch_thread(...)     -> ThreadData  // devuelve EmailMessage (neutral)
    fetch_metadata(...)   -> MailboxMetadata  // folders/labels neutralizados
}
```

- `GmailClient` pasa a ser una impl; se agregan `ImapClient` y (fase 2) `GraphClient`.
- `AppState.gmail` → `AppState.mailbox: Box<dyn MailboxProvider>`, elegido por un campo
  `provider` en la sesión.
- La sesión gana un discriminante `provider` + almacenamiento de credenciales por proveedor
  (reusar el AES-GCM ya existente de `apps/api/src/auth/mod.rs`).
- Renombrar `gmail_thread_id` / `gmail_message_id` → `thread_id` / `message_id`: mecánico
  pero toca storage + tipos del frontend.
- Neutralizar el concepto "label" → "carpeta/label", con "primary inbox" específico por
  proveedor.

## 11. Tamaño honesto del esfuerzo

| Pieza | Esfuerzo | Nota |
|---|---|---|
| Motor de análisis/reportes | **0** | ya es neutral |
| Extraer trait + refactor Gmail | Bajo (1–2 d) | mecánico |
| Adaptador IMAP | **Medio (3–5 d)** | reconstrucción de hilos es lo fino |
| Auth/credenciales + UX de conexión | Medio (2–3 d) | |
| Detección por MX + routing opción 3 | Bajo-Medio (1–2 d) | |
| Frontend: lista de 3, form IMAP, "labels"→"carpetas" | Medio (2–4 d) | |
| Adaptador Microsoft Graph (fase 2) | Medio (3–5 d) | reusa el trait |

**Orden sugerido:** trait → adaptador IMAP (validar con Hostinger/Ninfa) → detección+routing
→ Microsoft Graph como fase 2 cuando Outlook empresarial lo pida.

## 12. Cabos sueltos / casos borde a resolver al implementar

- **Gateways de seguridad** (Proofpoint `*.pphosted.com`, Mimecast `*.mimecast.com`)
  enmascaran el MX real → no fiarse solo del MX; caer a autodiscover u ofrecer OAuth primero.
- **Consentimiento de admin** en Workspace/M365 (allowlisting de apps OAuth de terceros).
- **Qué exactamente** mostrar cuando el MX es ambiguo (definir umbral de "alta confianza").
- **Postura de seguridad** de guardar credenciales IMAP vs tokens OAuth (cifrado ya existe;
  preferir OAuth-IMAP/XOAUTH2 donde el proveedor lo ofrezca).

---

## Toques concretos de backend (cuando se implemente)

- Añadir `login_hint` (Google) y `login_hint`+`domain_hint` (Microsoft) al armado de la URL
  OAuth en `apps/api/src/http/mod.rs`.
- Extraer trait `MailboxProvider`; `GmailClient` como impl; `AppState.mailbox`.
- Endpoint de detección por MX para la opción 3.
- Nuevo connect de Microsoft Graph (espejo de `gmail_connect_login`).
- Adaptador IMAP (crate sugerido: `async-imap`) con reconstrucción de hilos (JWZ).
- Campo `provider` + credenciales por proveedor en `UserSession` y storage.
