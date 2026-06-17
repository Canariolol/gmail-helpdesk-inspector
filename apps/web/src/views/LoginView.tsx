import { BarChart3, CheckCircle2, FileText, LogIn, Mail, ShieldCheck, Sparkles, X } from "lucide-react";
import { useMemo, useState } from "react";
import { API_BASE_URL } from "../api/client";

const waitlistEmail = "admin@orionsolutions.cl";

export function LoginView() {
  const [showOAuthDetails, setShowOAuthDetails] = useState(false);
  const [waitlistForm, setWaitlistForm] = useState({
    name: "",
    email: "",
    company: "",
    teamSize: "1-2",
    problem: "",
  });
  const [waitlistSubmitted, setWaitlistSubmitted] = useState(false);
  const loginUrl = `${API_BASE_URL}/auth/google/login`;

  const mailtoHref = useMemo(() => {
    const subject = encodeURIComponent("Solicitud beta privada — Gmail Helpdesk Inspector");
    const body = encodeURIComponent(
      [
        `Nombre: ${waitlistForm.name}`,
        `Email corporativo: ${waitlistForm.email}`,
        `Empresa: ${waitlistForm.company}`,
        `Equipo de soporte: ${waitlistForm.teamSize}`,
        `Problema a resolver: ${waitlistForm.problem}`,
      ].join("\n"),
    );
    return `mailto:${waitlistEmail}?subject=${subject}&body=${body}`;
  }, [waitlistForm]);

  function updateField(key: keyof typeof waitlistForm, value: string) {
    setWaitlistForm((current) => ({ ...current, [key]: value }));
  }

  function handleWaitlistSubmit(event: React.FormEvent) {
    event.preventDefault();
    setWaitlistSubmitted(true);
    window.location.href = mailtoHref;
  }

  return (
    <main className="landing-screen">
      <nav className="landing-nav" aria-label="Navegación pública">
        <a className="landing-brand" href="#top" aria-label="Gmail Helpdesk Inspector">
          <img src="/logo-192.png" alt="" />
          <span>Gmail Helpdesk Inspector</span>
        </a>
        <div className="landing-nav-links">
          <a href="#como-funciona">Cómo funciona</a>
          <a href="#privacidad">Privacidad</a>
          <a href="#faq">FAQ</a>
          <a className="landing-nav-cta" href="#waitlist">Solicitar acceso</a>
        </div>
      </nav>

      <section id="top" className="landing-hero">
        <div className="landing-hero-copy">
          <span className="landing-kicker">Beta privada · Google Workspace</span>
          <h1>Tu casilla de soporte tiene historias que contar. Ahora puedes leerlas.</h1>
          <p>
            Analiza Gmail como canal de helpdesk y convierte hilos en métricas operativas reales: solicitudes válidas,
            casos respondidos, primera respuesta y asuntos que requieren revisión — sin tocar un solo mensaje.
          </p>
          <div className="landing-actions">
            <a className="btn-primary" href="#waitlist">
              <Mail size={18} /> Solicitar acceso a la beta
            </a>
            <button type="button" className="btn-ghost" onClick={() => setShowOAuthDetails(true)}>
              <LogIn size={18} /> Ya tengo acceso
            </button>
          </div>
          <div className="landing-trust-row" aria-label="Garantías de privacidad">
            <span>Gmail readonly</span>
            <span>No modifica correos</span>
            <span>IA opt-in</span>
            <span>Reportes metrics-only por defecto</span>
          </div>
        </div>
        <div className="landing-hero-card" aria-label="Vista previa de métricas">
          <div className="hero-card-head">
            <span>Resumen semanal</span>
            <strong>Demo</strong>
          </div>
          <div className="hero-metric-grid">
            <div><span>Analizados</span><strong>428</strong></div>
            <div><span>Válidos</span><strong>173</strong></div>
            <div><span>Respondidos</span><strong>91%</strong></div>
            <div><span>Revisión</span><strong>12</strong></div>
          </div>
          <div className="hero-thread-list">
            <span><CheckCircle2 size={15} /> Solicitud respondida · 42 min</span>
            <span><Sparkles size={15} /> Ambiguo: revisar responsabilidad</span>
            <span><ShieldCheck size={15} /> Newsletter ignorada</span>
          </div>
        </div>
      </section>

      <section className="landing-section landing-problem">
        <div>
          <span className="landing-section-label">El problema</span>
          <h2>¿Cuántas solicitudes reales llegaron este mes?</h2>
        </div>
        <div className="landing-copy-stack">
          <p>
            Si tu equipo usa Gmail como soporte, sabes que responde correos, pero no siempre cuántos eran solicitudes
            reales, cuántos quedaron sin respuesta o dónde se perdió tiempo.
          </p>
          <p>
            Los spreadsheets manuales se desactualizan. Un ticket desk puede ser demasiado pesado para equipos chicos.
            Gmail Helpdesk Inspector agrega una capa de medición sin reemplazar tu flujo actual.
          </p>
        </div>
      </section>

      <section id="como-funciona" className="landing-section">
        <div className="landing-section-head">
          <span className="landing-section-label">Cómo funciona</span>
          <h2>Tres pasos. Sin configurar servidores.</h2>
        </div>
        <div className="landing-step-grid">
          <article>
            <span>1</span>
            <h3>Conecta Workspace</h3>
            <p>Autoriza acceso de solo lectura a Gmail vía Google OAuth. La beta está pensada para cuentas empresariales.</p>
          </article>
          <article>
            <span>2</span>
            <h3>Define tu política</h3>
            <p>Configura dominios internos, criterios de solicitud válida, exclusiones, retención y si quieres IA.</p>
          </article>
          <article>
            <span>3</span>
            <h3>Genera métricas</h3>
            <p>Obtén reportes con trazabilidad a hilos reales y una cola clara de casos ambiguos para revisión humana.</p>
          </article>
        </div>
      </section>

      <section className="landing-section">
        <div className="landing-section-head">
          <span className="landing-section-label">Capacidades</span>
          <h2>Lo que mide. Lo que ignora.</h2>
        </div>
        <div className="landing-feature-grid">
          <article><BarChart3 size={22} /><h3>Métricas operativas</h3><p>Recibidos, válidos, respondidos, sin respuesta, tiempos de primera respuesta y confianza del reporte.</p></article>
          <article><FileText size={22} /><h3>Trazabilidad</h3><p>Cada número se puede revisar desde los hilos y mensajes compactos que lo originaron.</p></article>
          <article><ShieldCheck size={22} /><h3>Privacidad por diseño</h3><p>No almacenamos cuerpos completos. La lectura de contenido es transitoria durante el análisis.</p></article>
          <article><Sparkles size={22} /><h3>IA como apoyo</h3><p>La auditoría IA no viene activada por defecto. Requiere consentimiento y conserva revisión manual.</p></article>
        </div>
      </section>

      <section id="privacidad" className="landing-section landing-trust-panel">
        <div className="landing-section-head">
          <span className="landing-section-label">Trust & privacy</span>
          <h2>Diseñado para que puedas explicarlo a tu cliente.</h2>
        </div>
        <div className="landing-trust-grid">
          <div><strong>Solo lectura, siempre.</strong><p>El scope solicitado es <code>gmail.readonly</code>. No permite enviar, editar, archivar, etiquetar ni borrar correos.</p></div>
          <div><strong>IA opt-in.</strong><p>El análisis estándar puede funcionar sin IA. Si la activas, se envían extractos compactos y configurados por política.</p></div>
          <div><strong>Metadata, no cuerpos completos.</strong><p>Persistimos IDs, fechas, remitentes, asuntos, clasificaciones y métricas; no el cuerpo completo de tus correos.</p></div>
          <div><strong>Reportes prudentes.</strong><p>Los reportes programados son metrics-only por defecto y pueden omitir asuntos/remitentes.</p></div>
        </div>
      </section>

      <section id="faq" className="landing-section">
        <div className="landing-section-head">
          <span className="landing-section-label">FAQ</span>
          <h2>Preguntas frecuentes</h2>
        </div>
        <div className="landing-faq-grid">
          <details open><summary>¿Funciona con cualquier Gmail?</summary><p>No. La beta está enfocada en Google Workspace. Cuentas personales @gmail.com no son el objetivo inicial.</p></details>
          <details><summary>¿La app modifica mi correo?</summary><p>No. Usa solo <code>gmail.readonly</code>; no envía, borra, etiqueta ni archiva mensajes.</p></details>
          <details><summary>¿La IA lee todos mis correos?</summary><p>No. La IA es opcional y solo procesa los hilos incluidos en un análisis cuando la política lo permite.</p></details>
          <details><summary>¿Puedo recibir reportes automáticos?</summary><p>Sí, puedes activar una programación weekday 08:00 local. Por defecto el contenido del reporte es solo métricas.</p></details>
        </div>
      </section>

      <section id="waitlist" className="landing-section landing-waitlist">
        <div>
          <span className="landing-section-label">Acceso limitado</span>
          <h2>Solicita acceso a la beta privada</h2>
          <p>
            Estamos aceptando pocas organizaciones Workspace para validar privacidad, métricas y operación antes de abrir
            el producto públicamente. Responderemos por correo.
          </p>
        </div>
        <form className="waitlist-form" onSubmit={handleWaitlistSubmit}>
          <label>Nombre<input required value={waitlistForm.name} onChange={(e) => updateField("name", e.target.value)} placeholder="Tu nombre" /></label>
          <label>Email corporativo<input required type="email" value={waitlistForm.email} onChange={(e) => updateField("email", e.target.value)} placeholder="nombre@empresa.com" /></label>
          <label>Empresa<input required value={waitlistForm.company} onChange={(e) => updateField("company", e.target.value)} placeholder="Nombre de empresa" /></label>
          <label>Personas atendiendo soporte<select value={waitlistForm.teamSize} onChange={(e) => updateField("teamSize", e.target.value)}><option>1-2</option><option>3-10</option><option>+10</option></select></label>
          <label className="waitlist-form-wide">Problema que quieres resolver<textarea value={waitlistForm.problem} onChange={(e) => updateField("problem", e.target.value)} placeholder="Ej: medir solicitudes sin respuesta en nuestra casilla soporte@…" rows={3} /></label>
          <button type="submit" className="btn-primary">Solicitar acceso a la beta</button>
          {waitlistSubmitted && <p className="waitlist-note">Se abrió tu cliente de correo con la solicitud prellenada.</p>}
        </form>
      </section>

      <footer className="landing-footer">
        <span>Gmail Helpdesk Inspector · Beta privada</span>
        <span>No estamos afiliados a Google LLC.</span>
        <a href={`mailto:${waitlistEmail}`}>{waitlistEmail}</a>
      </footer>

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
              Vas a autorizar acceso de solo lectura a tu Gmail Workspace para construir métricas auditables de helpdesk.
            </p>
            <ul className="permission-list">
              <li><strong>Scope:</strong> <code>gmail.readonly</code>.</li>
              <li>No permite enviar, editar, etiquetar, archivar ni borrar correos.</li>
              <li>Los tokens se guardan cifrados y podrás revocar acceso desde tu cuenta Google.</li>
              <li>Los cuerpos completos solo se usan transitoriamente durante el análisis/auditoría.</li>
            </ul>
            <div className="modal-actions">
              <button type="button" className="btn-ghost" onClick={() => setShowOAuthDetails(false)}>Cancelar</button>
              <a className="btn-primary" href={loginUrl}><LogIn size={18} /> Continuar con Google</a>
            </div>
          </section>
        </div>
      )}
    </main>
  );
}
