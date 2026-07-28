# TASK-022 — Error states / 429 UX

Fecha: 2026-06-17

## Implementado

- `api/client.ts` ahora devuelve un mensaje 429 más específico para límites de análisis manuales.
- `App.tsx` resetea errores de create/start al reintentar.
- `ResumenView` recibe `analysisError` y muestra `action-error-banner`.
- Si el error contiene `Demasiadas`, el banner explica que la cuota protege la beta y que el scheduler no consume cuota manual.

## Archivos

- `apps/web/src/api/client.ts`
- `apps/web/src/App.tsx`
- `apps/web/src/views/ResumenView.tsx`
- `apps/web/src/styles/components.css`
