import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import type { ChipTone } from "../../lib/ui";

type Props = {
  icon: LucideIcon;
  label: string;
  value: ReactNode;
  tone: ChipTone;
  onClick?: () => void;
  /** Métrica protagonista: cifra en acento con el pulso Cadencia debajo. */
  featured?: boolean;
};

export function MetricCard({ icon: Icon, label, value, tone, onClick, featured }: Props) {
  const content = (
    <>
      <span className={`icon-chip tone-${tone}`}>
        <Icon size={18} />
      </span>
      <span className="metric-label">{label}</span>
      <strong className={featured ? "metric-value metric-value-featured" : "metric-value"}>{value}</strong>
      {featured && <span className="pulse-bar" aria-hidden="true" />}
    </>
  );
  if (onClick) {
    return (
      <button type="button" className="metric-card clickable" onClick={onClick}>
        {content}
      </button>
    );
  }
  return <div className="metric-card">{content}</div>;
}
