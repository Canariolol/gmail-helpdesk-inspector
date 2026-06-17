import { AlertTriangle, CheckCircle2, Sparkles } from "lucide-react";
import { useState } from "react";
import type { OrgConfig } from "../../api/types";
import { split } from "../../lib/format";
import { RangeField } from "./RangeField";

type Props = {
  loading: boolean;
  onAnalyze: (payload: unknown) => void;
  orgConfig?: OrgConfig | null;
  onGoToSetup?: () => void;
};

const today = new Date();
const dateFrom = new Date(today);
dateFrom.setDate(today.getDate() - 12);
const defaultDateFrom = dateFrom.toISOString().slice(0, 10);
const defaultDateTo = today.toISOString().slice(0, 10);

export function FilterBar({ loading, onAnalyze, orgConfig, onGoToSetup }: Props) {
  const [dateFromVal, setDateFrom] = useState(defaultDateFrom);
  const [dateToVal, setDateTo] = useState(defaultDateTo);
  const [timeFrom, setTimeFrom] = useState("00:00");
  const [timeTo, setTimeTo] = useState("23:59");

  // Legacy state — only used when orgConfig is not available
  const [domains, setDomains] = useState("");
  const [ignoredDomains, setIgnoredDomains] = useState("google.com,calendar.google.com");
  const [ignoredKeywords, setIgnoredKeywords] = useState("newsletter,boletín,promoción");

  const hasConfig = orgConfig != null;
  const isReady = hasConfig && orgConfig.setup_state.ready_for_analysis;
  const isNotReady = hasConfig && !orgConfig.setup_state.ready_for_analysis;

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();

    if (hasConfig) {
      // New mode: date window + optional policy version
      onAnalyze({
        date_from: dateFromVal,
        date_to: dateToVal,
        time_from: timeFrom,
        time_to: timeTo,
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
      });
    }
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
