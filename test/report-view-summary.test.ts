import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  aggregateCostStatus,
  summarizeVisible,
  tokenTotal,
  type CostStatus,
} from "../ui/report/view-summary.ts";
import type { Metrics, Span } from "../ui/report/generated/report-dto-v2.js";

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
  assert.equal(summary.totalTokens, 15);
});

test("reduces every displayable token-total shape with stable precedence", () => {
  const cases: Array<[string, Metrics, number | undefined]> = [
    ["direct input and output", { inputTokens: 12, outputTokens: 3 }, 15],
    ["direct partial", { outputTokens: 3 }, undefined],
    ["reported total", { totalTokens: 20 }, 20],
    ["cumulative input and output", { totalInputTokens: 30, totalOutputTokens: 7 }, 37],
    ["cumulative partial", { totalInputTokens: 30 }, undefined],
    ["accumulated total", { totalAccumulatedTokens: 40 }, 40],
    ["direct before aggregate", { inputTokens: 1, outputTokens: 2, totalTokens: 99 }, 3],
    ["no displayable total", { cachedInputTokens: 8, contextWindowTokens: 128_000 }, undefined],
  ];

  for (const [name, metrics, expected] of cases) {
    assert.equal(tokenTotal(metrics), expected, name);
  }

  const summary = summarizeVisible(cases.slice(0, 6).map(([, metrics]) =>
    viewSpan("estimated", 0, metrics),
  ));
  assert.equal(summary.totalTokens, 112);
  assert.equal(summary.costStatus, "estimated");
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
    availability: {
      repository: { state: "available", reason: "fixture" },
      turn: { state: "source_unavailable", reason: "fixture" },
      model: { state: "available", reason: "fixture" },
      tokens: { state: "available", reason: "fixture" },
      latency: { state: "available", reason: "fixture" },
      sourceLocation: { state: "private_lookup", reason: "fixture" },
      requestContent: { state: "private_lookup", reason: "fixture" },
      responseContent: { state: "private_lookup", reason: "fixture" },
    },
    attributes: {},
    metrics,
    cost: { status, rate_table: {}, cost: { assumption: "fixture" } },
    ...(estimatedCost === undefined ? {} : { estimatedCost }),
  };
}
