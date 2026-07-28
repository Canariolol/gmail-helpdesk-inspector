# TASK-007B — Implementación UI/UX configuración guiada

Modo: implementación frontend. No se tocó backend, `.env`, secretos ni archivos fuera del scope.

## Estado

Completado. `npm --prefix apps/web run build` pasa sin errores.

## Resumen

Se implementó una UI de configuración guiada incremental basada en el contrato planificado de TASK-007:

- **Vista `ConfiguracionView`**: wizard de 6 pasos (Organización, Equipo, Qué cuenta, Auditoría IA, Programación, Retención).
- **Banner de setup en Resumen**: alerta visible si `setup_state.ready_for_analysis === false`.
- **FilterBar adaptativo**: modo simplificado cuando hay org config disponible; modo legacy cuando el backend no responde.
- **Sidebar**: ítem "Configuración" con ícono Settings.
- **Degradación segura**: si `GET /me/org/config` falla, la app usa legacy FilterBar sin romper login ni análisis.

## Archivos tocados

| Archivo | Tipo de cambio |
|---|---|
| `apps/web/src/api/types.ts` | Agregados: `OrgConfig`, `PolicyDraft`, `SetupState`, tipos anidados, `PutConfigResponse` |
| `apps/web/src/views/ConfiguracionView.tsx` | **Nuevo** — wizard de 6 pasos con stepper, draft local, PUT /me/org/config |
| `apps/web/src/components/layout/Sidebar.tsx` | Agregado `"configuracion"` a `AppView` y nav item Settings |
| `apps/web/src/App.tsx` | `useQuery` para `org-config`, renderiza `ConfiguracionView`, pasa `orgConfig` a ResumenView |
| `apps/web/src/views/ResumenView.tsx` | Props `orgConfig` y `onGoToSetup`; banner de setup pendiente |
| `apps/web/src/components/filters/FilterBar.tsx` | Props `orgConfig` y `onGoToSetup`; modo policy (simplified) vs legacy |
| `apps/web/src/styles/components.css` | Estilos wizard, setup-banner, filter-bar indicators, spin animation |

## Contrato API consumido

- `GET /me/org/config` → `OrgConfig` (provisionada implícitamente por el backend si no existe)
- `PUT /me/org/config` → `PutConfigResponse` (guarda draft y crea PolicyVersion si el hash cambia)
- `POST /analysis-runs` → payload simplificado `{ date_from, date_to, time_from, time_to, policy_version_id? }` cuando hay orgConfig

## Checks realizados

- [x] `npm --prefix apps/web run build` — pasa, 0 errores TypeScript, 0 errores Vite
- [x] Pi revisó copy sensible y reemplazó una promesa demasiado fuerte sobre datos sensibles por un texto más preciso: los excerpts pueden contener texto sensible si está en el correo.
- [x] No se tocó `.env`, secretos ni rutas fuera de `apps/web/`
- [x] Legacy FilterBar funcional: si `orgConfig === null` (backend no disponible), los 3 campos legacy siguen visibles y funcionales
- [x] IA desactivada por defecto en el wizard; requiere checkbox explícito de consentimiento
- [x] Copy de `gmail.readonly` preservado en `LoginView` y `AyudaView`
- [x] No se prometió borrado irreversible, verificación de dominio Workspace ni persistencia de cuerpos completos

## ASSUMPTIONS

- Se asume que el backend implementará `GET /me/org/config` y `PUT /me/org/config` según el contrato del codex-plan.md de TASK-007.
- El campo `policy_version_id` en `POST /analysis-runs` es opcional (si se omite, backend usa versión vigente).
- Las zonas horarias disponibles en el selector son un subconjunto razonable para beta; se puede expandir server-side más adelante.
- El wizard siempre muestra todos los pasos para re-edición (no hay modo "solo lectura" post-setup en esta iteración).

## Handoffs

- **Backend**: implementar `GET /me/org/config` y `PUT /me/org/config` según contrato del codex-plan.
- **Backend**: actualizar `POST /analysis-runs` para aceptar `policy_version_id` y ventana de fechas sin payload de dominios.
- **P2 — OrgSettingsView**: vista de ajustes post-onboarding (resumen de política activa, historial de versiones).
- **P2 — Verificación de dominio Workspace**: requiere señal adicional de Google, no se implementa en beta.
- **P2 — Desconexión explícita de mailbox**: flujo "Desconectar casilla" antes de que el usuario revoque desde Google.
