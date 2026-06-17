import { LogIn, ShieldCheck, X } from "lucide-react";
import { useState } from "react";
import { API_BASE_URL } from "../api/client";
import { PrivacyCallout } from "../components/common/PrivacyCallout";

export function LoginView() {
  const [showOAuthDetails, setShowOAuthDetails] = useState(false);
  const loginUrl = `${API_BASE_URL}/auth/google/login`;

  return (
    <main className="login-screen">
      <div className="login-card">
        <img src="/logo-192.png" alt="" />
        <p className="login-eyebrow">Métricas auditables para Gmail Helpdesk</p>
        <h1>Convierte tu casilla de soporte en un reporte confiable</h1>
        <p>
          Analiza hilos de Gmail, identifica solicitudes reales, mide primera respuesta y revisa casos ambiguos sin
          modificar tu correo.
        </p>
        <PrivacyCallout compact />
        <button type="button" className="btn-primary" onClick={() => setShowOAuthDetails(true)}>
          <LogIn size={18} />
          Conectar con Google Gmail
        </button>
        <p className="login-fineprint">
          Antes de abrir Google te mostraremos exactamente qué permiso se solicita y qué datos se usan.
        </p>
      </div>

      {showOAuthDetails && (
        <div className="modal-backdrop" role="presentation">
          <section
            className="oauth-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="oauth-modal-title"
            aria-describedby="oauth-modal-description"
          >
            <button
              type="button"
              className="modal-close"
              aria-label="Cerrar explicación de permisos"
              onClick={() => setShowOAuthDetails(false)}
            >
              <X size={18} />
            </button>
            <div className="oauth-modal-icon" aria-hidden="true">
              <ShieldCheck size={28} />
            </div>
            <h2 id="oauth-modal-title">Antes de continuar con Google</h2>
            <p id="oauth-modal-description">
              Vas a autorizar acceso de solo lectura a tu Gmail para construir métricas auditables de helpdesk.
            </p>
            <ul className="permission-list">
              <li>
                <strong>Scope:</strong> <code>gmail.readonly</code>.
              </li>
              <li>No permite enviar, editar, etiquetar, archivar ni borrar correos.</li>
              <li>Los tokens se guardan cifrados y podrás revocar acceso desde tu cuenta Google.</li>
              <li>Los cuerpos completos solo se usan transitoriamente durante el análisis/auditoría.</li>
            </ul>
            <div className="modal-actions">
              <button type="button" className="btn-ghost" onClick={() => setShowOAuthDetails(false)}>
                Cancelar
              </button>
              <a className="btn-primary" href={loginUrl}>
                <LogIn size={18} />
                Continuar con Google
              </a>
            </div>
          </section>
        </div>
      )}
    </main>
  );
}
