Tengo suficiente contexto. Ahora redacto el plan completo.

---

# Landing Pública — Gmail Helpdesk Inspector (Beta Privada)
## Diseño estratégico · Modo plan

---

## 1. Diagnóstico rápido del producto

| Atributo | Valor real |
|---|---|
| Categoría | Auditor de métricas, no gestor de tickets |
| Acceso Gmail | `readonly` — nunca modifica nada |
| IA | Opt-in explícito, no activa por defecto |
| Público objetivo | Jefe de operaciones / gerente de área de Workspace SMB |
| Etapa | Beta privada — acceso por solicitud |
| Diferenciador | Trazabilidad: cada métrica se ancla a los hilos reales |

---

## 2. Estructura de la landing (jerarquía de secciones)

```
[NAVBAR]          Logo · "Cómo funciona" · "Privacidad" · [Solicitar acceso]

[HERO]            Headline · Subheadline · CTA primario · Trust badge

[PROBLEMA]        Agitación del dolor: sin datos no hay gestión

[CÓMO FUNCIONA]   3 pasos: Conectar → Analizar → Reportar

[FEATURES]        4 tarjetas de capacidades concretas

[TRUST / PRIVACY] Bloque dedicado: Gmail readonly, sin modificaciones, IA opt-in

[SOCIAL PROOF]    Placeholder para testimonios de beta testers

[FAQ]             6 preguntas, collapse opcional

[CTA FINAL]       Formulario de solicitud de acceso beta

[FOOTER]          Política de privacidad · Contacto · Logo
```

---

## 3. Copy — sección por sección

### NAVBAR

```
Gmail Helpdesk Inspector                    [Cómo funciona]  [Privacidad]  [Solicitar acceso →]
```

---

### HERO

**Headline:**
> Tu casilla de soporte tiene historias que contar.
> Ahora puedes leerlas.

**Subheadline:**
> Gmail Helpdesk Inspector analiza tu bandeja de soporte y convierte hilos en métricas operativas reales: correos atendidos, tiempos de respuesta, casos sin resolver — sin tocar un solo mensaje.

**CTA primario:**
> Solicitar acceso a la beta →

**Trust badge** (debajo del CTA, pequeño):
> Acceso Google solo lectura · No modifica Gmail · IA opcional · Datos de tu organización, no nuestros

---

### EL PROBLEMA (sección breve, 2 párrafos)

**Título:** ¿Cuántas solicitudes reales llegaron este mes?

> Si no puedes responder esa pregunta con seguridad, tienes el mismo problema que la mayoría de equipos que usan Gmail como sistema de soporte: saben que están respondiendo correos, pero no cuántos, ni a cuántos les respondieron a tiempo, ni cuántos se perdieron en el ruido.

> Los spreadsheets manuales fallan. Los sistemas de tickets son overkill para equipos pequeños. Y Gmail no trae métricas de helpdesk de fábrica.

---

### CÓMO FUNCIONA (3 pasos)

**Título:** Tres pasos. Sin configuración de servidor.

```
[1] Conecta tu cuenta Workspace
    Autoriza acceso de solo lectura a Gmail vía Google OAuth.
    No pedimos más permisos de los necesarios.

[2] Configura los criterios de tu análisis
    Define el período, los dominios de clientes y los correos a excluir.
    Tú decides qué cuenta como solicitud válida.

[3] Genera el reporte
    El sistema clasifica hilos, calcula métricas y te muestra
    cada número con los correos que lo respaldan.
```

---

### FEATURES (4 tarjetas)

**Título:** Lo que mide. Lo que ignora.

```
[Métricas que importan]
Correos recibidos, válidos, respondidos, no respondidos y tiempo
de primera respuesta. Todo con trazabilidad al hilo original.

[Filtrado inteligente]
Excluye automáticamente newsletters, correos internos, notificaciones
de Google y misceláneos. Solo cuentan las solicitudes reales de clientes.

[Revisión manual de casos ambiguos]
Cuando la clasificación no es obvia, el sistema te lo dice. Tú revisas
y corriges. Las métricas reflejan lo que tú decides.

[IA como apoyo, no como árbitro]
La auditoría con IA es completamente opcional. Puedes obtener reportes
confiables sin activarla. Cuando la activas, ves exactamente qué revisó.
```

---

### TRUST / PRIVACY (bloque destacado, fondo diferente)

**Título:** Diseñado para que puedas explicarlo a tu cliente.

> **Solo lectura, siempre.**
> El único permiso que pedimos es `gmail.readonly`. No enviamos correos. No archivamos mensajes. No ponemos etiquetas. Si quieres verificarlo, el código de los scopes es público.

> **Tus datos no alimentan nuestros modelos.**
> Los cuerpos de correo se procesan temporalmente durante el análisis y se descartan. No los almacenamos. No los usamos para entrenamiento.

> **IA opt-in, no por defecto.**
> El análisis estándar usa solo reglas deterministas. La auditoría con IA se activa explícitamente y envía texto plano al worker — sin adjuntos, sin imágenes, sin metadata sensible.

> **Metadata, no contenido.**
> Lo que guardamos: IDs de hilos, fechas, remitentes, asuntos, clasificaciones y métricas agregadas. No el cuerpo completo de tus correos.

---

### SOCIAL PROOF (placeholder beta)

**Título:** Probado por equipos en beta privada

```
[Avatar]  "[Cita real de beta tester]"
          — Nombre · Cargo · Empresa (ciudad)

[Avatar]  "[Cita real de beta tester]"
          — Nombre · Cargo · Empresa (ciudad)
```

