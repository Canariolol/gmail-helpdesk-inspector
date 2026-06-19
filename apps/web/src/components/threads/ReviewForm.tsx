import { useEffect, useRef, useState } from "react";
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
  const [valid, setValid] = useState(detail.thread.is_valid_client_request);
  const [firstClient, setFirstClient] = useState(detail.thread.first_client_message_id ?? "");
  const [firstReply, setFirstReply] = useState(detail.thread.first_internal_reply_message_id ?? "");
  const [lastInternal, setLastInternal] = useState(detail.thread.last_internal_message_id ?? "");
  const [notes, setNotes] = useState("");
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

  const clientOptions = detail.messages.filter((message) => message.is_external && !message.is_automated);
  const internalOptions = detail.messages.filter((message) => message.is_internal && !message.is_automated);

  return (
    <form
      className="review-form"
      onSubmit={(event) => {
        event.preventDefault();
        onReview({
          new_classification: classification,
          is_valid_client_request: valid,
          is_answered: answered,
          first_client_message_id: firstClient || null,
          first_internal_reply_message_id: firstReply || null,
          last_internal_message_id: lastInternal || null,
          notes: notes || null,
        });
      }}
    >
      <h3>Revisión manual</h3>
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
        <input type="checkbox" checked={valid} onChange={(event) => setValid(event.target.checked)} />
        Solicitud válida
      </label>
      <label className="checkline">
        <input type="checkbox" checked={answered} onChange={(event) => setAnswered(event.target.checked)} />
        Respondido
      </label>
      <label>
        Primer mensaje cliente
        <select value={firstClient} onChange={(event) => setFirstClient(event.target.value)}>
          <option value="">Sin seleccionar</option>
          {clientOptions.map((message) => (
            <option key={message.id} value={message.id}>
              {message.from_email} · {formatDateTime(message.date)}
            </option>
          ))}
        </select>
      </label>
      <label>
        Primera respuesta interna
        <select value={firstReply} onChange={(event) => setFirstReply(event.target.value)}>
          <option value="">Sin seleccionar</option>
          {internalOptions.map((message) => (
            <option key={message.id} value={message.id}>
              {message.from_email} · {formatDateTime(message.date)}
            </option>
          ))}
        </select>
      </label>
      <label>
        Último envío interno
        <select value={lastInternal} onChange={(event) => setLastInternal(event.target.value)}>
          <option value="">Sin seleccionar</option>
          {internalOptions.map((message) => (
            <option key={message.id} value={message.id}>
              {message.from_email} · {formatDateTime(message.date)}
            </option>
          ))}
        </select>
      </label>
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
