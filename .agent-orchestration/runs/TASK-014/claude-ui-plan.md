Tengo suficiente contexto del codebase. Aquí está el diseño completo.

---

# Diseño UI: Observabilidad Scheduler/Run

## 1. Ubicación recomendada

**Integración en `ConfiguracionView`** (no nueva vista de sidebar).

**Justificación:**
- El scheduler ya vive en el paso 5 del wizard. El estado operacional es la contraparte natural de esa config.
- El sidebar ya tiene 8 ítems; evitar el ítem número 9.
- `PrivacidadDatosView` sigue el patrón de "panel de solo lectura dentro de la misma shell" — esta vista lo imita.

**Punto de entrada alternativo:** un chip de estado clickeable en `ResumenView` (banner de 1 línea), que lleva a `configuracion` con scroll al panel de estado. Detalle más abajo.

**Ruta de implementación:**
```
apps/web/src/
  views/ConfiguracionView.tsx        ← añadir <SchedulerStatusPanel> encima del wizard
  components/scheduler/
    SchedulerStatusPanel.tsx         ← componente contenedor
    SchedulerRunRow.tsx              ← fila de último run
    SchedulerErrorNotice.tsx         ← aviso de error redactado
  api/types.ts                       ← añadir tipos OperationsStatus
```

---

## 2. Tipos TypeScript sugeridos

```typescript
// apps/web/src/api/types.ts — añadir al final

export type SchedulerRunOutcome =
  | "success"
  | "partial_success"
  | "failed"
  | "skipped";

export interface SchedulerLastRun {
  run_id: string | null;
  started_at: string | null;
  completed_at: string | null;
  outcome: SchedulerRunOutcome | null;
  threads_processed: number | null;
  threads_total: number | null;
  error_hint: string | null;      // redactado por backend — e.g. "gmail_auth_error"
  duration_seconds: number | null;
}

export interface SchedulerConfigSnapshot {
  enabled: boolean;
  preset: string;                 // e.g. "weekdays_08_local"
  timezone: string;
  recipient_count: number;
  report_mode: "metrics_only" | "metrics_and_review_items";
  policy_version: number | null;
}

export interface OperationsStatus {
  scheduler: SchedulerConfigSnapshot;
  last_run: SchedulerLastRun | null;
  next_run_estimate: string | null;    // ISO 8601 o null si disabled
  current_run_id: string | null;       // non-null si hay run en curso ahora
}
```

---

## 3. Componentes y estados

### 3.1 `SchedulerStatusPanel` (contenedor)

Panel de solo lectura insertado **antes** del wizard en `ConfiguracionView`. Se monta solo si `orgConfig` existe y el endpoint responde.

**Estados:**

| Estado | Descripción | Visual |
|---|---|---|
| `loading` | Query en vuelo | 3 skeleton rows (mismo patrón que MetricCard) |
| `endpoint_unavailable` | 404/500 del backend | Callout naranja suave, copy: *"Estado del scheduler no disponible aún"* — igual al patrón de `isError` en ConfiguracionView |
| `scheduler_disabled` | `scheduler.enabled = false` | Panel gris neutro con icono `PauseCircle`. Copy: *"Análisis automático desactivado"* |
| `idle_ok` | Habilitado, último run exitoso | Panel con `next_run_estimate` prominente, chip verde "Último run: OK" |
| `idle_last_failed` | Habilitado, último run falló | Panel con banner ámbar, `SchedulerErrorNotice` |
| `running_now` | `current_run_id != null` | Banner animado (spinner + `Loader2`) con progreso si viene del polling de `/analysis-runs` |

### 3.2 `SchedulerRunRow` (presentacional)

Fila compacta para el último run — reutiliza `RunStatusChip` existente.

```
[ChipTone icon] Último análisis   [fecha relativa]   [chip estado]   [N hilos / M totales]
```

Si `last_run = null`: copy *"Ningún análisis automático ejecutado todavía"* + icono `Clock`.

### 3.3 `SchedulerErrorNotice` (presentacional)

Callout ámbar (usando las vars `--chip-amber-*` del design system).

```
⚠  El último análisis automático falló
   Causa registrada: [error_hint humanizado]
   Revisa la conexión de Gmail en Privacidad y datos → Cuenta.
```

`error_hint` mapping (solo lectura, no viene del usuario — seguro mostrar):

| Backend hint | Copy visible |
|---|---|
| `gmail_auth_error` | *Error de autenticación Gmail* |
| `quota_exceeded` | *Límite de cuota alcanzado* |
| `policy_not_ready` | *Configuración incompleta* |
| `unknown` | *Error interno (ver logs del sistema)* |

### 3.4 Banner de acceso rápido en `ResumenView` (opcional, bajo riesgo)

