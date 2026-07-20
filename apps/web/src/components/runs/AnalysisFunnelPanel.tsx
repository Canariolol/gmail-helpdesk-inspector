import { useState } from "react";
import { Bot, ChevronDown, Clock, Filter, Inbox, ListChecks } from "lucide-react";
import type { AnalysisRun, DroppedThreadInfo } from "../../api/types";
import { formatDateTime } from "../../lib/format";

const DROP_REASON_LABELS: Record<string, string> = {
  dropped_not_primary_inbox: "Fuera de la pestaña Principal",
  dropped_no_external_in_window: "Sin actividad del cliente en la ventana",
};

const DROP_REASON_TONE: Record<string, string> = {
  dropped_not_primary_inbox: "tone-amber",
  dropped_no_external_in_window: "tone-orange",
};

type Props = {
  run: AnalysisRun;
};

// Hace visible la pérdida silenciosa: los hilos descartados antes de clasificar
// no se guardan, así que sin este panel el usuario no tenía forma de ver por qué
// el total analizado es menor que lo encontrado en la casilla.
export function AnalysisFunnelPanel({ run }: Props) {
  const [open, setOpen] = useState(false);
  const funnel = run.metrics.funnel;
  if (!funnel) return null;

  const droppedNotPrimary = funnel.dropped_not_primary_inbox;
  const droppedWindow = funnel.dropped_no_external_in_window;
  const skippedByPlan = funnel.skipped_by_plan_cap ?? 0;
  const totalDropped = droppedNotPrimary + droppedWindow + skippedByPlan;
  const aiBatch = funnel.ai_batch_classified ?? 0;
  const aiUnique = funnel.ai_unique_threads ?? 0;
  if (totalDropped === 0 && aiUnique === 0) return null;

  const candidates = run.total_candidate_threads;
  const analyzed = run.metrics.total_threads;

  return (
    <section className="funnel-panel">
      <button
        type="button"
        className="funnel-head"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
      >
        <span className="funnel-kicker">
          <Filter size={16} />
          Embudo del análisis
        </span>
        <span className="funnel-subtitle">
          Se encontraron {candidates}; se analizaron {analyzed} y {aiUnique} requirieron revisión con IA.
        </span>
        <ChevronDown size={18} className={open ? "funnel-chevron open" : "funnel-chevron"} />
      </button>

      {open && (
        <div className="funnel-body">
          <div className="funnel-stages">
            <FunnelStat icon={Inbox} label="Encontrados en la casilla" value={candidates} tone="tone-blue" />
            <FunnelStat
              icon={Filter}
              label="Fuera de la bandeja principal"
              value={droppedNotPrimary}
              tone="tone-amber"
            />
            <FunnelStat
              icon={Clock}
              label="Sin actividad del cliente en la ventana"
              value={droppedWindow}
              tone="tone-orange"
            />
            <FunnelStat icon={ListChecks} label="Analizados" value={analyzed} tone="tone-mint" />
            <FunnelStat icon={Bot} label="Auditados con Mira" value={aiBatch} tone="tone-violet" />
          </div>

          {skippedByPlan > 0 && (
            <p className="funnel-hint">
              {skippedByPlan} hilos analizables quedaron fuera por el límite del plan
              {funnel.more_beyond_retrieved ? "; el proveedor además indicó que había más resultados" : ""}.
            </p>
          )}

          {droppedWindow > 0 && (
            <p className="funnel-hint">
              Estos correos llegaron fuera del rango de fechas u horas analizado. Si buscabas correos
              de otro día, ajusta el rango arriba y vuelve a analizar.
            </p>
          )}

          {funnel.dropped_samples.length > 0 && (
            <div className="funnel-dropped">
              <h4>Hilos que quedaron fuera</h4>
              <ul>
                {funnel.dropped_samples.map((dropped) => (
                  <DroppedRow key={dropped.thread_id} dropped={dropped} />
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </section>
  );
}

type FunnelStatProps = {
  icon: typeof Inbox;
  label: string;
  value: number;
  tone: string;
};

function FunnelStat({ icon: Icon, label, value, tone }: FunnelStatProps) {
  return (
    <div className="funnel-stat">
      <span className={`icon-chip ${tone}`}>
        <Icon size={16} />
      </span>
      <strong>{value}</strong>
      <span className="muted-note">{label}</span>
    </div>
  );
}

function DroppedRow({ dropped }: { dropped: DroppedThreadInfo }) {
  const tone = DROP_REASON_TONE[dropped.reason] ?? "tone-gray";
  const label = DROP_REASON_LABELS[dropped.reason] ?? dropped.reason;
  return (
    <li>
      <span className="funnel-dropped-subject">{dropped.subject}</span>
      <span className="muted-note">{formatDateTime(dropped.first_message_at)}</span>
      <span className={`chip ${tone}`}>{label}</span>
    </li>
  );
}
