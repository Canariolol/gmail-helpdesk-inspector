import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  Loader2,
  PauseCircle,
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

type SectionKey = "org" | "equipo" | "cuenta" | "ia" | "programacion" | "retencion";

const SECTIONS: Array<{ key: SectionKey; label: string; optional: boolean }> = [
  { key: "org", label: "Organización", optional: false },
  { key: "equipo", label: "Equipo", optional: false },
  { key: "cuenta", label: "Qué cuenta", optional: false },
  { key: "ia", label: "Auditoría IA", optional: false },
  { key: "programacion", label: "Programación", optional: true },
  { key: "retencion", label: "Retención", optional: true },
];

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
      maxAuditMessages: 4,
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

function formatDateTime(iso: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString("es-CL", {
    day: "2-digit",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatExecutionStatus(status: string): string {
  if (status === "running") return "En curso";
  if (status === "completed") return "Completada";
  if (status === "failed") return "Con problemas";
  return "No disponible";
}

function formatMissingSetupItem(item: string): string {
  if (item === "internal_domains") return "dominios de tu equipo";
  if (item === "valid_request_criteria") return "qué correos deben contar como solicitudes";
  if (item === "report_recipients") return "destinatarios de reportes";
  return "un dato de configuración";
}

function SchedulerStatusPanel({ status }: { status: OperationsStatus | undefined }) {
  if (!status) {
    return (
      <div className="scheduler-status-panel muted">
        <Loader2 size={18} className="spin" />
        <span>Cargando estado operativo…</span>
      </div>
    );
  }
  const last = status.scheduler.last_state;
  const failed = last?.status === "failed";
  return (
    <div className={`scheduler-status-panel${failed ? " warning" : ""}`}>
      <div className="scheduler-status-head">
        <div>
          <span className="mono-label">Operación automática</span>
          <h3>
            {status.scheduler.enabled ? <Activity size={18} /> : <PauseCircle size={18} />}
            {status.scheduler.enabled ? "Análisis programado activo" : "Análisis programado desactivado"}
          </h3>
        </div>
        <span className={status.scheduler.enabled ? "wizard-ready-badge" : "wizard-not-ready-badge"}>
          {status.scheduler.enabled ? "Lunes a viernes, 08:00" : "Solo manual"}
        </span>
      </div>
      <div className="scheduler-status-grid">
        <div><span>Zona horaria</span><strong>{status.scheduler.timezone}</strong></div>
        <div><span>Próximo intento</span><strong>{formatDateTime(status.scheduler.next_run_estimate)}</strong></div>
        <div><span>Destinatarios</span><strong>{status.scheduler.recipients_count}</strong></div>
        <div><span>Reglas de análisis</span><strong>v{status.policy.policy_version}</strong></div>
      </div>
      {last ? (
        <p className="scheduler-status-copy">
          Última ejecución: <strong>{last.window_date_from} → {last.window_date_to}</strong> · estado <strong>{formatExecutionStatus(last.status)}</strong>
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
    <div className="operations-history-panel">
      <div className="scheduler-status-head">
        <div>
          <span className="mono-label">Historial operativo</span>
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
                <strong>{entry.kind === "scheduler_attempt" ? "Análisis programado" : "Análisis"}</strong>
                <span>
                  {formatDateTime(entry.started_at)}
                  {entry.window_date_from && entry.window_date_to
                    ? ` · ${entry.window_date_from} → ${entry.window_date_to}`
                    : ""}
                </span>
              </div>
              <div>
                <span className="operations-history-status">{formatExecutionStatus(entry.status)}</span>
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
  const [draft, setDraft] = useState<WizardDraft>(() => initDraft(null));
  const [formError, setFormError] = useState<string | null>(null);
  const [isDirty, setIsDirty] = useState(false);
  const [setupState, setSetupState] = useState(orgConfig?.setup_state ?? null);
  const [activeSection, setActiveSection] = useState<SectionKey>("org");
  const initialized = useRef(false);
  const sectionRefs = useRef<Partial<Record<SectionKey, HTMLElement | null>>>({});

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
    }
  }, [orgConfig]);

  // Scroll-spy: marca en la sub-navegación la sección visible.
  useEffect(() => {
    const elements = SECTIONS.map((s) => sectionRefs.current[s.key]).filter(
      (el): el is HTMLElement => Boolean(el),
    );
    if (elements.length === 0) return;
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((entry) => entry.isIntersecting)
          .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
        const first = visible[0]?.target.getAttribute("data-section");
        if (first) setActiveSection(first as SectionKey);
      },
      { rootMargin: "-15% 0px -65% 0px" },
    );
    elements.forEach((el) => observer.observe(el));
    return () => observer.disconnect();
  }, [isLoading, isError]);

  const saveMutation = useMutation({
    mutationFn: (body: ReturnType<typeof buildPutBody>) =>
      api<PutConfigResponse>("/me/org/config", { method: "PUT", body: JSON.stringify(body) }),
    onSuccess: (res) => {
      setIsDirty(false);
      setSetupState(res.setup_state);
      queryClient.invalidateQueries({ queryKey: ["org-config"] });
      queryClient.invalidateQueries({ queryKey: ["operations-status"] });
    },
  });

  function set<K extends keyof WizardDraft>(key: K, val: WizardDraft[K]) {
    saveMutation.reset();
    setIsDirty(true);
    setDraft((d) => ({ ...d, [key]: val }));
  }

  function sectionComplete(key: SectionKey): boolean {
    if (key === "org") return draft.orgName.trim().length > 0;
    if (key === "equipo") return splitLines(draft.internalDomainsText).length > 0;
    if (key === "cuenta") return splitLines(draft.validCriteriaText).length > 0;
    if (key === "programacion")
      return !draft.schedulerEnabled || splitLines(draft.reportRecipientsText).length > 0;
    return true;
  }

  function validateSection(key: SectionKey): string | null {
    if (key === "org" && !draft.orgName.trim()) return "El nombre de la organización es requerido.";
    if (key === "equipo" && splitLines(draft.internalDomainsText).length === 0)
      return "Agrega al menos un dominio interno (sin @). Ej: tuempresa.com";
    if (key === "cuenta" && splitLines(draft.validCriteriaText).length === 0)
      return "Describe al menos un criterio de solicitud válida.";
    if (key === "programacion" && draft.schedulerEnabled && splitLines(draft.reportRecipientsText).length === 0)
      return "Agrega al menos un destinatario para activar el análisis automático.";
    return null;
  }

  function goToSection(key: SectionKey) {
    sectionRefs.current[key]?.scrollIntoView({ behavior: "smooth", block: "start" });
    setActiveSection(key);
  }

  function handleSave(finalize = false) {
    if (finalize) {
      for (const section of SECTIONS) {
        const message = validateSection(section.key);
        if (message) {
          setFormError(message);
          goToSection(section.key);
          return;
        }
      }
    }
    setFormError(null);
    saveMutation.mutate(buildPutBody(draft, finalize));
  }

  if (isLoading) {
    return (
      <div className="view">
        <div className="card cfg-loading">
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
            <AlertTriangle size={20} color="var(--warn)" />
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
  const ready = Boolean(setupState?.ready_for_analysis);

  const registerSection = (key: SectionKey) => (el: HTMLElement | null) => {
    sectionRefs.current[key] = el;
  };

  return (
    <div className="view cfg-view">
      {/* Cabecera compacta */}
      <header className="cfg-head">
        <div>
          <h2>Configuración</h2>
          {orgConfig && (
            <p className="cfg-head-meta">
              {orgConfig.mailbox.google_account_email}
              {orgConfig.policy_version && <> · política v{orgConfig.policy_version.version}</>}
              <> · solo lectura</>
            </p>
          )}
        </div>
        {ready ? (
          <span className="wizard-ready-badge">
            <CheckCircle2 size={15} /> Lista para análisis
          </span>
        ) : (
          <span className="wizard-not-ready-badge">
            <AlertTriangle size={15} /> Configuración pendiente
          </span>
        )}
      </header>
      {!ready && setupState?.missing && setupState.missing.length > 0 && (
        <p className="cfg-missing">
          Falta completar: {setupState.missing.map(formatMissingSetupItem).join(", ")}
        </p>
      )}

      <div className="cfg-layout">
        {/* Sub-navegación con estado por sección */}
        <nav className="cfg-subnav" aria-label="Secciones de configuración">
          {SECTIONS.map((section) => {
            const complete = sectionComplete(section.key);
            return (
              <button
                key={section.key}
                type="button"
                className={`cfg-subnav-item${activeSection === section.key ? " on" : ""}`}
                onClick={() => goToSection(section.key)}
              >
                <span className={`cfg-subnav-state${complete ? " ok" : ""}`}>
                  {complete ? "✓" : "—"}
                </span>
                {section.label}
                {section.optional && <small>opc</small>}
              </button>
            );
          })}
        </nav>

        {/* Secciones abiertas */}
        <div className="cfg-sections">
          <section className="cfg-section" data-section="org" ref={registerSection("org")}>
            <div className="cfg-section-head">
              <span className="mono-label">01 · Organización</span>
              <h3>Organización y contexto</h3>
              <p>
                Funciona con Gmail, Google Workspace, Outlook y Microsoft 365. Usamos un permiso de
                solo lectura y la zona horaria de tu equipo para calcular ventanas y reportes.
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
              La zona horaria se usa para calcular tiempos de respuesta y programar análisis
              automáticos. Elige la zona de la cuenta principal si tu equipo opera en múltiples
              regiones.
            </p>
          </section>

          <section className="cfg-section" data-section="equipo" ref={registerSection("equipo")}>
            <div className="cfg-section-head">
              <span className="mono-label">02 · Equipo</span>
              <h3>¿Quién forma parte de tu equipo?</h3>
              <p>Los correos enviados desde estos dominios se contarán como respuestas internas.</p>
            </div>
            <div className="field">
              <label htmlFor="internal-domains">
                Dominios internos<span className="cfg-required">*</span>
              </label>
              <textarea
                id="internal-domains"
                rows={4}
                value={draft.internalDomainsText}
                onChange={(e) => set("internalDomainsText", e.target.value)}
                placeholder={"tuempresa.com\notro-dominio.cl"}
              />
              <span className="cfg-hint">Un dominio por línea, sin @</span>
            </div>
            {domains.length > 0 && (
              <div className="wizard-preview">
                Los correos de {domains.map((d) => `@${d}`).join(", ")} se contarán como respuestas
                de tu equipo.
              </div>
            )}
          </section>

          <section className="cfg-section" data-section="cuenta" ref={registerSection("cuenta")}>
            <div className="cfg-section-head">
              <span className="mono-label">03 · Qué cuenta</span>
              <h3>¿Qué tipo de correos recibes?</h3>
              <p>Define qué cuenta como solicitud válida y qué ignorar en el análisis.</p>
            </div>
            <div className="field">
              <label htmlFor="valid-criteria">
                Criterios de solicitud válida<span className="cfg-required">*</span>
              </label>
              <textarea
                id="valid-criteria"
                rows={4}
                value={draft.validCriteriaText}
                onChange={(e) => set("validCriteriaText", e.target.value)}
                placeholder={"Clientes externos piden soporte técnico u operativo\nSolicitudes de acceso o activación de cuentas"}
              />
              <span className="cfg-hint">
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
              <span className="cfg-hint">
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
              <span className="cfg-hint">
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
              <span className="cfg-hint">
                Hilos cuyo asunto contenga estas palabras se clasifican como misc. Uno por línea.
              </span>
            </div>
          </section>

          <section className="cfg-section" data-section="ia" ref={registerSection("ia")}>
            <div className="cfg-section-head">
              <span className="mono-label">04 · Auditoría IA</span>
              <h3>Auditoría con IA</h3>
              <p>
                La IA clasifica hilos para detectar casos ambiguos que necesitan revisión manual.
                Viene activada para potenciar la calidad del servicio; puedes desactivarla cuando
                quieras.
              </p>
            </div>
            <div className="wizard-ai-card">
              <h4>¿Qué datos procesa?</h4>
              <ul>
                <li>Participantes, fecha, asunto y texto del mensaje reducido a un máximo de {draft.maxBodyCharsPerMessage} caracteres</li>
                <li>Hasta 4 mensajes clave por hilo, sin duplicados</li>
                <li>
                  Un mensaje corto puede incluirse completo dentro de ese límite; los extractos pueden contener texto sensible
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
              <p className="wizard-help">
                Mientras esté activa, fragmentos minimizados de tus hilos de soporte se procesan
                mediante el proveedor Amazon Bedrock para clasificación automática. El cambio aplica
                al próximo análisis.
              </p>
            ) : (
              <p className="wizard-help">
                Desactivaste la auditoría IA. Los hilos con clasificación incierta irán a revisión
                manual. Puedes reactivarla cuando quieras; al hacerlo confirmas su uso para los
                próximos análisis.
              </p>
            )}
          </section>

          <section className="cfg-section" data-section="programacion" ref={registerSection("programacion")}>
            <div className="cfg-section-head">
              <span className="mono-label">05 · Programación</span>
              <h3>
                Análisis automático y reportes <span className="wizard-optional">Opcional</span>
              </h3>
              <p>Configura cuándo analizar automáticamente y cómo recibir resúmenes.</p>
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
              <span className="cfg-hint">
                El cambio se aplica cuando guardas o publicas la configuración.
              </span>
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
                <span className="cfg-hint">
                  Uno por línea. Es obligatorio si activas el análisis automático.
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

            {/* Estado operativo: vive donde pertenece, junto a la programación. */}
            {!operationsStatus.isError && <SchedulerStatusPanel status={operationsStatus.data} />}
            {!operationsHistory.isError && <OperationsHistoryPanel history={operationsHistory.data} />}
          </section>

          <section className="cfg-section" data-section="retencion" ref={registerSection("retencion")}>
            <div className="cfg-section-head">
              <span className="mono-label">06 · Retención</span>
              <h3>
                Retención de datos <span className="wizard-optional">Opcional</span>
              </h3>
              <p>¿Cuánto tiempo conservar los resultados de análisis?</p>
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
                <strong>¿Qué define este plazo?</strong> La fecha de expiración que se guarda junto a
                cada análisis y que usaremos para las políticas de borrado de datos internos de la
                aplicación.
              </p>
              <p>
                <strong>¿Qué NO se modifica?</strong> Nada de tu casilla. La app solo tiene permiso
                de lectura.
              </p>
              <p style={{ color: "var(--text-muted)" }}>
                En esta versión inicial, los flujos destructivos se activarán de forma controlada.
                Puedes cambiar esta configuración en cualquier momento; los análisis ya realizados
                conservan su fecha original.
              </p>
            </div>
          </section>
        </div>
      </div>

      {/* Barra de guardado sticky */}
      <div className="cfg-savebar">
        <div className="cfg-savebar-status">
          {formError && (
            <span className="cfg-savebar-error">
              <AlertTriangle size={15} /> {formError}
            </span>
          )}
          {!formError && saveMutation.isError && (
            <span className="cfg-savebar-error">
              <AlertTriangle size={15} />
              {saveMutation.error instanceof Error
                ? saveMutation.error.message
                : "No se pudo guardar la configuración. Revisa tu conexión e intenta de nuevo."}
            </span>
          )}
          {!formError && saveMutation.isSuccess && (
            <span className="cfg-savebar-ok">
              <CheckCircle2 size={15} />
              Guardado.
              {saveMutation.variables?.finalize
                ? ready
                  ? " La casilla está lista para análisis."
                  : " Completa las secciones pendientes para habilitar el análisis."
                : " Puedes seguir editando el borrador."}
            </span>
          )}
          {!formError && isDirty && !saveMutation.isPending && !saveMutation.isError && (
            <span className="cfg-hint">Cambios sin guardar.</span>
          )}
        </div>
        <div className="cfg-savebar-actions">
          <button
            type="button"
            className="btn-ghost"
            onClick={() => handleSave(false)}
            disabled={saveMutation.isPending}
          >
            {saveMutation.isPending ? <Loader2 size={16} className="spin" /> : null}
            Guardar borrador
          </button>
          <button
            type="button"
            className="btn-primary"
            onClick={() => handleSave(true)}
            disabled={saveMutation.isPending}
          >
            {saveMutation.isPending ? <Loader2 size={16} className="spin" /> : null}
            {ready ? "Publicar cambios" : "Completar configuración"}
          </button>
        </div>
      </div>
    </div>
  );
}
