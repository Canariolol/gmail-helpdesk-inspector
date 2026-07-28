# TASK-002 — Brief UX: beta privada hacia SaaS público Gmail Helpdesk Inspector

**Autor:** Claude (UI/UX) | **Fecha:** 2026-06-17 | **Modelo:** Sonnet, esfuerzo máximo  
**Basado en:** README.md, specs_prd.md, roles.md, path-ownership.md, TASK-001/codex-plan.md, código fuente en `apps/web/src/`

---

## 1. Posicionamiento del producto en una frase

> **Gmail Helpdesk Inspector** convierte tu casilla de Gmail en un panel de métricas auditables de helpdesk: sin convertirte en sistema de tickets, sin modificar tus correos, con acceso solo lectura verificable.

---

## 2. Usuario objetivo y jobs-to-be-done

**Perfil primario:** responsable de soporte o gerente de operaciones de empresa 10–200 personas que usa Gmail como único canal de soporte, necesita justificar métricas ante dirección, y tiene dudas sobre qué permisos implica conectar su Gmail a una herramienta externa.

| Cuándo... | Quiero... | Para... |
|---|---|---|
| Tengo que presentar métricas el lunes | Generar un reporte rápido | No trabajar el fin de semana con Excel |
| Un cliente me reclama que no respondimos | Encontrar ese hilo con timestamp | Tener evidencia verificable |
| Sospecho correos sin responder | Ver cuántos quedan y cuáles son | Priorizarlos hoy |
| El sistema clasifica algo mal | Corregirlo manualmente | Que las métricas sean mías, no de la IA |
| Alguien pregunta "¿qué hace con mis correos?" | Mostrar claramente los permisos | Mantener la confianza del equipo |

---

## 3. Flujo ideal

```
[Landing/Login]
     ↓
[OAuthExplainModal: explicar gmail.readonly ANTES del popup]
     ↓
[Configuración] → Fechas · Dominios · Exclusiones · ConsentToggle IA
     ↓
[Análisis en curso] → Progreso + "Tus correos no se modifican"
     ↓
[Dashboard / Resumen]
     ├── [Hilos] → tabla filtrable + panel de detalle lateral
     ├── [Revisión manual] → cola ambiguos + formulario
     ├── [Reportes] → consolidado + tendencias
     └── [Privacidad y datos] → Account panel + desconectar + borrar
```

**Primer uso:** wizard de 3 pasos (Fechas → Dominios → Exclusiones + IA).  
**Usuarios recurrentes:** FilterBar compacto, pero con defaults desde config guardada del usuario (actualmente hardcodeada — blocker para SaaS).

---

## 4. Mapa de pantallas y componentes

### Pantallas — estado y prioridad

| Pantalla | Estado actual | Prioridad |
|---|---|---|
| `LoginView` | Existe — mínima, CTA casual | **Alta** |
| `FilterBar` | Existe — defaults hardcodeados | **Alta** |
| Pantalla de autorización OAuth | No existe | **Alta** (nueva) |
| Onboarding wizard | No existe | Media (nueva) |
| `ResumenView` | Existe — funcional | Media |
| `HilosView` + `ManualReview` | Existe — parcial | Media |
| Privacidad / Cuenta | No existe | **Alta** (nueva) |
| Banner sesión expirada | No existe | **Alta** (nueva) |
| `ReportesView`, `RunsView`, `AyudaView` | Existen | Baja |

### Componentes nuevos necesarios

| Componente | Propósito |
|---|---|
| `PrivacyCallout` | Banner de privacidad reutilizable (Login, FilterBar, Onboarding) |
| `OAuthExplainModal` | Modal previo al popup OAuth con explicación del scope |
| `ConsentToggle` | Toggle IA con texto de consentimiento inline |
| `ConfidenceBanner` | Score de confianza prominente con barra de color y contexto |
| `AccountPanel` | Datos almacenados + desconectar Gmail + borrar cuenta |
| `SessionExpiredBanner` | Banner global cuando el refresh token caduca |
| `ReviewDeltaToast` | Feedback de impacto en métricas al guardar revisión |

