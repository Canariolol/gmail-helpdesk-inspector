import { Ban, CheckCheck, CheckCircle2, ClipboardList, Clock, HelpCircle, History, Mail, Reply, ShieldCheck, Timer } from "lucide-react";
import { useMemo, useState } from "react";
import type { AnalysisRun } from "../api/types";
import { ReportTrendCharts } from "../components/charts/ReportTrendCharts";
import { EmptyState } from "../components/common/EmptyState";
import { MetricCard } from "../components/common/MetricCard";
import { RangeField } from "../components/filters/RangeField";
import { aggregateRuns, buildTrendPoints, selectReportRuns } from "../lib/aggregate";
import { formatDuration, formatPercent } from "../lib/format";

type Props = {
  runs: AnalysisRun[];
};

export function ReportesView({ runs }: Props) {
  const completedRuns = useMemo(() => runs.filter((run) => run.status === "completed"), [runs]);
  const [fromOverride, setFromOverride] = useState("");
  const [toOverride, setToOverride] = useState("");

  if (completedRuns.length === 0) {
    return (
      <div className="view">
        <div className="card">
          <EmptyState icon={History} message="Aún no hay análisis completados para generar reportes." />
        </div>
      </div>
    );
  }

  const defaultFrom = completedRuns.reduce(
    (min, run) => (run.config.date_from < min ? run.config.date_from : min),
    completedRuns[0].config.date_from,
  );
  const defaultTo = completedRuns.reduce(
    (max, run) => (run.config.date_to > max ? run.config.date_to : max),
    completedRuns[0].config.date_to,
  );
  const from = fromOverride || defaultFrom;
  const to = toOverride || defaultTo;

  const selectedRuns = selectReportRuns(completedRuns, from, to);
  const totals = aggregateRuns(selectedRuns);
  const points = buildTrendPoints(selectedRuns);

  return (
    <div className="view">
      <section className="card report-header">
        <div>
          <h2>Reporte consolidado</h2>
          <p className="muted-note">
            Resumen de los análisis completados que coinciden con el período seleccionado.
          </p>
        </div>
        <RangeField label="Periodo del reporte" type="date" fromValue={from} toValue={to} onFromChange={setFromOverride} onToChange={setToOverride} />
      </section>

      {selectedRuns.length === 0 ? (
        <div className="card">
          <EmptyState icon={History} message="No hay análisis completados dentro del rango seleccionado." />
        </div>
      ) : (
        <>
          <div className="metric-grid primary">
            <MetricCard icon={History} label="Análisis incluidos" value={totals.runCount} tone="blue" />
            <MetricCard icon={Mail} label="Total hilos" value={totals.totalThreads} tone="blue" />
            <MetricCard icon={CheckCircle2} label="Válidas" value={totals.validRequests} tone="mint" />
            <MetricCard icon={Reply} label="Respondidas" value={totals.answered} tone="teal" />
            <MetricCard icon={Clock} label="Sin respuesta" value={totals.unanswered} tone="orange" />
            <MetricCard icon={HelpCircle} label="Pendientes de revisión" value={totals.pendingReview} tone="amber" />
          </div>
          <div className="metric-grid primary">
            <MetricCard icon={Ban} label="Ignoradas" value={totals.ignored} tone="gray" />
            <MetricCard icon={ClipboardList} label="Correcciones manuales" value={totals.manualOverrides} tone="purple" />
            <MetricCard icon={ShieldCheck} label="Clasificación automática" value={formatPercent(totals.reportConfidence)} tone="purple" description="Porcentaje de conversaciones que Mira pudo clasificar sin pedir una revisión manual, considerando todos los análisis incluidos." />
            <MetricCard icon={Timer} label="T. medio respuesta" value={formatDuration(totals.avgFirstResponseMinutes)} tone="gray" />
            <MetricCard icon={CheckCheck} label="Cierre medio" value={formatDuration(totals.avgResolutionMinutes)} tone="gray" />
          </div>
          {selectedRuns.length >= 2 ? (
            <ReportTrendCharts points={points} />
          ) : (
            <div className="card">
              <p className="muted-note">Se necesitan al menos 2 análisis completados en el rango para mostrar tendencias.</p>
            </div>
          )}
        </>
      )}
    </div>
  );
}
