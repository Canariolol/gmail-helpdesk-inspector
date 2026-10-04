import { Ban, Check, HelpCircle, LogOut, Mail } from "lucide-react";
import { useState } from "react";
import { api } from "../../api/client";
import type { MailboxProviderId } from "../../api/types";
import { AccessShell } from "./AccessShell";

type DetectedProvider = MailboxProviderId | "unsupported" | "unknown";

interface MailboxConnectGateProps {
  accountEmail: string;
  needsReauth?: boolean;
  /** Proveedores que este despliegue puede ofrecer, según `/mailbox/providers`. */
  providers: MailboxProviderId[];
  connectUrlFor: (provider: MailboxProviderId) => string;
  planName: string | null;
  isTrial: boolean;
  onLogout: () => void;
}

const GUARANTEES = [
  "No enviamos ni respondemos desde tu casilla",
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
  imap: { name: "Conectar por IMAP", hint: "Correo de hosting y otros proveedores con TLS" },
};

export function MailboxConnectGate({
  accountEmail,
  needsReauth=false,
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
        <h1>{needsReauth ? "Vuelve a conectar tu casilla de correo" : "Conecta la casilla de correo que vas a auditar"}</h1>
        {needsReauth && <p role="alert" className="access-error">La autorización de lectura venció o fue revocada. Vuelve a conectar la casilla para continuar.</p>}
        <p className="access-lede">
          Autoriza la lectura de la casilla que quieres medir. Es distinto de tu cuenta:
          aquí eliges qué bandeja auditar. Con Google y Microsoft solicitamos permisos de
          solo lectura; por IMAP, Mira utiliza únicamente operaciones de lectura.
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
        {providers.filter((provider) => provider !== "imap").map((provider, index) => (
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
        {providers.includes("microsoft") && <SharedMicrosoftConnect connectUrl={connectUrlFor("microsoft")} />}
        {providers.includes("imap") && <ImapConnect accountEmail={accountEmail} />}
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

function SharedMicrosoftConnect({ connectUrl }: { connectUrl: string }) {
  const [open, setOpen] = useState(false);
  const [mailbox, setMailbox] = useState("");
  if (!open) return <button type="button" className="btn-ghost" onClick={() => setOpen(true)}>Conectar un buzón compartido de Microsoft 365</button>;
  return <form className="access-detect" onSubmit={(event) => {
    event.preventDefault();
    const url = new URL(connectUrl, window.location.origin);
    url.searchParams.set("target_mailbox", mailbox.trim());
    window.location.href = url.toString();
  }}>
    <label htmlFor="shared-mailbox">Dirección del buzón compartido</label>
    <input id="shared-mailbox" type="email" required value={mailbox} onChange={(event) => setMailbox(event.target.value)} placeholder="soporte@empresa.cl" />
    <p className="access-note-muted">Inicia sesión en Microsoft con una cuenta que tenga permiso para leer ese buzón. Tu administrador puede tener que autorizar la conexión y habilitar copias de los mensajes enviados en el buzón compartido. Si se guardan sólo en la cuenta del delegado, Mira no puede observar esas respuestas.</p>
    <button type="submit" className="btn-primary">Autorizar lectura del buzón</button>
  </form>;
}

function ImapConnect({ accountEmail }: { accountEmail: string }) {
  const [open, setOpen] = useState(false);
  const [mailbox, setMailbox] = useState(accountEmail);
  const [host, setHost] = useState("");
  const [username, setUsername] = useState(accountEmail);
  const [password, setPassword] = useState("");
  const [sentFolder, setSentFolder] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!open) return <button type="button" className="btn-ghost" onClick={() => setOpen(true)}>Conectar por IMAP</button>;
  return <form className="access-detect" onSubmit={async (event) => {
    event.preventDefault();setSaving(true);setError(null);
    try {
      await api("/mailbox/connect/imap", { method: "POST", body: JSON.stringify({ mailbox_email: mailbox.trim(), host: host.trim(), port: 993, username: username.trim(), password, sent_folder: sentFolder.trim() || null }) });
      setPassword("");window.location.reload();
    } catch (cause) { setError(cause instanceof Error ? cause.message : "No se pudo conectar la casilla."); }
    finally { setSaving(false); }
  }}>
    <label htmlFor="imap-mailbox">Casilla que vas a auditar</label>
    <input id="imap-mailbox" type="email" required value={mailbox} onChange={(event) => setMailbox(event.target.value)} />
    <label htmlFor="imap-host">Servidor IMAP</label>
    <input id="imap-host" required value={host} onChange={(event) => setHost(event.target.value)} placeholder="mail.empresa.cl" autoCapitalize="none" />
    <label htmlFor="imap-user">Usuario de acceso</label>
    <input id="imap-user" required autoComplete="username" value={username} onChange={(event) => setUsername(event.target.value)} />
    <label htmlFor="imap-password">Contraseña o contraseña de aplicación</label>
    <input id="imap-password" type="password" required autoComplete="current-password" value={password} onChange={(event) => setPassword(event.target.value)} />
    <label htmlFor="imap-sent">Carpeta de enviados, opcional</label>
    <input id="imap-sent" value={sentFolder} onChange={(event) => setSentFolder(event.target.value)} placeholder="Sent" />
    <p className="access-note-muted">La conexión usa TLS en el puerto 993. Las credenciales se guardan cifradas. Revisa la carpeta de enviados para que Mira pueda identificar respuestas.</p>
    {error && <p role="alert">{error}</p>}
    <button type="submit" className="btn-primary" disabled={saving}>{saving ? "Validando conexión…" : "Conectar casilla"}</button>
  </form>;
}

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
        const url = new URL(connectUrlFor(provider), window.location.origin);
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
          Tu casilla usa otro proveedor. Puedes conectarla por IMAP con los datos de servidor
          y acceso que te entregue tu proveedor de correo.
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
