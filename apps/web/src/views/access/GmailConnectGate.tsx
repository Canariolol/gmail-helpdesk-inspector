import { Ban, Check, LogOut, Mail } from "lucide-react";
import { AccessShell } from "./AccessShell";

interface GmailConnectGateProps {
  accountEmail: string;
  connectUrl: string;
  planName: string | null;
  isTrial: boolean;
  onLogout: () => void;
}

const GUARANTEES = [
  "No enviamos ni respondemos correos",
  "No etiquetamos, archivamos ni borramos nada",
  "No guardamos el cuerpo completo de tus correos",
];

export function GmailConnectGate({
  accountEmail,
  connectUrl,
  planName,
  isTrial,
  onLogout,
}: GmailConnectGateProps) {
  return (
    <AccessShell>
      <div className="access-head">
        <span className="access-badge access-badge-ok">
          <Check size={14} /> {isTrial ? "Prueba activa" : "Plan activo"}
          {planName ? ` · ${planName}` : ""}
        </span>
        <h1>Conecta la casilla de Gmail que vas a auditar</h1>
        <p className="access-lede">
          Último paso. Autoriza con Google el acceso de <strong>solo lectura</strong> a la casilla
          que quieres medir. Es distinto de tu cuenta: aquí eliges qué bandeja auditar.
        </p>
      </div>

      <ul className="access-guarantees">
        {GUARANTEES.map((item) => (
          <li key={item}>
            <Ban size={15} /> {item}
          </li>
        ))}
      </ul>

      <div className="access-actions">
        <a className="btn-primary" href={connectUrl}>
          <Mail size={18} /> Conectar Gmail (solo lectura)
        </a>
      </div>

      <div className="access-foot-row">
        <span className="access-note-muted">Cuenta: {accountEmail}</span>
        <button type="button" className="access-text-btn" onClick={onLogout}>
          <LogOut size={15} /> Cerrar sesión
        </button>
      </div>
    </AccessShell>
  );
}
