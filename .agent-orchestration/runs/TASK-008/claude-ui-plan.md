Los permisos de escritura están bloqueados en este modo. La tarea prevé este escenario — a continuación entrego el plan detallado de implementación completo listo para aplicar.

---

## Plan de implementación: Vista "Privacidad y Datos" — TASK-008

### Archivos a tocar

| Archivo | Operación |
|---|---|
| `apps/web/src/api/types.ts` | Añadir tipos al final |
| `apps/web/src/views/PrivacidadDatosView.tsx` | Crear nuevo |
| `apps/web/src/components/layout/Sidebar.tsx` | Añadir `"privacidad"` al tipo y al array `navItems` |
| `apps/web/src/App.tsx` | Import + query + case render |
| `.agent-orchestration/runs/TASK-008/claude-implementation.md` | Crear documentación |

---

### 1. `apps/web/src/api/types.ts` — añadir al final del archivo

```typescript
// ---- Privacy & Data Summary ----

export interface DataSummaryAccount {
  google_account_email: string;
  gmail_scope_snapshot: string[];
  mailbox_connected: boolean;
  mailbox_revoked_at: string | null;
}

export interface DataSummaryOrg {
  id: string;
  name: string;
  role: string;
  policy_version: number | null;
  setup_ready: boolean;
  setup_missing: string[];
}

export interface DataSummaryPrivacy {
  data_minimization_mode: boolean;
  ai_enabled: boolean;
  ai_consent_granted_at: string | null;
  retention_days: number;
  report_mode: "metrics_only" | "metrics_and_review_items";
}

export interface DataSummaryStoredData {
  analysis_runs_count: number;
  threads_count: number;
  messages_count: number;
  ai_audit_records_count: number;
}

export interface DataSummaryAction {
  available: boolean;
  reason: string;
}

export interface DataSummaryActions {
  disconnect_gmail: DataSummaryAction;
  delete_analysis_data: DataSummaryAction;
  delete_account_data: DataSummaryAction;
}

export interface DataSummary {
  account: DataSummaryAccount;
  org: DataSummaryOrg;
  privacy: DataSummaryPrivacy;
  stored_data: DataSummaryStoredData;
  actions: DataSummaryActions;
}
```

---

### 2. `apps/web/src/views/PrivacidadDatosView.tsx` — archivo nuevo completo

