import type { AnalysisRun, AnalysisStatus, Classification, EmailThread } from "../api/types";

export const classificationLabels: Record<Classification, string> = {
  valid_client_request: "Solicitud válida",
  internal: "Interno",
  automated: "Automático",
  newsletter: "Boletín",
  spam: "Correo no deseado",
  misc: "Ignorado",
  ambiguous: "Ambiguo",
};

export const statusLabels: Record<AnalysisStatus, string> = {
  pending: "Pendiente",
  running: "Analizando",
  completed: "Completado",
  failed: "Fallido",
};

const sourceLabels: Record<string, string> = {
  rules: "Clasificado automáticamente",
  heuristics: "Clasificado automáticamente",
  ai: "Revisado por Mira",
  manual: "Revisado por tu equipo",
};

const reasonLabels: Record<string, string> = {
  "thread has no readable messages": "El hilo no tiene mensajes legibles.",
  "thread starts with an internal sender; outbound-origin threads are excluded": "El hilo comenzó con un correo interno; se excluye por ser saliente.",
  "thread starts with an automated external sender": "El hilo comenzó con un remitente externo automático.",
  "subject matches ignored keyword": "El asunto coincide con una palabra ignorada.",
  "all participants are internal": "Todos los participantes son internos.",
  "external messages look automated": "Los mensajes externos parecen automáticos.",
  "subject looks like newsletter": "El asunto parece un boletín o correo promocional.",
  "sender/domain is ignored by configuration": "El remitente o dominio está configurado como ignorado.",
  "no clear human external sender found": "No se encontró un remitente externo humano claro.",
  "first relevant message comes from an external human sender": "El primer mensaje relevante viene de un remitente externo humano.",
  "found later internal non-automated reply": "Se encontró una respuesta interna posterior no automática.",
  "no later internal reply found": "No se encontró una respuesta interna posterior.",
  "thread shape is suspicious and should be audited": "Mira detectó señales que requieren revisión.",
  "AI audit auto-applied above strict threshold": "Mira confirmó esta clasificación con alta confianza.",
  "AI audit requires manual confirmation": "Mira necesita tu confirmación para esta clasificación.",
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

export function threadReceivedAt(thread: EmailThread): string | null {
  return thread.first_message_at ?? thread.first_client_message_at;
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

export function formatNumber(value: number): string {
  return new Intl.NumberFormat("es-CL").format(value);
}

export function formatPercent(value: number | null | undefined): string {
  if (value === null || value === undefined) return "Sin dato";
  return `${Math.round(value * 100)}%`;
}

export function formatSource(value: string): string {
  return sourceLabels[value] ?? value;
}

export function formatReason(value: string): string {
  if (reasonLabels[value]) return reasonLabels[value];
  if (value.startsWith("AI audit failed:")) {
    return value.replace("AI audit failed:", "Mira no pudo completar esta revisión:");
  }
  return value;
}

export function runRangeLabel(run: AnalysisRun): string {
  const { date_from, date_to, time_from, time_to } = run.config;
  return `${date_from} ${time_from ?? "00:00"} a ${date_to} ${time_to ?? "23:59"}`;
}
