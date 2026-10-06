import { useQuery } from "@tanstack/react-query";
import { api } from "../../../api/client";
import type { BillingPlan } from "../../../api/types";
import { EnterpriseContactCard } from "../../access/EnterpriseContactCard";
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
          <p>Empieza con Mira Free, sin tarjeta. Los planes de pago se cobran en CLP por Mercado Pago.</p>
        </header>

        {plans.isLoading && <p className="lp-pricing-state">Cargando planes…</p>}
        {plans.isError && (
          <div className="lp-pricing-state" role="alert">
            <p>No pudimos cargar los planes. Revisa tu conexión e inténtalo nuevamente.</p>
            <button type="button" className="lp-btn-ghost" onClick={() => plans.refetch()}>
              Reintentar
            </button>
          </div>
        )}
        {plans.data && (
          <PricingPlans
            plans={plans.data}
            onChoosePlan={() => onSignup()}
            loadingPlanId={null}
            ctaLabel="Crear cuenta"
          />
        )}

        <EnterpriseContactCard />

        <p className="lp-pricing-note">
          Los análisis manuales y programados comparten los cupos mensuales del plan. Los precios en
          USD son una referencia mensual; el cobro se realiza en pesos chilenos (CLP). Puedes cancelar
          la renovación desde tu cuenta y conservar el acceso hasta el fin del período adquirido.
        </p>
      </section>
    </PublicPage>
  );
}