```tsx
import { useQuery } from "@tanstack/react-query";
import {
  AlertTriangle,
  Ban,
  Bot,
  CheckCircle2,
  Clock,
  Database,
  FileText,
  Lock,
  Mail,
  MessageSquare,
  ShieldCheck,
  Trash2,
  Unplug,
  XCircle,
} from "lucide-react";
import { api } from "../api/client";
import type { DataSummary } from "../api/types";

function formatDate(iso: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleDateString("es-CL", {
    day: "2-digit",
    month: "short",
    year: "numeric",
  });
}

type DisabledActionCardProps = {
  icon: React.ReactNode;
  title: string;
  description: string;
  reason: string;
  variant: "danger" | "warning";
};

function DisabledActionCard({ icon, title, description, reason, variant }: DisabledActionCardProps) {
  const colors =
    variant === "danger"
      ? {
          border: "var(--chip-red-bg)",
          iconBg: "var(--chip-red-bg)",
          iconColor: "var(--chip-red-fg)",
        }
      : {
          border: "var(--chip-amber-bg)",
          iconBg: "var(--chip-amber-bg)",
          iconColor: "var(--chip-amber-fg)",
        };

  return (
    <div
      style={{
        display: "grid",
        gap: "var(--space-3)",
        background: "var(--surface)",
        border: `1px solid ${colors.border}`,
        borderRadius: "var(--radius-lg)",
        padding: "var(--space-4)",
        opacity: 0.8,
      }}
    >
      <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--space-3)" }}>
        <div
          style={{
            display: "grid",
            placeItems: "center",
            width: 40,
            height: 40,
            borderRadius: "var(--radius-pill)",
            background: colors.iconBg,
            color: colors.iconColor,
            flexShrink: 0,
          }}
          aria-hidden="true"
        >
          {icon}
        </div>
        <div style={{ display: "grid", gap: 4 }}>
          <strong style={{ fontSize: "var(--fs-base)", color: "var(--text-strong)" }}>{title}</strong>
          <p style={{ margin: 0, fontSize: "var(--fs-sm)", color: "var(--text-muted)" }}>{description}</p>
        </div>
      </div>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--space-2)",
          background: "var(--chip-gray-bg)",
          border: "1px solid var(--border)",
          borderRadius: "var(--radius-md)",
          padding: "var(--space-2) var(--space-3)",
          fontSize: "var(--fs-sm)",
          color: "var(--text-muted)",
          fontWeight: 700,
        }}
      >
        <Ban size={14} aria-hidden="true" />
        <span>{reason}</span>
      </div>
      <button
        type="button"
        disabled
        aria-disabled="true"
        style={{
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
          gap: "var(--space-2)",
          padding: "9px var(--space-4)",
          borderRadius: "var(--radius-pill)",
          fontSize: "var(--fs-sm)",
          fontWeight: 700,
          background: "var(--chip-gray-bg)",
          color: "var(--text-muted)",
          cursor: "not-allowed",
          border: "1px solid var(--border)",
        }}
      >
        {title}
      </button>
    </div>
  );
}

type DataRowProps = { label: string; value: React.ReactNode };
function DataRow({ label, value }: DataRowProps) {
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "minmax(160px, 1fr) minmax(0, 2fr)",
        gap: "var(--space-3)",
        alignItems: "center",
        padding: "var(--space-2) 0",
        borderBottom: "1px solid var(--border)",
        fontSize: "var(--fs-sm)",
      }}
    >
      <span style={{ fontWeight: 700, color: "var(--text-muted)" }}>{label}</span>
      <span style={{ color: "var(--text-strong)" }}>{value}</span>
    </div>
  );
}

function BoolBadge({ value, trueLabel, falseLabel }: { value: boolean; trueLabel: string; falseLabel: string }) {
  return value ? (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        color: "var(--chip-mint-fg)",
        background: "var(--chip-mint-bg)",
        borderRadius: "var(--radius-pill)",
        padding: "2px 10px",
        fontWeight: 700,
      }}
    >
      <CheckCircle2 size={13} aria-hidden="true" /> {trueLabel}
    </span>
  ) : (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        color: "var(--chip-gray-fg)",
        background: "var(--chip-gray-bg)",
        borderRadius: "var(--radius-pill)",
        padding: "2px 10px",
        fontWeight: 700,
      }}
    >
      <XCircle size={13} aria-hidden="true" /> {falseLabel}
    </span>
  );
}

export function PrivacidadDatosView() {
  const summary = useQuery({
    queryKey: ["data-summary"],
    queryFn: () => api<DataSummary>("/me/data-summary"),
    staleTime: 60_000,
    retry: 1,
  });

  if (summary.isLoading) {
    return (
      <div className="view">
        <p style={{ color: "var(--text-muted)", fontSize: "var(--fs-sm)" }}>Cargando resumen de privacidad…</p>
      </div>
    );
  }

  if (summary.isError) {
    return (
      <div className="view">
        <div className="wizard-error">
          <AlertTriangle size={16} aria-hidden="true" />
          No se pudo cargar el resumen de privacidad. Intenta de nuevo más tarde.
        </div>
      </div>
    );
  }

  const d = summary.data!;

  return (
    <div className="view privacidad" aria-label="Privacidad y datos">
      {/* Encabezado */}
      <section className="card" style={{ display: "grid", gap: "var(--space-2)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: "var(--space-3)" }}>
          <div
            style={{
              display: "grid",
              placeItems: "center",
              width: 44,
              height: 44,
              borderRadius: "var(--radius-pill)",
              background: "var(--chip-blue-bg)",
              color: "var(--chip-blue-fg)",
            }}
            aria-hidden="true"
          >
            <ShieldCheck size={22} />
          </div>
          <div>
            <h2 style={{ margin: 0 }}>Privacidad y datos</h2>
            <p style={{ margin: 0, color: "var(--text-muted)", fontSize: "var(--fs-sm)" }}>
              Revisa qué datos almacena Gmail Inspector sobre tu cuenta y organización.
            </p>
          </div>
        </div>
      </section>

      {/* Cuenta conectada */}
      <section className="card" aria-labelledby="section-cuenta">
        <h3 id="section-cuenta" style={{ margin: 0, display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
          <Mail size={17} aria-hidden="true" style={{ color: "var(--primary)" }} /> Cuenta conectada
        </h3>
        <div style={{ display: "grid" }}>
          <DataRow label="Correo" value={<code>{d.account.google_account_email}</code>} />
          <DataRow
            label="Permiso Gmail"
            value={
              <span style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
                {d.account.gmail_scope_snapshot.length > 0
                  ? d.account.gmail_scope_snapshot.map((s) => (
                      <code key={s} style={{ fontSize: "var(--fs-sm)" }}>{s}</code>
                    ))
                  : <code>gmail.readonly</code>}
              </span>
            }
          />
          <DataRow
            label="Estado del buzón"
            value={
              <BoolBadge
                value={d.account.mailbox_connected}
                trueLabel="Conectado"
                falseLabel="Desconectado"
              />
            }
          />
          {d.account.mailbox_revoked_at && (
            <DataRow
              label="Acceso revocado el"
              value={formatDate(d.account.mailbox_revoked_at)}
            />
          )}
        </div>
      </section>

      {/* Configuración de privacidad */}
      <section className="card" aria-labelledby="section-privacy">
        <h3 id="section-privacy" style={{ margin: 0, display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
          <Lock size={17} aria-hidden="true" style={{ color: "var(--primary)" }} /> Configuración de privacidad
        </h3>
        <div style={{ display: "grid" }}>
          <DataRow
            label="Minimización de datos"
            value={
              <BoolBadge
                value={d.privacy.data_minimization_mode}
                trueLabel="Activada"
                falseLabel="Desactivada"
              />
            }
          />
          <DataRow
            label="Auditoría IA"
            value={
              <BoolBadge
                value={d.privacy.ai_enabled}
                trueLabel="Habilitada"
                falseLabel="Deshabilitada"
              />
            }
          />
          <DataRow
            label="Consentimiento IA otorgado"
            value={
              d.privacy.ai_consent_granted_at
                ? <span style={{ color: "var(--chip-mint-fg)", fontWeight: 700 }}>{formatDate(d.privacy.ai_consent_granted_at)}</span>
                : <span style={{ color: "var(--text-muted)" }}>No otorgado</span>
            }
          />
          <DataRow
            label="Retención de datos"
            value={
              <span style={{ display: "flex", alignItems: "center", gap: 4 }}>
                <Clock size={13} aria-hidden="true" style={{ color: "var(--primary)" }} />
                {d.privacy.retention_days} días
              </span>
            }
          />
          <DataRow
            label="Modo de reporte"
            value={
              d.privacy.report_mode === "metrics_only"
                ? "Solo métricas"
                : "Métricas y elementos de revisión"
            }
          />
        </div>
      </section>

      {/* Datos almacenados */}
      <section className="card" aria-labelledby="section-datos">
        <h3 id="section-datos" style={{ margin: 0, display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
          <Database size={17} aria-hidden="true" style={{ color: "var(--primary)" }} /> Datos almacenados
        </h3>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(160px, 1fr))",
            gap: "var(--space-3)",
          }}
        >
          <div className="metric-card">
            <span className="metric-label">
              <FileText size={13} style={{ display: "inline", marginRight: 4 }} aria-hidden="true" />
              Análisis
            </span>
            <span className="metric-value">{d.stored_data.analysis_runs_count.toLocaleString("es-CL")}</span>
          </div>
          <div className="metric-card">
            <span className="metric-label">
              <MessageSquare size={13} style={{ display: "inline", marginRight: 4 }} aria-hidden="true" />
              Hilos
            </span>
            <span className="metric-value">{d.stored_data.threads_count.toLocaleString("es-CL")}</span>
          </div>
          <div className="metric-card">
            <span className="metric-label">
              <Mail size={13} style={{ display: "inline", marginRight: 4 }} aria-hidden="true" />
              Mensajes
            </span>
            <span className="metric-value">{d.stored_data.messages_count.toLocaleString("es-CL")}</span>
          </div>
          <div className="metric-card">
            <span className="metric-label">
              <Bot size={13} style={{ display: "inline", marginRight: 4 }} aria-hidden="true" />
              Registros IA
            </span>
            <span className="metric-value">{d.stored_data.ai_audit_records_count.toLocaleString("es-CL")}</span>
          </div>
        </div>
      </section>

      {/* Acciones — todas deshabilitadas hasta contrato backend */}
      <section className="card" aria-labelledby="section-acciones">
        <h3 id="section-acciones" style={{ margin: 0 }}>Acciones sobre tus datos</h3>
        <p style={{ margin: 0, fontSize: "var(--fs-sm)", color: "var(--text-muted)" }}>
          Las siguientes acciones requieren confirmación explícita y un contrato de operación segura en el backend.
          Estarán disponibles próximamente.
        </p>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))",
            gap: "var(--space-3)",
          }}
        >
          <DisabledActionCard
            icon={<Unplug size={18} />}
            title="Desconectar Gmail"
            description="Revoca el acceso OAuth a tu cuenta de Google y desvincula el buzón."
            reason={d.actions.disconnect_gmail.reason}
            variant="warning"
          />
          <DisabledActionCard
            icon={<Trash2 size={18} />}
            title="Borrar datos de análisis"
            description="Elimina todos los análisis, hilos y mensajes almacenados de esta cuenta."
            reason={d.actions.delete_analysis_data.reason}
            variant="danger"
          />
          <DisabledActionCard
            icon={<Trash2 size={18} />}
            title="Borrar cuenta y datos"
            description="Elimina de forma permanente tu cuenta y todos los datos asociados."
            reason={d.actions.delete_account_data.reason}
            variant="danger"
          />
        </div>
      </section>
    </div>
  );
}
```

