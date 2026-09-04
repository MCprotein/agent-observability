import type { Metrics, Span } from "./generated/report-dto-v2.js";

export type CostStatus = "estimated" | "incomplete" | "unknown";
export type TokenStatus = "complete" | "incomplete" | "unavailable";

export interface ViewSummary {
  sessions: number;
  turns: number;
  llmRequests: number;
  toolExecutions: number;
  errors: number;
  inputTokens: number;
  outputTokens: number;
  totalTokens: number | undefined;
  tokenStatus: TokenStatus;
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
    totalTokens: undefined,
    tokenStatus: "unavailable",
    estimatedCost: 0,
    costStatus: aggregateCostStatus(billable.map((span) => span.cost.status)),
  };
  let completeTokenTotal = 0;
  let completeTokenSpans = 0;
  let incompleteTokenSpans = 0;
  for (const span of spans) {
    if (span.sessionId) sessions.add(span.sessionId);
    if (span.turnId) turns.add(span.turnId);
    if (span.kind === "llm.request") summary.llmRequests += 1;
    if (span.kind === "tool.execution") summary.toolExecutions += 1;
    if (span.status === "error") summary.errors += 1;
    summary.inputTokens += span.metrics.inputTokens ?? 0;
    summary.outputTokens += span.metrics.outputTokens ?? 0;
    const spanTokenTotal = tokenTotal(span.metrics);
    if (spanTokenTotal !== undefined && span.availability.tokens.state === "available") {
      completeTokenTotal += spanTokenTotal;
      completeTokenSpans += 1;
    } else if (span.availability.tokens.state !== "not_applicable") {
      incompleteTokenSpans += 1;
    }
    summary.estimatedCost += span.estimatedCost ?? 0;
  }
  summary.sessions = sessions.size;
  summary.turns = turns.size;
  summary.tokenStatus = incompleteTokenSpans > 0
    ? "incomplete"
    : completeTokenSpans > 0 ? "complete" : "unavailable";
  summary.totalTokens = summary.tokenStatus === "complete" ? completeTokenTotal : undefined;
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
