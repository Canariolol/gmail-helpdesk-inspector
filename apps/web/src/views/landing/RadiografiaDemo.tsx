import { useEffect, useRef, useState } from "react";
import { RotateCcw, Sparkles } from "lucide-react";

/**
 * La radiografía: un hilo de correo que se analiza frente al visitante.
 * Al pulsar "Analizar", una línea de escaneo recorre el correo y van
 * apareciendo las anotaciones de Mira (remitente externo, recepción,
 * clasificación, tiempo de primera respuesta) hasta actualizar el marcador.
 */

const STEP_DELAYS_MS = [300, 700, 1300, 1900, 2600];
const RESPONSE_MINUTES = 42;

function prefersReducedMotion(): boolean {
  return typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function RadiografiaDemo() {
  const [step, setStep] = useState(0);
  const [chrono, setChrono] = useState(0);
  const timeouts = useRef<number[]>([]);
  const raf = useRef<number>(0);

  useEffect(() => {
    return () => {
      timeouts.current.forEach(clearTimeout);
      cancelAnimationFrame(raf.current);
    };
  }, []);

  // El cronómetro corre cuando la anotación de respuesta aparece.
  useEffect(() => {
    if (step < 5) return;
    if (prefersReducedMotion()) {
      setChrono(RESPONSE_MINUTES);
      return;
    }
    const start = performance.now();
    const tick = (now: number) => {
      const progress = Math.min((now - start) / 800, 1);
      setChrono(Math.round(RESPONSE_MINUTES * progress));
      if (progress < 1) raf.current = requestAnimationFrame(tick);
    };
    raf.current = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf.current);
  }, [step]);

  function analyze() {
    timeouts.current.forEach(clearTimeout);
    setChrono(0);
    if (prefersReducedMotion()) {
      setStep(6);
      return;
    }
    setStep(1);
    timeouts.current = STEP_DELAYS_MS.map((ms, i) =>
      window.setTimeout(() => setStep(i + 2), ms),
    );
  }

  const running = step > 0 && step < 6;
  const done = step >= 6;

  return (
    <div className="rx">
      <div className={`rx-mail${running ? " rx-scanning" : ""}`}>
        <span className="rx-scanline" aria-hidden="true" />

        <div className="rx-mail-head">
          <div className="rx-mail-row">
            <span className="rx-mail-label">De</span>
            <span>
              carla@<em className={step >= 2 ? "rx-mark on" : "rx-mark"}>clienteexterno.cl</em>
            </span>
            <span className={`rx-tag${step >= 2 ? " on" : ""}`}>externo</span>
          </div>
          <div className="rx-mail-row">
            <span className="rx-mail-label">Para</span>
            <span>soporte@tuempresa.cl</span>
          </div>
          <div className="rx-mail-row">
            <span className="rx-mail-label">Asunto</span>
            <strong>Error al generar factura</strong>
            <span className={`rx-chip${step >= 4 ? " on" : ""}`}>Solicitud válida</span>
          </div>
        </div>

        <div className="rx-msg">
          <header>
            <strong>Carla Fuentes</strong>
            <span className="rx-time">
              lu 09:12
              <i className={`rx-tag${step >= 3 ? " on" : ""}`}>recepción</i>
            </span>
          </header>
          <p>
            Hola, al descargar la factura de junio el sistema arroja un error 500. ¿Me pueden
            ayudar? Es urgente para el cierre contable.
          </p>
        </div>

        <div className={`rx-msg rx-msg-reply${step >= 5 ? " on" : ""}`}>
          <header>
            <strong>Soporte · tuempresa.cl</strong>
            <span className="rx-time">
              lu 09:54
              <i className={`rx-tag rx-tag-ok${step >= 5 ? " on" : ""}`}>
                1ª respuesta · {chrono} min
              </i>
            </span>
          </header>
          <p>
            Hola Carla, lo reproducimos y ya está corregido: vuelve a intentar la descarga. Quedamos
            atentos.
          </p>
        </div>

        <div className="rx-actions">
          {step === 0 && (
            <button type="button" className="lp-btn-primary" onClick={analyze}>
              <Sparkles size={16} /> Analizar este hilo
            </button>
          )}
          {done && (
            <button type="button" className="lp-btn-ghost" onClick={analyze}>
              <RotateCcw size={15} /> Analizar de nuevo
            </button>
          )}
        </div>
      </div>

      <aside className="rx-board" aria-label="Métricas del análisis">
        <span className="rx-board-title">Marcador</span>
        <div className={`rx-stat${done ? " pop" : ""}`}>
          <span>Válidas</span>
          <strong>{done ? 173 : 172}</strong>
        </div>
        <div className="rx-stat">
          <span>Respondidas</span>
          <strong>91%</strong>
        </div>
        <div className={`rx-stat${done ? " pop" : ""}`}>
          <span>1ª respuesta</span>
          <strong>{done ? `${RESPONSE_MINUTES} min` : "—"}</strong>
        </div>
        <p className="rx-board-note">
          {done
            ? "Cada cifra queda enlazada a este hilo: auditable de punta a punta."
            : "Pulsa analizar y mira cómo se construye la métrica."}
        </p>
      </aside>
    </div>
  );
}
