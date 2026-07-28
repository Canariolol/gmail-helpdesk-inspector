Tengo todo el contexto necesario. Aquí está el plan UX completo.

---

# Plan UX — TASK-007: Configuración guiada para beta privada

## Diagnóstico de partida

`FilterBar.tsx` expone directamente la superficie técnica de la API: fechas, hora, dominos internos, dominios ignorados, palabras ignoradas — todo hardcodeado para una empresa real. Ese modelo debe desaparecer. La nueva UI no configura filtros; **configura una política operacional que produce filtros.**

---

## 1. Flujo de configuración guiada

El wizard no es un formulario de configuración; es un diálogo sobre cómo opera el equipo. Cada pantalla hace una sola pregunta de política, no solicita campos técnicos.

### Secuencia de pasos

```
[1] Organización y contexto
[2] Conectar Gmail          ← extiende el modal existente de LoginView
[3] Casilla de soporte
[4] Quién responde           ← reemplaza "dominios internos"
[5] Qué cuenta               ← reemplaza "keywords ignoradas"
[6] Auditoría IA             ← opt-in explícito
[7] Scheduler y reportes
[8] Retención
[9] Vista previa             ← preview con hilos de ejemplo antes del primer run
```

**Regla de navegación:** cada paso es válido de forma independiente. El usuario puede guardar como borrador y continuar después. El botón "Siguiente" solo avanza; no guarda hasta confirmación en el paso final. Los pasos 6–8 son opcionales para la primera configuración y se pueden configurar después desde ajustes.

---

### Paso 1 — Organización y contexto

**Pregunta central:** ¿Cómo se llama tu equipo y dónde operan?

**Campos:**
- Nombre de la organización (texto libre)
- Zona horaria (selector IANA con búsqueda — no un dropdown enorme, sino un input con autocompletado de las 40 zonas más comunes primero)
- Idioma del reporte (Español / English — solo estos en beta)

**Ayuda contextual inline:**
> La zona horaria se usa para calcular tiempos de respuesta y programar análisis automáticos. Si tu equipo opera en múltiples zonas, elige la del mailbox principal.

---

### Paso 2 — Conectar Gmail

Reutiliza el modal de permisos de `LoginView` (`oauth-modal`) como pantalla completa dentro del wizard, con adición de:

- **Badge "Solo lectura"** prominente con `gmail.readonly`
- Aclaración beta Workspace: *"En esta versión beta solo se admiten cuentas Google Workspace organizacionales."*
- Lista de lo que NO hace la app (enviar, etiquetar, archivar, borrar) — ya existe en el modal, solo se ensancha visualmente
- Estado de conexión persistente: si ya está conectado, muestra email conectado + botón "Cambiar cuenta"

---

### Paso 3 — Casilla de soporte

**Pregunta central:** ¿Cuál es la casilla que quieres analizar?

**Campos:**
- Email de la casilla principal (pre-relleno con el email autenticado, editable para casillas compartidas/alias)
- Propósito (selector): *Soporte técnico / Atención al cliente / Operaciones / Otro*
- ¿Es una casilla compartida o alias? (toggle con explicación de impacto)

**Microcopy de ayuda:**
> Si tu equipo responde desde `soporte@tuempresa.com` pero recibe desde `hola@tuempresa.com`, configura el alias aquí. Los análisis medirán tiempos de respuesta solo desde esta casilla.

---

### Paso 4 — Quién responde (reemplaza "dominios internos")

**Pregunta central:** ¿Quién forma parte de tu equipo?

**NO** es: "Lista de dominios internos separados por coma."

**ES:**
1. *"¿Cuál es el dominio de email de tu empresa?"* → `@tuempresa.com`
   - La app deriva internamente la regla de dominio interno.
   - Preview inline inmediato: *"Los correos enviados desde @tuempresa.com se contarán como respuestas de tu equipo."*
