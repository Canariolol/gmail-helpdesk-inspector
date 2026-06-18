import { X } from "lucide-react";
import type { BillingPlan, BillingPlanId } from "../../api/types";
import { PaymentNotes } from "./PaymentNotes";
import { PricingPlans } from "./PricingPlans";

type PlansModalMode = "checkout" | "change";

interface PlansModalProps {
  plans: BillingPlan[];
  currentPlanId: BillingPlanId | null;
  currentPlanName: string | null;
  onChoosePlan: (planId: BillingPlanId) => void;
  loadingPlanId: string | null;
  error: string | null;
  onClose: () => void;
  /** "change" actualiza el plan vigente (sin reautorizar); "checkout" inicia un pago nuevo. */
  mode?: PlansModalMode;
}

export function PlansModal({
  plans,
  currentPlanId,
  currentPlanName,
  onChoosePlan,
  loadingPlanId,
  error,
  onClose,
  mode = "change",
}: PlansModalProps) {
  const isChange = mode === "change";

  return (
    <div className="access-modal-backdrop" role="presentation" onClick={onClose}>
      <section
        className="access-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="plans-modal-title"
        onClick={(event) => event.stopPropagation()}
      >
        <button type="button" className="access-modal-close" aria-label="Cerrar" onClick={onClose}>
          <X size={18} />
        </button>
        <div className="access-head">
          <h2 id="plans-modal-title">{isChange ? "Cambia tu plan" : "Elige un plan"}</h2>
          <p className="access-lede">
            {currentPlanName ? `Tu plan actual es ${currentPlanName}. ` : ""}
            {isChange
              ? "El cambio aplica de inmediato sobre tu suscripción de Mercado Pago, sin volver a pagar."
              : "Elige un plan para activar tu auditoría."}
          </p>
        </div>

        {error && (
          <div className="access-error" role="alert">
            {error}
          </div>
        )}

        <PricingPlans
          plans={plans}
          onChoosePlan={onChoosePlan}
          loadingPlanId={loadingPlanId}
          currentPlanId={isChange ? currentPlanId : null}
          ctaLabel={isChange ? "Cambiar a este plan" : "Continuar con Mercado Pago"}
          loadingLabel={isChange ? "Cambiando…" : "Creando checkout…"}
        />

        <PaymentNotes />
      </section>
    </div>
  );
}