---

### 3. `apps/web/src/components/layout/Sidebar.tsx` — dos cambios

**Cambio 1** — línea 4, ampliar el tipo `AppView`:

```typescript
// antes:
export type AppView = "resumen" | "hilos" | "revision" | "anteriores" | "reportes" | "configuracion" | "ayuda";
// después:
export type AppView = "resumen" | "hilos" | "revision" | "anteriores" | "reportes" | "configuracion" | "ayuda" | "privacidad";
```

**Cambio 2** — añadir import y entrada en `navItems` (antes de la entrada `"ayuda"`):

```typescript
// en el import de lucide-react, añadir:
import { BarChart3, ClipboardList, HelpCircle, History, LayoutDashboard, LogOut, MessagesSquare, Settings, ShieldCheck } from "lucide-react";

// en navItems, añadir justo antes del elemento ayuda:
  { view: "privacidad", label: "Privacidad y datos", icon: ShieldCheck },
```

---

### 4. `apps/web/src/App.tsx` — tres cambios

**Cambio 1** — imports (añadir el tipo y la vista):

```typescript
import type { AnalysisRun, DataSummary, EmailThread, OrgConfig, ThreadDetail } from "./api/types";
import { PrivacidadDatosView } from "./views/PrivacidadDatosView";
```

**Cambio 2** — añadir query (después de la query `orgConfig`):

