import type { EmailThread, ThreadDetail } from "../api/types";
import { EmptyState } from "../components/common/EmptyState";
import { ThreadDetailPanel } from "../components/threads/ThreadDetailPanel";
import { ThreadTable } from "../components/threads/ThreadTable";
import { filterThreads } from "../lib/threads";

type Props = {
  threads: EmailThread[];
  filter: string;
  onFilter: (filter: string) => void;
  forcedFilter?: "review";
  selectedThreadId: string | null;
  onSelectThread: (id: string) => void;
  detail: ThreadDetail | undefined;
  onReview: (payload: unknown) => void;
  savingReview: boolean;
  hasRun: boolean;
};

export function HilosView(props: Props) {
  const effectiveFilter = props.forcedFilter ?? props.filter;
  const visibleThreads = filterThreads(props.threads, effectiveFilter);
  const title = props.forcedFilter === "review" ? "Revisión manual" : "Hilos auditables";

  if (!props.hasRun) {
    return (
      <div className="view">
        <div className="card">
          <EmptyState message="Aún no hay un análisis seleccionado. Crea uno desde Resumen." />
        </div>
      </div>
    );
  }

  return (
    <div className="view">
      <div className="split-view">
        <ThreadTable
          title={title}
          threads={visibleThreads}
          filter={effectiveFilter}
          onFilter={props.onFilter}
          filterLocked={Boolean(props.forcedFilter)}
          selectedThreadId={props.selectedThreadId}
          onSelect={props.onSelectThread}
        />
        <aside className="detail-rail">
          {props.detail ? (
            <ThreadDetailPanel detail={props.detail} onReview={props.onReview} saving={props.savingReview} />
          ) : (
            <div className="card">
              <EmptyState message="Selecciona un hilo para revisar la trazabilidad." />
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}
