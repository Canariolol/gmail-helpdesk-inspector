# Plan de refactor visual → Cadencia

Referencias: mockup `design/propuestas-rediseno.html` (pestaña Cadencia) y
contrato `design/TOKENS.md`. Auditoría hecha el 2026-07-18 navegando todas las
vistas con el usuario de prueba.

## Diagnóstico: por qué se ve "igual que antes"

Lo implementado hasta ahora fue solo la **capa de tokens**: paletas y carga de
fuentes. El layout y los componentes siguen siendo los del diseño anterior:

| Cadencia (mockup) | App actual |
|---|---|
| Sidebar 212px, fondo `--side-bg` oscuro, íconos + etiquetas, ítem activo con pulso | Sidebar ~75px claro, solo íconos, activo con fondo pastel |
| Casi sin cards: bloques abiertos sobre `--bg`, separados por filetes | Todo vive dentro de cards blancas redondeadas con sombra |
| Títulos y cifras en **Sora** (`--font-display`), tabular | Títulos y cifras en Inter (Sora está cargada pero casi no se usa) |
| Etiquetas de datos en **IBM Plex Mono** mayúsculas con tracking | Etiquetas en Inter normal |
| KPIs como cifras abiertas con separadores verticales y "pulso" en la métrica destacada | KPIs como cards `MetricCard` |
| Botones sólidos radius 10px; pills de filtro compactas con fecha mono | Botones pill con gradiente; pills grandes |
| Un solo acento semántico + chips outline/translúcidos | Chips pastel rellenos, acento decorativo |

Por eso la percepción correcta del usuario: "solo cambió la paleta y la fuente".

## Fase 0 · Fundamentos globales (transforma todo el panel de una vez)

Archivos: `layout.css`, `components.css`, `base.css`, `Sidebar.tsx`.

1. **Sidebar** (`Sidebar.tsx` + `layout.css`)
   - 212px, fondo `--side-bg`, marca en Sora con avatar.
   - Ítems: ícono 16px + etiqueta 13px, color `--side-muted`; hover `--side-ink`.
   - Activo: color `--side-on` + barra "pulso" de 3px con `--pulse-grad` al borde izquierdo (sin fondo de caja).
   - Badge de revisión pendiente en mono.
   - Bloque usuario abajo con `border-top: --side-line`, email en 12.5px.
2. **Tipografía**
   - `h1–h3`, títulos de vista y **todas las cifras**: `--font-display` con `font-variant-numeric: tabular-nums`.
   - Etiquetas de métricas, subtítulos de bloque, metadatos (fechas, contadores): `--font-mono`, mayúsculas, `letter-spacing: .08em`, 10.5–11px, `--text-muted`.
   - Cuerpo sigue en `--font-sans`.
3. **De-cardificación**
   - `.card` pasa a bloque abierto: sin fondo, sin sombra, sin borde-caja; separación entre bloques por aire (32–40px) y filete `--border` de 1px cuando ayude.
   - `--surface` queda reservado a controles (inputs, selects, pills, dropdowns) y a superficies flotantes (modales, popovers, toasts).
   - Excepciones deliberadas que SÍ mantienen superficie: previsualización de reporte (es un "documento") y modales.
4. **Botones**: primario sólido `--accent`/`--accent-ink` radius 10px (hover `--accent-strong`); ghost con borde `--border` y texto `--text`; destructivo ghost con `--danger`. Eliminar gradientes en botones.
5. **Banners** (`setup-banner`, `action-error-banner`, avisos): bloque abierto con filete izquierdo de 3px (`--warn`/`--danger`/`--accent`) y fondo `--accent-soft`-equivalente, sin card completa.
6. **Barra de filtros** (`filters.css`): pills compactas `--surface` radius 10px, fechas y horas en `--font-mono`; botón "✦ Analizar" sólido.
7. **Empty states** (`EmptyState.tsx`): sin card gigante; ícono + mensaje + CTA directamente sobre `--bg`, centrado, con un pulso decorativo corto.

## Fase 1 · Vistas de trabajo diario

### Resumen (`ResumenView`, `MetricCard`, `charts/`)
- KPI strip abierta: flex con separadores verticales `--border`; por KPI: etiqueta mono arriba, cifra Sora 28–30px; métrica destacada (`Válidas`) en `--accent` con pulso de 44px debajo y delta en mono `--ok`.
- Gráficos sin card: título Sora 14px + subtítulo mono; barras redondeadas 5px arriba apoyadas en línea base `--border`; barras en `--bar-rest`, pico/actual con gradiente `--accent-2→--accent` y valor directo encima en mono. `ClassificationBarChart.tsx`: reemplazar colores hardcodeados por tokens.
- Lista de hilos recientes: filas abiertas con filete inferior, asunto 600, metadata mono, chip semántico a la derecha.

