# TASK-021 — Onboarding copy

Fecha: 2026-06-17

## Implementado

- Landing pública posiciona la beta como Google Workspace-only.
- Modal OAuth ahora habla de Gmail Workspace y scope `gmail.readonly`.
- `ConfiguracionView` paso 1 aclara Workspace, solo lectura y zona horaria.
- Estado de configuración muestra `Casilla Workspace` + `Scope Gmail readonly`.
- Paso IA mantiene opt-in explícito y consentimiento separado.
- Paso retención se ajustó para no afirmar borrado automático inmediato; indica que se guarda fecha de expiración y que flujos destructivos se activarán controladamente durante beta.

## Archivos

- `apps/web/src/views/LoginView.tsx`
- `apps/web/src/views/ConfiguracionView.tsx`
