import { Ban, Check, LogOut, Mail } from "lucide-react";
import type { MailboxProviderId } from "../../api/types";
import { AccessShell } from "./AccessShell";

interface MailboxConnectGateProps {
  accountEmail: string;
  /** Proveedores que este despliegue puede ofrecer, según `/mailbox/providers`. */
  providers: MailboxProviderId[];
  connectUrlFor: (provider: MailboxProviderId) => string;
  planName: string | null;
  isTrial: boolean;
  onLogout: () => void;
}

const GUARANTEES = [
  "No enviamos ni respondemos correos",
  "No etiquetamos, archivamos ni borramos nada",
  "No guardamos el cuerpo completo de tus correos",
];

/**
 * Los subtítulos importan tanto como los botones: sin ellos, quien tiene un
 * dominio propio (`@mi-empresa.cl`) no adivina que igual debe apretar Google o
 * Microsoft, porque el sufijo del correo no revela el proveedor.
 */
const PROVIDERS: Record<MailboxProviderId, { name: string; hint: string }> = {
  google: { name: "Conectar con Google", hint: "Gmail y dominios en Google Workspace" },
  microsoft: { name: "Conectar con Microsoft", hint: "Outlook y Microsoft 365" },
};

export function MailboxConnectGate({
  accountEmail,
  providers,
  connectUrlFor,
  planName,
  isTrial,
  onLogout,
}: MailboxConnectGateProps) {
  return (
    <AccessShell>
      <div className="access-head">
        <span className="access-badge access-badge-ok">
          <Check size={14} /> {isTrial ? "Prueba activa" : "Plan activo"}
          {planName ? ` · ${planName}` : ""}
        </span>
        <h1>Conecta la casilla de correo que vas a auditar</h1>
        <p className="access-lede">
          Último paso. Autoriza el acceso de <strong>solo lectura</strong> a la casilla que quieres
          medir. Es distinto de tu cuenta: aquí eliges qué bandeja auditar.
        </p>
      </div>

      <ul className="access-guarantees">
        {GUARANTEES.map((item) => (
          <li key={item}>
            <Ban size={15} /> {item}
          </li>
        ))}
      </ul>

      <div className="access-actions access-actions-stack">
        {providers.map((provider, index) => (
          <a
            key={provider}
            className={index === 0 ? "btn-primary" : "btn-ghost"}
            href={connectUrlFor(provider)}
          >
            <Mail size={18} />
            <span className="access-provider-label">
              <strong>{PROVIDERS[provider].name}</strong>
              <small>{PROVIDERS[provider].hint}</small>
            </span>
          </a>
        ))}
      </div>

      <p className="access-note-muted">
        Si tu organización usa Google Workspace o Microsoft 365, puede que tu administrador deba
        autorizar Mira antes de que la conexión funcione.
      </p>

      <div className="access-foot-row">
        <span className="access-note-muted">Cuenta: {accountEmail}</span>
        <button type="button" className="access-text-btn" onClick={onLogout}>
          <LogOut size={15} /> Cerrar sesión
        </button>
      </div>
    </AccessShell>
  );
}
