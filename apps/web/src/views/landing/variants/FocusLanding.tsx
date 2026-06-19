import { ArrowRight, Check, Lock } from "lucide-react";
import { ADVANTAGES, BRAND, HERO, REPORTS, STEPS, TRUST_BAND } from "../content";
import { DemoInspector } from "../mocks/DemoInspector";
import { ReporteMock } from "../mocks/ReporteMock";
import { Reveal } from "../Reveal";

interface VariantProps {
  onLogin: () => void;
}

export function FocusLanding({ onLogin }: VariantProps) {
  const hero = HERO.focus;

  return (
    <div className="lp lp-focus">
      <header className="lp-focus-nav">
        <span className="lp-brand">
          <img src="/logo-192.png" alt="" />
          {BRAND.short}
        </span>
        <button type="button" className="lp-focus-link" onClick={onLogin}>
          {hero.cta}
        </button>
      </header>

      <main>
        <section className="lp-focus-hero">
          <div className="lp-focus-hero-head">
            <span className="lp-focus-kicker">{hero.kicker}</span>
            <h1>
              {hero.title} <em><br />{hero.highlight}</em>
            </h1>
            <p>{hero.subtitle}</p>
            <div className="lp-focus-hero-actions">
              <button type="button" className="lp-btn-primary lp-focus-cta" onClick={onLogin}>
                {hero.cta} <ArrowRight size={18} />
              </button>
              <span className="lp-inline-note lp-focus-note">
                <Lock size={14} /> Solo lectura. Punto.
              </span>
            </div>
          </div>
          <DemoInspector />
        </section>

        <section className="lp-focus-trustband" aria-label="Garantías de privacidad">
          <strong>{TRUST_BAND.headline}</strong>
          <ul>
            {TRUST_BAND.items.map((item) => (
              <li key={item}>
                <Check size={15} /> {item}
              </li>
            ))}
          </ul>
        </section>

        <section className="lp-focus-advantages">
          {ADVANTAGES.map((advantage, index) => (
            <Reveal key={advantage.id} delay={index * 60}>
              <article className="lp-focus-adv">
                <span className="lp-focus-adv-icon">
                  <advantage.Icon size={20} />
                </span>
                <div>
                  <h3>{advantage.title}</h3>
                  <p>{advantage.body}</p>
                </div>
              </article>
            </Reveal>
          ))}
        </section>

        <Reveal className="lp-focus-reports">
          <div className="lp-focus-reports-copy">
            <span className="lp-focus-eyebrow">{REPORTS.eyebrow}</span>
            <h2>{REPORTS.title}</h2>
            <p>{REPORTS.body}</p>
            <ul className="lp-focus-reports-list">
              {REPORTS.bullets.map((bullet) => (
                <li key={bullet}>
                  <Check size={16} /> {bullet}
                </li>
              ))}
            </ul>
          </div>
          <div className="lp-focus-reports-mock">
            <ReporteMock />
          </div>
        </Reveal>

        <Reveal className="lp-focus-section">
          <span className="lp-focus-eyebrow">Cómo funciona</span>
          <h2>Tres pasos. Sin servidores.</h2>
          <ol className="lp-focus-steps-row">
            {STEPS.map((step) => (
              <li key={step.n}>
                <span className="lp-focus-step-n">{step.n}</span>
                <h3>{step.title}</h3>
                <p>{step.body}</p>
              </li>
            ))}
          </ol>
        </Reveal>

        <Reveal className="lp-focus-final">
          <h2>Empieza a medir hoy.</h2>
          <button type="button" className="lp-btn-primary lp-focus-cta" onClick={onLogin}>
            {hero.cta} <ArrowRight size={18} />
          </button>
        </Reveal>
      </main>

      <footer className="lp-focus-footer">
        <span>{BRAND.name}</span>
        <span>Beta privada · no afiliado a Google LLC.</span>
      </footer>
    </div>
  );
}