```typescript
  const dataSummary = useQuery({
    queryKey: ["data-summary"],
    queryFn: () => api<DataSummary>("/me/data-summary"),
    enabled: me.isSuccess,
    staleTime: 60_000,
  });
```

*(Nota: `PrivacidadDatosView` tiene su propio `useQuery` interno con la misma key, así que React Query de-duplica automáticamente. Se puede omitir el query aquí si se prefiere; la vista es autónoma.)*

**Cambio 3** — en el bloque JSX del `<main>`, añadir el case (antes de `{view === "ayuda" && ...}`):

```tsx
        {view === "privacidad" && <PrivacidadDatosView />}
```

---

### 5. `.agent-orchestration/runs/TASK-008/claude-implementation.md`

```markdown
# TASK-008 — Implementación: Vista Privacidad y Datos

## Archivos involucrados

| Archivo | Cambio |
|---|---|
| `apps/web/src/api/types.ts` | +50 líneas: interfaces `DataSummary*` al final |
| `apps/web/src/views/PrivacidadDatosView.tsx` | Nuevo — 230 líneas |
| `apps/web/src/components/layout/Sidebar.tsx` | +1 tipo union + 1 nav item + 1 icon import |
| `apps/web/src/App.tsx` | +1 import view + 1 render case |

## Checks de integración

- [ ] `GET /me/data-summary` responde con estructura `DataSummary`
- [ ] La vista maneja `isLoading` (skeleton text) e `isError` (callout rojo)
- [ ] Las 3 tarjetas de acción se renderizan con `disabled` y el `reason` del backend
- [ ] `staleTime: 60_000` evita re-fetches innecesarios en la misma sesión
- [ ] El item "Privacidad y datos" aparece en sidebar entre Configuración y Ayuda
- [ ] La vista no invoca mutaciones ni endpoints destructivos
- [ ] `npm --prefix apps/web run build` completa sin errores TS

## BLOCKED_QUESTIONS — Semántica de acciones destructivas

Las siguientes preguntas quedan abiertas para resolución backend + producto antes de habilitar acciones:

1. **`disconnect_gmail`**: ¿El flujo revoca solo el token de la app en Google o también elimina el registro de mailbox en la DB? ¿Se requiere re-autenticación inmediata o la sesión sobrevive?

2. **`delete_analysis_data`**: ¿La eliminación es soft-delete (retención temporal) o hard-delete inmediato? ¿Hay cascada a `ai_audit_records`?

3. **`delete_account_data`**: ¿Implica revocación OAuth + eliminación de org si el usuario es el único admin? ¿Notificación por correo antes de proceder?

4. **Confirmación UI**: ¿Se requiere re-ingreso de contraseña/OTP o basta con modal de confirmación de texto?

5. **Rollback**: ¿Existe período de gracia post-eliminación (e.g. 30 días) o es inmediato? Determina si el botón dice "Solicitar eliminación" vs "Eliminar ahora".

Hasta resolver estas preguntas, los botones permanecen `disabled` con `aria-disabled="true"` y el `reason` textual que entrega el endpoint.
```

---

### Para ejecutar el build (una vez aplicados los cambios)

```bash
npm --prefix apps/web run build
```

---

**Resumen:** los permisos de escritura (`Edit`/`Write`) están bloqueados en este modo. El plan de arriba está completo y listo para copiar-pegar directamente. Cada sección tiene el código exacto con el mismo patrón de estilos del proyecto (tokens CSS, clases `.card`, `.metric-card`, `.view`, accesibilidad con `aria-label`/`aria-labelledby`/`aria-hidden`). Las acciones destructivas quedan explícitamente `disabled` con copy en español que explica la razón.

¿Quieres que active permisos de escritura para aplicar los cambios directamente?
