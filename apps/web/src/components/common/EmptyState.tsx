import { Mail } from "lucide-react";
import type { LucideIcon } from "lucide-react";

type Props = {
  icon?: LucideIcon;
  message: string;
};

export function EmptyState({ icon: Icon = Mail, message }: Props) {
  return (
    <div className="empty-state">
      <span className="icon-chip tone-orange">
        <Icon size={20} />
      </span>
      <span>{message}</span>
    </div>
  );
}