### Hilos (`HilosView`, `components/threads/`)
- Maestro-detalle: lista de filas abiertas (hover `--surface-soft`), seleccionada con barra pulso izquierda.
- Detalle: timeline de mensajes con conectores de 1px `--border`, horas en mono; razones de clasificación como bloque con filete izquierdo `--info`.

### Revisión manual
- Mismo lenguaje de filas; contador de pendientes en mono en el encabezado.
- Acciones de corrección inline (chips clicables outline); al guardar, feedback con `--ok` sin modal.

## Fase 2 · Configuración (rediseño estructural — la vista crítica)

Problemas actuales: tres cards de estado apiladas arriba (estado, análisis
programado, últimos eventos) sin jerarquía con la edición; stepper wizard de 6
pasos horizontal que no comunica dónde estás; card gigante con textareas
enormes; mezcla de estado operativo con edición de política.

Propuesta (`ConfiguracionView.tsx` — único cambio estructural TSX grande):

1. **Layout de dos columnas**:
   - Izquierda (sticky, ~200px): sub-navegación de secciones — Organización,
     Equipo, Qué cuenta, Auditoría IA, Programación, Retención — cada una con
     su estado en mono (`✓` configurada / `—` pendiente). Reemplaza al stepper.
   - Derecha: las secciones como bloques abiertos uno bajo otro (scroll), cada
     una con título Sora + descripción corta + sus campos. Scroll-spy marca la
     sección visible en la sub-nav.
2. **Cabecera compacta**: título "Configuración" + chip de estado global
   (`Configuración pendiente` / `Al día`) + cuenta Gmail en mono. Nada más.
3. **Reubicación**: "Análisis programado" y "Últimos eventos" dejan de flotar
   arriba y viven dentro de la sección **Programación** (donde pertenecen).
4. **Campos**: labels en mono; textareas con altura contenida (4–5 líneas,
   auto-grow) y helper text `--text-muted`; requerido con `--accent`, no rojo.
5. **Primera vez / modo guiado**: mismo layout; las secciones no completadas
   muestran CTA "Continuar con [siguiente]" al pie; se elimina el paradigma
   wizard de pasos bloqueados.
6. **Guardado**: barra de acciones sticky al pie de la columna derecha
   (Guardar borrador / Publicar política) visible solo con cambios sin guardar.

## Fase 3 · Vistas restantes

- **Análisis (RunsView)**: tabla abierta de ejecuciones — columnas fecha ISO,
  rango, hilos, duración en mono; estado con `RunStatusChip` tokenizado; fila
  hover `--surface-soft`; sin cards.
- **Reportes**: formulario de generación como bloque abierto (labels mono);
  la previsualización del reporte mantiene superficie blanca (documento).
- **Plan y cuenta**: quitar la card exterior; bloques abiertos con títulos mono
  en mayúsculas (CUENTA / PLAN / GMAIL / APARIENCIA / SESIONES) separados por
  filetes; acciones destructivas en ghost `--danger`.
- **Privacidad y datos**: ya es una ficha de definición — quitar cajas: pares
  label (mono, izquierda) / valor (derecha) con filetes suaves; chips de estado
  se mantienen.
- **Ayuda**: editorial — títulos Sora, listas abiertas, la leyenda de
  clasificaciones usa los chips reales; numerales mono para los pasos.

## Fase 4 · Landing

Plan ya propuesto en conversación (hero Cadencia con 91% + sparkline animado,
atmósfera glow+grano tokenizada, mocks enmarcados con tilt, mascota optimizada
WebP, numerales mono gigantes, franja de confianza en `--side-bg`). Se ejecuta
después del panel para reusar los componentes ya refinados.

## Método de trabajo

- Una fase por vez; al final de cada fase, verificación visual con navegador
  en Arena + un tema oscuro (Vino) antes de continuar.
- `components.css` (2 097 líneas) se toca por secciones; los estilos nuevos
  solo consumen tokens del contrato.
- Sin cambios de lógica/datos salvo `Sidebar.tsx` (etiquetas) y
  `ConfiguracionView.tsx` (reestructuración de secciones).
