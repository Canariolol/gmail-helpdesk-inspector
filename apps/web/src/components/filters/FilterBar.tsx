import { AlertTriangle, CheckCircle2, Save, Sparkles, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { FilterPreset, GmailLabel, OrgConfig } from "../../api/types";
import { split } from "../../lib/format";
import { RangeField } from "./RangeField";

type Props = {
  loading: boolean;
  onAnalyze: (payload: unknown) => void;
  orgConfig?: OrgConfig | null;
  onGoToSetup?: () => void;
  filterPresets?: FilterPreset[];
  onSavePreset?: (payload: unknown) => void;
  onDeletePreset?: (id: string) => void;
};

// Fecha de hoy en la zona horaria del helpdesk (America/Santiago), formato YYYY-MM-DD.
// Se evalúa en la zona local del desk para evitar el corrimiento de día de toISOString() (UTC).
const HELPDESK_TZ = "America/Santiago";
function todayInHelpdeskTz(): string {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: HELPDESK_TZ,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date());
}

// Token que el backend traduce a operador de búsqueda de Gmail: las etiquetas de
// usuario se referencian por nombre; las de sistema (INBOX, CATEGORY_*) por id.
function labelToken(label: GmailLabel): string {
  return label.label_type === "user" ? label.name : label.id;
}

function selectedValues(select: HTMLSelectElement): string[] {
  return Array.from(select.selectedOptions).map((option) => option.value);
}

