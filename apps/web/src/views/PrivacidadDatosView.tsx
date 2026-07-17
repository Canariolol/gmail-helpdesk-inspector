import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  Ban,
  Bot,
  CheckCircle2,
  Clock,
  Database,
  FileText,
  Loader2,
  Lock,
  Mail,
  MessageSquare,
  ShieldCheck,
  Trash2,
  Unplug,
  XCircle,
} from "lucide-react";
import { useState, type ReactNode } from "react";
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

function humanizeReason(reason: string): string {
  if (reason === "already_disconnected") return "Gmail ya está desconectado";
  if (reason === "account_deletion_policy_pending") return "El borrado de cuenta aún está en definición";
  if (reason === "requires_confirmation") return "Requiere confirmación";
  return reason.split("_").join(" ");
}

function formatMissingSetupItem(item: string): string {
  if (item === "internal_domains") return "dominios de tu equipo";
  if (item === "valid_request_criteria") return "qué correos deben contar como solicitudes";
  if (item === "report_recipients") return "destinatarios de reportes";
  return "un dato de configuración";
}

function formatRole(role: string): string {
  if (role === "owner") return "Propietario";
  if (role === "member") return "Miembro";
  return "Miembro de la organización";
}

function DataRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="privacy-data-row">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function BoolBadge({ value, trueLabel, falseLabel }: { value: boolean; trueLabel: string; falseLabel: string }) {
  return value ? (
    <span className="privacy-badge ok"><CheckCircle2 size={13} /> {trueLabel}</span>
  ) : (
    <span className="privacy-badge muted"><XCircle size={13} /> {falseLabel}</span>
  );
}

