import assert from "node:assert/strict";
import test from "node:test";
import {
  buildFilteredView,
  buildTimeline,
  isPersistableDimensions,
  paginate,
  parseSavedFilters,
  sameDimensions,
  serializeSavedFilters,
  type DimensionFilters,
} from "../ui/report/view-state.ts";
import type { Span, Trace } from "../ui/report/generated/report-dto-v2.js";

test("keeps large local report view work and rendered slices bounded", () => {
  const spans = Array.from({ length: 4_096 }, (_, index) => spanFixture(index));
  const traces = Array.from({ length: 256 }, (_, index) => traceFixture(index));
  const state = {
    repo: "repo-a",
    session: undefined,
    agent: "codex",
    model: "model-a",
    text: "",
    trace: undefined,
  };

  const view = buildFilteredView(spans, traces, state);
  const spanPage = paginate(view.spans, 0, 200);
  const tracePage = paginate(view.traces, 0, 100);
  const timeline = buildTimeline(view.spans, 120);

  assert.equal(view.spans.length, 2_048);
  assert.equal(view.traces.length, 128);
  assert.equal([...view.spansByTrace.values()].flat().length, 2_048);
  assert.equal(spanPage.items.length, 200);
  assert.equal(spanPage.count, 11);
  assert.equal(tracePage.items.length, 100);
  assert.equal(tracePage.count, 2);
  assert.equal(timeline.length, 120);
  assert.equal(timeline.every((item) => item.leftPercent >= 0 && item.leftPercent <= 100), true);
  assert.equal(timeline.every((item) => item.widthPercent >= 0.8 && item.widthPercent <= 100), true);
});

test("clamps pages and infers timeline duration without mutating input order", () => {
  const first = spanFixture(0);
  const second = spanFixture(1);
  const third = spanFixture(2);
  const spans = [first, second, third];
  first.startTimeUnixMs = 1_000;
  first.endTimeUnixMs = 1_100;
  second.startTimeUnixMs = 1_050;
  second.endTimeUnixMs = null;
  second.metrics.durationMs = 200;
  third.startTimeUnixMs = 1_400;
  third.endTimeUnixMs = 1_400;

  const page = paginate(spans, 99, 2);
  const timeline = buildTimeline(spans, 3);

  assert.equal(page.index, 1);
  assert.deepEqual(page.items.map((span) => span.spanId), ["span-2"]);
  assert.deepEqual(timeline.map((item) => item.span.spanId), ["span-0", "span-1", "span-2"]);
  const [firstTimeline, secondTimeline] = timeline;
  assert.ok(firstTimeline && secondTimeline);
  assert.equal(secondTimeline.widthPercent > firstTimeline.widthPercent, true);
});

test("loads only bounded structured saved filters", () => {
  const valid = { repo: "repo-a", agent: "codex" };
  const opaqueSession = `id:sha256:${"a".repeat(64)}`;
  const parsed = parseSavedFilters(JSON.stringify({ version: 1, filters: [
    valid,
    { repo: "repo-b", text: "must-not-persist" },
    { model: 42 },
    { session: "session-a" },
  ] }), 2);

  assert.deepEqual(parsed, []);
  assert.deepEqual(parseSavedFilters(JSON.stringify({ version: 1, filters: [
    valid,
    valid,
    { repo: undefined, session: undefined, agent: undefined, model: undefined },
    { session: opaqueSession },
  ] }), 20), [valid, { session: opaqueSession }]);
  assert.equal(sameDimensions(
    { repo: "repo-a", session: undefined, agent: "codex", model: undefined },
    { repo: "repo-a", session: undefined, agent: "codex", model: undefined },
  ), true);
  assert.deepEqual(parseSavedFilters("not-json", 20), []);
  assert.deepEqual(parseSavedFilters(JSON.stringify({
    version: 1,
    filters: [{ repo: "x".repeat(513) }],
  }), 20), []);
  assert.deepEqual(parseSavedFilters("[" + " ".repeat(32_768) + "]", 20), []);
});

test("rejects sensitive-looking values at the saved-view sink", () => {
  for (const filters of [
    { repo: "/private/work/project" },
    { repo: "user@example.com" },
    { agent: "raw prompt content" },
    { session: "session-plain-text" },
  ] as unknown as DimensionFilters[]) {
    assert.equal(isPersistableDimensions(filters), false);
    assert.throws(() => serializeSavedFilters([filters]), /non-persistable/);
  }
  assert.equal(isPersistableDimensions({
    repo: "agent-observability",
    session: `id:sha256:${"a".repeat(64)}`,
    agent: "codex",
    model: "provider/gpt-test",
  }), true);
});

function spanFixture(index: number): Span {
  const traceIndex = index % 256;
  return {
    schemaVersion: "agent_observability.v1",
    traceId: `trace-${traceIndex}`,
    spanId: `span-${index}`,
    parentSpanId: null,
    kind: index % 3 === 0 ? "llm.request" : "tool.execution",
    name: `operation-${index}`,
    status: index % 31 === 0 ? "error" : "ok",
    startTimeUnixMs: 1_000 + index * 10,
    endTimeUnixMs: 1_005 + index * 10,
    repo: "repo-a",
    agent: { name: "codex", model: index % 2 === 0 ? "model-a" : "model-b" },
    availability: {
      repository: { state: "available", reason: "reported_by_adapter" },
      turn: { state: "source_unavailable", reason: "source_not_provided" },
      model: { state: "available", reason: "reported_by_adapter" },
      tokens: { state: "source_unavailable", reason: "source_not_provided" },
      latency: { state: "available", reason: "reported_by_adapter" },
      sourceLocation: { state: "private_lookup", reason: "local_opt_in_lookup_required" },
      requestContent: { state: "private_lookup", reason: "local_opt_in_lookup_required" },
      responseContent: { state: "private_lookup", reason: "local_opt_in_lookup_required" },
    },
    sessionId: `session-${traceIndex}`,
    attributes: {},
    metrics: { durationMs: 5 },
    cost: { status: "unknown", rate_table: {}, cost: { assumption: "fixture" } },
  };
}

function traceFixture(index: number): Trace {
  return {
    traceId: `trace-${index}`,
    repo: "repo-a",
    spans: 16,
    errors: 0,
    inputTokens: 0,
    outputTokens: 0,
    estimatedCost: 0,
    startTimeUnixMs: 1_000 + index * 10,
    endTimeUnixMs: 1_005 + index * 10,
    sessions: [`session-${index}`],
    turns: [],
  };
}
