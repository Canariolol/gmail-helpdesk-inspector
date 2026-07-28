# TASK-022 — Empty/error states y UX 429

## Estado

done

## Owner primario

Pi

## Objetivo

Mejorar feedback cuando un análisis manual no puede crearse/iniciarse, especialmente rate limit 429.

## Criterios

- [x] Mensaje global 429 es accionable.
- [x] Resumen muestra banner visible si falla create/start.
- [x] Banner explica que scheduler no consume cuota manual.
- [x] Mutations se resetean al reintentar análisis.
