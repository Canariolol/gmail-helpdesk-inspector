import { CheckCircle2, Sparkles, Tags } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { FilterPreset, OrgConfig } from "../../api/types";
import { split } from "../../lib/format";
import { DateRangePopover } from "./DateRangePopover";
import { todayInHelpdeskTz } from "./dateUtils";
import { LabelPickerModal } from "./LabelPickerModal";
import { INBOX_TOKEN, tokenDisplayName } from "./labels";
import { SavedFiltersPopover } from "./SavedFiltersPopover";
import { TimeRangePopover } from "./TimeRangePopover";

type Props = {
  loading: boolean;
  onAnalyze: (payload: unknown) => void;
  orgConfig?: OrgConfig | null;
  filterPresets?: FilterPreset[];
  onSavePreset?: (payload: unknown) => void;
  onDeletePreset?: (id: string) => void;
};

export function FilterBar({
  loading,
  onAnalyze,
  orgConfig,
  filterPresets = [],
  onSavePreset,
  onDeletePreset,
}: Props) {
  // El selector arranca siempre en la fecha actual (hoy), recalculada en cada montaje.
  const [dateFromVal, setDateFrom] = useState(todayInHelpdeskTz);
  const [dateToVal, setDateTo] = useState(todayInHelpdeskTz);
  const [timeFrom, setTimeFrom] = useState("00:00");
  const [timeTo, setTimeTo] = useState("23:59");

  // Selección de etiquetas de Gmail a incluir en la recuperación.
  const [includeLabels, setIncludeLabels] = useState<string[]>([]);
  // Exclusión negativa: se conserva para round-trip de presets antiguos; la UI
  // nueva trabaja solo con inclusión.
  const [excludeLabels, setExcludeLabels] = useState<string[]>([]);
  const [labelModalOpen, setLabelModalOpen] = useState(false);

  // Presets guardados
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

  // INBOX va incluida por defecto cuando llega la metadata del buzón, salvo que
  // exista un preset por defecto (ese manda). El usuario puede quitarla luego.
  const seededInbox = useRef(false);
  useEffect(() => {
    if (seededInbox.current || labels.length === 0) return;
    seededInbox.current = true;
    if (filterPresets.some((preset) => preset.is_default)) return;
    if (labels.some((label) => label.id.toUpperCase() === INBOX_TOKEN)) {
      setIncludeLabels((current) => (current.length === 0 ? [INBOX_TOKEN] : current));
    }
  }, [labels, filterPresets]);

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

  function handleSavePreset(name: string) {
    if (!onSavePreset) return;
    onSavePreset({
      name,
      include_labels: includeLabels,
      exclude_labels: excludeLabels,
      ignored_senders: [],
      ignored_domains: hasConfig ? [] : split(ignoredDomains),
      ignored_keywords: hasConfig ? [] : split(ignoredKeywords),
      is_default: false,
    });
  }

  return (
    <form className="filter-bar" onSubmit={handleSubmit}>
      <DateRangePopover
        fromValue={dateFromVal}
        toValue={dateToVal}
        onChange={(from, to) => {
          setDateFrom(from);
          setDateTo(to);
        }}
      />
      <TimeRangePopover
        fromValue={timeFrom}
        toValue={timeTo}
        onChange={(from, to) => {
          setTimeFrom(from);
          setTimeTo(to);
        }}
      />

      {labels.length > 0 && (
        <button
          type="button"
          className="label-picker-trigger"
          aria-label="Bandejas y etiquetas a analizar"
          onClick={() => setLabelModalOpen(true)}
        >
          <Tags size={16} />
          <span className="lp-trigger-text">
            {includeLabels.length === 0
              ? "Recibidos"
              : includeLabels.map((token) => tokenDisplayName(token, labels)).join(", ")}
          </span>
          <span className="lp-trigger-count">{includeLabels.length}</span>
        </button>
      )}

      <LabelPickerModal
        open={labelModalOpen}
        labels={labels}
        includedTokens={includeLabels}
        onChange={setIncludeLabels}
        onClose={() => setLabelModalOpen(false)}
      />

      {(filterPresets.length > 0 || onSavePreset) && (
        <SavedFiltersPopover
          presets={filterPresets}
          selectedId={selectedPresetId}
          onApply={applyPreset}
          onClear={() => setSelectedPresetId("")}
          onSave={onSavePreset ? handleSavePreset : undefined}
          onDelete={
            onDeletePreset
              ? (id) => {
                  onDeletePreset(id);
                  if (id === selectedPresetId) setSelectedPresetId("");
                }
              : undefined
          }
        />
      )}

      {/* Mode indicator or legacy fields */}
      {isReady && (
        <div className="filter-bar-policy">
          <CheckCircle2 size={14} />
          <span>
            Configuración lista
            {" · "}
            {orgConfig.mailbox.workspace_domain}
          </span>
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
