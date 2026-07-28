# TASK-020 — Landing/waitlist beta privada

Fecha: 2026-06-17

## Implementado

La vista pública no autenticada (`LoginView`) ahora funciona como landing de beta privada:

- navbar pública;
- hero con propuesta de valor;
- CTA waitlist + CTA para usuarios con acceso;
- badges de trust: Gmail readonly, no modifica correos, IA opt-in, reportes metrics-only;
- problema / cómo funciona / features;
- bloque trust & privacy;
- FAQ;
- formulario waitlist vía `mailto:` sin backend nuevo;
- modal OAuth existente actualizado para Workspace + readonly.

## Archivos

- `apps/web/src/views/LoginView.tsx`
- `apps/web/src/styles/components.css`

## Decisión técnica

Se implementó en la misma app Vite para minimizar despliegue y mantener un solo flujo OAuth durante beta privada. Una app marketing separada queda para fase pública/pricing.