Una línea entre el header y las MetricCards:

```
[CalendarClock 16px]  Próximo análisis automático: mañana 08:00 (América/Santiago)   [→ ver estado]
```

Clickeable → `onGoToSetup()` (prop ya existe). Solo se muestra si `scheduler.enabled = true`.

---

## 4. Copy en español

| Elemento | Copy |
|---|---|
| Panel header | **Estado del scheduler** |
| Scheduler desactivado | *El análisis automático está desactivado. Puedes habilitarlo en el paso "Programación" de esta configuración.* |
| Próximo run | **Próximo análisis:** {fecha relativa} · {hora} ({timezone}) |
| Run en curso | **Análisis en curso** — iniciado hace {N} minutos |
| Último run OK | **Último análisis:** {fecha} · {N} hilos procesados |
| Último run fallido | **Último análisis falló** · {fecha} |
| Sin runs aún | *Ningún análisis automático ejecutado todavía* |
| Policy version | *Política activa: v{N}* |
| Recipients | *Reportes a {N} destinatario(s)* |
| Error notice title | **El último análisis automático no se completó** |
| Error CTA | *Revisa la conexión de Gmail en [Privacidad y datos →]()* |
| Endpoint no disponible | *El estado del scheduler no está disponible todavía. Los análisis manuales siguen funcionando normalmente.* |
| Loading | *Verificando estado del scheduler…* |

---

## 5. Criterios de accesibilidad

Siguiendo los patrones ya presentes en el repo (`aria-current="page"`, `aria-label` en nav):

- **Roles semánticos:** el panel usa `<section aria-labelledby="scheduler-status-heading">`. El heading `h3` recibe el `id`.
- **Live region:** si `current_run_id != null`, el contenedor del spinner lleva `aria-live="polite"` + `aria-busy="true"`. Al completar, `aria-live` anuncia el resultado.
- **Estado del chip:** los chips de outcome (OK/Falló) no dependen solo del color — incluyen texto explícito + `role="status"` en el callout de error.
- **Fechas:** las fechas relativas ("hace 2 horas") se complementan con `<time dateTime={isoString}>` para lectores de pantalla.
- **Skeleton:** los placeholders de carga llevan `aria-hidden="true"` con un `<span className="sr-only">Cargando estado del scheduler</span>` visible solo para AT.
- **Botones de acción futura** (ej. "Reintentar manualmente"): si se añaden, llevan `type="button"` y `aria-label` descriptivo.
- **Contraste:** todos los estados usan las vars `--chip-*-fg/bg` del design system que ya superan WCAG AA sobre `--surface`.

---

## 6. Riesgos

| Riesgo | Mitigación |
|---|---|
| Endpoint aún no implementado | Mismo patrón de `isError` de ConfiguracionView: callout suave, el wizard sigue funcionando |
| `next_run_estimate` en UTC vs. local del usuario | Mostrar siempre con timezone label; usar `Intl.DateTimeFormat` con la tz del `scheduler.timezone` del snapshot |
| Polling agresivo de `/me/operations/status` | `staleTime: 60_000`, `refetchInterval: 30_000` solo si `current_run_id != null` (idéntico al patrón de `runs` en App.tsx) |
| `error_hint` con info sensible si el backend no redacta bien | El componente solo mapea un conjunto cerrado de hints conocidos; cualquier valor fuera del enum muestra el copy genérico "Error interno" |
| Run en curso detectado aquí vs. en RunsView | Fuente de verdad única: el `current_run_id` de este endpoint apunta al mismo `AnalysisRun.id` que ya existe en la query `["analysis-runs"]`. Usar `queryClient.getQueryData` para enriquecer sin re-fetch |

---

## 7. BLOCKED_QUESTIONS para el backend

1. **¿`next_run_estimate` es calculado en tiempo de respuesta o pre-calculado al guardar config?** Afecta si se puede cachear o requiere revalidación frecuente.
2. **¿`error_hint` es un enum cerrado o texto libre?** El componente asume enum; si es libre, el backend debe especificar el contrato antes de implementar el mapping.
3. **¿Puede haber `current_run_id` apuntando a un run que NO aparece en `/analysis-runs`?** (e.g., run de otro mailbox en cuenta multi-mailbox futura). Si sí, el panel necesita su propio fetch de estado de run.
4. **¿El endpoint `/me/operations/status` requiere el mismo scope OAuth que el resto, o necesita scope adicional?** Relevante para el flujo de error de autenticación.
5. **¿`partial_success` tiene semántica distinta de `success`?** ¿Debe mostrarse como advertencia o como éxito con nota?

---

**Próximo paso:** si apruebas el diseño, el siguiente turno implementa `SchedulerStatusPanel` + tipos en una sola PR de lectura pura — cero mutaciones al backend.