### Componentes existentes a mejorar

| Componente | Mejora |
|---|---|
| `LoginView` | Copy de valor + bullets de privacidad + CTA formal |
| `FilterBar` | Defaults desde config usuario; `ConsentToggle` para IA |
| `StatusBanner` | Ocultar tokens IA por default; agregar mensaje de privacidad |
| `ReviewForm` | `reviewer_label` desde email autenticado, no "local-user" |
| `ThreadDetailPanel` | Badge de fuente de clasificación + indicador de mensajes determinantes |
| `Sidebar` | `aria-current` en nav activo; avatar con inicial; item "Privacidad y datos" |
| `AyudaView` | Sección de privacidad y datos almacenados |

---

## 5. Estados vacíos / loading / error / success

### Vacíos
- **Resumen sin análisis:** icono BarChart, "Tu primera auditoría de helpdesk", CTA "Crear mi primer análisis", bullets de privacidad debajo en gris.
- **Revisión sin ambiguos:** CheckCircle verde, "Sin hilos pendientes — el sistema clasificó todo con confianza suficiente."

### Loading
- Primer fetch de runs → skeleton de MetricCards.
- Análisis en curso → `StatusBanner` con mensaje dinámico por fase.
- Cargando detalle de hilo → skeleton de timeline.

### Error

| Error | Mensaje |
|---|---|
| Sesión expirada | Banner sticky: "Tu sesión expiró. [Reconectar]" |
| Refresh token caducado (7d Testing OAuth) | "Tu acceso a Gmail caducó. Necesitas reconectar." |
| `status: failed` | StatusBanner rojo existente — mantener y mejorar copy |
| Error al guardar revisión | Toast: "No se pudo guardar. Inténtalo de nuevo." |
| Error de red | Toast: "Sin conexión. Verificando..." |

### Success
- Análisis completado → StatusBanner verde, métricas aparecen con transición suave.
- Revisión guardada → Toast: "Revisión guardada. Métricas actualizadas."

---

## 6. Microcopy crítico

### Frase maestra (LoginView y onboarding)
> "Gmail Helpdesk Inspector accede a tu casilla con permiso de **solo lectura** (`gmail.readonly`). Nunca envía, modifica, etiqueta ni elimina correos."

### Bullets de confianza
- Solo lectura — nunca escribe ni modifica nada en tu Gmail.
- Sin retención de cuerpos — guardamos metadatos, asuntos y fragmentos, no el texto completo.
- Revocable — puedes desconectar desde tu cuenta o desde Google en cualquier momento.
- Auditable — cada métrica muestra los hilos que la originaron.

### Pantalla previa al popup OAuth
```
Antes de continuar
Vas a autorizar acceso de solo lectura a tu Gmail.
• Scope: gmail.readonly (solo lectura)
• No puede enviar, editar, etiquetar ni borrar correos
• Tus tokens se almacenan cifrados
• Puedes revocar en Google en cualquier momento

[Ver política de privacidad]     [Continuar con Google →]
```

### Toggle de IA (FilterBar)
```
[checkbox] Auditoría con IA
  Fragmentos de asuntos y metadatos de hilos ambiguos se envían
  a Claude (Amazon Bedrock) para mejorar la clasificación.
  No se envían cuerpos completos de correos.
  [¿Qué envía exactamente?] → modal de detalle
```

### ReviewForm — encabezado
> "Tu corrección reemplaza la decisión automática y actualiza las métricas del reporte."

### Campo "Dominios internos"
> Dominio(s) de tu empresa (ej: empresa.cl). Los correos entre estas direcciones se excluyen del conteo.

---

## 7. Recomendaciones visuales para dashboard auditable

El diseño actual (crema/naranja, Nunito, tarjetas redondeadas) es correcto. Las recomendaciones son de **legibilidad de datos** y **señal de auditoría**, no de identidad visual.

