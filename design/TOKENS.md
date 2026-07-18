# Sistema de diseño "Cadencia" — contrato de tokens

Fuente de verdad: `apps/web/src/styles/tokens.css`. Mockups de referencia:
`design/propuestas-rediseno.html` (pestaña Cadencia).

## El contrato

1. Los componentes consumen **solo variables del contrato**; nunca colores,
   fuentes ni sombras literales.
2. Todo tema define el set completo de tokens de color; lo que no redefine
   hereda de `:root` (Arena).
3. Los chips y estados (`--ok`, `--warn`, `--info`, `--danger`) son
   **semánticos**: comunican clasificación o estado, no decoración.
4. Radios, espaciado y tipografías no se tematizan.
5. `--primary*` existe solo por compatibilidad con CSS anterior; los
   componentes nuevos usan `--accent*`.

## Temas

Se activan con `data-theme` en `<html>` (sin atributo = Arena).

| Tema    | Modo   | Fondo     | Acento    | data-theme |
|---------|--------|-----------|-----------|------------|
| Arena   | claro  | `#F7F3EC` | `#F2670C` | *(default)* |
| Niebla  | claro  | `#F4F6F5` | `#2F7D6A` | `niebla`   |
| Grafito | oscuro | `#1A1816` | `#F98A3C` | `grafito`  |
| Musgo   | oscuro | `#141A15` | `#5DB87A` | `musgo`    |
| Vino    | oscuro | `#20131A` | `#E8505B` | `vino`     |

## Grupos de tokens

- **Superficies**: `--bg`, `--surface`, `--surface-soft`, `--border`, `--border-strong`
- **Texto**: `--text`, `--text-strong`, `--text-muted`
- **Acento**: `--accent`, `--accent-2`, `--accent-ink` (texto sobre acento),
  `--accent-strong` (hover), `--accent-soft` (fondos tenues),
  `--pulse-grad` (la firma del diseño: subrayado de gradiente del dato protagonista)
- **Sidebar**: `--side-bg`, `--side-ink`, `--side-muted`, `--side-line`,
  `--side-on` (ítem activo). El sidebar siempre contrasta con `--bg`.
- **Estado**: `--ok`, `--warn`, `--info`, `--danger`
- **Datos**: `--bar-rest` (marcas en reposo de gráficos; la marca destacada usa el acento)
- **Chips**: `--chip-<color>-bg` / `--chip-<color>-fg`
- **Tipografía**: `--font-sans` (Inter, texto), `--font-display` (Sora, títulos
  y cifras), `--font-mono` (IBM Plex Mono, etiquetas de datos y metadatos)

## Lenguaje visual Cadencia (guía para el refinamiento)

- Casi sin cards: los bloques viven sobre `--bg` separados por aire y
  filetes `--border`; `--surface` se reserva para controles (pills, inputs).
- Etiquetas de datos en `--font-mono`, mayúsculas, tracking amplio; cifras en
  `--font-display` con `font-variant-numeric: tabular-nums`.
- El "pulso" (`--pulse-grad`, barrita redondeada) subraya solo el dato
  protagonista: un pulso por vista.
- Radios suaves (`--radius-sm/md`) en controles; sin bordes duros de grilla.
