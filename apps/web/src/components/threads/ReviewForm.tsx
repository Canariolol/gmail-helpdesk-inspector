import { useEffect, useRef, useState } from "react";
import { AlertTriangle } from "lucide-react";
import type { Classification, ThreadDetail } from "../../api/types";
import { classificationLabels, formatDateTime } from "../../lib/format";

type Props = {
  detail: ThreadDetail;
  onReview: (payload: unknown) => void;
  saving: boolean;
  reviewError: string | null;
  reviewSavedAt: number | null;
};

// El padre monta este formulario con key={thread.id}, así el estado se
// reinicia al cambiar de hilo sin efectos de sincronización.
export function ReviewForm({ detail, onReview, saving, reviewError, reviewSavedAt }: Props) {
  const [classification, setClassification] = useState<Classification>(detail.thread.classification);
  const [answered, setAnswered] = useState(detail.thread.is_answered);
  const [notes, setNotes] = useState(detail.thread.notes ?? "");
  const [savedFlash, setSavedFlash] = useState(false);

  // reviewSavedAt cambia con cada guardado exitoso. Como el form se remonta por
  // hilo (key=thread.id), el ref evita un flash falso al abrir un hilo nuevo
  // después de haber guardado en otro: la primera corrida del efecto se ignora.
  const firstSavedRun = useRef(true);
  useEffect(() => {
    if (firstSavedRun.current) {
      firstSavedRun.current = false;
      return;
    }
    setSavedFlash(true);
    const timer = setTimeout(() => setSavedFlash(false), 4000);
    return () => clearTimeout(timer);
  }, [reviewSavedAt]);

  // Los mensajes de traza no se eligen: salen del análisis de la casilla. Se
  // muestran como dato y se reenvían tal cual para no borrarlos al guardar.
  const firstClient = detail.thread.first_client_message_id ?? null;
  const firstReply = detail.thread.first_internal_reply_message_id ?? null;
  const lastInternal = detail.thread.last_internal_message_id ?? null;

  const describe = (messageId: string | null) => {
    const message = detail.messages.find((item) => item.id === messageId);
    if (!message) return "—";
    return `${message.from_email} · ${formatDateTime(message.date)}`;
  };

  return (
    <form
      className="review-form"
      onSubmit={(event) => {
        event.preventDefault();
        onReview({
          new_classification: classification,
          is_answered: answered,
          first_client_message_id: firstClient,
          first_internal_reply_message_id: firstReply,
          last_internal_message_id: lastInternal,
          notes: notes || null,
        });
      }}
    >
      <h3>Revisión manual</h3>
      {detail.thread.manual_review_required && detail.thread.classification_source === "ai" && (
        <p className="ai-suggestion">
          <AlertTriangle size={16} aria-hidden="true" />
          <span>
            Mira sugiere estado: <strong>{classificationLabels[detail.thread.classification]} · {detail.thread.is_answered ? "Respondido" : "Sin respuesta"}</strong>
          </span>
        </p>
      )}
      <label>
        Clasificación
        <select value={classification} onChange={(event) => setClassification(event.target.value as Classification)}>
          {Object.entries(classificationLabels).map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </label>
      <label className="checkline">
        <input type="checkbox" checked={answered} onChange={(event) => setAnswered(event.target.checked)} />
        Respondido
      </label>
      <dl className="review-trace">
        <div>
          <dt>Primer mensaje cliente</dt>
          <dd>{describe(firstClient)}</dd>
        </div>
        <div>
          <dt>Primera respuesta interna</dt>
          <dd>{describe(firstReply)}</dd>
        </div>
        <div>
          <dt>Último envío interno</dt>
          <dd>{describe(lastInternal)}</dd>
        </div>
      </dl>
      <label>
        Nota
        <textarea value={notes} onChange={(event) => setNotes(event.target.value)} placeholder="Añadir nota..." />
      </label>
      {reviewError && (
        <p className="review-status is-error" role="alert">
          No se pudo guardar la revisión: {reviewError}
        </p>
      )}
      {savedFlash && !saving && !reviewError && (
        <p className="review-status is-success" role="status">
          ✓ Revisión guardada. El estado del hilo se actualizó.
        </p>
      )}
      <button type="submit" className="btn-primary" disabled={saving}>
        {saving ? "Guardando…" : "Guardar revisión"}
      </button>
    </form>
  );
}
