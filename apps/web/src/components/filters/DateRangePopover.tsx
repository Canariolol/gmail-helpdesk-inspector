import { CalendarDays, ChevronLeft, ChevronRight } from "lucide-react";
import { useState } from "react";
import {
  addDays,
  buildMonthGrid,
  compareYmd,
  formatDayLabel,
  monthTitle,
  parseYmd,
  todayInHelpdeskTz,
} from "./dateUtils";
import { FilterPopover } from "./FilterPopover";

const WEEKDAYS = ["lu", "ma", "mi", "ju", "vi", "sá", "do"];

type Props = {
  fromValue: string;
  toValue: string;
  onChange: (from: string, to: string) => void;
};

type Preset = { label: string; range: () => [string, string] };

const PRESETS: Preset[] = [
  { label: "Hoy", range: () => [todayInHelpdeskTz(), todayInHelpdeskTz()] },
  { label: "Ayer", range: () => [addDays(todayInHelpdeskTz(), -1), addDays(todayInHelpdeskTz(), -1)] },
  { label: "Últimos 7 días", range: () => [addDays(todayInHelpdeskTz(), -6), todayInHelpdeskTz()] },
  { label: "Últimos 30 días", range: () => [addDays(todayInHelpdeskTz(), -29), todayInHelpdeskTz()] },
];

export function DateRangePopover({ fromValue, toValue, onChange }: Props) {
  // `view` sigue al inicio del rango salvo que el usuario navegue meses (override).
  const [viewOverride, setViewOverride] = useState<{ year: number; month: number } | null>(null);
  // `anchor` marca el primer clic mientras se elige un rango nuevo.
  const [anchor, setAnchor] = useState<string | null>(null);
  const [hover, setHover] = useState<string | null>(null);

  const base = parseYmd(fromValue);
  const view = viewOverride ?? { year: base.year, month: base.month };
  const cells = buildMonthGrid(view.year, view.month);
  const today = todayInHelpdeskTz();

  function shiftMonth(delta: number) {
    const next = new Date(view.year, view.month - 1 + delta, 1);
    setViewOverride({ year: next.getFullYear(), month: next.getMonth() + 1 });
  }

  function handleDayClick(ymd: string) {
    if (anchor === null) {
      setAnchor(ymd);
      onChange(ymd, ymd);
      return;
    }
    const ordered = compareYmd(anchor, ymd) <= 0 ? [anchor, ymd] : [ymd, anchor];
    onChange(ordered[0], ordered[1]);
    setAnchor(null);
    setHover(null);
    setViewOverride(null);
  }

  function applyPreset(preset: Preset) {
    const [from, to] = preset.range();
    onChange(from, to);
    setAnchor(null);
    setHover(null);
    setViewOverride(null);
  }

  function isInRange(ymd: string): boolean {
    const start = anchor ?? fromValue;
    const end = anchor ? (hover ?? anchor) : toValue;
    const lo = compareYmd(start, end) <= 0 ? start : end;
    const hi = compareYmd(start, end) <= 0 ? end : start;
    return compareYmd(ymd, lo) >= 0 && compareYmd(ymd, hi) <= 0;
  }

  const rangeLabel =
    fromValue === toValue
      ? formatDayLabel(fromValue)
      : `${formatDayLabel(fromValue)} → ${formatDayLabel(toValue)}`;

  const triggerContent = (
    <>
      <CalendarDays size={16} className="fpop-icon" />
      <span className="fpop-value">{rangeLabel}</span>
    </>
  );

  return (
    <FilterPopover
      triggerLabel={`Rango de fechas: ${rangeLabel}`}
      triggerContent={triggerContent}
      panelClassName="cal-panel"
    >
      {() => (
        <div className="cal">
          <div className="cal-presets">
            {PRESETS.map((preset) => (
              <button
                type="button"
                key={preset.label}
                className="cal-preset"
                onClick={() => applyPreset(preset)}
              >
                {preset.label}
              </button>
            ))}
          </div>

          <div className="cal-head">
            <button type="button" className="cal-nav" aria-label="Mes anterior" onClick={() => shiftMonth(-1)}>
              <ChevronLeft size={16} />
            </button>
            <span className="cal-title">{monthTitle(view.year, view.month)}</span>
            <button type="button" className="cal-nav" aria-label="Mes siguiente" onClick={() => shiftMonth(1)}>
              <ChevronRight size={16} />
            </button>
          </div>

          <div className="cal-weekdays" aria-hidden="true">
            {WEEKDAYS.map((day) => (
              <span key={day}>{day}</span>
            ))}
          </div>

          <div className="cal-grid" onMouseLeave={() => setHover(null)}>
            {cells.map((cell) => {
              const selected = cell.ymd === fromValue || cell.ymd === toValue || cell.ymd === anchor;
              const classes = [
                "cal-day",
                cell.inMonth ? "" : "is-muted",
                isInRange(cell.ymd) ? "in-range" : "",
                selected ? "is-selected" : "",
                cell.ymd === today ? "is-today" : "",
              ]
                .filter(Boolean)
                .join(" ");
              return (
                <button
                  type="button"
                  key={cell.ymd}
                  className={classes}
                  onClick={() => handleDayClick(cell.ymd)}
                  onMouseEnter={() => setHover(cell.ymd)}
                >
                  {parseYmd(cell.ymd).day}
                </button>
              );
            })}
          </div>
        </div>
      )}
    </FilterPopover>
  );
}
