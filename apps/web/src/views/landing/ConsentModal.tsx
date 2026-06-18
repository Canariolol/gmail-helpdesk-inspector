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
        <h2 id="lp-consent-title">Antes de continuar con Google</h2>
        <p id="lp-consent-desc">
          Vas a autorizar acceso de <strong>solo lectura</strong> a tu Gmail para construir métricas
          auditables de soporte.
        </p>
        <ul className="lp-modal-list">
          <li>Solo lectura. Punto: no enviamos, editamos, etiquetamos, archivamos ni borramos.</li>
          <li>El acceso queda guardado de forma segura y puedes revocarlo cuando quieras.</li>
          <li>No guardamos el contenido completo de tus correos.</li>
        </ul>
        <div className="lp-modal-actions">
          <button type="button" className="lp-btn-ghost" onClick={onClose}>
            Cancelar
          </button>
          <a className="lp-btn-primary" href={loginUrl}>
            <LogIn size={18} /> Continuar con Google
          </a>
        </div>
      </section>
    </div>
  );
}
