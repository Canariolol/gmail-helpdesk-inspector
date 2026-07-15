import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Activity,
  AlertTriangle,
  Bot,
  Building2,
  Calendar,
  CheckCircle2,
  Clock,
  FileCheck,
  Loader2,
  PauseCircle,
  Settings,
  Users,
} from "lucide-react";
import type { OperationsHistory, OperationsStatus, OrgConfig, PutConfigResponse } from "../api/types";
import { api } from "../api/client";

const TIMEZONES = [
  "America/Santiago",
  "America/Buenos_Aires",
  "America/Lima",
  "America/Bogota",
  "America/Mexico_City",
  "America/Sao_Paulo",
  "America/New_York",
  "America/Los_Angeles",
  "Europe/Madrid",
  "UTC",
];

const STEPS = [
  { id: 1, label: "Organización", icon: Building2, optional: false },
  { id: 2, label: "Equipo", icon: Users, optional: false },
  { id: 3, label: "Qué cuenta", icon: FileCheck, optional: false },
  { id: 4, label: "Auditoría IA", icon: Bot, optional: false },
  { id: 5, label: "Programación", icon: Calendar, optional: true },
  { id: 6, label: "Retención", icon: Clock, optional: true },
] as const;

type WizardDraft = {
  orgName: string;
  orgTimezone: string;
  internalDomainsText: string;
  validCriteriaText: string;
  nonResponsibilityText: string;
  ignoredDomainsText: string;
  ignoredKeywordsText: string;
  validSignalKeywordsText: string;
  aiEnabled: boolean;
  aiConsentChecked: boolean;
  maxAuditMessages: number;
  maxBodyCharsPerMessage: number;
  schedulerEnabled: boolean;
  reportRecipientsText: string;
  reportMode: "metrics_only" | "metrics_and_review_items";
  retentionDays: number;
};

type Props = {
  orgConfig: OrgConfig | null;
  isLoading: boolean;
  isError: boolean;
};

