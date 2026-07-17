import type { ThreadDetail } from "../../api/types";
import { formatDateTime, formatDuration, formatReason, threadReceivedAt } from "../../lib/format";
import { StatusBadge } from "../common/StatusBadge";
import { ReviewForm } from "./ReviewForm";

type Props = {
  detail: ThreadDetail;
  onReview: (payload: unknown) => void;
  saving: boolean;
  reviewError: string | null;
  reviewSavedAt: number | null;
};

export function ThreadDetailPanel({ detail, onReview, saving, reviewError, reviewSavedAt }: Props) {
  return (
    <div className="card thread-detail">
      <div className="thread-detail-head">
        <h2>{detail.thread.subject}</h2>
        <div className="thread-detail-tags">
          <StatusBadge classification={detail.thread.classification} />
          {detail.thread.manual_override_applied && (
            <span className="chip tone-teal">Revisión manual aplicada</span>
          )}
        </div>
      </div>
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
          <h3>Observaciones de Mira</h3>
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
      <ReviewForm
        key={detail.thread.id}
        detail={detail}
        onReview={onReview}
        saving={saving}
        reviewError={reviewError}
        reviewSavedAt={reviewSavedAt}
      />
    </div>
  );
}
