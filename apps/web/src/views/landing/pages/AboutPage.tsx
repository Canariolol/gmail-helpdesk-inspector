import { ArrowRight, Eye, HeartHandshake, Target } from "lucide-react";
import { PublicPage } from "../PublicPage";

type Props = {
  onLogin: () => void;
  onSignup: () => void;
};

const VALUES = [
  {
    Icon: Eye,
    title: "Transparencia",
    body: "Cada métrica lleva a las conversaciones que la originaron. Puedes revisar los mensajes y corregir su clasificación.",
  },
  {
    Icon: HeartHandshake,
    title: "Respeto por los datos",
    body: "Leemos la casilla sin modificarla. Tú autorizas la auditoría con IA y puedes desactivarla desde Configuración.",
  },
  {
    Icon: Target,
    title: "Foco",
    body: "Medimos las solicitudes y respuestas observadas en tu correo. La atención realizada por otros canales requiere contexto adicional.",
  },
];

export function AboutPage({ onLogin, onSignup }: Props) {
  return (
    <PublicPage onLogin={onLogin} onSignup={onSignup}>
      <section className="lp-about">
        <header className="lp-about-head">
          <span className="lp-focus-eyebrow">Sobre nosotros</span>
          <h1>Medimos lo que tu casilla ya está haciendo</h1>
          <p>
            Mira Helpdesk convierte las conversaciones de atención en métricas que puedes revisar.
            Defines qué cuenta como solicitud, quién responde y qué casos ignorar. La versión actual
            admite una casilla y una cuenta propietaria por organización.
          </p>
        </header>

        <div className="lp-about-grid">
          {VALUES.map((value) => (
            <article key={value.title} className="lp-about-card">
              <span className="lp-about-icon">
                <value.Icon size={20} />
              </span>
              <h3>{value.title}</h3>
              <p>{value.body}</p>
            </article>
          ))}
        </div>

        <div className="lp-about-cta">
          <h2>¿Listo para medir tu casilla?</h2>
          <button type="button" className="lp-btn-primary lp-focus-cta" onClick={onSignup}>
            Crear cuenta <ArrowRight size={18} />
          </button>
        </div>
      </section>
    </PublicPage>
  );
}
