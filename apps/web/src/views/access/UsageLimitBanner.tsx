import { AlertTriangle, ArrowUpRight } from "lucide-react";

interface UsageLimitBannerProps {
  runsCreated: number;
  runsLimit: number;
  planName: string | null;
  onUpgrade: () => void;
}

export function UsageLimitBanner({
  runsCreated,
  runsLimit,
  planName,
  onUpgrade,
}: UsageLimitBannerProps) {
  return (
    <div className="usage-limit-banner" role="status">
      <span className="usage-limit-icon" aria-hidden="true">
        <AlertTriangle size={18} />
      </span>
      <div className="usage-limit-copy">
        <strong>Llegaste al límite de análisis de tu plan{planName ? ` ${planName}` : ""}.</strong>
        <span>
          Usaste {runsCreated} de {runsLimit} análisis este mes. Tu cuota se renueva el 1 del
          próximo mes. Para seguir analizando ahora, sube de plan.
        </span>
      </div>
      <button type="button" className="btn-primary usage-limit-cta" onClick={onUpgrade}>
        Subir de plan <ArrowUpRight size={16} />
      </button>
    </div>
  );
}
