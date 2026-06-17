import { AlertTriangle, CheckCheck, CheckCircle2, Clock, Gauge, HelpCircle, Mail, Reply, ShieldCheck, Timer } from "lucide-react";
import type { AnalysisRun, EmailThread, OrgConfig, ThreadDetail } from "../api/types";
import { ClassificationBarChart } from "../components/charts/ClassificationBarChart";
import { CompositionDonut } from "../components/charts/CompositionDonut";
import { EmptyState } from "../components/common/EmptyState";
import { MetricCard } from "../components/common/MetricCard";
import { FilterBar } from "../components/filters/FilterBar";
import { StatusBanner } from "../components/runs/StatusBanner";
import { ThreadDetailPanel } from "../components/threads/ThreadDetailPanel";
import { ThreadTable } from "../components/threads/ThreadTable";
import { formatDuration, formatPercent } from "../lib/format";
import { filterThreads } from "../lib/threads";

type Props = {
  run: AnalysisRun | null;
  threads: EmailThread[];
  threadFilter: string;
  onThreadFilter: (filter: string) => void;
  onMetricFilter: (filter: string) => void;
  selectedThreadId: string | null;
  onSelectThread: (id: string) => void;
  detail: ThreadDetail | undefined;
  onReview: (payload: unknown) => void;
  savingReview: boolean;
  onAnalyze: (payload: unknown) => void;
  analyzing: boolean;
  onStartRun: () => void;
  startingRun: boolean;
  onViewDetails: () => void;
  orgConfig?: OrgConfig | null;
  onGoToSetup?: () => void;
  analysisError?: string | null;
};

export function ResumenView(props: Props) {
  const { run } = props;
  const visibleThreads = filterThreads(props.threads, props.threadFilter);
  const setupNotReady = props.orgConfig !== null && props.orgConfig !== undefined
    && !props.orgConfig.setup_state.ready_for_analysis;

  return (
    <div className="view">
      {setupNotReady && props.orgConfig && (
        <div className="setup-banner" role="alert">
          <AlertTriangle size={18} />
          <div>
            <strong>Configuración incompleta</strong>
            <p>
              Completa la configuración de tu organización antes de iniciar un análisis.
              {props.orgConfig.setup_state.missing.length > 0 && (
                <> Faltan: {props.orgConfig.setup_state.missing.join(", ")}.</>
              )}
            </p>
          </div>
          {props.onGoToSetup && (
            <button type="button" className="btn-primary" onClick={props.onGoToSetup}>
              Completar configuración
            </button>
          )}
        </div>
      )}
      <FilterBar
        loading={props.analyzing}
        onAnalyze={props.onAnalyze}
        orgConfig={props.orgConfig}
        onGoToSetup={props.onGoToSetup}
      />
      {props.analysisError && (
        <div className="action-error-banner" role="alert">
          <AlertTriangle size={18} />
          <div>
            <strong>No se pudo iniciar el análisis</strong>
            <p>{props.analysisError}</p>
            {props.analysisError.toLowerCase().includes("demasiadas") && (
              <span>El límite protege la beta privada y evita ejecuciones duplicadas. Intenta más tarde o usa el scheduler configurado.</span>
            )}
          </div>
        </div>
      )}
      {!run ? (
        <div className="card">
          <EmptyState message="Crea tu primer análisis con los filtros de arriba." />
        </div>
      ) : (
        <div className="split-view">
          <div className="view-col">
            <StatusBanner run={run} onStart={props.onStartRun} starting={props.startingRun} onViewDetails={props.onViewDetails} />
            <div className="metric-grid primary">
              <MetricCard icon={Mail} label="Total analizados" value={run.metrics.total_threads} tone="blue" onClick={() => props.onMetricFilter("all")} />
              <MetricCard icon={CheckCircle2} label="Válidos" value={run.metrics.valid_requests} tone="mint" onClick={() => props.onMetricFilter("valid_client_request")} />
              <MetricCard icon={Reply} label="Respondidos" value={run.metrics.answered} tone="teal" onClick={() => props.onMetricFilter("answered")} />
              <MetricCard icon={Clock} label="Sin respuesta" value={run.metrics.unanswered} tone="orange" onClick={() => props.onMetricFilter("unanswered")} />
              <MetricCard icon={HelpCircle} label="Ambiguos" value={run.metrics.ambiguous} tone="amber" onClick={() => props.onMetricFilter("review")} />
              <MetricCard icon={ShieldCheck} label="Confianza" value={formatPercent(run.metrics.report_confidence)} tone="purple" />
            </div>
            <div className="metric-grid times">
              <MetricCard icon={Timer} label="T. medio respuesta" value={formatDuration(run.metrics.avg_first_response_minutes)} tone="gray" onClick={() => props.onMetricFilter("answered")} />
              <MetricCard icon={Gauge} label="P90 respuesta" value={formatDuration(run.metrics.p90_first_response_minutes)} tone="gray" onClick={() => props.onMetricFilter("answered")} />
              <MetricCard icon={CheckCheck} label="Cierre medio" value={formatDuration(run.metrics.avg_resolution_minutes)} tone="gray" onClick={() => props.onMetricFilter("answered")} />
            </div>
            <div className="charts-row">
              <ClassificationBarChart metrics={run.metrics} />
              <CompositionDonut metrics={run.metrics} />
            </div>
            <ThreadTable
              title="Hilos auditables"
              threads={visibleThreads}
              filter={props.threadFilter}
              onFilter={props.onThreadFilter}
              selectedThreadId={props.selectedThreadId}
              onSelect={props.onSelectThread}
            />
          </div>
          <aside className="detail-rail">
            {props.detail ? (
              <ThreadDetailPanel detail={props.detail} onReview={props.onReview} saving={props.savingReview} />
            ) : (
              <div className="card">
                <EmptyState message="Selecciona un hilo para revisar la trazabilidad." />
              </div>
            )}
          </aside>
        </div>
      )}
    </div>
  );
}
