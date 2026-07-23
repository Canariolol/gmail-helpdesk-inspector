import { CalendarDays } from "lucide-react";
import type { AnalysisRun } from "../../api/types";
import { formatDateTime, formatPercent, runRangeLabel } from "../../lib/format";
import { RunStatusChip } from "../common/RunStatusChip";

type Props = {
  run: AnalysisRun;
  isActive: boolean;
  onSelect: (id: string) => void;
};

export function RunCard({ run, isActive, onSelect }: Props) {
  return (
    <button type="button" className={isActive ? "run-card active" : "run-card"} onClick={() => onSelect(run.id)}>
      <div className="run-card-head">
        <span className="run-card-range">
          <CalendarDays size={16} />
          {runRangeLabel(run)}
        </span>
        <RunStatusChip status={run.status} />
      </div>
      <div className="run-card-meta">
        <span>Creado: {formatDateTime(run.created_at)}</span>
        <span>
          {run.metrics.total_threads} hilos · {run.metrics.valid_requests} válidas · {run.metrics.answered} respondidas ·
          clasificación automática {formatPercent(run.metrics.report_confidence)}
        </span>
      </div>
    </button>
  );
}
