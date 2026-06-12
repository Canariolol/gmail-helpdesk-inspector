import type { ThreadDetail } from "../../api/types";
import { formatDateTime, formatDuration, formatReason, threadReceivedAt } from "../../lib/format";
import { ReviewForm } from "./ReviewForm";

type Props = {
  detail: ThreadDetail;
  onReview: (payload: unknown) => void;
  saving: boolean;
};

export function ThreadDetailPanel({ detail, onReview, saving }: Props) {
  return (
    <div className="card thread-detail">
      <h2>{detail.thread.subject}</h2>
      <div className="trace-summary">
        <span>
          <strong>Recepción</strong>
          {formatDateTime(threadReceivedAt(detail.thread))}
        </span>
        <span>
          <strong>1a respuesta</strong>
          {formatDateTime(detail.thread.first_internal_reply_at)} · {formatDuration(detail.thread.response_time_minutes)}
        </span>
        <span>
          <strong>Último envío</strong>
          {formatDateTime(detail.thread.last_internal_message_at)} · {formatDuration(detail.thread.resolution_time_minutes)}
        </span>
      </div>
      {detail.thread.reasons.length > 0 && (
        <div className="reason-box">
          {detail.thread.reasons.map((reason) => (
            <span key={reason}>{formatReason(reason)}</span>
          ))}
        </div>
      )}
      <div className="timeline">
        {detail.messages.map((message) => (
          <article key={message.id} className={message.is_internal ? "message internal" : "message external"}>
            <header>
              <strong>{message.from_email}</strong>
              <time>{formatDateTime(message.date)}</time>
            </header>
            <p>{message.snippet || "Sin snippet disponible"}</p>
          </article>
        ))}
      </div>
      <ReviewForm key={detail.thread.id} detail={detail} onReview={onReview} saving={saving} />
    </div>
  );
}
