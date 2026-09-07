import assert from "node:assert/strict";
import test from "node:test";
import type {
  DashboardQueryRequestV1,
  DashboardQueryResponseV1,
} from "../ui/report/generated/dashboard-query-v1.js";
import { validateDashboardQueryRequestV1 } from "../ui/report/generated/validate-dashboard-query-v1.ts";
import { PagedDashboardClient } from "../ui/report/paged-client.ts";
import {
  dashboardFiltersFromSavedDimensions,
  facetLimitDisclosure,
  savedDimensionsFromDashboardFilters,
} from "../ui/report/paged.ts";
import {
  isPersistableDimensions,
  parseSavedFilters,
  serializeSavedFilters,
} from "../ui/report/view-state.ts";

test("paged saved views reuse the bounded legacy storage contract without persisting text", () => {
  const session = `id:sha256:${"a".repeat(64)}`;
  const dimensions = savedDimensionsFromDashboardFilters({
    repo: ["agent-observability"],
    session: [session],
    agent: ["codex"],
    model: ["provider/gpt-test"],
    text: "PRIVATE_BROWSER_REQUEST",
  });

  assert.equal(isPersistableDimensions(dimensions), true);
  const encoded = serializeSavedFilters([dimensions]);
  assert.equal(encoded.includes("PRIVATE_BROWSER_REQUEST"), false);
  assert.deepEqual(parseSavedFilters(encoded, 20), [dimensions]);
  assert.deepEqual(dashboardFiltersFromSavedDimensions(dimensions), {
    repo: ["agent-observability"],
    session: [session],
    agent: ["codex"],
    model: ["provider/gpt-test"],
  });
});

test("paged saved views fail closed for ambiguous or sensitive dimensions", () => {
  const dimensions = savedDimensionsFromDashboardFilters({
    repo: ["repo-a", "repo-b"],
    agent: ["raw prompt content"],
  });

  assert.deepEqual(dimensions, {
    repo: undefined,
    session: undefined,
    agent: "raw prompt content",
    model: undefined,
  });
  assert.equal(isPersistableDimensions(dimensions), false);
  assert.throws(() => serializeSavedFilters([dimensions]), /non-persistable/);
  assert.deepEqual(parseSavedFilters("[" + " ".repeat(32_768) + "]", 20), []);
});

test("paged UI discloses bounded facet menus only when the backend reports truncation", () => {
  assert.equal(facetLimitDisclosure(false), undefined);
  assert.equal(
    facetLimitDisclosure(true),
    "Filter suggestions show up to 125 values per dimension. Totals and trace pages still cover all matching data.",
  );
});

test("initial paged requests omit absent cursors and satisfy the wire validator", async () => {
  const projected = (value: string): string => `id:sha256:${value.repeat(64).slice(0, 64)}`;
  const snapshot = {
    id: "b".repeat(64),
    generation: "1",
    visibilityEpoch: "1",
    generatedAt: "2026-09-07T00:00:00.000Z",
    state: "current",
  } as const;
  const common = {
    schemaVersion: "agent_observability.dashboard_query.v1",
    snapshot,
    scope: { filters: {}, selectedTraceId: null, coldExcluded: true },
    work: { state: "pending" },
  } as const;
  const requests: DashboardQueryRequestV1[] = [];
  const client = new PagedDashboardClient({
    transport: async (request) => {
      requests.push(request);
      if (request.kind === "bootstrap") {
        return {
          ...common,
          kind: "bootstrap",
          limits: {
            traceRows: 100,
            spanRows: 200,
            timelineRows: 120,
            facetValues: 500,
            requestBytes: 8_192,
            responseBytes: 1_048_576,
          },
        } as DashboardQueryResponseV1;
      }
      if (request.kind === "traces") {
        return { ...common, kind: "traces", rows: [], pagination: { nextCursor: null } } as DashboardQueryResponseV1;
      }
      if (request.kind === "facets") {
        return { ...common, kind: "facets", rows: [], pagination: { nextCursor: null } } as DashboardQueryResponseV1;
      }
      if (request.kind === "summary") {
        return {
          ...common,
          kind: "summary",
          work: {
            state: "complete",
            kpis: {
              sessions: 0,
              turns: 0,
              llm: 0,
              tools: 0,
              errors: 0,
              inputTokens: null,
              outputTokens: null,
              totalTokens: null,
              tokenStatus: "unavailable",
              estimatedCost: null,
              costStatus: "unknown",
              currency: null,
            },
          },
          pagination: { nextCursor: null },
        } as DashboardQueryResponseV1;
      }
      if (request.kind === "spans") {
        return {
          ...common,
          kind: "spans",
          scope: { ...common.scope, selectedTraceId: projected("a") },
          rows: [],
          pagination: { nextCursor: null },
        } as DashboardQueryResponseV1;
      }
      throw new Error(`Unexpected ${request.kind} request`);
    },
  });

  client.start();
  await client.settled();
  client.selectTrace(projected("a"));
  await client.settled();

  assert.deepEqual(requests.map((request) => request.kind), ["bootstrap", "traces", "facets", "summary", "spans"]);
  for (const request of requests) {
    assert.equal(validateDashboardQueryRequestV1(request), true, `${request.kind} request must validate`);
    assert.equal("cursor" in request, false, `${request.kind} request must omit an absent cursor`);
  }
});

test("clearing selected detail ignores a late span response", async () => {
  const projected = (value: string): string => `id:sha256:${value.repeat(64).slice(0, 64)}`;
  const snapshot = {
    id: "b".repeat(64),
    generation: "1",
    visibilityEpoch: "1",
    generatedAt: "2026-09-07T00:00:00.000Z",
    state: "current",
  } as const;
  const common = {
    schemaVersion: "agent_observability.dashboard_query.v1",
    snapshot,
    scope: { filters: {}, selectedTraceId: projected("a"), coldExcluded: true },
    work: { state: "pending" },
  } as const;
  let resolveDetail!: (response: DashboardQueryResponseV1) => void;
  const detailResponse = new Promise<DashboardQueryResponseV1>((resolve) => { resolveDetail = resolve; });
  const client = new PagedDashboardClient({
    autoLoad: false,
    transport: async (request) => {
      if (request.kind === "bootstrap") {
        return {
          ...common,
          kind: "bootstrap",
          limits: {
            traceRows: 100,
            spanRows: 200,
            timelineRows: 120,
            facetValues: 500,
            requestBytes: 8_192,
            responseBytes: 1_048_576,
          },
        } as DashboardQueryResponseV1;
      }
      if (request.kind === "spans") {
        return { ...common, kind: "spans", rows: [], pagination: { nextCursor: null } } as DashboardQueryResponseV1;
      }
      if (request.kind === "span") return detailResponse;
      throw new Error(`Unexpected ${request.kind} request`);
    },
  });

  client.start();
  await client.settled();
  client.selectTrace(projected("a"));
  await client.settled();
  client.selectSpan(projected("b"));
  await Promise.resolve();
  client.clearSelectedDetail();
  assert.equal(client.state.selectedSpan, undefined);
  assert.deepEqual(client.state.privateDetail, { state: "idle" });

  resolveDetail({ ...common, kind: "span", detail: {} } as DashboardQueryResponseV1);
  await client.settled();
  assert.equal(client.state.selectedSpan, undefined);
  assert.deepEqual(client.state.privateDetail, { state: "idle" });
});