2. *"¿Algún miembro del equipo usa un dominio diferente?"* → campo adicional opcional para aliases/consultores.
3. Toggle: *"¿Tu equipo también recibe copias de sistemas internos (ERP, tickets, CRM)?"* → si sí, los clasifica como `automated` en vez de contar como respuestas.

**Sin exponer** la string técnica de dominios. El modelo interno puede tener `["@tuempresa.com", "@subdominio.com"]`; el usuario ve un diálogo.

---

### Paso 5 — Qué cuenta como solicitud (reemplaza "keywords ignoradas")

**Pregunta central:** ¿Qué tipo de correos llegan a esta casilla?

**Formato:** tarjetas de selección múltiple con descripción, no checkboxes técnicas.

```
┌─────────────────────────────────┐  ┌──────────────────────────────────┐
│  📩 Solicitudes de clientes     │  │  🤖 Notificaciones automáticas   │
│  externos                       │  │                                  │
│  Correos de personas que piden  │  │  Confirmaciones, alertas de      │
│  atención de tu equipo          │  │  sistemas, formularios web       │
│                                 │  │                                  │
│  [Sí, llegan ✓]                 │  │  [Sí, llegan ✓]                  │
└─────────────────────────────────┘  └──────────────────────────────────┘

┌─────────────────────────────────┐  ┌──────────────────────────────────┐
│  📰 Boletines y marketing       │  │  👥 Conversaciones internas      │
│                                 │  │                                  │
│  Newsletters, promociones,      │  │  Coordinación entre miembros     │
│  listas de correo               │  │  del equipo                      │
│                                 │  │                                  │
│  [Sí, llegan ✓]                 │  │  [Sí, llegan ✓]                  │
└─────────────────────────────────┘  └──────────────────────────────────┘
```

Cada tarjeta seleccionada abre una opción: *"¿Cómo quieres tratarlos?" → Ignorar / Clasificar como [tipo] / Revisar manualmente.*

**Preview de clasificación** al pie: muestra 3–5 ejemplos de asuntos genéricos (NO datos reales del usuario) y cómo quedarían clasificados con la configuración actual. Esto hace tangible la política sin revelar datos sensibles.

> *Ejemplo: "Re: Confirmación de pago #12345" → Automático (ignorado) ✓*
> *Ejemplo: "Problema con acceso al sistema" → Solicitud válida ✓*

**Presets editables** opcionales: *Soporte TI / Atención al cliente / Operaciones* — cargan configuraciones razonables que el usuario puede ajustar.

---

### Paso 6 — Auditoría IA (opt-in)

**Pantalla dedicada de consentimiento.** No es un toggle escondido en un formulario.

**Estructura:**

```
┌──────────────────────────────────────────────────────────────┐
│  🤖  Clasificación con IA                                    │
│                                                              │
│  La IA analiza cada hilo para clasificarlo y detectar        │
│  casos ambiguos que necesitan revisión manual.               │
│                                                              │
│  ¿Qué datos envía?                                           │
│  • Fragmento del asunto y primeras líneas del mensaje        │
│    (máx. 280 caracteres por mensaje)                         │
│  • Hasta 14 mensajes por hilo                                │
│  • NO se envían cuerpos completos                            │
│  • NO se incluyen datos bancarios, contraseñas ni adjuntos   │
│                                                              │
│  ¿Quién lo procesa?                                          │
│  Amazon Bedrock (Claude). Ver política de privacidad →       │
│                                                              │
│  ¿Puedo desactivarla?                                        │
│  Sí, en cualquier momento desde Configuración.               │
│  Los análisis futuros usarán solo clasificación por reglas.  │
│                                                              │
│  [  ] Activar auditoría IA  (desactivada por defecto)        │
│                                                              │
│  ─────────────────────────────────────────────────────────  │
│  Sin IA activa, los casos ambiguos irán a revisión manual.  │
└──────────────────────────────────────────────────────────────┘
```

