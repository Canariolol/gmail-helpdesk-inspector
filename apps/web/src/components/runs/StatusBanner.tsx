import { BarChart3, CheckCircle2, CircleAlert, Hourglass, Play } from "lucide-react";
import type { AnalysisRun } from "../../api/types";
import { formatDateTime } from "../../lib/format";

type Props = {
  run: AnalysisRun;
  onStart: () => void;
  starting: boolean;
  onViewDetails: () => void;
};

const bannerCopy = {
  pending: { icon: Hourglass, title: "Análisis pendiente" },
  running: { icon: BarChart3, title: "Analizando correos..." },
  completed: { icon: CheckCircle2, title: "Análisis completado" },
  failed: { icon: CircleAlert, title: "Análisis fallido" },
} as const;

export function StatusBanner({ run, onStart, starting, onViewDetails }: Props) {
  const progress = run.total_candidate_threads
    ? Math.round((run.processed_threads / run.total_candidate_threads) * 100)
    : 0;
  const { icon: Icon, title } = bannerCopy[run.status];

  const subtitle =
    run.status === "completed"
      ? formatDateTime(run.completed_at)
      : run.status === "failed"
        ? (run.error_message ?? run.progress_message)
        : run.progress_message;

  return (
    <section className={`status-banner status-${run.status}`}>
      <div className="banner-content">
        <span className="banner-kicker">
          <Icon size={18} />
          {title}
        </span>
        {subtitle && <span className="banner-subtitle">{subtitle}</span>}
        <span className="banner-meta">
          {run.processed_threads}/{run.total_candidate_threads} hilos analizados
        </span>
        {run.status === "running" && (
          <div className="progress-bar" role="progressbar" aria-valuenow={progress} aria-valuemin={0} aria-valuemax={100}>
            <span style={{ width: `${progress}%` }} />
          </div>
        )}
        {(run.status === "pending" || run.status === "failed") && (
          <button type="button" className="btn-ghost" disabled={starting} onClick={onStart}>
            <Play size={16} />
            {run.status === "failed" ? "Reintentar" : "Iniciar análisis"}
          </button>
        )}
        {run.status === "completed" && (
          <button type="button" className="btn-ghost" onClick={onViewDetails}>
            Ver detalles
          </button>
        )}
      </div>
      <BannerArt failed={run.status === "failed"} />
    </section>
  );
}

function BannerArt({ failed }: { failed: boolean }) {
  return (
    <svg className="banner-art" viewBox="0 0 420 160" preserveAspectRatio="xMaxYMax slice" aria-hidden="true">
      <defs>
        <linearGradient id="banner-sky" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={failed ? "#fee2e2" : "#ffe3b3"} />
          <stop offset="60%" stopColor={failed ? "#fecaca" : "#ffc287"} />
          <stop offset="100%" stopColor={failed ? "#fca5a5" : "#ffab66"} />
        </linearGradient>
        <linearGradient id="banner-far" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={failed ? "#f87171" : "#e1739b"} />
          <stop offset="100%" stopColor={failed ? "#dc2626" : "#b25f93"} />
        </linearGradient>
        <linearGradient id="banner-near" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={failed ? "#b91c1c" : "#a14883"} />
          <stop offset="100%" stopColor={failed ? "#7f1d1d" : "#6d3361"} />
        </linearGradient>
      </defs>
      <rect width="420" height="160" fill="url(#banner-sky)" />
      <circle cx="330" cy="92" r="42" fill="#fff7e6" opacity="0.35" />
      <circle cx="330" cy="92" r="26" fill="#fffaf0" opacity="0.85" />
      <ellipse cx="140" cy="38" rx="46" ry="10" fill="#fff7ed" opacity="0.55" />
      <ellipse cx="250" cy="24" rx="34" ry="8" fill="#fff7ed" opacity="0.4" />
      <path d="M0 160 L70 78 L150 160 Z" fill="url(#banner-far)" opacity="0.55" />
      <path d="M90 160 L190 60 L300 160 Z" fill="url(#banner-far)" opacity="0.75" />
      <path d="M210 160 L330 72 L420 140 L420 160 Z" fill="url(#banner-near)" opacity="0.9" />
      <path d="M310 160 L420 96 L420 160 Z" fill="url(#banner-near)" />
    </svg>
  );
}
