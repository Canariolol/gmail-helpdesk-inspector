import { Ban, CheckCheck, CheckCircle2, ClipboardList, Clock, HelpCircle, History, Mail, Reply, ShieldCheck, Timer } from "lucide-react";
import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { AnalysisRun, ConsolidatedReport } from "../api/types";
import { ReportTrendCharts } from "../components/charts/ReportTrendCharts";
import { EmptyState } from "../components/common/EmptyState";
import { MetricCard } from "../components/common/MetricCard";
import { RangeField } from "../components/filters/RangeField";
import { buildTrendPoints, selectReportRuns } from "../lib/aggregate";
import { formatDuration, formatPercent } from "../lib/format";

type Props = {
  runs: AnalysisRun[];
};

export function ReportesView({ runs }: Props) {
  const completedRuns = useMemo(() => runs.filter((run) => run.status === "completed"), [runs]);
  const [fromOverride, setFromOverride] = useState("");
  const [toOverride, setToOverride] = useState("");

  const defaultFrom = completedRuns.reduce(
    (min, run) => (run.config.date_from < min ? run.config.date_from : min),
    completedRuns[0]?.config.date_from ?? "",
  );
  const defaultTo = completedRuns.reduce(
    (max, run) => (run.config.date_to > max ? run.config.date_to : max),
    completedRuns[0]?.config.date_to ?? "",
  );
  const from = fromOverride || defaultFrom;
  const to = toOverride || defaultTo;

  const selectedRuns = selectReportRuns(completedRuns, from, to);
  const hasIncompleteCoverage = selectedRuns.some(({ metrics }) => {
    const funnel = metrics.funnel;
    return funnel && ((funnel.failed_threads ?? 0) > 0 || (funnel.truncated_threads ?? 0) > 0 || funnel.truncated_by_plan || funnel.more_beyond_retrieved);
  });
  const points = buildTrendPoints(selectedRuns);
  const report = useQuery({
    queryKey: ["consolidated-report", from, to, completedRuns.map((run) => `${run.id}:${run.completed_at}`).join(",")],
    queryFn: () => api<ConsolidatedReport>(`/me/report?${new URLSearchParams({ date_from: from, date_to: to })}`),
    enabled: completedRuns.length > 0 && Boolean(from && to) && from <= to,
  });
  const totals = report.data?.metrics;

  if (completedRuns.length === 0) {
    return <div className="view"><div className="card"><EmptyState icon={History} message="Aún no hay análisis completados para generar reportes." /></div></div>;
  }

  return (
    <div className="view">
      <section className="card report-header">
        <div>
          <h2>Reporte consolidado</h2>
          <p className="muted-note">
            Solicitudes recibidas en el período, sin duplicar reanálisis. Cada solicitud conserva su resultado más reciente.
          </p>
          {report.data && <p className="muted-note">Zona horaria: {report.data.timezone}. Los tiempos son corridos.</p>}
          {hasIncompleteCoverage && <p className="muted-note" role="alert">Entre las ejecuciones consultadas hay recuperaciones parciales. Los totales incluyen los hilos disponibles; revisa el embudo de cobertura en Resumen.</p>}
        </div>
        <RangeField label="Periodo del reporte" type="date" fromValue={from} toValue={to} onFromChange={setFromOverride} onToChange={setToOverride} />
      </section>

      {from > to ? (
        <p role="alert">La fecha inicial debe ser anterior o igual a la final.</p>
      ) : report.isError ? (
        <div className="card" role="alert"><p>{report.error.message}</p><button type="button" className="btn-ghost" onClick={() => report.refetch()}>Reintentar</button></div>
      ) : report.isLoading ? (
        <p role="status">Calculando reporte…</p>
      ) : !totals || totals.total_threads === 0 ? (
        <div className="card">
          <EmptyState icon={History} message="No hay casos recibidos dentro del rango seleccionado." />
        </div>
      ) : (
        <>
          <div className="metric-grid primary">
            <MetricCard icon={History} label="Análisis consultados" value={report.data?.run_count ?? 0} tone="blue" />
            <MetricCard icon={Mail} label="Casos del período" value={totals.total_threads} tone="blue" />
            <MetricCard icon={CheckCircle2} label="Válidas" value={totals.valid_requests} tone="mint" />
            <MetricCard icon={Reply} label="Respondidas" value={totals.answered} tone="teal" />
            <MetricCard icon={Clock} label="Sin respuesta registrada" value={totals.unanswered} tone="orange" />
            <MetricCard icon={HelpCircle} label="Pendientes de revisión" value={totals.pending_review} tone="amber" />
          </div>
          <div className="metric-grid primary">
            <MetricCard icon={Ban} label="Ignoradas" value={totals.ignored} tone="gray" />
            <MetricCard icon={ClipboardList} label="Correcciones manuales" value={totals.manual_overrides} tone="purple" />
            <MetricCard icon={ShieldCheck} label="Sin revisión pendiente" value={formatPercent(totals.report_confidence)} tone="purple" description="Porcentaje sin revisión pendiente. No representa la exactitud de la clasificación." />
            <MetricCard icon={Timer} label="T. medio respuesta" value={formatDuration(totals.avg_first_response_minutes)} tone="gray" description="Tiempo corrido, incluye noches y fines de semana." />
            <MetricCard icon={CheckCheck} label="Hasta último envío" value={formatDuration(totals.avg_resolution_minutes)} tone="gray" description="Tiempo corrido hasta el último envío del equipo; no acredita cierre o resolución." />
          </div>
          {selectedRuns.length >= 2 ? (
            <><p className="muted-note">Zona horaria: {report.data?.timezone}. Los gráficos comparan ejecuciones completas, que pueden cubrir períodos distintos o solapados.</p><ReportTrendCharts points={points} /></>
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