**Jerarquía de métricas:** La `Confianza` actualmente aparece al final. Debe ser el primer elemento que el usuario vea — es la garantía del reporte. Propuesta: `ConfidenceBanner` prominente arriba, con porcentaje en `--fs-hero`, barra de color (verde/ámbar/rojo), y link "Ver hilos ambiguos →".

**Trazabilidad en MetricCards:** Agregar `cursor: pointer`, hover con box-shadow elevado e icono "→" en esquina inferior derecha. Tooltip: "Ver los X hilos que componen esta métrica". La affordance clickeable actualmente no es obvia.

**Timeline de mensajes:** Diferenciar visualmente mensajes externos (izquierda, fondo claro) de internos (derecha, fondo naranja suave). Marcar explícitamente el "primer mensaje cliente" y "primera respuesta interna" — son los puntos que determinan el tiempo de respuesta.

**Fuente de clasificación:** Badge visible junto al asunto del hilo: `rules | heuristics | ai | manual`. Si hay override manual, mostrar "Revisado manualmente" con icono lápiz.

**Tokens IA en StatusBanner:** Ocultar por default. Mostrar en su lugar: "IA auditó X hilos ambiguos." Tokens disponibles en modo avanzado.

---

## 8. Mejoras concretas sobre la UI existente

### 8.1 `LoginView` — CRÍTICO (solo frontend, sin bloqueante)
El CTA "Conéctate, preciosura =*" es demasiado informal para una decisión de acceso a Gmail corporativo. Propuesta: copy de valor + bullets de privacidad + CTA "Conectar con Google Gmail" formal.

### 8.2 `FilterBar` — defaults hardcodeados (bloqueante: DEP-01)
`domains = "@west-ingenieria.cl"` y fechas hardcodeadas en estado del componente. Blocker para multi-usuario. Cargar desde `scheduleConfigs/{email}` en Firestore. Fechas default = última semana dinámica.

### 8.3 `ReviewForm` — reviewer_label (bloqueante: DEP-05)
`reviewer_label: "local-user"` hardcodeado contamina la trazabilidad de auditoría. Pasar `reviewerEmail` como prop desde `me.data?.email` en `App.tsx`.

### 8.4 Pantalla `AccountPanel` — faltante (bloqueante: DEP-02, DEP-03)
Nueva vista en Sidebar "Privacidad y datos": datos almacenados, scope activo, link a myaccount.google.com, botones "Desconectar Gmail" y "Borrar todos mis datos" con confirmación modal.

### 8.5 Onboarding — primer uso
`EmptyState` actual con "Crea tu primer análisis" es insuficiente. Para primer uso: FilterBar expandida con explicaciones por campo, título "Configura tu primer análisis", recordatorio de privacidad debajo del CTA.

### 8.6 `SessionExpiredBanner` — faltante
Cuando el refresh token expira (7 días en OAuth Testing), la app regresa a login sin explicación. Banner global sticky con "Tu sesión expiró. [Reconectar]" sin perder contexto de vista.

### 8.7 Avatar en `Sidebar`
Reemplazar logo del producto como avatar por inicial del email en círculo de color. Link a "Privacidad y datos".

---

## 9. Accesibilidad y responsive

### Hallazgos en código actual

| Elemento | Hallazgo | Corrección |
|---|---|---|
| `Sidebar` nav buttons | Sin `aria-current` en item activo | `aria-current={view === item.view ? "page" : undefined}` |
| `StatusBanner` progress bar | Sin `aria-label` | `aria-label="Progreso del análisis"` |
| `FilterBar` inputs | `<span>` label sin asociación formal | `<label htmlFor>` nativo |
| HTML lang | Verificar `<html lang="es">` en index.html | Confirmar o agregar |
| MetricCards clickeables | Si son `<div onClick>`, inaccesibles por teclado | Cambiar a `<button>` |

### Responsive — breakpoints propuestos

| Breakpoint | Layout |
|---|---|
| ≥1200px | Sidebar + split-view (actual) |
| 768–1199px | Sidebar colapsado a íconos; split-view en columna única |
| <768px | Sidebar como drawer; detalle de hilo = pantalla completa con "← Volver" |

