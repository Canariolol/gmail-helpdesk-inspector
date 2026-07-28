import type { AnalysisStatus, Classification } from "../api/types";

export type ChipTone = "mint" | "blue" | "orange" | "purple" | "teal" | "amber" | "red" | "gray";

export const classificationTones: Record<Classification, ChipTone> = {
  valid_client_request: "mint",
  internal: "blue",
  automated: "gray",
  newsletter: "purple",
  spam: "red",
  misc: "gray",
  ambiguous: "amber",
};

export const statusTones: Record<AnalysisStatus, ChipTone> = {
  pending: "gray",
  running: "blue",
  completed: "mint",
  failed: "red",
};

// Recharts escribe estos valores como atributos SVG (fill/stroke), que sí
// aceptan var(--token): los gráficos siguen el tema activo del contrato.
export const chartColors = {
  valid: "var(--ok)",
  answered: "var(--info)",
  unanswered: "var(--accent)",
  ignored: "var(--chip-gray-fg)",
  ambiguous: "var(--chip-purple-fg)",
  avgResponse: "var(--accent)",
  p90Response: "var(--chip-purple-fg)",
  resolution: "var(--chip-teal-fg)",
  axis: "var(--text-muted)",
  grid: "var(--border)",
};
