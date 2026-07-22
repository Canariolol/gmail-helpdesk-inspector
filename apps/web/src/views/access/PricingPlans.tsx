import { Check, Sparkles } from "lucide-react";
import type { BillingPlan, BillingPlanId } from "../../api/types";

export function formatClp(value: number): string {
  return new Intl.NumberFormat("es-CL").format(value);
}

interface PricingPlansProps {
  plans: BillingPlan[];
  onChoosePlan: (planId: BillingPlanId) => void;
  loadingPlanId: string | null;
  ctaLabel?: string;
  loadingLabel?: string;
  /** En modo cambio de plan, marca el plan vigente como no seleccionable. */
  currentPlanId?: BillingPlanId | null;
}

export function PricingPlans({
  plans,
  onChoosePlan,
  loadingPlanId,
  ctaLabel = "Continuar con Mercado Pago",
  loadingLabel = "Creando checkout…",
  currentPlanId = null,
}: PricingPlansProps) {
  return (
    <div className="pricing-grid">
      {plans.map((plan) => {
        const isLoading = loadingPlanId === plan.id;
        const isCurrent = currentPlanId === plan.id;
        const buttonClass = plan.highlighted ? "btn-primary" : "btn-ghost";
        return (
          <article key={plan.id} className={`pricing-card${plan.highlighted ? " is-featured" : ""}`}>
            {plan.highlighted && (
              <span className="pricing-tag">
                <Sparkles size={13} /> Recomendado
              </span>
            )}
            <h3 className="pricing-name">{plan.name}</h3>

            <p className="pricing-price">
              <strong>${formatClp(plan.clp_monthly)}</strong>
              <span>CLP / mes</span>
            </p>
            <p className="pricing-usd">≈ USD {plan.usd_reference_monthly} · referencia internacional</p>

            {plan.trial_days > 0 ? (
              <p className="pricing-trial">{plan.trial_days} días de prueba · con medio de pago</p>
            ) : (
              <p className="pricing-trial pricing-trial-empty">Sin período de prueba</p>
            )}

            <ul className="pricing-feats">
              <li>
                <Check size={15} /> {plan.limits.runs_per_month} análisis al mes
              </li>
              <li>
                <Check size={15} /> {formatClp(plan.limits.retrieved_threads_per_month)} correos revisados/mes
              </li>
              <li>
                <Check size={15} /> {formatClp(plan.limits.ai_analyzed_threads_per_month)} conversaciones revisadas por Mira/mes
              </li>
              <li>
                <Check size={15} /> {plan.limits.mailboxes} casilla{plan.limits.mailboxes > 1 ? "s" : ""} ·{" "}
                {plan.limits.members} usuario{plan.limits.members > 1 ? "s" : ""}
              </li>
            </ul>

            <button
              type="button"
              className={buttonClass}
              disabled={isLoading || isCurrent}
              onClick={() => onChoosePlan(plan.id)}
            >
              {isCurrent ? "Plan actual" : isLoading ? loadingLabel : ctaLabel}
            </button>
          </article>
        );
      })}
    </div>
  );
}