**Microcopy si se activa:**
> Al activar esto aceptas que fragmentos de tus hilos de soporte se procesen mediante Amazon Bedrock para clasificación automática. Puedes desactivarlo en cualquier momento; los cambios aplican al próximo análisis.

**Microcopy si se mantiene desactivada:**
> Sin IA, la clasificación usa solo reglas de dominio y palabras clave. Los hilos sin clasificación clara aparecerán en "Revisión manual".

---

### Paso 7 — Scheduler y reportes

**Dos sub-secciones independientes en la misma pantalla:**

#### Análisis automático
```
¿Cada cuándo quieres analizar la casilla?

  ○ Manual (yo inicio cada análisis)
  ○ Días laborales  →  ¿A qué hora?  [08:00]
  ○ Personalizado   →  abre selector de días/horas
```

**Microcopy:**
> El análisis automático cubre desde el último análisis hasta el momento de ejecución. Si no se ejecutó el día anterior, cubre el período completo.

#### Reportes por email
```
¿Quieres recibir resúmenes por email?

  ○ No, solo veo los reportes aquí
  ○ Sí  →  ¿A qué dirección(es)?  [campo múltiple]

  Contenido del reporte:
  ● Solo métricas (recomendado)
  ○ Métricas + lista de asuntos en revisión
     ⚠ Incluye asuntos de correos de tus clientes
        fuera de esta aplicación
```

**Microcopy de advertencia para la opción con asuntos:**
> Los asuntos de correos se incluirán en el email del reporte. Asegúrate de que los destinatarios tienen autorización para ver esta información.

---

### Paso 8 — Retención

**Pantalla simplificada, sin jerga técnica:**

```
¿Cuánto tiempo conservar los análisis?

  ○ 30 días  (recomendado para beta)
  ○ 90 días
  ○ Personalizado  →  [número] días

  ¿Qué se elimina al vencer el plazo?
  Los resultados del análisis, métricas y clasificaciones.
  NO se elimina nada de tu Gmail.
```

**Microcopy de footer:**
> Puedes cambiar esta configuración en cualquier momento. Los análisis ya realizados conservan su fecha de expiración original.

---

### Paso 9 — Vista previa antes del primer run

**No es un resumen de configuración técnica.** Es una vista que simula qué verían en el dashboard con esa política.

Estructura:
1. **"Con esta configuración, analizarás..."**: casilla + equipo + período default
2. **Ejemplos de clasificación** con 4–6 tarjetas de hilos ficticios que muestran cómo quedarían clasificados
3. **"¿Qué no entraría?"**: muestra 2–3 ejemplos de correos excluidos y por qué
4. **Aviso IA** (si desactivada): *"Sin IA activa, los hilos ambiguos irán a revisión manual."*
5. Botón primario: **"Guardar configuración"** — solo aquí se persiste
6. Botón secundario: **"Volver a revisar"**

---

## 2. Pantallas y componentes mínimos — primer incremento

**Principio:** no construir todo el wizard antes del primer run. Construir lo necesario para reemplazar `FilterBar.tsx` con algo que no exponga hardcodes y permita avanzar a beta.

### Componentes nuevos requeridos

| Componente | Descripción | Prioridad |
|---|---|---|
| `SetupWizard` | Shell del wizard con stepper y estado de borrador | P0 |
| `OrgStep` | Nombre + timezone | P0 |
| `MailboxStep` | Casilla + propósito | P0 |
| `TeamStep` | Dominio empresa → política interna | P0 |
| `PolicyStep` | Tarjetas de tipo de correo | P0 |
| `AiConsentStep` | Pantalla opt-in IA | P0 |
| `PolicyPreview` | Vista previa antes del primer run | P0 |
| `ScheduleStep` | Scheduler + reportes | P1 |
| `RetentionStep` | Retención simple | P1 |
| `OrgSettingsView` | Vista de ajustes post-onboarding | P2 |

### Componentes modificados

