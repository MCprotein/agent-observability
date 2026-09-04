/* Generated from contracts/report-dto-v2.schema.json. Do not edit. */

// ui/report/view-summary.ts
var TOKEN_METRICS = [
  "inputTokens",
  "outputTokens",
  "cachedInputTokens",
  "cacheCreationInputTokens",
  "reasoningOutputTokens"
];
function summarizeVisible(spans) {
  const sessions = /* @__PURE__ */ new Set();
  const turns = /* @__PURE__ */ new Set();
  const billable = spans.filter(hasTokenMetrics);
  const summary = {
    sessions: 0,
    turns: 0,
    llmRequests: 0,
    toolExecutions: 0,
    errors: 0,
    inputTokens: 0,
    outputTokens: 0,
    estimatedCost: 0,
    costStatus: aggregateCostStatus(billable.map((span) => span.cost.status))
  };
  for (const span of spans) {
    if (span.sessionId) sessions.add(span.sessionId);
    if (span.turnId) turns.add(span.turnId);
    if (span.kind === "llm.request") summary.llmRequests += 1;
    if (span.kind === "tool.execution") summary.toolExecutions += 1;
    if (span.status === "error") summary.errors += 1;
    summary.inputTokens += span.metrics.inputTokens ?? 0;
    summary.outputTokens += span.metrics.outputTokens ?? 0;
    summary.estimatedCost += span.estimatedCost ?? 0;
  }
  summary.sessions = sessions.size;
  summary.turns = turns.size;
  return summary;
}
function aggregateCostStatus(statuses) {
  const estimated = statuses.filter((status) => status === "estimated").length;
  const incomplete = statuses.filter((status) => status === "incomplete").length;
  const unknown = statuses.filter((status) => status === "unknown").length;
  if (estimated === 0 && incomplete === 0) return "unknown";
  if (incomplete > 0 || unknown > 0) return "incomplete";
  return "estimated";
}
function hasTokenMetrics(span) {
  return TOKEN_METRICS.some((key) => span.metrics[key] !== void 0);
}
export {
  aggregateCostStatus,
  summarizeVisible
};
