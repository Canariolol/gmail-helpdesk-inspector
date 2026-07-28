import { LogOut } from "lucide-react";
import type { BillingInterval, BillingPlan, BillingPlanId } from "../../api/types";
import { AccessShell } from "./AccessShell";
import { EnterpriseContactCard } from "./EnterpriseContactCard";
import { PaymentNotes } from "./PaymentNotes";
import { PricingPlans } from "./PricingPlans";

interface PricingGateProps {
  email: string;
  plans: BillingPlan[];
  onChoosePlan: (planId: BillingPlanId, billingInterval: BillingInterval) => void;
  loadingPlanId: string | null;
  error: string | null;
  onLogout: () => void;
}

export function PricingGate({
  email,
  plans,
  onChoosePlan,
  loadingPlanId,
  error,
  onLogout,
}: PricingGateProps) {
  return (
    <AccessShell wide>
      <div className="access-head">
        <span className="access-eyebrow">Cuenta creada · {email}</span>
        <h1>Elige un plan para activar tu auditoría</h1>
        <p className="access-lede">
          Tu cuenta ya está lista. Elige un plan para empezar a medir tu casilla. El plan Pro
          incluye 30 días de prueba.
        </p>
      </div>

      {error && (
        <div className="access-error" role="alert">
          {error}
        </div>
      )}

      <PricingPlans plans={plans} onChoosePlan={onChoosePlan} loadingPlanId={loadingPlanId} />
      <EnterpriseContactCard />

      <PaymentNotes />

      <button type="button" className="access-text-btn" onClick={onLogout}>
        <LogOut size={15} /> Cerrar sesión
      </button>
    </AccessShell>
  );
}