| Componente | Cambio necesario |
|---|---|
| `FilterBar.tsx` | **Eliminar** una vez que el wizard persiste la política en backend. En el ínterin, puede quedar como overrides temporales de un run solo para el owner |
| `LoginView.tsx` | Reutilizar el `oauth-modal` como `GmailPermissionsModal` standalone usable en paso 2 |
| `AyudaView.tsx` | Añadir sección "Tu configuración actual" con resumen de política activa y enlace a ajustes |
| `App.tsx` | Detectar si org tiene policy configurada; si no, redirigir al wizard antes de permitir análisis |

### Rutas de UI propuestas

```
/setup              → SetupWizard (solo si no hay policy configurada)
/settings/org       → OrgSettingsView (post-onboarding)
/settings/ai        → AiConsentStep standalone
/settings/schedule  → ScheduleStep standalone
```

---

## 3. Microcopy de privacidad, IA, retención y reportes

### Bloque de privacidad reutilizable (extiende `PrivacyCallout`)

El `PrivacyCallout` existente ya tiene buen tono. Para el wizard se necesita una variante expandida:

```
🔒 Lo que nunca hacemos
   • No enviamos, editamos ni borramos correos de tu Gmail
   • No guardamos cuerpos completos de los mensajes
   • El análisis IA usa solo fragmentos minimizados
   • Puedes revocar el acceso desde tu cuenta Google en cualquier momento
```

### Microcopy de IA — tres momentos

**En la tarjeta de onboarding (paso 6):**
> La IA clasifica hilos para que puedas enfocarte en los casos que realmente necesitan atención. Solo ve fragmentos de texto, no los correos completos.

**En el banner de análisis en curso:**
> Clasificando con IA · X hilos procesados · Tokens usados: N

**En la vista de hilo con clasificación IA:**
> Clasificado por IA · Confianza: 94% · [Ver razones]

### Microcopy de retención

**En el dashboard, badge en cada run:**
> Expira en 23 días · [Ampliar]

**En ajustes:**
> Los análisis se eliminan automáticamente después de 30 días. Solo se borran los resultados y métricas; tu Gmail no se modifica.

### Microcopy de reportes

**Toggle de contenido del reporte:**
> **Solo métricas** — El reporte incluye totales, promedios y tendencias. No incluye asuntos, remitentes ni contenido de correos.

> **Métricas + asuntos en revisión** — Incluye los asuntos de los hilos que necesitan revisión manual. Asegúrate de que todos los destinatarios tienen acceso autorizado a esta información.

---

## 4. Cómo evitar el checklist de keywords

El problema raíz de `FilterBar.tsx` es que expone el modelo de datos en lugar del modelo mental del usuario. Cinco principios concretos para evitar esto:

1. **Preguntas de intención, no de configuración.** No "Palabras ignoradas: [input]" sino "¿Llegan boletines a esta casilla?" → la app deriva las reglas.

2. **Presets por rol operacional.** El usuario selecciona "Soporte TI" y obtiene una política razonable. Puede ajustarla, pero no parte de cero.

3. **Preview inmediato.** Cada cambio de política muestra en tiempo real cómo clasificaría ejemplos ficticios. El feedback hace tangible lo abstracto sin exponer datos reales.

4. **Reglas explicables, no strings.** Cuando un hilo se ignora, la UI dice "Ignorado: remitente en lista de automatizados", no "matched keyword: newsletter". La regla tiene nombre de política, no de keyword.

5. **Progresividad.** La configuración básica (pasos 1–5) es suficiente para el primer run. Los pasos avanzados (AI, scheduler, retención) se pueden configurar después. Esto evita abrumar al usuario en el onboarding.

---

## 5. Estados vacíos, errores y confirmaciones

### Estado vacío: sin organización configurada

```
┌──────────────────────────────────────┐
│  📬  Configura tu casilla de soporte │
│                                      │
│  Antes de analizar necesitas         │
│  configurar tu organización y        │
│  conectar Gmail.                     │
│                                      │
│  [Comenzar configuración →]          │
└──────────────────────────────────────┘
```

