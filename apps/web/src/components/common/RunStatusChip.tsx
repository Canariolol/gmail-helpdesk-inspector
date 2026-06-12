import type { AnalysisStatus } from "../../api/types";
import { statusLabels } from "../../lib/format";
import { statusTones } from "../../lib/ui";

export function RunStatusChip({ status }: { status: AnalysisStatus }) {
  return <span className={`chip tone-${statusTones[status]}`}>{statusLabels[status]}</span>;
}
