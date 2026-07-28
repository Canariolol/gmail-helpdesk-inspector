import { useState } from "react";
import { Check, Sparkles } from "lucide-react";
import type { BillingInterval, BillingPlan, BillingPlanId } from "../../api/types";

export function formatClp(value: number): string {
  return new Intl.NumberFormat("es-CL").format(value);
}

interface PricingPlansProps {
  plans: BillingPlan[];
  onChoosePlan: (planId: BillingPlanId, billingInterval: BillingInterval) => void;
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
  const [billingInterval, setBillingInterval] = useState<BillingInterval>("monthly");
  const hasAnnualOption = plans.some((plan) => plan.clp_annual !== null);

  return (
    <div>
      {hasAnnualOption && (
        <div className="pricing-interval-toggle" role="group" aria-label="Ciclo de facturación">
          <button
            type="button"
            className={billingInterval === "monthly" ? "is-active" : ""}
            onClick={() => setBillingInterval("monthly")}
          >
            Mensual
          </button>
          <button
            type="button"
            className={billingInterval === "annual" ? "is-active" : ""}
            onClick={() => setBillingInterval("annual")}
          >
            Anual · 2 meses gratis
          </button>
        </div>
      )}

      <div className="pricing-grid">
        {plans.map((plan) => {
          const isLoading = loadingPlanId === plan.id;
          const isCurrent = currentPlanId === plan.id;
          const buttonClass = plan.highlighted ? "btn-primary" : "btn-ghost";
          const showsAnnual = billingInterval === "annual" && plan.clp_annual !== null;
          const price = showsAnnual ? plan.clp_annual! : plan.clp_monthly;
          return (
            <article key={plan.id} className={`pricing-card${plan.highlighted ? " is-featured" : ""}`}>
              {plan.highlighted && (
                <span className="pricing-tag">
                  <Sparkles size={13} /> Recomendado
                </span>
              )}
              <h3 className="pricing-name">{plan.name}</h3>

              <p className="pricing-price">
                <strong>${formatClp(price)}</strong>
                <span>CLP / {showsAnnual ? "año" : "mes"}</span>
              </p>
              <p className="pricing-usd">≈ USD {plan.usd_reference_monthly} · referencia internacional</p>
              <p className="pricing-iva-note">Precios en CLP, IVA incluido</p>

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
                onClick={() => onChoosePlan(plan.id, showsAnnual ? "annual" : "monthly")}
              >
                {isCurrent ? "Plan actual" : isLoading ? loadingLabel : ctaLabel}
              </button>
            </article>
          );
        })}
      </div>
    </div>
  );
}
