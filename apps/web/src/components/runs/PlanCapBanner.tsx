import { ArrowUpRight, Scissors } from "lucide-react";
import type { AnalysisRun } from "../../api/types";

type Props = {
  run: AnalysisRun;
  planName: string | null;
  onUpgrade?: () => void;
};

// Banner honesto + gancho de upgrade: aparece cuando el tope de hilos analizados
// del plan recortó el análisis. Es el requisito de "que quede informado en la
// respuesta" y, a la vez, el mejor momento para invitar a subir de plan (sobre los
// datos reales del usuario, en el instante de demanda insatisfecha).
export function PlanCapBanner({ run, planName, onUpgrade }: Props) {
  const funnel = run.metrics.funnel;
  if (!funnel?.truncated_by_plan) return null;

  const analyzed = run.metrics.total_threads;
  const wouldBe = funnel.would_be_analyzed ?? analyzed;
  const moreMark = funnel.more_beyond_retrieved ? "+" : "";
  const cap = funnel.plan_analyzed_cap ?? analyzed;
  const planLabel = planName ? ` ${planName}` : " gratis";

  return (
    <div className="usage-limit-banner" role="status">
      <span className="usage-limit-icon" aria-hidden="true">
        <Scissors size={18} />
      </span>
      <div className="usage-limit-copy">
        <strong>
          Analizamos {analyzed} de {wouldBe}
          {moreMark} hilos con actividad de clientes.
        </strong>
        <span>
          Tu plan{planLabel} audita hasta {cap} hilos por análisis. Hay más correos en este
          rango sin auditar; no es una falla. Sube de plan para auditarlos todos.
        </span>
      </div>
      {onUpgrade && (
        <button type="button" className="btn-primary usage-limit-cta" onClick={onUpgrade}>
          Subir de plan <ArrowUpRight size={16} />
        </button>
      )}
    </div>
  );
}
