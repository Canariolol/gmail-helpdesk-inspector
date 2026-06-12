import { Sparkles } from "lucide-react";
import { useState } from "react";
import { split } from "../../lib/format";
import { RangeField } from "./RangeField";

type Props = {
  loading: boolean;
  onAnalyze: (payload: unknown) => void;
};

export function FilterBar({ loading, onAnalyze }: Props) {
  const [dateFrom, setDateFrom] = useState("2026-06-01");
  const [dateTo, setDateTo] = useState("2026-06-12");
  const [timeFrom, setTimeFrom] = useState("00:00");
  const [timeTo, setTimeTo] = useState("23:59");
  const [domains, setDomains] = useState("@west-ingenieria.cl");
  const [ignoredDomains, setIgnoredDomains] = useState("google.com,calendar.google.com");
  const [ignoredKeywords, setIgnoredKeywords] = useState("newsletter,boletín,promoción");

  return (
    <form
      className="filter-bar"
      onSubmit={(event) => {
        event.preventDefault();
        onAnalyze({
          date_from: dateFrom,
          date_to: dateTo,
          time_from: timeFrom,
          time_to: timeTo,
          timezone: "America/Santiago",
          internal_domains: split(domains),
          ignored_senders: [],
          ignored_domains: split(ignoredDomains),
          ignored_keywords: split(ignoredKeywords),
        });
      }}
    >
      <RangeField label="Fechas" type="date" fromValue={dateFrom} toValue={dateTo} onFromChange={setDateFrom} onToChange={setDateTo} />
      <RangeField label="Horario" type="time" fromValue={timeFrom} toValue={timeTo} onFromChange={setTimeFrom} onToChange={setTimeTo} />
      <div className="field">
        <span className="field-label">Dominios internos</span>
        <input value={domains} onChange={(event) => setDomains(event.target.value)} />
      </div>
      <div className="field">
        <span className="field-label">Dominios ignorados</span>
        <input value={ignoredDomains} onChange={(event) => setIgnoredDomains(event.target.value)} />
      </div>
      <div className="field">
        <span className="field-label">Palabras ignoradas</span>
        <input value={ignoredKeywords} onChange={(event) => setIgnoredKeywords(event.target.value)} />
      </div>
      <button type="submit" className="btn-primary" disabled={loading}>
        <Sparkles size={17} />
        Analizar
      </button>
    </form>
  );
}
