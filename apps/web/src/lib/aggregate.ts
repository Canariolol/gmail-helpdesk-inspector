import type { AnalysisRun, Metrics } from "../api/types";

export interface RunTrendPoint {
  runId: string;
  label: string;
  createdAt: string;
  avgFirstResponse: number | null;
  p90FirstResponse: number | null;
  avgResolution: number | null;
  valid: number;
  answered: number;
  unanswered: number;
  ignored: number;
  ambiguous: number;
  pendingReview: number;
  confidence: number;
}

export function selectReportRuns(runs: AnalysisRun[], from: string, to: string): AnalysisRun[] {
  return runs.filter(
    (run) =>
      run.status === "completed" &&
      run.config.date_from <= to &&
      run.config.date_to >= from,
  );
}

function shortDate(value: string): string {
  return `${value.slice(8, 10)}-${value.slice(5, 7)}`;
}

export function buildTrendPoints(runs: AnalysisRun[]): RunTrendPoint[] {
  return [...runs]
    .sort((a, b) => a.created_at.localeCompare(b.created_at))
    .map((run) => ({
      runId: run.id,
      label: `${shortDate(run.config.date_from)} a ${shortDate(run.config.date_to)}`,
      createdAt: run.created_at,
      avgFirstResponse: run.metrics.avg_first_response_minutes,
      p90FirstResponse: run.metrics.p90_first_response_minutes,
      avgResolution: run.metrics.avg_resolution_minutes,
      valid: run.metrics.valid_requests,
      answered: run.metrics.answered,
      unanswered: run.metrics.unanswered,
      ignored: run.metrics.ignored,
      ambiguous: run.metrics.ambiguous,
      pendingReview: run.metrics.pending_review,
      confidence: run.metrics.report_confidence,
    }));
}
