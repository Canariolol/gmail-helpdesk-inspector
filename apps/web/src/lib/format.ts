import type { AnalysisRun, AnalysisStatus, Classification } from "../api/types";

export const classificationLabels: Record<Classification, string> = {
  valid_client_request: "Solicitud válida",
  internal: "Interno",
  automated: "Automático",
  newsletter: "Newsletter",
  spam: "Spam",
  misc: "Ignorado",
  ambiguous: "Ambiguo",
};

export const statusLabels: Record<AnalysisStatus, string> = {
  pending: "Pendiente",
  running: "Analizando",
  completed: "Completado",
  failed: "Fallido",
};

export function split(value: string): string[] {
  return value.split(",").map((item) => item.trim()).filter(Boolean);
}

export function formatDateTime(value: string | null | undefined): string {
  if (!value) return "Sin dato";
  return new Intl.DateTimeFormat("es-CL", {
    timeZone: "America/Santiago",
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

export function formatDuration(value: number | null | undefined): string {
  if (value === null || value === undefined) return "Sin dato";
  const minutes = Math.max(0, Math.round(value));
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours < 24) return rest ? `${hours} h ${rest} min` : `${hours} h`;
  const days = Math.floor(hours / 24);
  const dayHours = hours % 24;
  return dayHours ? `${days} d ${dayHours} h` : `${days} d`;
}

export function formatPercent(value: number | null | undefined): string {
  if (value === null || value === undefined) return "Sin dato";
  return `${Math.round(value * 100)}%`;
}

export function runRangeLabel(run: AnalysisRun): string {
  const { date_from, date_to, time_from, time_to } = run.config;
  return `${date_from} ${time_from ?? "00:00"} a ${date_to} ${time_to ?? "23:59"}`;
}
