import { Ban, Check, HelpCircle, LogOut, Mail } from "lucide-react";
import { useState } from "react";
import { api } from "../../api/client";
import type { MailboxProviderId } from "../../api/types";
import { AccessShell } from "./AccessShell";

type DetectedProvider = MailboxProviderId | "unsupported" | "unknown";

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
        <NotSureOption providers={providers} connectUrlFor={connectUrlFor} />
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

type NotSureProps = {
  providers: MailboxProviderId[];
  connectUrlFor: (provider: MailboxProviderId) => string;
};

/**
 * Tercera opción: quien no sabe qué proveedor usa escribe su correo y lo
 * encaminamos solos. El dominio no revela el proveedor (`@mi-empresa.cl` puede
 * ser Google, Microsoft u otro), así que el backend lo resuelve por los MX y
 * aquí solo redirigimos al OAuth que corresponde, ya con la casilla preseleccionada.
 */
function NotSureOption({ providers, connectUrlFor }: NotSureProps) {
  const [open, setOpen] = useState(false);
  const [email, setEmail] = useState("");
  const [checking, setChecking] = useState(false);
  const [outcome, setOutcome] = useState<"unsupported" | "unknown" | "error" | null>(null);

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    setChecking(true);
    setOutcome(null);
    try {
      const result = await api<{ provider: DetectedProvider }>(
        `/mailbox/detect?email=${encodeURIComponent(email.trim())}`,
      );
      const provider = result.provider;
      if ((provider === "google" || provider === "microsoft") && providers.includes(provider)) {
        // Redirección de página completa, como si hubiera apretado el botón.
        const url = new URL(connectUrlFor(provider));
        url.searchParams.set("login_hint", email.trim());
        window.location.href = url.toString();
        return;
      }
      setOutcome(provider === "unknown" ? "unknown" : "unsupported");
    } catch {
      setOutcome("error");
    } finally {
      setChecking(false);
    }
  };

  if (!open) {
    return (
      <button type="button" className="btn-ghost" onClick={() => setOpen(true)}>
        <HelpCircle size={18} />
        <span className="access-provider-label">
          <strong>No estoy seguro / otro</strong>
          <small>Escribe tu correo y detectamos el proveedor por ti</small>
        </span>
      </button>
    );
  }

  return (
    <form className="access-detect" onSubmit={handleSubmit}>
      <label className="access-note-muted" htmlFor="detect-email">
        Escribe la dirección de la casilla que quieres auditar
      </label>
      <input
        id="detect-email"
        type="email"
        required
        autoFocus
        placeholder="soporte@mi-empresa.cl"
        value={email}
        onChange={(event) => setEmail(event.target.value)}
      />
      <button type="submit" className="btn-primary" disabled={checking || !email.trim()}>
        {checking ? "Detectando…" : "Continuar"}
      </button>

      {outcome === "unsupported" && (
        <p className="access-note-muted">
          <strong>Tu casilla no está en Google ni en Microsoft.</strong> Estamos implementando
          el soporte para otros proveedores de correo (Hostinger, cPanel, Zoho y similares).
          Por ahora Mira solo puede auditar casillas de Google y Microsoft. Guardamos tu
          correo y te avisamos apenas esté disponible.
        </p>
      )}
      {outcome === "unknown" && (
        <p className="access-note-muted">
          No pudimos identificar el proveedor de ese dominio. Revisa que la dirección esté
          bien escrita o usa directamente uno de los botones de arriba.
        </p>
      )}
      {outcome === "error" && (
        <p className="access-note-muted">
          No pudimos hacer la detección en este momento. Inténtalo de nuevo o usa uno de los
          botones de arriba.
        </p>
      )}
    </form>
  );
}
