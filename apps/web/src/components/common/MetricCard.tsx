import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import type { ChipTone } from "../../lib/ui";

type Props = {
  icon: LucideIcon;
  label: string;
  value: ReactNode;
  tone: ChipTone;
  onClick?: () => void;
};

export function MetricCard({ icon: Icon, label, value, tone, onClick }: Props) {
  const content = (
    <>
      <span className={`icon-chip tone-${tone}`}>
        <Icon size={18} />
      </span>
      <span className="metric-label">{label}</span>
      <strong className="metric-value">{value}</strong>
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
