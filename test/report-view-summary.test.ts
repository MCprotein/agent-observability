import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { aggregateCostStatus, summarizeVisible, type CostStatus } from "../ui/report/view-summary.ts";
import type { Metrics, Span } from "../ui/report/generated/report-dto-v1.js";

const fixture = JSON.parse(
  await readFile(new URL("../contracts/report-view-reduction-v1.fixture.json", import.meta.url), "utf8"),
);

test("keeps browser cost-status reduction in parity with the versioned contract", () => {
  assert.equal(fixture.schemaVersion, "agent_observability.report_view_reduction.v1");
  for (const testCase of fixture.cases) {
    assert.equal(aggregateCostStatus(testCase.statuses), testCase.expectedStatus, testCase.name);
  }
});

test("reduces only already-priced billable span scalars", () => {
  const spans = [
    viewSpan("estimated", 0.25, { inputTokens: 12, outputTokens: 3 }),
    viewSpan("unknown", undefined, { durationMs: 7 }),
  ];

  const summary = summarizeVisible(spans);

  assert.equal(summary.costStatus, "estimated");
  assert.equal(summary.estimatedCost, 0.25);
  assert.equal(summary.inputTokens, 12);
  assert.equal(summary.outputTokens, 3);
});

function viewSpan(status: CostStatus, estimatedCost: number | undefined, metrics: Metrics): Span {
  return {
    schemaVersion: "agent_observability.v1",
    traceId: "trace-1",
    spanId: "span-1",
    parentSpanId: null,
    kind: "llm.request",
    name: "Model request",
    status: "ok",
    startTimeUnixMs: 1,
    endTimeUnixMs: 2,
    repo: "agent-observability",
    agent: { name: "codex", model: "gpt-test" },
    attributes: {},
    metrics,
    cost: { status, rate_table: {}, cost: { assumption: "fixture" } },
    ...(estimatedCost === undefined ? {} : { estimatedCost }),
  };
}
