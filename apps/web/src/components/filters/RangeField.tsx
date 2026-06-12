import { CalendarDays, Clock } from "lucide-react";

type Props = {
  label: string;
  type: "date" | "time";
  fromValue: string;
  toValue: string;
  onFromChange: (value: string) => void;
  onToChange: (value: string) => void;
};

export function RangeField({ label, type, fromValue, toValue, onFromChange, onToChange }: Props) {
  const Icon = type === "date" ? CalendarDays : Clock;
  return (
    <div className="field">
      <span className="field-label">{label}</span>
      <div className={`range-pill range-${type}`}>
        <Icon size={16} className="range-icon" />
        <input type={type} aria-label={`${label} desde`} value={fromValue} onChange={(event) => onFromChange(event.target.value)} />
        <span className="range-sep">→</span>
        <input type={type} aria-label={`${label} hasta`} value={toValue} onChange={(event) => onToChange(event.target.value)} />
      </div>
    </div>
  );
}