### Estado vacío: sin runs

```
Todo listo. Inicia tu primer análisis para
ver métricas y clasificaciones.

[▶ Analizar ahora]
```

### Estado vacío: Gmail desconectado

```
⚠ Tu casilla no está conectada

El acceso a Gmail expiró o fue revocado.
Necesitas reconectar para continuar.

[Reconectar Gmail]  Cómo verificar desde Google →
```

### Estado vacío: IA desactivada, revisión manual acumulada

```
N hilos esperan revisión manual

La auditoría IA está desactivada. Puedes
revisar manualmente o activar IA para
clasificación automática.

[Revisar manualmente]  [Activar IA]
```

### Error: wizard con paso inválido

```
⚠ Falta completar esta sección

[Descripción del campo requerido]

Puedes guardar como borrador y continuar después.

[Guardar borrador]  [Completar ahora]
```

### Error: policy guardada con fallo de red

```
No se pudo guardar la configuración.

Tu configuración está guardada localmente.
Revisa tu conexión e intenta de nuevo.

[Reintentar]
```

### Confirmación: primer run con AI activada

```
┌────────────────────────────────────────────────┐
│  🤖  Iniciar análisis con IA                   │
│                                                │
│  Se enviarán fragmentos de hasta 14 mensajes   │
│  por hilo a Amazon Bedrock para clasificación. │
│                                                │
│  No se envían cuerpos completos ni adjuntos.   │
│                                                │
│  [Cancelar]            [Confirmar y analizar]  │
└────────────────────────────────────────────────┘
```

### Confirmación: cambio de política con runs activos

```
⚠ Tienes análisis en curso

Cambiar la política de clasificación no afecta
el análisis en curso. Se aplicará al próximo run.

[Cancelar]  [Guardar de todas formas]
```

### Confirmación: desactivar IA

```
Al desactivar la IA, los próximos análisis
usarán solo clasificación por reglas.

Los análisis ya realizados no cambian.

[Cancelar]  [Desactivar IA]
```

---

## 6. Dependencias de contrato/API que debe entregar backend

La UI del wizard depende de los siguientes endpoints. Backend debe entregarlos antes de que la UI sea funcional.

### Endpoints requeridos en P0

| Endpoint | Método | Descripción |
|---|---|---|
| `/orgs/me` | GET | Org del usuario autenticado; 404 si no tiene org |
| `/orgs` | POST | Crear org (paso 1 del wizard) |
| `/orgs/me` | PUT | Actualizar nombre/timezone/locale |
| `/orgs/me/policy` | GET | Policy activa de la org |
| `/orgs/me/policy` | PUT | Guardar/actualizar policy completa |
| `/orgs/me/mailboxes` | GET | Lista de mailboxes de la org |
| `/orgs/me/mailboxes` | POST | Conectar mailbox |

### Endpoints requeridos en P1

| Endpoint | Método | Descripción |
|---|---|---|
| `/orgs/me/policy/preview` | POST | Simular clasificación con una policy draft (para el paso 9) |
| `/orgs/me/schedule` | GET/PUT | Configuración del scheduler |
| `/orgs/me/reports` | GET/PUT | Configuración de reportes |
| `/orgs/me/retention` | GET/PUT | Política de retención |

### Cambios en endpoints existentes

| Endpoint | Cambio requerido |
|---|---|
| `POST /analysis-runs` | Debe aceptar `policy_version_id` o derivar policy de la org activa; ya no acepta `internal_domains`, `ignored_keywords`, etc. directamente como hace el `FilterBar` actual |
| `GET /analysis-runs` | Debe devolver `policy_snapshot` o referencia a versión de policy usada |

### Contrato mínimo para `OrgPolicy`

