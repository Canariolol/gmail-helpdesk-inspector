import { AlertTriangle, ArrowUpRight } from "lucide-react";

interface UsageLimitBannerProps {
  used: number;
  limit: number;
  // Etiqueta de la unidad agotada: "análisis" o "hilos analizados".
  unitLabel: string;
  planName: string | null;
  onUpgrade: () => void;
}

export function UsageLimitBanner({
  used,
  limit,
  unitLabel,
  planName,
  onUpgrade,
}: UsageLimitBannerProps) {
  return (
    <div className="usage-limit-banner" role="status">
      <span className="usage-limit-icon" aria-hidden="true">
        <AlertTriangle size={18} />
      </span>
      <div className="usage-limit-copy">
        <strong>
          Llegaste al límite de {unitLabel} de tu plan{planName ? ` ${planName}` : ""}.
        </strong>
        <span>
          Usaste {used} de {limit} {unitLabel} este mes. Tu cuota se renueva el 1 del próximo
          mes. Para seguir analizando ahora, sube de plan.
        </span>
      </div>
      <button type="button" className="btn-primary usage-limit-cta" onClick={onUpgrade}>
        Subir de plan <ArrowUpRight size={16} />
      </button>
    </div>
  );
}