Componentes críticos en mobile: `FilterBar` colapsable, `MetricCard` en grilla 2 columnas, `ThreadDetailPanel` pantalla completa.

---

## 10. Dependencias y preguntas para Codex / Pi

### Bloqueantes para Claude

| ID | Descripción |
|---|---|
| DEP-01 | Endpoint/Firestore para leer y guardar config del usuario (dominios, exclusiones) |
| DEP-02 | `POST /auth/disconnect` — revocar OAuth y limpiar sesión |
| DEP-03 | `DELETE /users/me` — borrar todos los datos del usuario |
| DEP-04 | `GET /auth/me` incluya nombre o foto de Google (para avatar) |
| DEP-05 | `PATCH /threads/:id/manual-review` valide ownership (GHMI-SEC-001) |
| DEP-06 | `reviewer_label` debe venir del backend o del email autenticado |

### Preguntas de producto para Pi

| ID | Pregunta |
|---|---|
| PROD-01 | ¿Beta privada con autoregistro o solo por invitación? |
| PROD-02 | ¿Quién aprueba los textos legales de privacidad? |
| PROD-03 | ¿El modo sin IA es MVP o la IA es siempre activa? |
| PROD-04 | ¿El scheduler diario tendrá UI de configuración? |
| PROD-05 | ¿Google personal o solo Workspace? |
| PROD-06 | ¿Retención de análisis: indefinida o con lifecycle? |

---

## 11. Escalar a Opus — recomendación

**No escalar en esta pasada.** Los gaps más críticos son de backend (DEP-01 a DEP-06) y deben resolverlos Codex/Pi primero. El brief actual es suficiente para convertirse en tareas de implementación. Escalar a Opus cuando se requiera: sistema de diseño completo, flujo de billing, o re-análisis profundo por brechas detectadas por Pi.

---

## Handoff para Pi / Codex

### Cambios que Claude puede implementar ahora (sin bloqueantes)
1. `LoginView` — copy + privacidad + CTA formal.
2. `Sidebar` — `aria-current` + avatar + item "Privacidad y datos".
3. `StatusBanner` — tokens ocultos + mensaje de privacidad.
4. `ThreadDetailPanel` — badge de fuente de clasificación.
5. `AyudaView` — sección de privacidad y datos.
6. Nuevos: `PrivacyCallout`, `OAuthExplainModal`, `ConfidenceBanner`, `SessionExpiredBanner`.
7. CSS responsive — breakpoints mobile/tablet.

### Cambios que requieren backend (Codex primero)
8. `FilterBar` sin hardcodes + `ConsentToggle` → **espera DEP-01**.
9. `ReviewForm` con reviewer real → **espera DEP-05**.
10. `AccountPanel` (nueva vista) → **espera DEP-02 + DEP-03**.

### Riesgos

| Riesgo | Nivel |
|---|---|
| FilterBar con defaults hardcodeados llega a SaaS público | Alto |
| `reviewer_label: "local-user"` contamina auditoría multiusuario | Alto |
| Refresh token expira a 7 días (OAuth Testing) sin aviso al usuario | Alto |
| LoginView casual genera desconfianza en acceso Gmail corporativo | Medio |
| Sin AccountPanel no hay forma de desconectar desde la app | Medio |

### Secuencia sugerida
```
1. (Claude, ya) LoginView + PrivacyCallout + OAuthExplainModal
2. (Claude, ya) Sidebar + SessionExpiredBanner + AyudaView
3. (Codex)      DEP-01: config usuario en Firestore
4. (Claude)     FilterBar sin hardcodes + ConsentToggle
5. (Codex)      DEP-02 + DEP-03: disconnect + delete
6. (Claude)     AccountPanel
7. (Codex)      DEP-05: authz en PATCH manual-review
8. (Claude)     ReviewForm con reviewer real
9. (Claude)     Responsive CSS + auditoría a11y
```

