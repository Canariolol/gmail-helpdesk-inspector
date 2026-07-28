# TASK-004 — Implementación Pi UI quick wins de confianza

Fecha: 2026-06-17

## Cambios realizados

### `apps/web/src/views/LoginView.tsx`

- Reemplazado CTA informal por CTA profesional: “Conectar con Google Gmail”.
- Agregado copy de valor orientado a confianza y métricas auditables.
- Agregado modal previo al OAuth que explica:
  - scope `gmail.readonly`;
  - no envío/edición/etiquetado/borrado de correos;
  - tokens cifrados y revocabilidad;
  - uso transitorio de cuerpos completos durante análisis/auditoría.

### `apps/web/src/components/common/PrivacyCallout.tsx`

- Nuevo componente reutilizable para explicar permisos, privacidad y trazabilidad.

### `apps/web/src/components/layout/Sidebar.tsx`

- Agregado `aria-current="page"` al ítem activo.
- Reemplazado avatar de logo por iniciales del usuario.
- Etiqueta visible “Gmail readonly”.
- Renombrado ítem de ayuda a “Ayuda y privacidad”.

### `apps/web/src/views/AyudaView.tsx`

- Nueva sección “Privacidad y permisos”.
- Reutiliza `PrivacyCallout`.
- Documenta no modificación de Gmail y no persistencia de cuerpos completos.

### CSS

- Estilos para login profesional, privacy callout, modal OAuth y avatar de usuario.

## Checks ejecutados

```bash
npm --prefix apps/web run build
```

Resultado: OK. Vite mantiene advertencia existente de chunk >500kB.

## Riesgos / handoff

- El modal previo OAuth es frontend-only y no cambia el endpoint `/auth/google/login`.
- Aún no hay pantalla real de disconnect/delete data; queda para task backend/frontend posterior.
- El `ReviewForm` aún envía `reviewer_label`, pero backend ya lo ignora y usa email autenticado. Puede limpiarse en una task pequeña.