*Nota de implementación: dejar espacio para 2-3 testimonios reales. Mientras no haya, mostrar solo el CTA.*

---

### FAQ

**Título:** Preguntas que ya nos han hecho

```
¿Funciona con cualquier cuenta de Gmail?
No. Está diseñado exclusivamente para Google Workspace (cuentas empresariales).
Las cuentas @gmail.com personales no son compatibles.

¿Necesito instalar algo?
No. Es una aplicación web. Autorizas el acceso desde tu navegador y listo.

¿Qué pasa si revoco el acceso de Google?
La conexión se corta de inmediato. No mantenemos copias de tu correo
ni acceso persistente sin tu token activo.

¿La IA lee todos mis correos?
No. La auditoría con IA es opt-in y solo se activa si tú la solicitas
para un análisis específico. En modo estándar, la IA no interviene.

¿Cuánto tiempo tarda un análisis?
Depende del volumen. Un mes con ~500 hilos tarda entre 1 y 3 minutos.

¿Puedo exportar los reportes?
En beta: vista en pantalla y métricas consolidadas. Exportación a PDF/CSV
está en el roadmap para la versión pública.
```

---

### CTA FINAL

**Título:** Acceso beta limitado. Sin costo durante la beta.

> Estamos aceptando un número pequeño de organizaciones para la beta privada. Si tu equipo usa Gmail como canal de soporte y quieres métricas reales, solicita acceso.

**Formulario:**
```
[Nombre]  [Email corporativo]  [Nombre de empresa]
[¿Cuántas personas atienden la casilla de soporte?]  (select: 1-2 / 3-10 / +10)
[Cuéntanos en una línea qué problema quieres resolver]

                    [Solicitar acceso a la beta →]

Responderemos en 48 horas hábiles.
```

---

### FOOTER

```
Gmail Helpdesk Inspector · Beta privada

Política de privacidad    Términos de uso    Contacto: admin@orionsolutions.cl

No estamos afiliados a Google LLC.
Google Workspace y Gmail son marcas registradas de Google LLC.
```

---

## 4. Estética recomendada

Basada en tu perfil (naranja/crema, tono juguetón intencional, gerencial):

| Token | Valor |
|---|---|
| `--color-bg` | `oklch(98% 0.012 75)` — crema suave |
| `--color-surface` | `oklch(95% 0.018 70)` — crema más profundo |
| `--color-accent` | `oklch(68% 0.18 45)` — naranja cálido |
| `--color-text` | `oklch(20% 0.02 70)` — marrón casi negro |
| `--color-muted` | `oklch(55% 0.01 70)` |
| Tipografía | `Instrument Serif` (headlines) + `Inter` (cuerpo) |
| Bordes | `4px` radius en tarjetas, `8px` en botones |
| Sombras | Sombras cálidas con tint naranja, no grises neutros |

Dirección visual: **Swiss editorial con calidez** — no un SaaS genérico azul.

---

## 5. Implementación recomendada

### Opción A — Ruta en la app existente (recomendada para beta)

Añadir una ruta `/` pública en el app React/Vite actual, separada de la app autenticada. Ventajas:
- Un solo repo, un solo deploy
- Reutiliza el design system que ya existe
- No requiere infraestructura adicional

Implementación mínima:
```
apps/web/src/views/LandingView.tsx     ← componente de landing
apps/web/src/views/SolicitudView.tsx   ← formulario de beta (guarda en tu DB o Formspree)
```
En el router: rutas `/` y `/solicitar` no requieren autenticación.

### Opción B — Sitio estático separado (para producción pública futura)

Astro o Next.js estático en un subdominio (`about.tudominio.com`). Mejor para SEO y performance a futuro, pero agrega complejidad operacional ahora. **No recomendado para la beta.**

### Formulario de acceso

Para la beta: **Formspree** o una ruta interna `POST /api/beta-request` que guarda en SQLite y te manda un email. Evita montar un CRM complejo por ahora.

---

## 6. ¿Amerita Opus?

**No para esta tarea.** Sonnet 4.6 es el modelo correcto aquí.

Opus se justifica cuando necesitas:
- Razonamiento multi-capa sobre trade-offs ambiguos (arquitectura de sistema compleja, decisiones de producto con señales contradictorias)
- Análisis exhaustivo de competidores con muchas dimensiones
- Generación de copy que requiere muchos ciclos de critique y refinamiento estratégico

Esta landing es un ejercicio de síntesis de información conocida + aplicación de principios de UX/copywriting B2B. Sonnet lo ejecuta bien y es 5-8x más barato por token.

**Usa Opus si** más adelante necesitas: analizar transcripts de entrevistas de usuarios, diseñar la estrategia de pricing, o hacer análisis competitivo profundo contra alternativas como Zendesk Lite o Freshdesk.

---

## 7. Lista de criterios antes de publicar

- [ ] Formulario de beta guarda solicitudes sin perder ninguna (test end-to-end)
- [ ] Trust block revisado por alguien que no conoce el producto (¿es creíble sin contexto?)
- [ ] FAQ responde las 3 objeciones reales más comunes (readonly, IA, datos)
- [ ] Disclaimer de marca Google en footer
- [ ] No hay emails hardcodeados visibles (usa `mailto:` o formulario)
- [ ] Mobile-first: el hero funciona en 375px
- [ ] Analytics básicos (Plausible o similar, privacy-first) antes de abrir tráfico
