import { useQuery } from "@tanstack/react-query";
import { api } from "../../../api/client";
import type { BillingPlan } from "../../../api/types";
import { PricingPlans } from "../../access/PricingPlans";
import { PublicPage } from "../PublicPage";

type Props = {
  onLogin: () => void;
  onSignup: () => void;
};

export function PricingPage({ onLogin, onSignup }: Props) {
  const plans = useQuery({
    queryKey: ["public-plans"],
    queryFn: () => api<BillingPlan[]>("/public/plans"),
  });

  return (
    <PublicPage onLogin={onLogin} onSignup={onSignup}>
      <section className="lp-pricing">
        <header className="lp-pricing-head">
          <span className="lp-focus-eyebrow">Planes</span>
          <h1>Precios claros, sin sorpresas</h1>
          <p>Paga en CLP con MercadoPago. Cancela cuando quieras.</p>
        </header>

        {plans.isLoading && <p className="lp-pricing-state">Cargando planes…</p>}
        {plans.isError && (
          <p className="lp-pricing-state">No pudimos cargar los planes. Intenta nuevamente más tarde.</p>
        )}
        {plans.data && (
          <PricingPlans
            plans={plans.data}
            onChoosePlan={() => onSignup()}
            loadingPlanId={null}
            ctaLabel="Crear cuenta"
          />
        )}

        <p className="lp-pricing-note">
          Los precios en USD son solo referencia internacional. El cobro se realiza en pesos chilenos
          (CLP).
        </p>
      </section>
    </PublicPage>
  );
}
