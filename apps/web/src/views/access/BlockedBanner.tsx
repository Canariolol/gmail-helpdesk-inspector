import { ArrowRight, ShieldAlert } from "lucide-react";

interface BlockedBannerProps {
  /** Lleva a la vista de Plan y cuenta para reactivar. */
  onReactivate: () => void;
}

export function BlockedBanner({ onReactivate }: BlockedBannerProps) {
  return (
    <div className="blocked-banner" role="status">
      <span className="blocked-banner-icon" aria-hidden="true">
        <ShieldAlert size={18} />
      </span>
      <div className="blocked-banner-copy">
        <strong>Tu plan está en pausa.</strong>
        <span>
          No puedes generar ni ver análisis hasta reactivar. Tu configuración y los datos de tu
          cuenta siguen disponibles.
        </span>
      </div>
      <button type="button" className="btn-primary blocked-banner-cta" onClick={onReactivate}>
        Reactivar plan <ArrowRight size={16} />
      </button>
    </div>
  );
}