function splitLines(text: string): string[] {
  return text
    .split(/[\n,]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}

function initDraft(config: OrgConfig | null): WizardDraft {
  if (!config) {
    return {
      orgName: "",
      orgTimezone: "America/Santiago",
      internalDomainsText: "",
      validCriteriaText: "",
      nonResponsibilityText: "",
      ignoredDomainsText: "google.com\ncalendar.google.com",
      ignoredKeywordsText: "newsletter\nboletín\npromoción",
      validSignalKeywordsText: "",
      aiEnabled: true,
      aiConsentChecked: true,
      maxAuditMessages: 14,
      maxBodyCharsPerMessage: 280,
      schedulerEnabled: false,
      reportRecipientsText: "",
      reportMode: "metrics_only",
      retentionDays: 30,
    };
  }
  const { draft, org } = config;
  return {
    orgName: org.name,
    orgTimezone: org.default_timezone,
    internalDomainsText: draft.analysis_policy.internal_domains.join("\n"),
    validCriteriaText: draft.analysis_policy.valid_request_criteria.join("\n"),
    nonResponsibilityText: draft.analysis_policy.non_responsibility_rules.join("\n"),
    ignoredDomainsText: draft.analysis_policy.ignored_domains.join("\n"),
    ignoredKeywordsText: draft.analysis_policy.ignored_keywords.join("\n"),
    validSignalKeywordsText: (draft.analysis_policy.valid_signal_keywords ?? []).join("\n"),
    aiEnabled: draft.ai_policy.enabled,
    aiConsentChecked: draft.ai_policy.enabled,
    maxAuditMessages: draft.ai_policy.max_audit_messages,
    maxBodyCharsPerMessage: draft.ai_policy.max_body_chars_per_message,
    schedulerEnabled: draft.schedule_report_policy.scheduler_enabled,
    reportRecipientsText: draft.schedule_report_policy.report_recipients.join("\n"),
    reportMode: draft.schedule_report_policy.report_content.mode,
    retentionDays: draft.retention_policy.retention_days,
  };
}

function buildPutBody(d: WizardDraft, finalize = false) {
  return {
    finalize,
    org: { name: d.orgName.trim(), default_timezone: d.orgTimezone, locale: "es-CL" },
    analysis_policy: {
      timezone: d.orgTimezone,
      internal_domains: splitLines(d.internalDomainsText),
      responder_emails: [],
      mailbox_aliases: [],
      valid_request_criteria: splitLines(d.validCriteriaText),
      non_responsibility_rules: splitLines(d.nonResponsibilityText),
      ignored_senders: [],
      ignored_domains: splitLines(d.ignoredDomainsText),
      ignored_keywords: splitLines(d.ignoredKeywordsText),
      valid_signal_keywords: splitLines(d.validSignalKeywordsText),
      default_time_from: "00:00",
      default_time_to: "23:59",
      max_threads_per_run: 50,
    },
    ai_policy: {
      enabled: d.aiEnabled,
      consent_confirmed: d.aiConsentChecked,
      auto_apply_threshold: 0.92,
      manual_review_threshold: 0.72,
      max_audit_messages: d.maxAuditMessages,
      max_body_chars_per_message: d.maxBodyCharsPerMessage,
    },
    schedule_report_policy: {
      scheduler_enabled: d.schedulerEnabled,
      preset: "weekdays_08_local",
      timezone: d.orgTimezone,
      report_recipients: d.schedulerEnabled ? splitLines(d.reportRecipientsText) : [],
      report_content: { mode: d.reportMode, include_subjects: false, include_senders: false },
      failure_notice_enabled: true,
    },
    retention_policy: { retention_days: d.retentionDays },
  };
}

function stepForMissing(missing: string[]): number {
  if (missing.includes("internal_domains")) return 2;
  if (missing.includes("valid_request_criteria")) return 3;
  if (missing.includes("report_recipients")) return 5;
  return 1;
}

function formatDateTime(iso: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString("es-CL", {
    day: "2-digit",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function SchedulerStatusPanel({ status }: { status: OperationsStatus | undefined }) {
  if (!status) {
    return (
      <div className="card scheduler-status-panel muted">
        <Loader2 size={18} className="spin" />
        <span>Cargando estado operativo…</span>
      </div>
    );
  }
  const last = status.scheduler.last_state;
  const failed = last?.status === "failed";
  return (
    <div className={`card scheduler-status-panel${failed ? " warning" : ""}`}>
      <div className="scheduler-status-head">
        <div>
          <span className="wizard-step-desc">Operación automática</span>
          <h3>
            {status.scheduler.enabled ? <Activity size={18} /> : <PauseCircle size={18} />}
            {status.scheduler.enabled ? "Scheduler activo" : "Scheduler desactivado"}
          </h3>
        </div>
        <span className={status.scheduler.enabled ? "wizard-ready-badge" : "wizard-not-ready-badge"}>
          {status.scheduler.enabled ? "weekdays 08:00 local" : "manual"}
        </span>
      </div>
      <div className="scheduler-status-grid">
        <div><span>Zona horaria</span><strong>{status.scheduler.timezone}</strong></div>
        <div><span>Próximo intento</span><strong>{formatDateTime(status.scheduler.next_run_estimate)}</strong></div>
        <div><span>Destinatarios</span><strong>{status.scheduler.recipients_count}</strong></div>
        <div><span>Policy</span><strong>v{status.policy.policy_version}</strong></div>
      </div>
      {last ? (
        <p className="scheduler-status-copy">
          Última ventana: <strong>{last.window_date_from} → {last.window_date_to}</strong> · estado <strong>{last.status}</strong>
          {last.run_id ? <> · run <code>{last.run_id.slice(0, 8)}</code></> : null}
        </p>
      ) : (
        <p className="scheduler-status-copy">Aún no hay ejecuciones automáticas registradas.</p>
      )}
      {status.scheduler.last_error_redacted && (
        <div className="wizard-error">
          <AlertTriangle size={16} /> {status.scheduler.last_error_redacted}
        </div>
      )}
    </div>
  );
}

function OperationsHistoryPanel({ history }: { history: OperationsHistory | undefined }) {
  if (!history) {
    return null;
  }
  return (
    <div className="card operations-history-panel">
      <div className="scheduler-status-head">
        <div>
          <span className="wizard-step-desc">Historial operativo</span>
          <h3>
            <Activity size={18} /> Últimos eventos
          </h3>
        </div>
        <span className="wizard-ready-badge">{history.total_count} registrados</span>
      </div>
      {history.entries.length === 0 ? (
        <p className="scheduler-status-copy">Aún no hay ejecuciones registradas.</p>
      ) : (
        <div className="operations-history-list">
          {history.entries.map((entry) => (
            <div key={entry.id} className={`operations-history-item status-${entry.status}`}>
              <div>
                <strong>{entry.kind === "scheduler_attempt" ? "Scheduler" : "Análisis"}</strong>
                <span>
                  {formatDateTime(entry.started_at)}
                  {entry.window_date_from && entry.window_date_to
                    ? ` · ${entry.window_date_from} → ${entry.window_date_to}`
                    : ""}
                </span>
              </div>
              <div>
                <span className="operations-history-status">{entry.status}</span>
                {entry.error_category && <span className="operations-history-error">{entry.error_category}</span>}
              </div>
              {entry.error_redacted && <p>{entry.error_redacted}</p>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export function ConfiguracionView({ orgConfig, isLoading, isError }: Props) {
  const queryClient = useQueryClient();
  const [step, setStep] = useState(1);
  const [draft, setDraft] = useState<WizardDraft>(() => initDraft(null));
  const [stepError, setStepError] = useState<string | null>(null);
  const [setupState, setSetupState] = useState(orgConfig?.setup_state ?? null);
  const initialized = useRef(false);

  const operationsStatus = useQuery({
    queryKey: ["operations-status"],
    queryFn: () => api<OperationsStatus>("/me/operations/status"),
    enabled: Boolean(orgConfig),
    retry: false,
    staleTime: 30_000,
  });

  const operationsHistory = useQuery({
    queryKey: ["operations-history"],
    queryFn: () => api<OperationsHistory>("/me/operations/history?limit=8"),
    enabled: Boolean(orgConfig),
    retry: false,
    staleTime: 30_000,
  });

  useEffect(() => {
    if (orgConfig && !initialized.current) {
      initialized.current = true;
      setDraft(initDraft(orgConfig));
      setSetupState(orgConfig.setup_state);
      if (!orgConfig.setup_state.ready_for_analysis) {
        setStep(stepForMissing(orgConfig.setup_state.missing));
      }
    }
  }, [orgConfig]);

  const saveMutation = useMutation({
    mutationFn: (body: ReturnType<typeof buildPutBody>) =>
      api<PutConfigResponse>("/me/org/config", { method: "PUT", body: JSON.stringify(body) }),
    onSuccess: (res) => {
      setSetupState(res.setup_state);
      queryClient.invalidateQueries({ queryKey: ["org-config"] });
      queryClient.invalidateQueries({ queryKey: ["operations-status"] });
    },
  });

  function set<K extends keyof WizardDraft>(key: K, val: WizardDraft[K]) {
    setDraft((d) => ({ ...d, [key]: val }));
  }

  function validate(s: number): string | null {
    if (s === 1 && !draft.orgName.trim()) return "El nombre de la organización es requerido.";
    if (s === 2 && splitLines(draft.internalDomainsText).length === 0)
      return "Agrega al menos un dominio interno (sin @). Ej: tuempresa.com";
    if (s === 3 && splitLines(draft.validCriteriaText).length === 0)
      return "Describe al menos un criterio de solicitud válida.";
    if (s === 5 && draft.schedulerEnabled && splitLines(draft.reportRecipientsText).length === 0)
      return "Agrega al menos un destinatario para activar el análisis automático.";
    return null;
  }

  function validateAll(): { step: number; message: string } | null {
    for (const candidate of STEPS) {
      const message = validate(candidate.id);
      if (message) return { step: candidate.id, message };
    }
    return null;
  }

  function handleNext() {
    const err = validate(step);
    if (err) { setStepError(err); return; }
    setStepError(null);
    setStep((s) => Math.min(s + 1, STEPS.length));
  }

  function handlePrev() {
    setStepError(null);
    saveMutation.reset();
    setStep((s) => Math.max(s - 1, 1));
  }

  function handleSave(finalize = false) {
    const validation = finalize ? validateAll() : null;
    const err = validation?.message ?? null;
    if (err) {
      if (validation) setStep(validation.step);
      setStepError(err);
      return;
    }
    setStepError(null);
    saveMutation.mutate(buildPutBody(draft, finalize));
  }

  if (isLoading) {
    return (
      <div className="view">
        <div className="card" style={{ display: "flex", alignItems: "center", justifyContent: "center", gap: 12, padding: 48 }}>
          <Loader2 size={22} className="spin" />
          <span>Cargando configuración…</span>
        </div>
      </div>
    );
  }

  if (isError) {
    return (
      <div className="view">
        <div className="card" style={{ display: "grid", gap: 12 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <AlertTriangle size={20} color="var(--chip-amber-fg)" />
            <strong>No pudimos cargar la configuración</strong>
          </div>
          <p style={{ color: "var(--text-muted)", fontSize: "var(--fs-sm)" }}>
            Revisa tu conexión e inténtalo nuevamente.
          </p>
        </div>
      </div>
    );
  }

  const domains = splitLines(draft.internalDomainsText);

  return (
    <div className="view">
      {/* Estado de setup */}
      <div className="wizard-state-card card">
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", flexWrap: "wrap", gap: 12 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <Settings size={18} color="var(--primary-strong)" />
            <h2 style={{ margin: 0 }}>Configuración de organización</h2>
          </div>
          {setupState?.ready_for_analysis ? (
            <span className="wizard-ready-badge">
              <CheckCircle2 size={15} /> Lista para análisis
            </span>
          ) : (
            <span className="wizard-not-ready-badge">
              <AlertTriangle size={15} /> Configuración pendiente
            </span>
          )}
        </div>
        {!setupState?.ready_for_analysis && setupState?.missing && setupState.missing.length > 0 && (
          <p style={{ fontSize: "var(--fs-sm)", color: "var(--text-muted)" }}>
            Faltan: {setupState.missing.join(", ")}
          </p>
        )}
        {orgConfig && (
          <p style={{ fontSize: "var(--fs-sm)", color: "var(--text-muted)" }}>
            Casilla Workspace: <strong style={{ color: "var(--text)" }}>{orgConfig.mailbox.google_account_email}</strong>
            {orgConfig.policy_version && (
              <> · Política v{orgConfig.policy_version.version}</>
            )}
            <> · Scope Gmail readonly</>
          </p>
        )}
      </div>

      {!operationsStatus.isError && <SchedulerStatusPanel status={operationsStatus.data} />}
      {!operationsHistory.isError && <OperationsHistoryPanel history={operationsHistory.data} />}

      {/* Wizard */}
      <div className="wizard">
        {/* Stepper */}
        <div className="wizard-stepper" role="list" aria-label="Pasos de configuración">
          {STEPS.map((s, i) => {
            const done = s.id < step;
            const active = s.id === step;
            return (
              <div key={s.id} style={{ display: "flex", alignItems: "center", flex: i < STEPS.length - 1 ? 1 : undefined, minWidth: 0 }}>
                <div
                  role="listitem"
                  className={`wizard-step-dot${active ? " active" : ""}${done ? " done" : ""}`}
                  aria-current={active ? "step" : undefined}
                >
                  <div className="dot">
                    {done ? <CheckCircle2 size={14} /> : s.id}
                  </div>
                  <span className="dot-label">{s.label}</span>
                </div>
                {i < STEPS.length - 1 && (
                  <div className={`wizard-step-sep${done ? " done" : ""}`} aria-hidden="true" />
                )}
              </div>
            );
          })}
        </div>

        {/* Contenido del paso */}
        <div className="wizard-body">
          {step === 1 && (
            <div className="wizard-fields">
              <div>
                <h3 className="wizard-step-title">Organización y contexto</h3>
                <p className="wizard-step-desc">
                  Funciona con cualquier cuenta de Google. Usamos Gmail solo lectura y la zona horaria de tu equipo
                  para calcular ventanas y reportes.
                </p>
              </div>
              <div className="field">
                <label htmlFor="org-name">Nombre de la organización</label>
                <input
                  id="org-name"
                  value={draft.orgName}
                  onChange={(e) => set("orgName", e.target.value)}
                  placeholder="Ej. Acme Corp"
                />
              </div>
              <div className="field">
                <label htmlFor="org-tz">Zona horaria</label>
                <select
                  id="org-tz"
                  value={draft.orgTimezone}
                  onChange={(e) => set("orgTimezone", e.target.value)}
                >
                  {TIMEZONES.map((tz) => (
                    <option key={tz} value={tz}>{tz}</option>
                  ))}
                </select>
              </div>
              <p className="wizard-help">
                La zona horaria se usa para calcular tiempos de respuesta y programar análisis automáticos.
                Elige la zona del mailbox principal si tu equipo opera en múltiples regiones.
              </p>
            </div>
          )}

          {step === 2 && (
            <div className="wizard-fields">
              <div>
                <h3 className="wizard-step-title">¿Quién forma parte de tu equipo?</h3>
                <p className="wizard-step-desc">
                  Los correos enviados desde estos dominios se contarán como respuestas internas.
                </p>
              </div>
              <div className="field">
                <label htmlFor="internal-domains">
                  Dominios internos
                  <span style={{ fontWeight: 400, color: "var(--chip-red-fg)" }}> *</span>
                </label>
                <textarea
                  id="internal-domains"
                  rows={4}
                  value={draft.internalDomainsText}
                  onChange={(e) => set("internalDomainsText", e.target.value)}
                  placeholder={"tuempresa.com\notro-dominio.cl"}
                />
                <span style={{ fontSize: "var(--fs-sm)", color: "var(--text-muted)" }}>
                  Un dominio por línea, sin @
                </span>
              </div>
              {domains.length > 0 && (
                <div className="wizard-preview">
                  Los correos de {domains.map((d) => `@${d}`).join(", ")} se contarán como respuestas de tu equipo.
                </div>
              )}
            </div>
          )}

          {step === 3 && (
            <div className="wizard-fields">
              <div>
                <h3 className="wizard-step-title">¿Qué tipo de correos recibes?</h3>
                <p className="wizard-step-desc">
                  Define qué cuenta como solicitud válida y qué ignorar en el análisis.
                </p>
              </div>
              <div className="field">
                <label htmlFor="valid-criteria">
                  Criterios de solicitud válida
                  <span style={{ fontWeight: 400, color: "var(--chip-red-fg)" }}> *</span>
                </label>
                <textarea
                  id="valid-criteria"
                  rows={4}
                  value={draft.validCriteriaText}
                  onChange={(e) => set("validCriteriaText", e.target.value)}
                  placeholder={"Clientes externos piden soporte técnico u operativo\nSolicitudes de acceso o activación de cuentas"}
                />
                <span style={{ fontSize: "var(--fs-sm)", color: "var(--text-muted)" }}>
                  Describe los tipos de correos que sí cuentan como solicitud. Uno por línea.
                </span>
              </div>
              <div className="field">
                <label htmlFor="valid-signal-keywords">Palabras que confirman un ticket (opcional)</label>
                <textarea
                  id="valid-signal-keywords"
                  rows={3}
                  value={draft.validSignalKeywordsText}
                  onChange={(e) => set("validSignalKeywordsText", e.target.value)}
                  placeholder={"ticket\nincidencia\ncaso\nfolio"}
                />
                <span style={{ fontSize: "var(--fs-sm)", color: "var(--text-muted)" }}>
                  Si la respuesta del equipo menciona alguna de estas palabras, el correo se cuenta
                  como solicitud válida y la IA confirma el veredicto. Una por línea.
                </span>
              </div>
              <div className="field">
                <label htmlFor="non-responsibility">Reglas de no responsabilidad (opcional)</label>
                <textarea
                  id="non-responsibility"
                  rows={3}
                  value={draft.nonResponsibilityText}
                  onChange={(e) => set("nonResponsibilityText", e.target.value)}
                  placeholder={"Ventas, facturación y marketing no cuentan como mesa de ayuda"}
                />
              </div>
              <div className="field">
                <label htmlFor="ignored-domains">Dominios ignorados</label>
                <textarea
                  id="ignored-domains"
                  rows={3}
                  value={draft.ignoredDomainsText}
                  onChange={(e) => set("ignoredDomainsText", e.target.value)}
                  placeholder={"google.com\ncalendar.google.com"}
                />
                <span style={{ fontSize: "var(--fs-sm)", color: "var(--text-muted)" }}>
                  Correos desde estos dominios se excluirán del análisis. Uno por línea.
                </span>
              </div>
              <div className="field">
                <label htmlFor="ignored-keywords">Palabras clave ignoradas</label>
                <textarea
                  id="ignored-keywords"
                  rows={3}
                  value={draft.ignoredKeywordsText}
                  onChange={(e) => set("ignoredKeywordsText", e.target.value)}
                  placeholder={"newsletter\nboletín\npromoción"}
                />
                <span style={{ fontSize: "var(--fs-sm)", color: "var(--text-muted)" }}>
                  Hilos cuyo asunto contenga estas palabras se clasifican como misc. Uno por línea.
                </span>
              </div>
            </div>
          )}

          {step === 4 && (
            <div className="wizard-fields">
              <div>
                <h3 className="wizard-step-title">Auditoría con IA</h3>
                <p className="wizard-step-desc">
                  La IA clasifica hilos para detectar casos ambiguos que necesitan revisión manual.
                  Viene activada para potenciar la calidad del servicio; puedes desactivarla cuando quieras.
                </p>
              </div>
              <div className="wizard-ai-card">
                <h4>¿Qué datos procesa?</h4>
                <ul>
                  <li>Participantes, fecha, asunto y texto del mensaje reducido a un máximo de {draft.maxBodyCharsPerMessage} caracteres</li>
                  <li>Hasta {draft.maxAuditMessages} mensajes por hilo</li>
                  <li>
                    Un mensaje corto puede incluirse completo dentro de ese límite; los excerpts pueden contener texto sensible
                  </li>
                  <li>
                    <strong>NO</strong> se procesan adjuntos, imágenes ni headers completos
                  </li>
                </ul>
                <p>Procesado por Amazon Bedrock. Puedes desactivarla en cualquier momento desde esta pantalla.</p>
              </div>
              <label className="checkline" style={{ cursor: "pointer" }}>
                <input
                  type="checkbox"
                  checked={draft.aiEnabled}
                  onChange={(e) => {
                    set("aiEnabled", e.target.checked);
                    // Activarla (incl. reactivarla tras un opt-out) confirma el consentimiento.
                    set("aiConsentChecked", e.target.checked);
                  }}
                />
                <span>Auditoría IA activada</span>
              </label>
              {draft.aiEnabled ? (
                <p className="wizard-help" style={{ fontSize: "var(--fs-sm)" }}>
                  Mientras esté activa, fragmentos minimizados de tus hilos de soporte se procesan mediante el
                  proveedor Amazon Bedrock para clasificación automática. El cambio aplica al próximo análisis.
                </p>
              ) : (
                <p className="wizard-help">
                  Desactivaste la auditoría IA. Los hilos con clasificación incierta irán a revisión manual.
                  Puedes reactivarla cuando quieras; al hacerlo confirmas su uso para los próximos análisis.
                </p>
              )}
            </div>
          )}

          {step === 5 && (
            <div className="wizard-fields">
              <div>
                <h3 className="wizard-step-title">
                  Análisis automático y reportes{" "}
                  <span className="wizard-optional">Opcional</span>
                </h3>
                <p className="wizard-step-desc">
                  Configura cuándo analizar automáticamente y cómo recibir resúmenes.
                </p>
              </div>
              <div className="field">
                <label>Análisis automático</label>
                <label className="checkline" style={{ cursor: "pointer", fontWeight: 400 }}>
                  <input
                    type="checkbox"
                    checked={draft.schedulerEnabled}
                    onChange={(e) => set("schedulerEnabled", e.target.checked)}
                  />
                  <span>Analizar automáticamente los días laborables a las 08:00 (hora local)</span>
                </label>
              </div>
              {draft.schedulerEnabled && (
                <div className="field">
                  <label htmlFor="report-recipients">Destinatarios de reportes por email</label>
                  <textarea
                    id="report-recipients"
                    rows={3}
                    value={draft.reportRecipientsText}
                    onChange={(e) => set("reportRecipientsText", e.target.value)}
                    placeholder={"operaciones@tuempresa.com\ngerencia@tuempresa.com"}
                  />
                  <span style={{ fontSize: "var(--fs-sm)", color: "var(--text-muted)" }}>
                    Uno por línea. Requerido si el scheduler está activado.
                  </span>
                </div>
              )}
              <div className="field">
                <label>Contenido del reporte</label>
                <div className="radio-group">
                  <label className="checkline" style={{ cursor: "pointer", fontWeight: 400 }}>
                    <input
                      type="radio"
                      name="reportMode"
                      value="metrics_only"
                      checked={draft.reportMode === "metrics_only"}
                      onChange={() => set("reportMode", "metrics_only")}
                    />
                    <span>
                      <strong>Solo métricas</strong> (recomendado) — sin asuntos ni remitentes de correos
                    </span>
                  </label>
                  <label className="checkline" style={{ cursor: "pointer", fontWeight: 400, alignItems: "flex-start" }}>
                    <input
                      type="radio"
                      name="reportMode"
                      value="metrics_and_review_items"
                      style={{ marginTop: 3 }}
                      checked={draft.reportMode === "metrics_and_review_items"}
                      onChange={() => set("reportMode", "metrics_and_review_items")}
                    />
                    <span>
                      <strong>Métricas + asuntos en revisión</strong>
                      <span className="wizard-warning">
                        ⚠ Incluye asuntos de correos de clientes en el reporte. Asegúrate de que los
                        destinatarios tienen autorización para ver esta información.
                      </span>
                    </span>
                  </label>
                </div>
              </div>
            </div>
          )}

          {step === 6 && (
            <div className="wizard-fields">
              <div>
                <h3 className="wizard-step-title">
                  Retención de datos{" "}
                  <span className="wizard-optional">Opcional</span>
                </h3>
                <p className="wizard-step-desc">¿Cuánto tiempo conservar los resultados de análisis?</p>
              </div>
              <div className="field">
                <label>Período de retención</label>
                <div className="radio-group">
                  {([30, 60, 90] as const).map((days) => (
                    <label key={days} className="checkline" style={{ cursor: "pointer", fontWeight: 400 }}>
                      <input
                        type="radio"
                        name="retention"
                        value={days}
                        checked={draft.retentionDays === days}
                        onChange={() => set("retentionDays", days)}
                      />
                      <span>
                        {days} días{days === 30 && " (recomendado)"}
                      </span>
                    </label>
                  ))}
                </div>
              </div>
              <div className="wizard-info-box">
                <p>
                  <strong>¿Qué define este plazo?</strong> La fecha de expiración que se guarda junto a cada análisis
                  y que usaremos para las políticas de borrado de datos internos de la aplicación.
                </p>
                <p>
                  <strong>¿Qué NO se modifica?</strong> Nada de tu Gmail. La app solo tiene permiso de lectura.
                </p>
                <p style={{ color: "var(--text-muted)" }}>
                  En esta versión inicial, los flujos destructivos se activarán de forma controlada. Puedes cambiar esta
                  configuración en cualquier momento; los análisis ya realizados conservan su fecha original.
                </p>
              </div>
            </div>
          )}

          {/* Errores y estado de guardado */}
          {stepError && (
            <div className="wizard-error" style={{ marginTop: 16 }}>
              <AlertTriangle size={16} />
              {stepError}
            </div>
          )}
          {saveMutation.isError && (
            <div className="wizard-error" style={{ marginTop: 16 }}>
              <AlertTriangle size={16} />
              {saveMutation.error instanceof Error
                ? saveMutation.error.message
                : "No se pudo guardar la configuración. Revisa tu conexión e intenta de nuevo."}
            </div>
          )}
          {saveMutation.isSuccess && (
            <div className="wizard-success" style={{ marginTop: 16 }}>
              <CheckCircle2 size={16} />
              Configuración guardada correctamente.
              {saveMutation.variables?.finalize
                ? setupState?.ready_for_analysis
                  ? " La casilla está lista para análisis."
                  : " Completa los pasos restantes para habilitar el análisis."
                : " Puedes continuar editando el borrador."}
            </div>
          )}

          {/* Navegación */}
          <div className="wizard-nav">
            <button
              type="button"
              className="btn-ghost"
              onClick={handlePrev}
              disabled={step === 1}
            >
              Anterior
            </button>
            <div className="wizard-nav-end">
              <button
                type="button"
                className="btn-ghost"
                onClick={() => handleSave(false)}
                disabled={saveMutation.isPending}
              >
                {saveMutation.isPending ? <Loader2 size={16} className="spin" /> : null}
                Guardar borrador
              </button>
              {step < STEPS.length ? (
                <button type="button" className="btn-primary" onClick={handleNext}>
                  Siguiente
                </button>
              ) : (
                <button
                  type="button"
                  className="btn-primary"
                  onClick={() => handleSave(true)}
                  disabled={saveMutation.isPending}
                >
                  {saveMutation.isPending ? <Loader2 size={16} className="spin" /> : null}
                  Completar configuración
                </button>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
