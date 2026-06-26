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
    body: "Contenido de ejemplo. Cada métrica es auditable hasta el correo que la originó.",
  },
  {
    Icon: HeartHandshake,
    title: "Respeto por los datos",
    body: "Contenido de ejemplo. Pedimos el mínimo acceso necesario y nunca reemplazamos tu correo.",
  },
  {
    Icon: Target,
    title: "Foco",
    body: "Contenido de ejemplo. Hacemos una cosa bien: medir tu casilla de soporte.",
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
            Contenido de ejemplo pendiente de redacción. Aquí va la historia del producto, el
            problema que resuelve y a quién está dirigido.
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