```json
{
  "org_id": "string",
  "version": 1,
  "timezone": "America/Santiago",
  "locale": "es",
  "mailbox": {
    "email": "soporte@tuempresa.com",
    "purpose": "support"
  },
  "internal_domains": ["tuempresa.com"],
  "email_types": {
    "client_requests": true,
    "automated": true,
    "newsletters": true,
    "internal": true
  },
  "ai": {
    "enabled": false,
    "consent_given_at": null
  },
  "schedule": {
    "enabled": false,
    "preset": "workdays",
    "hour": "08:00"
  },
  "reports": {
    "enabled": false,
    "recipients": [],
    "include_subjects": false
  },
  "retention_days": 30
}
```

**Invariante de UX:** la UI nunca debe enviar `ignored_keywords` como string cruda. Las reglas de exclusión se derivan server-side desde `email_types` y la configuración de política. El `FilterBar.tsx` actual es el único lugar que genera esa superficie — su eliminación es parte de este incremento.

---

## 7. Riesgos de promesas UX que requieren confirmación legal/producto

Estos ítems **no se pueden implementar sin confirmación del dueño del producto.** Se documentan aquí para que Pi los eleve.

### Riesgo UX-R01 — Preview de clasificación implica precisión que no existe

La pantalla de vista previa (paso 9) muestra cómo quedarían clasificados los hilos. Si el usuario interpreta esto como una garantía de exactitud, puede llevar a expectativas incorrectas sobre la calidad del análisis automático.

**Requiere:** texto de descargo claro en la preview. Sugerencia: *"Esta es una simulación basada en tu configuración. Los resultados reales pueden variar."*

### Riesgo UX-R02 — Microcopy de "subprocessor IA" requiere revisión legal

El texto propuesto en el paso 6 nombra "Amazon Bedrock (Claude)" como procesador. Este lenguaje tiene implicaciones en términos de DPA, GDPR y expectativas de privacidad. Ningún agente puede aprobar ese texto.

**Requiere:** revisión por el owner de producto o asesor legal antes de que ese microcopy sea público. Hasta entonces, usar lenguaje más genérico: *"procesado por un servicio de IA externo"*.

### Riesgo UX-R03 — "Puedes exportar tus datos" no está implementado

La sección de retención puede generar la expectativa implícita de que el usuario tiene derecho a exportar sus datos antes de que expiren. Esta funcionalidad no existe en el backend actual.

**Requiere:** no incluir en la UI de retención ningún botón o texto que sugiera exportación hasta que esté implementada.

### Riesgo UX-R04 — "Revoca acceso desde Google" redirige fuera de la app sin confirmación de estado

El microcopy actual en `LoginView` y `AyudaView` dice que el usuario puede revocar desde su cuenta Google. Si lo hace sin desconectar desde la app, el sistema queda en estado inconsistente (token revocado, mailbox en Firestore activo).

**Requiere:** agregar un flujo explícito de "Desconectar casilla" en la UI de ajustes que invalide el token por API antes de que el usuario vaya a Google.

---

## 8. BLOCKED_QUESTIONS

Solo tres preguntas genuinamente necesarias para implementar este plan. Todas las decisiones de D-011 ya están incorporadas.

---

### BQ-01 — Estrategia de migración para el usuario existente (owner actual)

La organización owner actual tiene datos en `scheduleConfigs/{email}`, `analysisRuns/{id}` y un `FilterBar.tsx` hardcodeado. Para beta, ¿cómo se migra?

- **Opción A: Migración silenciosa.** El sistema crea automáticamente una org y policy inicial derivada de los valores actuales del `FilterBar`. El usuario los revisa/ajusta la próxima vez que entra.
  - *Impacto:* El owner no pierde su historial. Riesgo: la policy migrada hereda los hardcodes de West Ingeniería; si se comparte la app, esos valores quedan expuestos como defaults.
- **Opción B: Onboarding obligatorio.** La próxima vez que el owner entra, se le muestra el wizard. Sus runs anteriores quedan en un estado "legacy" sin policy asociada.
  - *Impacto:* Limpio para SaaS, pero interrumpe el flujo actual del owner.
