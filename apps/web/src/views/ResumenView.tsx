import { AlertTriangle, CheckCheck, CheckCircle2, Clock, Gauge, HelpCircle, Mail, Reply, ShieldCheck, Timer } from "lucide-react";
import { useRef } from "react";
import type { AnalysisRun, EmailThread, FilterPreset, OrgConfig, ThreadDetail } from "../api/types";
import { ClassificationBarChart } from "../components/charts/ClassificationBarChart";
import { CompositionDonut } from "../components/charts/CompositionDonut";
import { EmptyState } from "../components/common/EmptyState";
import { MetricCard } from "../components/common/MetricCard";
import { FilterBar } from "../components/filters/FilterBar";
import { AnalysisFunnelPanel } from "../components/runs/AnalysisFunnelPanel";
import { PlanCapBanner } from "../components/runs/PlanCapBanner";
import { StatusBanner } from "../components/runs/StatusBanner";
import { ThreadDetailRail } from "../components/threads/ThreadDetailRail";
import { ThreadTable } from "../components/threads/ThreadTable";
import { formatDuration, formatPercent } from "../lib/format";
import { filterThreads } from "../lib/threads";

type Props = {
  run: AnalysisRun | null;
  threads: EmailThread[];
  threadFilter: string;
  onThreadFilter: (filter: string) => void;
  selectedThreadId: string | null;
  onSelectThread: (id: string) => void;
  onCloseThread: () => void;
  detail: ThreadDetail | undefined;
  onReview: (payload: unknown) => void;
  savingReview: boolean;
  reviewError: string | null;
  reviewSavedAt: number | null;
  onAnalyze: (payload: unknown) => void;
  analyzing: boolean;
  onStartRun: () => void;
  startingRun: boolean;
  orgConfig?: OrgConfig | null;
  onGoToSetup?: () => void;
  filterPresets?: FilterPreset[];
  onSavePreset?: (payload: unknown) => void;
  onDeletePreset?: (id: string) => void;
  analysisError?: string | null;
  planName?: string | null;
  onUpgrade?: () => void;
};

function formatMissingSetupItem(item: string): string {
  if (item === "internal_domains") return "dominios de tu equipo";
  if (item === "valid_request_criteria") return "qué correos deben contar como solicitudes";
  if (item === "report_recipients") return "destinatarios de reportes";
  return "un dato de configuración";
}

export function ResumenView(props: Props) {
  const { run } = props;
  const visibleThreads = filterThreads(props.threads, props.threadFilter);
  const threadsSectionRef = useRef<HTMLDivElement>(null);
  const setupNotReady = props.orgConfig !== null && props.orgConfig !== undefined
    && !props.orgConfig.setup_state.ready_for_analysis
    && props.orgConfig.account_unrestricted !== true;

  // Aplica un filtro y baja a "Hilos auditables" sin abandonar Resumen. Cierra
  // cualquier detalle abierto para no mostrar un hilo fuera del nuevo filtro.
  const focusThreads = (filter: string) => {
    props.onThreadFilter(filter);
    if (props.selectedThreadId) {
      props.onCloseThread();
    }
    const prefersReducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    threadsSectionRef.current?.scrollIntoView({
      behavior: prefersReducedMotion ? "auto" : "smooth",
      block: "start",
    });
  };

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
                <> Falta completar: {props.orgConfig.setup_state.missing.map(formatMissingSetupItem).join(", ")}.</>
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
        filterPresets={props.filterPresets}
        onSavePreset={props.onSavePreset}
        onDeletePreset={props.onDeletePreset}
      />
      {props.analysisError && (
        <div className="action-error-banner" role="alert">
          <AlertTriangle size={18} />
          <div>
            <strong>No se pudo iniciar el análisis</strong>
            <p>{props.analysisError}</p>
            {props.analysisError.toLowerCase().includes("demasiadas") && (
              <span>El límite protege el servicio y evita análisis duplicados. Intenta más tarde o espera el análisis automático programado.</span>
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
            <StatusBanner run={run} onStart={props.onStartRun} starting={props.startingRun} onViewDetails={() => focusThreads("all")} />
            <PlanCapBanner run={run} planName={props.planName ?? null} onUpgrade={props.onUpgrade} />
            <AnalysisFunnelPanel run={run} />
            <div className="metric-grid primary">
              <MetricCard icon={Mail} label="Total analizados" value={run.metrics.total_threads} tone="blue" onClick={() => focusThreads("all")} />
              <MetricCard icon={CheckCircle2} label="Válidos" value={run.metrics.valid_requests} tone="mint" onClick={() => focusThreads("valid_client_request")} />
              <MetricCard icon={Reply} label="Respondidos" value={run.metrics.answered} tone="teal" onClick={() => focusThreads("answered")} />
              <MetricCard icon={Clock} label="Sin respuesta" value={run.metrics.unanswered} tone="orange" onClick={() => focusThreads("unanswered")} />
              <MetricCard icon={HelpCircle} label="Pendientes de revisión" value={run.metrics.pending_review} tone="amber" onClick={() => focusThreads("review")} />
              <MetricCard icon={ShieldCheck} label="Confianza" value={formatPercent(run.metrics.report_confidence)} tone="purple" />
            </div>
            <div className="metric-grid times">
              <MetricCard icon={Timer} label="T. medio respuesta" value={formatDuration(run.metrics.avg_first_response_minutes)} tone="gray" onClick={() => focusThreads("answered")} />
              <MetricCard icon={Gauge} label="P90 respuesta" value={formatDuration(run.metrics.p90_first_response_minutes)} tone="gray" onClick={() => focusThreads("answered")} />
              <MetricCard icon={CheckCheck} label="Cierre medio" value={formatDuration(run.metrics.avg_resolution_minutes)} tone="gray" onClick={() => focusThreads("answered")} />
            </div>
            <div className="charts-row">
              <ClassificationBarChart metrics={run.metrics} />
              <CompositionDonut metrics={run.metrics} />
            </div>
            <div ref={threadsSectionRef} className="threads-anchor">
              <ThreadTable
                title="Hilos auditables"
                threads={visibleThreads}
                filter={props.threadFilter}
                onFilter={props.onThreadFilter}
                selectedThreadId={props.selectedThreadId}
                onSelect={props.onSelectThread}
              />
            </div>
          </div>
          <ThreadDetailRail
            detail={props.detail}
            open={Boolean(props.selectedThreadId)}
            onClose={props.onCloseThread}
            onReview={props.onReview}
            saving={props.savingReview}
            reviewError={props.reviewError}
            reviewSavedAt={props.reviewSavedAt}
          />
        </div>
      )}
    </div>
  );
}
