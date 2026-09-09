import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import validateDashboardQueryResponseV1, {
  validateDashboardQueryRequestV1,
} from "../ui/report/generated/validate-dashboard-query-v1.js";

const schemaVersion = "agent_observability.dashboard_query.v1";
const snapshotId = "a".repeat(64);
const snapshot = {
  id: snapshotId,
  generation: "42",
  visibilityEpoch: "7",
  generatedAt: "2026-09-07T00:00:00.000Z",
  state: "current",
} as const;
const scope = {
  filters: {},
  selectedTraceId: null,
  coldExcluded: true,
} as const;

const fixture = JSON.parse(
  await readFile("contracts/dashboard-query-v1.fixture.json", "utf8"),
) as Record<string, unknown>;
const corpus = JSON.parse(
  await readFile("contracts/dashboard-query-v1.parity.json", "utf8"),
) as ParityCorpus;

interface ParityCorpus {
  bases: Record<string, Record<string, unknown>>;
  cases: ParityCase[];
}

interface ParityCase {
  name: string;
  category: "positive" | "negative" | "privacy";
  base: string;
  operation: "none" | "set" | "remove";
  path: Array<string | number>;
  value?: unknown;
  valid: boolean;
}

test("generated dashboard validator imports directly and matches request/response fixtures", () => {
  assert.equal(
    validateDashboardQueryRequestV1({ schemaVersion, kind: "bootstrap" }),
    true,
  );
  assert.equal(
    validateDashboardQueryRequestV1({ schemaVersion, kind: "traces" }),
    false,
  );
  assert.equal(
    validateDashboardQueryRequestV1({ schemaVersion, kind: "traces", snapshotId }),
    true,
  );

  const pendingSummary = {
    schemaVersion,
    kind: "summary",
    snapshot,
    scope,
    work: { state: "pending" },
    pagination: { nextCursor: "opaque-summary-progress" },
  };
  assert.equal(validateDashboardQueryResponseV1(pendingSummary), true);
  assert.equal(
    validateDashboardQueryResponseV1({
      ...pendingSummary,
      pagination: { nextCursor: null },
    }),
    false,
  );
  assert.equal(
    validateDashboardQueryResponseV1({
      ...pendingSummary,
      snapshot: { ...snapshot, generation: "18446744073709551616" },
    }),
    false,
  );
});

test("canonical dashboard fixture passes the generated response validator", () => {
  assert.equal(validateDashboardQueryResponseV1(fixture), true);
});

test("request cursors are optional but reject explicit null", () => {
  const requests = [
    { schemaVersion, kind: "traces", snapshotId },
    {
      schemaVersion,
      kind: "spans",
      snapshotId,
      traceId: `id:sha256:${"b".repeat(64)}`,
    },
    { schemaVersion, kind: "summary", snapshotId },
    { schemaVersion, kind: "facets", snapshotId },
  ];

  for (const request of requests) {
    assert.equal(validateDashboardQueryRequestV1(request), true, request.kind);
    assert.equal(
      validateDashboardQueryRequestV1({ ...request, cursor: null }),
      false,
      request.kind,
    );
  }
});

test("availability reasons use the closed canonical flattened state contract", () => {
  const response = structuredClone(fixture);
  const row = (response.rows as Array<Record<string, unknown>>)[0];
  assert.ok(row);
  assert.deepEqual(row.availabilityReasons, [{
    field: "tokens",
    state: "not_applicable",
    reason: "span_kind_has_no_token_usage",
  }]);
  assert.equal(validateDashboardQueryResponseV1(response), true);

  for (const availabilityReasons of [
    [{ field: "tokens", state: "available", reason: "reported_by_adapter" }],
    [{ field: "tokens", state: "source_unavailable", reason: "arbitrary_reason" }],
    [{ field: "tokens", state: "withheld", reason: "source_not_provided" }],
    [
      { field: "tokens", state: "source_unavailable", reason: "source_not_provided" },
      { field: "tokens", state: "not_applicable", reason: "span_kind_has_no_token_usage" },
    ],
    [{
      field: "tokens",
      state: "not_applicable",
      reason: "span_kind_has_no_token_usage",
      detail: "must remain closed",
    }],
  ]) {
    const invalid = structuredClone(response);
    const invalidRow = (invalid.rows as Array<Record<string, unknown>>)[0];
    assert.ok(invalidRow);
    invalidRow.availabilityReasons = availabilityReasons;
    assert.equal(validateDashboardQueryResponseV1(invalid), false);
  }
});

test("complete token status describes total completeness only", () => {
  const base = corpus.bases.spansResponse;
  assert.ok(base);
  const response = structuredClone(base);
  const kpis = ((response.work as Record<string, unknown>).kpis ?? {}) as Record<string, unknown>;
  assert.equal(kpis.inputTokens, null);
  assert.equal(kpis.outputTokens, null);
  assert.equal(kpis.totalTokens, 120);
  assert.equal(kpis.tokenStatus, "complete");
  assert.equal(validateDashboardQueryResponseV1(response), true);

  kpis.totalTokens = null;
  assert.equal(validateDashboardQueryResponseV1(response), false);
});

test("generated dashboard validators match the shared Rust parity corpus", () => {
  const categories = new Set<string>();
  for (const parityCase of corpus.cases) {
    categories.add(parityCase.category);
    const base = corpus.bases[parityCase.base];
    assert.ok(base, `missing parity base: ${parityCase.base}`);
    const document = structuredClone(base);
    applyParityCase(document, parityCase);
    const valid = validateDashboardQueryRequestV1(document) ||
      validateDashboardQueryResponseV1(document);
    assert.equal(valid, parityCase.valid, parityCase.name);
  }
  assert.deepEqual([...categories].sort(), ["negative", "positive", "privacy"]);
});

function applyParityCase(document: Record<string, unknown>, parityCase: ParityCase): void {
  if (parityCase.operation === "none") return;
  const field = parityCase.path.at(-1);
  if (typeof field !== "string") throw new Error("parity case is missing a string field");
  let parent: unknown = document;
  for (const segment of parityCase.path.slice(0, -1)) {
    assert.equal(typeof parent, "object");
    assert.notEqual(parent, null);
    parent = Array.isArray(parent)
      ? parent[segment as number]
      : (parent as Record<string, unknown>)[segment as string];
  }
  assert.equal(typeof parent, "object");
  assert.notEqual(parent, null);
  assert.equal(Array.isArray(parent), false);
  const object = parent as Record<string, unknown>;
  if (parityCase.operation === "set") object[field] = parityCase.value;
  else delete object[field];
}
