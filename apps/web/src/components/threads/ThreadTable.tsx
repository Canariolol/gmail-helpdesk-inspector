import type { EmailThread } from "../../api/types";
import { formatDateTime, formatSource, threadReceivedAt } from "../../lib/format";
import { StatusBadge } from "../common/StatusBadge";
import { ThreadFilterSelect } from "./ThreadFilterSelect";

type Props = {
  threads: EmailThread[];
  filter: string;
  onFilter: (filter: string) => void;
  filterLocked?: boolean;
  selectedThreadId: string | null;
  onSelect: (id: string) => void;
  title: string;
};

export function ThreadTable(props: Props) {
  return (
    <section className="card thread-section">
      <div className="card-toolbar">
        <h2>{props.title}</h2>
        {!props.filterLocked && <ThreadFilterSelect value={props.filter} onChange={props.onFilter} />}
      </div>
      <div className="thread-table">
        <div className="thread-row header">
          <span>Asunto</span>
          <span>Recepción</span>
          <span>1a respuesta</span>
          <span>Último envío</span>
          <span>Estado</span>
          <span>Revisión</span>
        </div>
        {props.threads.map((thread) => (
          <button
            type="button"
            key={thread.id}
            className={thread.id === props.selectedThreadId ? "thread-row active" : "thread-row"}
            onClick={() => props.onSelect(thread.id)}
          >
            <span className="subject">{thread.subject}</span>
            <span>{formatDateTime(threadReceivedAt(thread))}</span>
            <span>{formatDateTime(thread.first_internal_reply_at)}</span>
            <span>{formatDateTime(thread.last_internal_message_at)}</span>
            <span>
              <StatusBadge classification={thread.classification} />
            </span>
            <span>
              {thread.manual_review_required ? (
                <span className="chip tone-orange">Revisar</span>
              ) : (
                <span className="chip tone-gray">{formatSource(thread.classification_source)}</span>
              )}
            </span>
          </button>
        ))}
        {props.threads.length === 0 && <p className="muted-note">No hay hilos para este filtro.</p>}
      </div>
    </section>
  );
}
