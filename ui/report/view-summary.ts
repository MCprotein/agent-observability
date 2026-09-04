import type { Metrics, Span } from "./generated/report-dto-v2.js";

export type CostStatus = "estimated" | "incomplete" | "unknown";

export interface ViewSummary {
  sessions: number;
  turns: number;
  llmRequests: number;
  toolExecutions: number;
  errors: number;
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  estimatedCost: number;
  costStatus: CostStatus;
}

const NON_TOTAL_TOKEN_METRICS = [
  "cachedInputTokens",
  "cacheCreationInputTokens",
  "reasoningOutputTokens",
] as const;

export function summarizeVisible(spans: Span[]): ViewSummary {
  const sessions = new Set<string>();
  const turns = new Set<string>();
  const billable = spans.filter(hasTokenMetrics);
  const summary: ViewSummary = {
    sessions: 0,
    turns: 0,
    llmRequests: 0,
    toolExecutions: 0,
    errors: 0,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    estimatedCost: 0,
    costStatus: aggregateCostStatus(billable.map((span) => span.cost.status)),
  };
  for (const span of spans) {
    if (span.sessionId) sessions.add(span.sessionId);
    if (span.turnId) turns.add(span.turnId);
    if (span.kind === "llm.request") summary.llmRequests += 1;
    if (span.kind === "tool.execution") summary.toolExecutions += 1;
    if (span.status === "error") summary.errors += 1;
    summary.inputTokens += span.metrics.inputTokens ?? 0;
    summary.outputTokens += span.metrics.outputTokens ?? 0;
    summary.totalTokens += tokenTotal(span.metrics) ?? 0;
    summary.estimatedCost += span.estimatedCost ?? 0;
  }
  summary.sessions = sessions.size;
  summary.turns = turns.size;
  return summary;
}

export function tokenTotal(metrics: Metrics): number | undefined {
  const direct = sumComplete(metrics.inputTokens, metrics.outputTokens);
  const cumulative = sumComplete(metrics.totalInputTokens, metrics.totalOutputTokens);
  return direct ?? metrics.totalTokens ?? cumulative ?? metrics.totalAccumulatedTokens;
}

export function aggregateCostStatus(statuses: string[]): CostStatus {
  const estimated = statuses.filter((status) => status === "estimated").length;
  const incomplete = statuses.filter((status) => status === "incomplete").length;
  const unknown = statuses.filter((status) => status === "unknown").length;
  if (estimated === 0 && incomplete === 0) return "unknown";
  if (incomplete > 0 || unknown > 0) return "incomplete";
  return "estimated";
}

function hasTokenMetrics(span: Span): boolean {
  return tokenTotal(span.metrics) !== undefined
    || NON_TOTAL_TOKEN_METRICS.some((key) => span.metrics[key] !== undefined);
}

function sumComplete(left: number | undefined, right: number | undefined): number | undefined {
  return left === undefined || right === undefined ? undefined : left + right;
}
