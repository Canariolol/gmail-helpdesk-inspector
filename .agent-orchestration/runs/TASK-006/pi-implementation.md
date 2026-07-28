# TASK-006 — Remover reviewer_label desde frontend

Fecha: 2026-06-17

## Cambio

- `apps/web/src/components/threads/ReviewForm.tsx` ya no envía `reviewer_label: "local-user"`.
- El backend controla el reviewer con `session.google_account_email` desde TASK-003.

## Check

```bash
npm --prefix apps/web run build
```

Resultado: OK. Vite mantiene advertencia existente de chunk >500kB.
