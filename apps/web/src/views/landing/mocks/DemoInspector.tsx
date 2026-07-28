import { Check, ChevronLeft, MousePointerClick } from "lucide-react";
import { useEffect, useState } from "react";
import {
  DEFAULT_DEMO_METRIC,
  DEMO_CLASS_LABELS,
  DEMO_CLASS_TONES,
  DEMO_METRICS,
  DEMO_THREADS_BY_METRIC,
  type DemoClassification,
  type DemoMetricId,
  type DemoThread,
} from "./demoData";

export function DemoInspector() {
  const [metricId, setMetricId] = useState<DemoMetricId>(DEFAULT_DEMO_METRIC);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const threads = DEMO_THREADS_BY_METRIC[metricId];
  const selected = threads.find((thread) => thread.id === selectedId) ?? null;

  const handleMetric = (id: DemoMetricId) => {
    setMetricId(id);
    setSelectedId(null);
  };

  return (
    <div className="lp-di">
      <header className="lp-di-bar">
        <div className="lp-di-dots">
          <span /> <span /> <span />
        </div>
        <span className="lp-di-bar-title">Resumen · últimos 7 días</span>
        <span className="lp-di-chip lp-tone-mint">Completado</span>
      </header>

      <div className="lp-di-metrics" role="tablist" aria-label="Métricas">
        {DEMO_METRICS.map((metric) => (
          <button
            key={metric.id}
            type="button"
            role="tab"
            aria-selected={metric.id === metricId}
            className={`lp-di-metric lp-tone-${metric.tone}${metric.id === metricId ? " is-active" : ""}`}
            onClick={() => handleMetric(metric.id)}
          >
            <metric.Icon size={15} />
            <strong>{metric.value}</strong>
            <span>{metric.label}</span>
          </button>
        ))}
      </div>

      <div className="lp-di-body">
        <div className="lp-di-list" aria-label="Correos analizados">
          {threads.map((thread) => (
            <button
              key={thread.id}
              type="button"
              className={`lp-di-row${thread.id === selectedId ? " is-active" : ""}`}
              onClick={() => setSelectedId(thread.id)}
            >
              <span className="lp-di-row-subject">{thread.subject}</span>
              <span className="lp-di-row-date">{thread.received}</span>
              <span className={`lp-di-chip lp-tone-${DEMO_CLASS_TONES[thread.classification]}`}>
                {DEMO_CLASS_LABELS[thread.classification]}
              </span>
            </button>
          ))}
        </div>

        <div className="lp-di-panel">
          {selected ? (
            <DemoDetail key={selected.id} thread={selected} />
          ) : (
            <div className="lp-di-empty">
              <span className="lp-di-arrow" aria-hidden="true">
                <ChevronLeft size={26} />
              </span>
              <MousePointerClick size={26} aria-hidden="true" />
              <p>Selecciona un correo para ver su trazabilidad.</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

type SaveState = "idle" | "filling" | "done";

function DemoDetail({ thread }: { thread: DemoThread }) {
  const [classification, setClassification] = useState<DemoClassification>(thread.classification);
  const [valid, setValid] = useState(thread.valid);
  const [answered, setAnswered] = useState(thread.answered);
  const [note, setNote] = useState("");
  const [saveState, setSaveState] = useState<SaveState>("idle");

  useEffect(() => {
    if (saveState === "filling") {
      const timer = setTimeout(() => setSaveState("done"), 620);
      return () => clearTimeout(timer);
    }
    if (saveState === "done") {
      const timer = setTimeout(() => setSaveState("idle"), 1300);
      return () => clearTimeout(timer);
    }
  }, [saveState]);

  return (
    <div className="lp-di-detail">
      <div className="lp-di-detail-head">
        <h4>{thread.subject}</h4>
        <span className={`lp-di-chip lp-tone-${DEMO_CLASS_TONES[classification]}`}>
          {DEMO_CLASS_LABELS[classification]}
        </span>
      </div>

      <div className="lp-di-trace">
        <span>
          <strong>Recepción</strong>
          {thread.received}
        </span>
        <span>
          <strong>1ª respuesta</strong>
          {thread.firstReply ?? "Sin respuesta aún"}
        </span>
        <span>
          <strong>Último envío</strong>
          {thread.lastSent ?? "—"}
        </span>
      </div>

      {thread.reasons && thread.reasons.length > 0 && (
        <div className="lp-di-reasons">
          {thread.reasons.map((reason) => (
            <span key={reason}>{reason}</span>
          ))}
        </div>
      )}

      <div className="lp-di-timeline">
        {thread.messages.map((message, index) => (
          <article
            key={index}
            className={`lp-di-msg ${message.internal ? "is-internal" : "is-external"}`}
          >
            <header>
              <strong>{message.from}</strong>
              <time>{message.date}</time>
            </header>
            <p>{message.snippet}</p>
          </article>
        ))}
      </div>

      <form
        className="lp-di-form"
        onSubmit={(event) => {
          event.preventDefault();
          if (saveState === "idle") setSaveState("filling");
        }}
      >
        <span className="lp-di-form-title">Revisión manual</span>
        <label>
          Clasificación
          <select
            value={classification}
            onChange={(event) => {
              setClassification(event.target.value as DemoClassification);
            }}
          >
            {Object.entries(DEMO_CLASS_LABELS).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label className="lp-di-check">
          <input
            type="checkbox"
            checked={valid}
            onChange={(event) => {
              setValid(event.target.checked);
            }}
          />
          Solicitud válida
        </label>
        <label className="lp-di-check">
          <input
            type="checkbox"
            checked={answered}
            onChange={(event) => {
              setAnswered(event.target.checked);
            }}
          />
          Respondido
        </label>
        <label>
          Nota
          <textarea
            value={note}
            rows={2}
            placeholder="Añadir nota…"
            onChange={(event) => {
              setNote(event.target.value);
            }}
          />
        </label>
        <div className="lp-di-save-row">
          <button
            type="submit"
            className={`lp-di-save${saveState !== "idle" ? " is-busy" : ""}`}
            disabled={saveState !== "idle"}
          >
            <span className="lp-di-save-fill" aria-hidden="true" />
            <span className="lp-di-save-text">Guardar revisión</span>
          </button>
          <span
            className={`lp-di-savecheck${saveState === "done" ? " is-shown" : ""}`}
            aria-hidden={saveState !== "done"}
          >
            <Check size={18} />
          </span>
        </div>
      </form>
    </div>
  );
}
