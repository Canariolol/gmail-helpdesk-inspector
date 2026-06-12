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

// Recharts necesita valores JS, no variables CSS.
export const chartColors = {
  valid: "#10b981",
  answered: "#3b82f6",
  unanswered: "#f97316",
  ignored: "#a8a29e",
  ambiguous: "#8b5cf6",
  avgResponse: "#f97316",
  p90Response: "#8b5cf6",
  resolution: "#14b8a6",
  axis: "#8c8178",
  grid: "#f0e3d6",
};