- **Opción C: Coexistencia.** El `FilterBar.tsx` sigue funcionando para el owner mientras no configure una org. La UI de wizard es solo para nuevos usuarios beta.
  - *Impacto:* Menor disrupción, pero mantiene el deuda técnica del hardcode más tiempo.
- **Recomendación:** Opción B con modo "borrador pre-relleno" — el wizard arranca con valores derivados del estado actual para que el owner solo confirme/ajuste, sin empezar desde cero.
- **Impacto si se decide mal:** Si se elige A sin limpiar hardcodes, los valores de West Ingeniería quedan como defaults de nuevas orgs beta. Si se elige B sin borrador, el owner pierde contexto de su configuración actual.

---

### BQ-02 — ¿El wizard es obligatorio o salteble para primeros usuarios beta?

Para la primera beta, ¿puede el usuario analizar sin completar el wizard?

- **Opción A: Obligatorio.** Sin policy configurada, el botón "Analizar" está deshabilitado con el mensaje del estado vacío propuesto.
  - *Impacto:* Garantiza que no hay runs sin policy; el sistema está limpio desde el primer día de beta.
- **Opción B: Opcional con fallback.** Se puede analizar con defaults seguros (dominio derivado del email, sin IA, retención 30 días). El wizard aparece como recomendación, no como barrera.
  - *Impacto:* Menor fricción en onboarding; riesgo de que usuarios beta no configuren nada y los datos queden sin política explícita.
- **Recomendación:** Opción A. Beta privada con usuarios invitados; la fricción del wizard es aceptable y garantiza datos limpios desde el inicio.
- **Impacto si se decide mal:** Con opción B en beta, es probable que algunos runs no tengan policy snapshot, lo que complica el versionado desde el principio.

---

### BQ-03 — Nivel de detalle del microcopy de IA hacia los invitados beta

El texto propuesto para el paso 6 nombra el provider (Amazon Bedrock). Para beta privada con usuarios de confianza, ¿qué nivel de detalle es apropiado?

- **Opción A: Genérico.** *"procesado por un modelo de lenguaje externo"* sin nombrar provider ni modelo.
  - *Impacto:* Menor exposición legal; puede generar desconfianza por opacidad.
- **Opción B: Provider nombrado.** *"Amazon Bedrock"* sin mencionar modelo específico.
  - *Impacto:* Transparente para usuarios técnicos en beta; requiere que el owner esté seguro de que el DPA con Anthropic/AWS cubre este uso.
- **Opción C: Provider + modelo.** *"Amazon Bedrock (Claude Haiku/Sonnet)"*.
  - *Impacto:* Máxima transparencia; requiere revisión legal antes de ser texto contractual.
- **Recomendación:** Opción B para beta privada. Los usuarios invitados merecen saber el provider; el modelo específico puede cambiar y no debería prometerse en la UI.
- **Impacto si se decide mal:** Si se usa texto genérico y más tarde se filtra que es Bedrock/Claude, puede percibirse como ocultamiento deliberado. Si se nombra el modelo y cambia, hay que actualizar UI y posiblemente reconfirmar consentimiento.

---

## Resumen de dependencias para implementación

```
UX puede diseñarse en paralelo con backend.
UX puede mockearse con datos estáticos para validar el wizard.
UX no puede completarse sin:
  1. GET /orgs/me y POST /orgs (para crear/detectar org)
  2. PUT /orgs/me/policy (para guardar wizard)
  3. Modificación de POST /analysis-runs para usar policy de org
     en lugar de recibir FilterBar payload directo
```

La eliminación de `FilterBar.tsx` como fuente de verdad de la policy **es el hito técnico que desbloquea la beta**. Hasta ese punto, el wizard puede convivir con el FilterBar: el wizard guarda la policy en backend, y el FilterBar se pre-rellena desde esa policy en lugar de desde hardcodes.
