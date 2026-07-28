import { Building2 } from "lucide-react";
import { CONTACTO } from "../landing/pages/legalContent";

const SCOPE_ITEMS = [
  "Cuatro o más casillas, con usuarios y permisos a medida",
  "Gmail y Microsoft 365 en la misma organización",
  "Retención configurable y objetivos de SLA",
  "SSO, registros administrativos e integraciones (API, webhooks, BI)",
];

/**
 * Empresa es a cotización: sin precio ni cupos fijos, por lo que no viene del
 * catálogo de planes (`BillingPlan[]`) ni pasa por checkout. Se muestra
 * separada de la grilla de planes con copy estático.
 */
export function EnterpriseContactCard() {
  const mailtoHref = `mailto:${CONTACTO}?subject=${encodeURIComponent("Mira Helpdesk — plan Empresa")}`;

  return (
    <section className="pricing-enterprise">
      <div className="pricing-enterprise-icon">
        <Building2 size={20} />
      </div>
      <div className="pricing-enterprise-body">
        <h3>Empresa</h3>
        <p>
          Para organizaciones con una operación más grande o requisitos particulares. Definimos
          casillas, volumen, integraciones, retención y soporte según tus necesidades — sujeto a
          evaluación, no viene con un precio fijo.
        </p>
        <ul>
          {SCOPE_ITEMS.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      </div>
      <a className="btn-ghost pricing-enterprise-cta" href={mailtoHref}>
        Conversemos
      </a>
    </section>
  );
}
