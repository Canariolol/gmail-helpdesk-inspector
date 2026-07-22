import type { EmailThread, ThreadDetail } from "../api/types";
import { EmptyState } from "../components/common/EmptyState";
import { ThreadDetailRail } from "../components/threads/ThreadDetailRail";
import { ThreadTable } from "../components/threads/ThreadTable";
import { filterThreads } from "../lib/threads";

type Props = {
  threads: EmailThread[];
  filter: string;
  onFilter: (filter: string) => void;
  forcedFilter?: "review";
  selectedThreadId: string | null;
  onSelectThread: (id: string) => void;
  onCloseThread: () => void;
  detail: ThreadDetail | undefined;
  onReview: (payload: unknown) => void;
  savingReview: boolean;
  reviewError: string | null;
  reviewSavedAt: number | null;
  hasRun: boolean;
};

export function HilosView(props: Props) {
  const effectiveFilter = props.forcedFilter ?? props.filter;
  const visibleThreads = filterThreads(props.threads, effectiveFilter);
  const title = props.forcedFilter === "review" ? "Revisión manual" : "Conversaciones";

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
        <ThreadDetailRail
          detail={props.detail}
          open={Boolean(props.selectedThreadId)}
          onClose={props.onCloseThread}
          onReview={props.onReview}
          saving={props.savingReview}
          reviewError={props.reviewError}
          reviewSavedAt={props.reviewSavedAt}
        />
      </div>
    </div>
  );
}
