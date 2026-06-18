import { Lock } from "lucide-react";

const REPORT_STATS = [
  { label: "Analizados", value: "428", tone: "blue" },
  { label: "Válidas", value: "173", tone: "mint" },
  { label: "Respondidas", value: "91%", tone: "teal" },
  { label: "Sin respuesta", value: "14", tone: "orange" },
  { label: "1ª respuesta", value: "42 min", tone: "amber" },
];

export function ReporteMock() {
  return (
    <div className="lp-rep" aria-hidden="true">
      <header className="lp-rep-head">
        <div className="lp-rep-dots">
          <span /> <span /> <span />
        </div>
        <span className="lp-rep-app">Bandeja de entrada</span>
      </header>

      <div className="lp-rep-meta">
        <p>
          <span>De</span> Helpdesk Inspector
        </p>
        <p>
          <span>Para</span> equipo@empresa.cl
        </p>
        <p className="lp-rep-subject">
          <span>Asunto</span> Reporte semanal de soporte · 9–15 jun
        </p>
      </div>

      <div className="lp-rep-body">
        <span className="lp-rep-greeting">Hola 👋 Esto pasó esta semana en tu casilla de soporte:</span>
        <div className="lp-rep-stats">
          {REPORT_STATS.map((stat) => (
            <div className={`lp-rep-stat lp-tone-${stat.tone}`} key={stat.label}>
              <strong>{stat.value}</strong>
              <span>{stat.label}</span>
            </div>
          ))}
        </div>
        <p className="lp-rep-note">
          <Lock size={13} /> Solo métricas · sin asuntos ni remitentes de clientes
        </p>
      </div>
    </div>
  );
}
