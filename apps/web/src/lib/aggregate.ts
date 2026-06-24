import type { AnalysisRun, Metrics } from "../api/types";

export interface ReportTotals {
  runCount: number;
  totalThreads: number;
  validRequests: number;
  answered: number;
  unanswered: number;
  ignored: number;
  ambiguous: number;
  pendingReview: number;
  manualOverrides: number;
  aiInputTokens: number;
  aiOutputTokens: number;
  avgFirstResponseMinutes: number | null;
  avgResolutionMinutes: number | null;
  reportConfidence: number | null;
}

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

function sumMetric(runs: AnalysisRun[], pick: (metrics: Metrics) => number): number {
  return runs.reduce((acc, run) => acc + pick(run.metrics), 0);
}

function weightedAverage(
  runs: AnalysisRun[],
  pickValue: (metrics: Metrics) => number | null,
  pickWeight: (metrics: Metrics) => number,
): number | null {
  let weightedSum = 0;
  let totalWeight = 0;
  for (const run of runs) {
    const value = pickValue(run.metrics);
    const weight = pickWeight(run.metrics);
    if (value === null || weight <= 0) continue;
    weightedSum += value * weight;
    totalWeight += weight;
  }
  return totalWeight > 0 ? weightedSum / totalWeight : null;
}

export function aggregateRuns(runs: AnalysisRun[]): ReportTotals {
  return {
    runCount: runs.length,
    totalThreads: sumMetric(runs, (m) => m.total_threads),
    validRequests: sumMetric(runs, (m) => m.valid_requests),
    answered: sumMetric(runs, (m) => m.answered),
    unanswered: sumMetric(runs, (m) => m.unanswered),
    ignored: sumMetric(runs, (m) => m.ignored),
    ambiguous: sumMetric(runs, (m) => m.ambiguous),
    pendingReview: sumMetric(runs, (m) => m.pending_review),
    manualOverrides: sumMetric(runs, (m) => m.manual_overrides),
    aiInputTokens: sumMetric(runs, (m) => m.ai_input_tokens),
    aiOutputTokens: sumMetric(runs, (m) => m.ai_output_tokens),
    avgFirstResponseMinutes: weightedAverage(runs, (m) => m.avg_first_response_minutes, (m) => m.answered),
    avgResolutionMinutes: weightedAverage(runs, (m) => m.avg_resolution_minutes, (m) => m.answered),
    reportConfidence: weightedAverage(runs, (m) => m.report_confidence, (m) => m.total_threads),
  };
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