function StatCard({ icon, label, value }: { icon: ReactNode; label: string; value: number | string }) {
  return (
    <div className="privacy-stat-card card">
      <div className="privacy-stat-icon" aria-hidden="true">{icon}</div>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function DisabledActionCard({
  icon,
  title,
  description,
  reason,
  tone,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  reason: string;
  tone: "warning" | "danger";
}) {
  return (
    <div className={`privacy-action-card ${tone}`}>
      <div className="privacy-action-head">
        <div className="privacy-action-icon" aria-hidden="true">{icon}</div>
        <div>
          <strong>{title}</strong>
          <p>{description}</p>
        </div>
      </div>
      <div className="privacy-action-disabled">
        <Ban size={14} /> {humanizeReason(reason)} · requiere confirmación explícita
      </div>
      <button type="button" disabled aria-disabled="true">
        {title}
      </button>
    </div>
  );
}

export function PrivacidadDatosView() {
  const queryClient = useQueryClient();
  const [confirmingGmailDisconnect, setConfirmingGmailDisconnect] = useState(false);
  const [confirmingDeletion, setConfirmingDeletion] = useState(false);
  const [deletionConfirmation, setDeletionConfirmation] = useState("");
  const summary = useQuery({
    queryKey: ["data-summary"],
    queryFn: () => api<DataSummary>("/me/data-summary"),
    retry: false,
    staleTime: 30_000,
  });
  const deleteAnalysisData = useMutation({
    mutationFn: () =>
      api<void>("/me/analysis-data", {
        method: "DELETE",
        body: JSON.stringify({ confirmation: deletionConfirmation }),
      }),
    onSuccess: () => {
      setConfirmingDeletion(false);
      setDeletionConfirmation("");
      queryClient.invalidateQueries({ queryKey: ["data-summary"] });
      queryClient.invalidateQueries({ queryKey: ["runs"] });
    },
  });
  const disconnectGmail = useMutation({
    mutationFn: () => api<void>("/gmail/disconnect", { method: "POST" }),
    onSuccess: () => {
      setConfirmingGmailDisconnect(false);
      queryClient.invalidateQueries({ queryKey: ["account"] });
      queryClient.invalidateQueries({ queryKey: ["org-config"] });
      queryClient.invalidateQueries({ queryKey: ["data-summary"] });
    },
  });

  if (summary.isLoading) {
    return (
      <div className="view">
        <div className="card privacy-loading">
          <Loader2 size={22} className="spin" />
          <span>Cargando resumen de privacidad…</span>
        </div>
      </div>
    );
  }

  if (summary.isError || !summary.data) {
    return (
      <div className="view">
        <div className="card privacy-error" role="alert">
          <AlertTriangle size={20} />
          <div>
            <strong>No se pudo cargar privacidad y datos</strong>
            <p>Intenta nuevamente. Esta pantalla solo consulta metadata y no ejecuta acciones destructivas.</p>
          </div>
        </div>
      </div>
    );
  }

  const data = summary.data;
  const readonlyScope = data.account.gmail_scope_snapshot.some((scope) => scope.includes("gmail.readonly"));

  return (
    <div className="view privacy-view">
      <section className="card privacy-hero">
        <div>
          <span className="privacy-kicker"><ShieldCheck size={15} /> Centro de confianza</span>
          <h2>Privacidad y datos</h2>
          <p>
            Revisa qué acceso tiene la app, qué datos derivados conserva y qué acciones de desconexión o borrado
            requieren confirmaciones explícitas antes de ejecutarse.
          </p>
        </div>
        <BoolBadge value={readonlyScope} trueLabel="Solo lectura" falseLabel="Permiso pendiente de confirmar" />
      </section>

      <div className="privacy-grid two">
        <section className="card privacy-panel">
          <h3><Mail size={18} /> Cuenta conectada</h3>
          <DataRow label="Cuenta Google" value={data.account.google_account_email} />
          <DataRow label="Cuenta de Gmail" value={<BoolBadge value={data.account.mailbox_connected} trueLabel="Conectada" falseLabel="Desconectada" />} />
          <DataRow label="Revocado el" value={formatDate(data.account.mailbox_revoked_at)} />
          <DataRow label="Permiso" value="Solo lectura" />
          <p className="privacy-note">
            La app puede leer la información necesaria para el análisis, pero no puede enviar, modificar, etiquetar ni
            borrar correos en Gmail.
          </p>
        </section>

        <section className="card privacy-panel">
          <h3><Lock size={18} /> Organización y política</h3>
          <DataRow label="Organización" value={data.org.name} />
          <DataRow label="Rol" value={formatRole(data.org.role)} />
          <DataRow label="Política activa" value={`v${data.org.policy_version}`} />
          <DataRow label="Configuración" value={<BoolBadge value={data.org.setup_ready} trueLabel="Lista" falseLabel="Pendiente" />} />
          {!data.org.setup_ready && data.org.setup_missing.length > 0 && (
            <p className="privacy-note warning">Falta completar: {data.org.setup_missing.map(formatMissingSetupItem).join(", ")}</p>
          )}
        </section>
      </div>

      <section className="card privacy-panel">
        <h3><Bot size={18} /> Uso de datos e IA</h3>
        <div className="privacy-grid two compact">
          <DataRow label="Uso mínimo de datos" value="Metadatos y extractos mínimos" />
          <DataRow label="Auditoría IA" value={<BoolBadge value={data.privacy.ai_enabled} trueLabel="Activa" falseLabel="Desactivada" />} />
          <DataRow label="Consentimiento IA" value={formatDate(data.privacy.ai_consent_granted_at)} />
          <DataRow label="Plazo configurado" value={`${data.privacy.retention_days} días`} />
          <DataRow label="Reportes" value={data.privacy.report_mode === "metrics_only" ? "Solo métricas" : "Métricas + elementos en revisión"} />
        </div>
        <p className="privacy-note">
          Los cuerpos completos no se conservan como datos de producto. Los extractos usados para IA se minimizan
          según la política vigente del análisis.
        </p>
        <p className="privacy-note warning">
          El plazo configurado todavía no activa un borrado automático. Puedes solicitar el borrado de tus análisis desde
          esta pantalla; el borrado de cuenta sigue en definición.
        </p>
      </section>

      <section className="privacy-stats">
        <StatCard icon={<FileText size={20} />} label="Análisis" value={data.stored_data.analysis_runs_count} />
        <StatCard icon={<MessageSquare size={20} />} label="Hilos derivados" value={data.stored_data.threads_count} />
        <StatCard icon={<Database size={20} />} label="Mensajes derivados" value={data.stored_data.messages_count} />
        <StatCard icon={<Clock size={20} />} label="Auditorías IA" value={data.stored_data.ai_audit_records_count ?? "No disponible"} />
      </section>

      <section className="card privacy-panel">
        <h3><Trash2 size={18} /> Acciones de datos</h3>
        <p className="privacy-note">
          Las acciones disponibles requieren confirmación. El borrado de cuenta sigue deshabilitado hasta definir sus
          efectos, retención y protecciones de seguridad.
        </p>
        <div className="privacy-actions">
          {data.actions.disconnect_gmail.available ? (
            <div className="privacy-action-card warning">
              <div className="privacy-action-head">
                <div className="privacy-action-icon" aria-hidden="true"><Unplug size={19} /></div>
                <div>
                  <strong>Desconectar Gmail</strong>
                  <p>Revoca acceso OAuth y detiene nuevos análisis automáticos.</p>
                </div>
              </div>
              {!confirmingGmailDisconnect ? (
                <button type="button" className="btn-ghost" onClick={() => setConfirmingGmailDisconnect(true)}>
                  Desconectar Gmail
                </button>
              ) : (
                <div className="cuenta-confirm">
                  <span>Dejarás de analizar esta casilla. Los análisis existentes se conservan. ¿Desconectar?</span>
                  {disconnectGmail.error && <span role="alert">{disconnectGmail.error.message}</span>}
                  <div className="cuenta-confirm-actions">
                    <button
                      type="button"
                      className="btn-ghost"
                      disabled={disconnectGmail.isPending}
                      onClick={() => disconnectGmail.mutate()}
                    >
                      {disconnectGmail.isPending ? "Desconectando…" : "Sí, desconectar"}
                    </button>
                    <button type="button" className="access-text-btn" onClick={() => setConfirmingGmailDisconnect(false)}>
                      Volver
                    </button>
                  </div>
                </div>
              )}
            </div>
          ) : (
            <DisabledActionCard
              icon={<Unplug size={19} />}
              title="Desconectar Gmail"
              description="Revoca acceso OAuth y detiene nuevos análisis automáticos."
              reason={data.actions.disconnect_gmail.reason}
              tone="warning"
            />
          )}
          {data.actions.delete_analysis_data.available ? (
            <div className="privacy-action-card danger">
              <div className="privacy-action-head">
                <div className="privacy-action-icon" aria-hidden="true"><Trash2 size={19} /></div>
                <div>
                  <strong>Borrar todos mis análisis</strong>
                  <p>Elimina análisis, hilos, mensajes derivados, auditorías IA y revisiones. No devuelve cuota ni borra tu cuenta.</p>
                </div>
              </div>
              {!confirmingDeletion ? (
                <button type="button" className="btn-ghost" onClick={() => setConfirmingDeletion(true)}>
                  Borrar análisis
                </button>
              ) : (
                <div className="cuenta-confirm">
                  <label htmlFor="delete-analysis-confirmation">Escribe <strong>BORRAR MIS ANALISIS</strong> para confirmar.</label>
                  <input
                    id="delete-analysis-confirmation"
                    value={deletionConfirmation}
                    onChange={(event) => setDeletionConfirmation(event.target.value)}
                    autoComplete="off"
                  />
                  {deleteAnalysisData.error && <span role="alert">{deleteAnalysisData.error.message}</span>}
                  <div className="cuenta-confirm-actions">
                    <button
                      type="button"
                      className="btn-ghost"
                      disabled={deletionConfirmation.trim() !== "BORRAR MIS ANALISIS" || deleteAnalysisData.isPending}
                      onClick={() => deleteAnalysisData.mutate()}
                    >
                      {deleteAnalysisData.isPending ? "Borrando…" : "Eliminar definitivamente"}
                    </button>
                    <button type="button" className="access-text-btn" onClick={() => setConfirmingDeletion(false)}>
                      Volver
                    </button>
                  </div>
                </div>
              )}
            </div>
          ) : (
            <DisabledActionCard
              icon={<Trash2 size={19} />}
              title="Borrar análisis"
              description="Eliminar resultados derivados de análisis. Estamos definiendo si esta acción será por análisis individual o para todos."
              reason={data.actions.delete_analysis_data.reason}
              tone="danger"
            />
          )}
          <DisabledActionCard
            icon={<Trash2 size={19} />}
            title="Borrar datos/cuenta"
            description="Eliminar configuración, políticas y datos derivados de la organización. Requiere confirmación fuerte."
            reason={data.actions.delete_account_data.reason}
            tone="danger"
          />
        </div>
      </section>
    </div>
  );
}
