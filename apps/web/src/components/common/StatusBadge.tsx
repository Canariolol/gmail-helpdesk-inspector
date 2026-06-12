import type { Classification } from "../../api/types";
import { classificationLabels } from "../../lib/format";
import { classificationTones } from "../../lib/ui";

export function StatusBadge({ classification }: { classification: Classification }) {
  return <span className={`chip tone-${classificationTones[classification]}`}>{classificationLabels[classification]}</span>;
}
