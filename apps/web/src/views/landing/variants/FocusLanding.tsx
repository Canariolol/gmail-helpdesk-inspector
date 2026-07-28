import { useEffect, useState } from "react";
import { ArrowRight, Check, Lock } from "lucide-react";
import { ADVANTAGES, HERO, REPORTS, STEPS, TRUST_BAND } from "../content";
import { DemoInspector } from "../mocks/DemoInspector";
import { PublicHeader } from "../PublicHeader";
import { PublicFooter } from "../PublicFooter";
import { RadiografiaDemo } from "../RadiografiaDemo";
import { ThemeDots } from "../ThemeDots";
import { ReporteMock } from "../mocks/ReporteMock";
import { Reveal } from "../Reveal";

interface VariantProps {
  onLogin: () => void;
  onSignup: () => void;
}

/** Cuenta de 0 al objetivo al montar (salta directo con reduced motion). */
function useCountUp(target: number, durationMs = 900): number {
  const [value, setValue] = useState(0);
  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      setValue(target);
      return;
    }
    let raf = 0;
    const start = performance.now();
    const tick = (now: number) => {
      const progress = Math.min((now - start) / durationMs, 1);
      // ease-out cúbico: entra rápido y aterriza suave
      setValue(Math.round(target * (1 - Math.pow(1 - progress, 3))));
      if (progress < 1) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [target, durationMs]);
  return value;
}

export function FocusLanding({ onLogin, onSignup }: VariantProps) {
  const hero = HERO.focus;
  const answered = useCountUp(91);

  return (
    <div className="lp lp-focus">
      <PublicHeader onLogin={onLogin} onSignup={onSignup} />

      <main>
        <section className="lp-focus-hero">
          <div className="lp-focus-hero-head">
            <span className="lp-focus-kicker">/ {hero.kicker}</span>
            <h1>
              {hero.title} <em>{hero.highlight}</em>
            </h1>
            <p>{hero.subtitle}</p>
            <div className="lp-focus-hero-actions">
              <button type="button" className="lp-btn-primary lp-focus-cta" onClick={onSignup}>
                {hero.cta} <ArrowRight size={18} />
              </button>
              <span className="lp-inline-note lp-focus-note">
                <Lock size={14} /> Solo lectura. Punto.
              </span>
            </div>
          </div>
          <div className="lp-focus-stat" aria-hidden="true">
            <strong>
              {answered}<i>%</i>
            </strong>
            <span className="lp-focus-stat-pulse" />
            <span className="lp-focus-stat-label">solicitudes respondidas / semana</span>
            <svg className="lp-focus-spark" width="240" height="56" viewBox="0 0 240 56">
              <line x1="0" y1="55" x2="240" y2="55" />
              <polyline points="4,30 42,34 80,10 118,22 156,36 194,44 236,18" />
            </svg>
          </div>
        </section>

        <Reveal className="lp-focus-rx">
          <span className="lp-focus-eyebrow">Así audita Mira</span>
          <h2>Un correo, radiografiado.</h2>
          <RadiografiaDemo />
        </Reveal>

        <Reveal>
          <section className="lp-focus-trustband" aria-label="Garantías de privacidad">
            <strong>{TRUST_BAND.headline}</strong>
            <ul>
              {TRUST_BAND.items.map((item, index) => {
                const action = item.replace(/^No\s+/i, "");
                return (
                  <li key={item} style={{ "--strike-i": index } as React.CSSProperties}>
                    <Check size={15} /> No <s className="lp-strike">{action}</s>
                  </li>
                );
              })}
            </ul>
          </section>
        </Reveal>

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
                {advantage.id === "ia" && (
                  <img
                    className="lp-adv-media"
                    src="/mira2.webp"
                    alt=""
                    loading="lazy"
                    width={900}
                    height={886}
                  />
                )}
              </article>
            </Reveal>
          ))}
        </section>

        <Reveal className="lp-focus-demo">
          <span className="lp-focus-eyebrow">Explora el panel</span>
          <DemoInspector />
        </Reveal>

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
          <h2>Tres pasos y listo.</h2>
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
          <img
            className="lp-mira-card"
            src="/mira1.webp"
            alt="Mira, la ninfa auditora, inspeccionando un correo con su lupa"
            loading="lazy"
            width={1000}
            height={871}
          />
          <div className="lp-focus-final-copy">
            <span className="lp-pulse-divider" aria-hidden="true" />
            <h2>Empieza a medir hoy.</h2>
            <p>Mira ya está lista para auditar tu casilla. Tú solo conéctala.</p>
            <button type="button" className="lp-btn-primary lp-focus-cta" onClick={onSignup}>
              {hero.cta} <ArrowRight size={18} />
            </button>
          </div>
        </Reveal>
      </main>

      <PublicFooter />
      <ThemeDots />
    </div>
  );
}
