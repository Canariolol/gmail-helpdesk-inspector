import { LogIn, ShieldCheck, X } from "lucide-react";

interface ConsentModalProps {
  loginUrl: string;
  onClose: () => void;
}

export function ConsentModal({ loginUrl, onClose }: ConsentModalProps) {
  return (
    <div className="lp-modal-backdrop" role="presentation" onClick={onClose}>
      <section
        className="lp-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="lp-consent-title"
        aria-describedby="lp-consent-desc"
        onClick={(event) => event.stopPropagation()}
      >
        <button type="button" className="lp-modal-close" aria-label="Cerrar" onClick={onClose}>
          <X size={18} />
        </button>
        <div className="lp-modal-icon" aria-hidden="true">
          <ShieldCheck size={26} />
        </div>
        <h2 id="lp-consent-title">Crea tu cuenta para empezar</h2>
        <p id="lp-consent-desc">
          Primero creas tu cuenta segura. Después de elegir plan o activar trial,
          podrás conectar Gmail con permiso de <strong>solo lectura</strong>.
        </p>
        <ul className="lp-modal-list">
          <li>La cuenta se maneja con WorkOS AuthKit.</li>
          <li>Gmail se conecta en un paso separado y solo para auditar métricas.</li>
          <li>Los pagos se procesan en CLP por Mercado Pago.</li>
        </ul>
        <div className="lp-modal-actions">
          <button type="button" className="lp-btn-ghost" onClick={onClose}>
            Cancelar
          </button>
          <a className="lp-btn-primary" href={loginUrl}>
            <LogIn size={18} /> Crear cuenta
          </a>
        </div>
      </section>
    </div>
  );
}