export function FilterBar({
  loading,
  onAnalyze,
  orgConfig,
  onGoToSetup,
  filterPresets = [],
  onSavePreset,
  onDeletePreset,
}: Props) {
  // El selector arranca siempre en la fecha actual (hoy), recalculada en cada montaje.
  const [dateFromVal, setDateFrom] = useState(todayInHelpdeskTz);
  const [dateToVal, setDateTo] = useState(todayInHelpdeskTz);
  const [timeFrom, setTimeFrom] = useState("00:00");
  const [timeTo, setTimeTo] = useState("23:59");

  // Selección de etiquetas de Gmail (incluir / excluir) para la recuperación.
  const [includeLabels, setIncludeLabels] = useState<string[]>([]);
  const [excludeLabels, setExcludeLabels] = useState<string[]>([]);

  // Presets guardados
  const [presetName, setPresetName] = useState("");
  const [selectedPresetId, setSelectedPresetId] = useState("");

  // Legacy state — only used when orgConfig is not available
  const [domains, setDomains] = useState("");
  const [ignoredDomains, setIgnoredDomains] = useState("google.com,calendar.google.com");
  const [ignoredKeywords, setIgnoredKeywords] = useState("newsletter,boletín,promoción");

  const hasConfig = orgConfig != null;
  // Cuentas privilegiadas nunca se bloquean por setup incompleto.
  const unrestricted = hasConfig && orgConfig.account_unrestricted === true;
  const isReady = hasConfig && orgConfig.setup_state.ready_for_analysis;
  const isNotReady = hasConfig && !orgConfig.setup_state.ready_for_analysis && !unrestricted;

  const labels = orgConfig?.mailbox_metadata?.labels ?? [];

  function applyPreset(preset: FilterPreset) {
    setSelectedPresetId(preset.id);
    setIncludeLabels(preset.include_labels ?? []);
    setExcludeLabels(preset.exclude_labels ?? []);
    if ((preset.ignored_domains ?? []).length > 0) {
      setIgnoredDomains(preset.ignored_domains.join(","));
    }
    if ((preset.ignored_keywords ?? []).length > 0) {
      setIgnoredKeywords(preset.ignored_keywords.join(","));
    }
  }

  // Auto-aplica el preset por defecto la primera vez que llegan los presets.
  const appliedDefault = useRef(false);
  useEffect(() => {
    if (appliedDefault.current || filterPresets.length === 0) return;
    const fallback = filterPresets.find((preset) => preset.is_default);
    if (fallback) applyPreset(fallback);
    appliedDefault.current = true;
  }, [filterPresets]);

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const labelSelection = {
      include_labels: includeLabels,
      exclude_labels: excludeLabels,
    };

    if (hasConfig) {
      // New mode: date window + label selection + optional policy version
      onAnalyze({
        date_from: dateFromVal,
        date_to: dateToVal,
        time_from: timeFrom,
        time_to: timeTo,
        ...labelSelection,
        ...(orgConfig.policy_version ? { policy_version_id: orgConfig.policy_version.id } : {}),
      });
    } else {
      // Legacy mode: full payload including domains and filters
      onAnalyze({
        date_from: dateFromVal,
        date_to: dateToVal,
        time_from: timeFrom,
        time_to: timeTo,
        timezone: "America/Santiago",
        internal_domains: split(domains),
        ignored_senders: [],
        ignored_domains: split(ignoredDomains),
        ignored_keywords: split(ignoredKeywords),
        ...labelSelection,
      });
    }
  }

  function handleSavePreset() {
    const name = presetName.trim();
    if (!name || !onSavePreset) return;
    onSavePreset({
      name,
      include_labels: includeLabels,
      exclude_labels: excludeLabels,
      ignored_senders: [],
      ignored_domains: hasConfig ? [] : split(ignoredDomains),
      ignored_keywords: hasConfig ? [] : split(ignoredKeywords),
      is_default: false,
    });
    setPresetName("");
  }

  return (
    <form className="filter-bar" onSubmit={handleSubmit}>
      <RangeField
        label="Fechas"
        type="date"
        fromValue={dateFromVal}
        toValue={dateToVal}
        onFromChange={setDateFrom}
        onToChange={setDateTo}
      />
      <RangeField
        label="Horario"
        type="time"
        fromValue={timeFrom}
        toValue={timeTo}
        onFromChange={setTimeFrom}
        onToChange={setTimeTo}
      />

      {labels.length > 0 && (
        <>
          <div className="field">
            <span className="field-label">Bandejas/etiquetas a incluir</span>
            <select
              multiple
              className="label-multiselect"
              value={includeLabels}
              onChange={(event) => setIncludeLabels(selectedValues(event.target))}
            >
              {labels.map((label) => (
                <option key={`inc-${label.id}`} value={labelToken(label)}>
                  {label.name}
                </option>
              ))}
            </select>
          </div>
          <div className="field">
            <span className="field-label">Etiquetas a excluir</span>
            <select
              multiple
              className="label-multiselect"
              value={excludeLabels}
              onChange={(event) => setExcludeLabels(selectedValues(event.target))}
            >
              {labels.map((label) => (
                <option key={`exc-${label.id}`} value={labelToken(label)}>
                  {label.name}
                </option>
              ))}
            </select>
          </div>
        </>
      )}

      {(filterPresets.length > 0 || onSavePreset) && (
        <div className="field filter-presets">
          <span className="field-label">Filtros guardados</span>
          <div className="preset-row">
            <select
              value={selectedPresetId}
              onChange={(event) => {
                const preset = filterPresets.find((item) => item.id === event.target.value);
                if (preset) applyPreset(preset);
                else setSelectedPresetId("");
              }}
            >
              <option value="">Sin preset</option>
              {filterPresets.map((preset) => (
                <option key={preset.id} value={preset.id}>
                  {preset.name}
                  {preset.is_default ? " (por defecto)" : ""}
                </option>
              ))}
            </select>
            {selectedPresetId && onDeletePreset && (
              <button
                type="button"
                className="preset-icon-btn"
                title="Eliminar preset"
                onClick={() => {
                  onDeletePreset(selectedPresetId);
                  setSelectedPresetId("");
                }}
              >
                <Trash2 size={15} />
              </button>
            )}
            {onSavePreset && (
              <>
                <input
                  value={presetName}
                  onChange={(event) => setPresetName(event.target.value)}
                  placeholder="Nombre del filtro"
                />
                <button
                  type="button"
                  className="preset-icon-btn"
                  title="Guardar filtro actual"
                  onClick={handleSavePreset}
                  disabled={!presetName.trim()}
                >
                  <Save size={15} />
                </button>
              </>
            )}
          </div>
        </div>
      )}

      {/* Mode indicator or legacy fields */}
      {isReady && (
        <div className="filter-bar-policy">
          <CheckCircle2 size={14} />
          <span>
            Política activa
            {orgConfig.policy_version ? ` v${orgConfig.policy_version.version}` : ""}
            {" · "}
            {orgConfig.mailbox.workspace_domain}
          </span>
        </div>
      )}

      {isNotReady && (
        <div className="filter-bar-setup-prompt">
          <AlertTriangle size={14} />
          <span>Completa la configuración antes de analizar.</span>
          {onGoToSetup && (
            <button
              type="button"
              onClick={onGoToSetup}
              style={{
                marginLeft: "auto",
                fontSize: "var(--fs-sm)",
                fontWeight: 700,
                color: "var(--chip-amber-fg)",
                textDecoration: "underline",
                background: "none",
                border: 0,
                padding: 0,
                cursor: "pointer",
              }}
            >
              Configurar →
            </button>
          )}
        </div>
      )}

      {!hasConfig && (
        <>
          <div className="field">
            <span className="field-label">Dominios internos</span>
            <input
              value={domains}
              onChange={(event) => setDomains(event.target.value)}
              placeholder="tuempresa.com"
            />
          </div>
          <div className="field">
            <span className="field-label">Dominios ignorados</span>
            <input
              value={ignoredDomains}
              onChange={(event) => setIgnoredDomains(event.target.value)}
            />
          </div>
          <div className="field">
            <span className="field-label">Palabras ignoradas</span>
            <input
              value={ignoredKeywords}
              onChange={(event) => setIgnoredKeywords(event.target.value)}
            />
          </div>
        </>
      )}

      <button
        type="submit"
        className="btn-primary"
        disabled={loading || isNotReady}
        title={isNotReady ? "Completa la configuración para poder analizar" : undefined}
      >
        <Sparkles size={17} />
        Analizar
      </button>
    </form>
  );
}
