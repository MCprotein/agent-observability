/* Generated from contracts/report-dto-v2.schema.json. Do not edit. */

// ui/report/view-summary.ts
var NON_TOTAL_TOKEN_METRICS = [
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
    totalTokens: void 0,
    tokenStatus: "unavailable",
    estimatedCost: 0,
    costStatus: aggregateCostStatus(billable.map((span) => span.cost.status))
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
    if (spanTokenTotal !== void 0 && span.availability.tokens.state === "available") {
      completeTokenTotal += spanTokenTotal;
      completeTokenSpans += 1;
    } else if (span.availability.tokens.state !== "not_applicable") {
      incompleteTokenSpans += 1;
    }
    summary.estimatedCost += span.estimatedCost ?? 0;
  }
  summary.sessions = sessions.size;
  summary.turns = turns.size;
  summary.tokenStatus = incompleteTokenSpans > 0 ? "incomplete" : completeTokenSpans > 0 ? "complete" : "unavailable";
  summary.totalTokens = summary.tokenStatus === "complete" ? completeTokenTotal : void 0;
  return summary;
}
function tokenTotal(metrics) {
  const direct = sumComplete(metrics.inputTokens, metrics.outputTokens);
  const cumulative = sumComplete(metrics.totalInputTokens, metrics.totalOutputTokens);
  return direct ?? metrics.totalTokens ?? cumulative ?? metrics.totalAccumulatedTokens;
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
  return tokenTotal(span.metrics) !== void 0 || NON_TOTAL_TOKEN_METRICS.some((key) => span.metrics[key] !== void 0);
}
function sumComplete(left, right) {
  return left === void 0 || right === void 0 ? void 0 : left + right;
}
export {
  aggregateCostStatus,
  summarizeVisible,
  tokenTotal
};
