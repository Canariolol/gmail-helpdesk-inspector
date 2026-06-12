import type { EmailThread } from "../../api/types";
import { formatDateTime, formatSource, threadReceivedAt } from "../../lib/format";
import { StatusBadge } from "../common/StatusBadge";
import { ThreadFilterSelect } from "./ThreadFilterSelect";

type Props = {
  threads: EmailThread[];
  filter: string;
  onFilter: (filter: string) => void;
  selectedThreadId: string | null;
  onSelect: (id: string) => void;
};

export function ThreadList(props: Props) {
  return (
    <section className="card thread-section">
      <div className="card-toolbar">
        <h2>Hilos auditables</h2>
        <ThreadFilterSelect value={props.filter} onChange={props.onFilter} />
      </div>
      <div className="thread-list">
        {props.threads.map((thread) => (
          <button
            type="button"
            key={thread.id}
            className={thread.id === props.selectedThreadId ? "thread-item active" : "thread-item"}
            onClick={() => props.onSelect(thread.id)}
          >
            <span className="subject">{thread.subject}</span>
            <span className="thread-item-meta">Recepción: {formatDateTime(threadReceivedAt(thread))}</span>
            <span className={thread.is_answered ? "chip tone-teal" : "chip tone-orange"}>
              {thread.is_answered ? "Respondido" : "Sin respuesta"}
            </span>
            <StatusBadge classification={thread.classification} />
            <span className="chip tone-gray">{thread.manual_review_required ? "Revisar" : formatSource(thread.classification_source)}</span>
          </button>
        ))}
        {props.threads.length === 0 && <p className="muted-note">No hay hilos para este filtro.</p>}
      </div>
    </section>
  );
}
