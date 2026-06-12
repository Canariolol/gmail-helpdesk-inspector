import { History } from "lucide-react";
import type { AnalysisRun } from "../api/types";
import { EmptyState } from "../components/common/EmptyState";
import { RunCard } from "../components/runs/RunCard";

type Props = {
  runs: AnalysisRun[];
  selectedRunId: string | null;
  onSelect: (id: string) => void;
};

export function RunsView({ runs, selectedRunId, onSelect }: Props) {
  return (
    <div className="view">
      <section className="card">
        <div className="card-toolbar">
          <h2>Análisis anteriores</h2>
        </div>
        {runs.length === 0 ? (
          <EmptyState icon={History} message="Todavía no has creado ningún análisis." />
        ) : (
          <div className="run-grid">
            {runs.map((run) => (
              <RunCard key={run.id} run={run} isActive={run.id === selectedRunId} onSelect={onSelect} />
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
