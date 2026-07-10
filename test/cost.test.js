import assert from "node:assert/strict";
import test from "node:test";
import {
  createSpanRecord,
  estimateCostForRecords,
  estimateSpanCost,
  normalizeRateTable,
} from "../src/index.js";

const RATE_TABLE = {
  version: "test-2026-07",
  currency: "USD",
  unit: "per_1m_tokens",
  assumption: "Fixture rates for tests; not billing truth.",
  models: {
    "gpt-test": {
      input_tokens: 2,
      output_tokens: 8,
      cached_input_tokens: 0.5,
      reasoning_output_tokens: 10,
    },
    "gpt-incomplete": {
      input_tokens: 1,
    },
  },
};

function llmSpan(model = "gpt-test") {
  return createSpanRecord({
    trace_id: "cost-trace",
    span_id: `llm-${model}`,
    span_kind: "llm.request",
    name: model,
    status: "ok",
    agent: { name: "codex", model },
    metrics: {
      input_tokens: 1_000_000,
      output_tokens: 500_000,
      cached_input_tokens: 100_000,
      reasoning_output_tokens: 10_000,
    },
  });
}

test("normalizes a local rate table", () => {
  const table = normalizeRateTable(RATE_TABLE);

  assert.equal(table.version, "test-2026-07");
  assert.equal(table.currency, "USD");
  assert.equal(table.unit, "per_1m_tokens");
  assert.equal(table.models["gpt-test"].input_tokens, 2);
});

test("rejects unsupported rate table units", () => {
  for (const unit of ["per_token", "", null, 123, {}, []]) {
    assert.throws(
      () =>
        normalizeRateTable({
          ...RATE_TABLE,
          unit,
        }),
      /rate table unit must be per_1m_tokens/,
    );
  }
});

test("estimates span cost from token metrics and rate table assumptions", () => {
  const cost = estimateSpanCost(llmSpan(), RATE_TABLE);

  assert.equal(cost.status, "estimated");
  assert.equal(cost.estimated_cost, 6.15);
  assert.equal(cost.currency, "USD");
  assert.equal(cost.rate_table.version, "test-2026-07");
  assert.equal(cost.cost.assumption, "Fixture rates for tests; not billing truth.");
  assert.equal(cost.cost.components.input_tokens.tokens, 1_000_000);
  assert.equal(cost.cost.components.output_tokens.rate_per_1m, 8);
});

test("marks missing token rates as incomplete", () => {
  const cost = estimateSpanCost(llmSpan("gpt-incomplete"), RATE_TABLE);

  assert.equal(cost.status, "incomplete");
  assert.equal(cost.reason, "missing_token_rates");
  assert.equal(cost.estimated_cost, 1);
  assert.deepEqual(cost.cost.missing, [
    "output_tokens",
    "cached_input_tokens",
    "reasoning_output_tokens",
  ]);
});

test("marks missing model rates and missing rate tables as unknown", () => {
  const missingModel = estimateSpanCost(llmSpan("gpt-missing"), RATE_TABLE);
  const missingTable = estimateCostForRecords([llmSpan()], null);

  assert.equal(missingModel.status, "unknown");
  assert.equal(missingModel.reason, "missing_model_rate");
  assert.equal(missingTable.status, "unknown");
  assert.equal(missingTable.reason, "missing_rate_table");
});

test("aggregates estimated, incomplete, and unknown cost state", () => {
  const aggregate = estimateCostForRecords(
    [llmSpan("gpt-test"), llmSpan("gpt-incomplete"), llmSpan("gpt-missing")],
    RATE_TABLE,
  );

  assert.equal(aggregate.status, "incomplete");
  assert.equal(aggregate.estimated_cost, 7.15);
  assert.equal(aggregate.cost.incomplete_count, 1);
  assert.equal(aggregate.cost.unknown_count, 1);
  assert.equal(aggregate.rate_table.version, "test-2026-07");
});
