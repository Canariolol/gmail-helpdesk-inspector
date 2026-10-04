import { useEffect, useRef, useState } from "react";
import { AlertTriangle } from "lucide-react";
import type { Classification, ThreadDetail } from "../../api/types";
import { classificationLabels, useDateTimeFormat } from "../../lib/format";

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
  const formatDateTime = useDateTimeFormat();
  const suggestion = detail.thread.ai_suggestion;
  const [classification, setClassification] = useState<Classification>(suggestion?.classification ?? detail.thread.classification);
  const [answered, setAnswered] = useState(suggestion?.is_answered ?? detail.thread.is_answered);
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

  const knownId = (proposed: string | null | undefined, current: string | null) =>
    proposed && detail.messages.some((message) => message.id === proposed) ? proposed : current;
  const [firstClient, setFirstClient] = useState<string | null>(() => knownId(suggestion?.first_client_message_id, detail.thread.first_client_message_id));
  const [firstReply, setFirstReply] = useState<string | null>(() => knownId(suggestion?.first_internal_reply_message_id, detail.thread.first_internal_reply_message_id));
  const [lastInternal, setLastInternal] = useState<string | null>(() => knownId(suggestion?.last_internal_message_id, detail.thread.last_internal_message_id));

  const requesterMessages = detail.messages.filter((message) => !message.is_internal && !message.is_automated);
  const responderMessages = detail.messages.filter((message) => message.is_internal && !message.is_automated);
  const traceSelect = (label: string, value: string | null, change: (value: string | null) => void, messages: typeof detail.messages) => <label>
    {label}
    <select value={value ?? ""} onChange={(event) => change(event.target.value || null)}>
      <option value="">Sin mensaje seleccionado</option>
      {messages.map((message) => <option key={message.id} value={message.id}>{message.from_email} · {formatDateTime(message.date)} · {message.snippet.slice(0, 60)}</option>)}
    </select>
  </label>;

  return (
    <form
      className="review-form"
      onSubmit={(event) => {
        event.preventDefault();
        onReview({
          new_classification: classification,
          is_answered: answered,
          first_client_message_id: firstClient,
          first_internal_reply_message_id: answered ? firstReply : null,
          last_internal_message_id: answered ? lastInternal : null,
          notes: notes || null,
        });
      }}
    >
      <h3>Revisión manual</h3>
      {suggestion && (
        <p className="ai-suggestion">
          <AlertTriangle size={16} aria-hidden="true" />
          <span>
            Mira sugiere: <strong>{classificationLabels[suggestion.classification]} · {suggestion.is_answered ? "Respondido" : "Sin respuesta registrada"}</strong>. La propuesta no cuenta como confirmada hasta que guardes tu revisión.
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
      {traceSelect("Mensaje de solicitud", firstClient, setFirstClient, requesterMessages)}
      {answered && <>
        <p className="wizard-help">Elige envíos dirigidos al solicitante en Para o CC. Si faltan destinatarios en la línea de tiempo, confirma la entrega en tu casilla antes de guardar.</p>
        {traceSelect("Primera respuesta del equipo", firstReply, setFirstReply, responderMessages)}
        {traceSelect("Último envío del equipo", lastInternal, setLastInternal, responderMessages)}
      </>}
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
