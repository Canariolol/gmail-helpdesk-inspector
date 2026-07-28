import { Clock } from "lucide-react";
import { useEffect, useRef } from "react";
import { FilterPopover } from "./FilterPopover";

type Props = {
  fromValue: string;
  toValue: string;
  onChange: (from: string, to: string) => void;
};

// Opciones cada 30 min, más el cierre de día "23:59".
function buildTimeOptions(): string[] {
  const options: string[] = [];
  for (let hour = 0; hour < 24; hour += 1) {
    for (const minute of [0, 30]) {
      options.push(`${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`);
    }
  }
  options.push("23:59");
  return options;
}

const TIME_OPTIONS = buildTimeOptions();

const PRESETS = [
  { label: "Todo el día", from: "00:00", to: "23:59" },
  { label: "Jornada laboral", from: "09:00", to: "18:00" },
  { label: "Mañana", from: "06:00", to: "12:00" },
  { label: "Tarde", from: "12:00", to: "18:00" },
];

type ColumnProps = {
  title: string;
  value: string;
  onSelect: (value: string) => void;
};

function TimeColumn({ title, value, onSelect }: ColumnProps) {
  const listRef = useRef<HTMLUListElement>(null);

  // Centra la opción activa al abrir, ajustando solo el scroll del contenedor
  // (no de la página). Solo al montar.
  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const index = TIME_OPTIONS.indexOf(value);
    if (index < 0) return;
    const item = list.children[index] as HTMLElement | undefined;
    if (item) {
      list.scrollTop = item.offsetTop - list.clientHeight / 2 + item.clientHeight / 2;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- solo al montar el panel
  }, []);

  return (
    <div className="timecol">
      <span className="timecol-title">{title}</span>
      <ul className="timecol-list" ref={listRef}>
        {TIME_OPTIONS.map((option) => (
          <li key={option}>
            <button
              type="button"
              className={`timecol-opt${option === value ? " is-active" : ""}`}
              aria-current={option === value}
              onClick={() => onSelect(option)}
            >
              {option}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function TimeRangePopover({ fromValue, toValue, onChange }: Props) {
  const triggerContent = (
    <>
      <Clock size={16} className="fpop-icon" />
      <span className="fpop-value">
        {fromValue} <span className="fpop-sep">→</span> {toValue}
      </span>
    </>
  );

  return (
    <FilterPopover
      triggerLabel={`Horario: de ${fromValue} a ${toValue}`}
      triggerContent={triggerContent}
      panelClassName="time-panel"
    >
      {(close) => (
        <div className="timepick">
          <div className="timepick-presets">
            {PRESETS.map((preset) => (
              <button
                type="button"
                key={preset.label}
                className="cal-preset"
                onClick={() => {
                  onChange(preset.from, preset.to);
                  close();
                }}
              >
                {preset.label}
              </button>
            ))}
          </div>
          <div className="timepick-cols">
            <TimeColumn title="Desde" value={fromValue} onSelect={(value) => onChange(value, toValue)} />
            <TimeColumn title="Hasta" value={toValue} onSelect={(value) => onChange(fromValue, value)} />
          </div>
        </div>
      )}
    </FilterPopover>
  );
}
