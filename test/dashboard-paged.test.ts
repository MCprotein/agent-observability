import assert from "node:assert/strict";
import test from "node:test";
import type {
  DashboardQueryRequestV1,
  DashboardQueryResponseV1,
  DashboardSnapshotV1,
  Span,
} from "../ui/report/generated/dashboard-query-v1.js";
import type { PrivateCodexTurnDetailV1 } from "../ui/report/generated/private-codex-turn-detail-v1.js";
import {
  createDashboardTransport,
  createPrivateDetailTransport,
  dashboardQueryEndpoint,
  DASHBOARD_CURSOR_HISTORY_LIMIT,
  DashboardTransportError,
  PagedDashboardClient,
  type DashboardQueryTransport,
} from "../ui/report/paged-client.ts";

const projected = (value: string): string => `id:sha256:${value.repeat(64).slice(0, 64)}`;
const snapshot: DashboardSnapshotV1 = {
  id: "b".repeat(64),
  generation: "42",
  visibilityEpoch: "7",
  generatedAt: "2026-09-07T00:00:00.000Z",
  state: "current",
};
const scope = { filters: {}, selectedTraceId: null, coldExcluded: true } as const;
const limits = {
  traceRows: 100,
  spanRows: 200,
  timelineRows: 120,
  facetValues: 500,
  requestBytes: 8_192,
  responseBytes: 1_048_576,
} as const;
const validateResponse = (value: unknown): value is DashboardQueryResponseV1 =>
  typeof value === "object" && value !== null && "kind" in value;

test("durable HTTP storage failures stop without a retry or JSON fallback", async () => {
  let requests = 0;
  const transport = createDashboardTransport({
    endpoint: "http://127.0.0.1:43192/report/test/query",
    validate: validateResponse,
    fetch: async () => { requests += 1; return new Response("", { status: 500 }); },
  });
  const client = new PagedDashboardClient({ transport, backoffMs: 0 });
  client.start();
  await client.settled();
  assert.equal(client.state.availability, "error");
  assert.match(client.state.reason ?? "", /HTTP 500/);
  assert.equal(requests, 1);
  client.dispose();
});

test("expires the bound snapshot without issuing queued follow-up queries", async () => {
  const requests: DashboardQueryRequestV1[] = [];
  const responses: DashboardQueryResponseV1[] = [
    response({ kind: "bootstrap", limits }),
    status("traces", "snapshot_expired"),
  ];
  const client = new PagedDashboardClient({
    transport: scriptedTransport(responses, requests),
    backoffMs: 0,
  });

  client.start();
  await client.settled();

  assert.equal(client.state.availability, "expired");
  assert.equal(client.state.snapshot, undefined);
  assert.equal(client.state.traces.rows.length, 0);
  assert.deepEqual(requests.map((request) => request.kind), ["bootstrap", "traces"]);
  assert.equal((requests[1] as DashboardQueryRequestV1 & { snapshotId?: string }).snapshotId, snapshot.id);
});

test("expired snapshot clears previously completed aggregates and facets", async () => {
  const requests: DashboardQueryRequestV1[] = [];
  const client = new PagedDashboardClient({
    transport: scriptedTransport([
      response({ kind: "bootstrap", limits }),
      response({ kind: "traces", rows: [], pagination: { nextCursor: "next", total: 1 } }),
      response({ kind: "facets", rows: [{ dimension: "repo", value: "old-repo" }], pagination: { nextCursor: null } }),
      response({ kind: "summary", pagination: { nextCursor: null }, work: { state: "complete", kpis: completeKpis() } }),
      status("traces", "snapshot_expired"),
    ], requests),
    backoffMs: 0,
  });
  client.start();
  await client.settled();
  assert.equal(client.state.summary.state, "complete");
  assert.equal(client.state.facets.length, 1);
  client.nextTraces();
  await client.settled();
  assert.equal(client.state.availability, "expired");
  assert.deepEqual(client.state.summary, { state: "idle" });
  assert.deepEqual(client.state.facets, []);
  assert.equal(client.state.scope, undefined);
});

test("large session facets cannot starve later agent and model dimensions", async () => {
  const requests: DashboardQueryRequestV1[] = [];
  const client = new PagedDashboardClient({
    transport: scriptedTransport([
      response({ kind: "bootstrap", limits }),
      response({ kind: "traces", rows: [], pagination: { nextCursor: null } }),
      response({ kind: "facets", rows: Array.from({ length: 500 }, (_, index) => ({ dimension: "session", value: `session-${index}` })), pagination: { nextCursor: "facets-next" } }),
      response({ kind: "summary", pagination: { nextCursor: null }, work: { state: "complete", kpis: completeKpis() } }),
      response({ kind: "facets", rows: [{ dimension: "agent", value: "codex" }, { dimension: "model", value: "gpt-test" }], pagination: { nextCursor: null } }),
    ], requests),
    backoffMs: 0,
  });
  client.start();
  await client.settled();
  assert.ok(client.state.facets.some((row) => row.dimension === "agent"));
  assert.ok(client.state.facets.some((row) => row.dimension === "model"));
  assert.ok(client.state.facets.length <= limits.facetValues);
  assert.equal(client.state.facetsLimited, true);
});

test("unowned AbortError fails current query and clears queued work", async () => {
  const requests: string[] = [];
  const client = new PagedDashboardClient({ transport: async (request, signal) => {
    requests.push(request.kind);
    if (request.kind === "bootstrap") return response({ kind: "bootstrap", limits });
    assert.equal(signal.aborted, false);
    throw new DOMException("network abort", "AbortError");
  } });
  client.start();
  await client.settled();
  assert.equal(client.state.availability, "error");
  assert.deepEqual(requests, ["bootstrap", "traces"]);
});

test("owned filter abort rejection cannot replace the new revision with an error", async () => {
  let first = true;
  const client = new PagedDashboardClient({ autoLoad: false, transport: async (_request, signal) => {
    if (first) {
      first = false;
      return new Promise<DashboardQueryResponseV1>((_resolve, reject) => {
        signal.addEventListener("abort", () => reject(new DOMException("cancelled", "AbortError")), { once: true });
      });
    }
    return response({ kind: "bootstrap", limits });
  } });
  client.start();
  await Promise.resolve();
  client.setFilters({ repo: ["new-repo"] });
  await client.settled();
  assert.equal(client.state.availability, "current");
  assert.deepEqual(client.state.filters, { repo: ["new-repo"] });
});

test("aborts filter work and ignores a late response from the prior revision", async () => {
  let resolveFirst!: (value: DashboardQueryResponseV1) => void;
  const first = new Promise<DashboardQueryResponseV1>((resolve) => { resolveFirst = resolve; });
  const requests: DashboardQueryRequestV1[] = [];
  let firstSignal: AbortSignal | undefined;
  let calls = 0;
  const transport: DashboardQueryTransport = async (request, signal) => {
    requests.push(request);
    calls += 1;
    if (calls === 1) {
      firstSignal = signal;
      return first;
    }
    return response({
      kind: "bootstrap",
      limits,
      scope: { ...scope, filters: { repo: ["repo-new"] } },
      snapshot: { ...snapshot, id: "c".repeat(64), generation: "43" },
    });
  };
  const client = new PagedDashboardClient({ transport, autoLoad: false });

  client.start();
  await Promise.resolve();
  client.setFilters({ repo: ["repo-new"] });
  assert.equal(firstSignal?.aborted, true);
  resolveFirst(response({ kind: "bootstrap", limits }));
  await client.settled();

  assert.equal(requests.length, 2);
  assert.deepEqual(requests[1]?.filters, { repo: ["repo-new"] });
  assert.equal(client.state.snapshot?.id, "c".repeat(64));
  assert.deepEqual(client.state.filters, { repo: ["repo-new"] });
});

test("surfaces building before retry and preserves the published stale state", async () => {
  const transitions: string[] = [];
  const responses: DashboardQueryResponseV1[] = [
    status("bootstrap", "building"),
    response({
      kind: "bootstrap",
      limits,
      snapshot: { ...snapshot, state: "stale" },
    }),
  ];
  const client = new PagedDashboardClient({
    autoLoad: false,
    backoffMs: 0,
    onState: (state) => transitions.push(state.availability),
    transport: scriptedTransport(responses, []),
  });

  client.start();
  await client.settled();

  assert.equal(transitions.includes("building"), true);
  assert.equal(client.state.availability, "stale");
  assert.equal(client.state.scope?.coldExcluded, true);
});

test("preserves sparse empty keyset continuation and advances without offset fallback", async () => {
  const requests: DashboardQueryRequestV1[] = [];
  const responses: DashboardQueryResponseV1[] = [
    response({ kind: "bootstrap", limits }),
    response({ kind: "traces", rows: [], pagination: { nextCursor: "trace-next", total: 1 } }),
    response({ kind: "facets", rows: [], pagination: { nextCursor: null } }),
    response({ kind: "summary", pagination: { nextCursor: null }, work: { state: "complete", kpis: completeKpis() } }),
    response({
      kind: "traces",
      rows: [{
        traceId: projected("a"),
        repo: "repo-a",
        spanCount: 1,
        errorCount: 0,
        startTimeUnixMs: 1,
        endTimeUnixMs: 2,
        availabilityReasons: [],
      }],
      pagination: { nextCursor: null, total: 1 },
    }),
  ];
  const client = new PagedDashboardClient({
    transport: scriptedTransport(responses, requests),
    backoffMs: 0,
  });

  client.start();
  await client.settled();
  assert.equal(client.state.traces.rows.length, 0);
  assert.equal(client.state.traces.nextCursor, "trace-next");
  assert.equal(client.state.summary.state, "complete");

  client.nextTraces();
  await client.settled();

  assert.equal(client.state.traces.rows[0]?.traceId, projected("a"));
  assert.equal(client.state.traces.page, 1);
  const final = requests.at(-1) as DashboardQueryRequestV1 & { snapshotId?: string; cursor?: string | null };
  assert.equal(final.kind, "traces");
  assert.equal(final.snapshotId, snapshot.id);
  assert.equal(final.cursor, "trace-next");
});

test("bounds trace and span cursor history while preserving absolute page numbers", async () => {
  const requests: DashboardQueryRequestV1[] = [];
  const transport: DashboardQueryTransport = async (request) => {
    requests.push(request);
    switch (request.kind) {
      case "bootstrap":
        return response({ kind: "bootstrap", limits });
      case "facets":
        return response({ kind: "facets", rows: [], pagination: { nextCursor: null } });
      case "summary":
        return response({ kind: "summary", pagination: { nextCursor: null }, work: { state: "complete", kpis: completeKpis() } });
      case "traces": {
        const page = cursorPage(request.cursor, "trace");
        return response({ kind: "traces", rows: [], pagination: { nextCursor: `trace-${page + 1}`, total: 1_000 } });
      }
      case "spans": {
        const page = cursorPage(request.cursor, "span");
        return response({ kind: "spans", rows: [], pagination: { nextCursor: `span-${page + 1}`, total: 1_000 } });
      }
      case "span":
        throw new Error("Unexpected span-detail request");
    }
  };
  const client = new PagedDashboardClient({ transport, backoffMs: 0 });

  client.start();
  await client.settled();
  for (let page = 0; page < 20; page += 1) {
    client.nextTraces();
    await client.settled();
  }
  assert.equal(client.state.traces.page, 20);

  for (let step = 1; step < DASHBOARD_CURSOR_HISTORY_LIMIT; step += 1) {
    client.previousTraces();
    await client.settled();
  }
  assert.equal(client.state.traces.page, 5);
  assert.equal(client.state.traces.canGoBack, false);
  const beforeUnavailableTracePrevious = requests.length;
  client.previousTraces();
  await client.settled();
  assert.equal(client.state.traces.page, 5);
  assert.equal(requests.length, beforeUnavailableTracePrevious);
  assert.equal(requests.at(-1)?.kind, "traces");
  assert.equal((requests.at(-1) as DashboardQueryRequestV1 & { cursor?: string | null }).cursor, "trace-5");

  client.selectTrace(projected("a"));
  await client.settled();
  for (let page = 0; page < 20; page += 1) {
    client.nextSpans();
    await client.settled();
  }
  assert.equal(client.state.spans.page, 20);

  for (let step = 1; step < DASHBOARD_CURSOR_HISTORY_LIMIT; step += 1) {
    client.previousSpans();
    await client.settled();
  }
  assert.equal(client.state.spans.page, 5);
  assert.equal(client.state.spans.canGoBack, false);
  const beforeUnavailableSpanPrevious = requests.length;
  client.previousSpans();
  await client.settled();
  assert.equal(client.state.spans.page, 5);
  assert.equal(requests.length, beforeUnavailableSpanPrevious);
  assert.equal(requests.at(-1)?.kind, "spans");
  assert.equal((requests.at(-1) as DashboardQueryRequestV1 & { cursor?: string | null }).cursor, "span-5");
});

test("continues pending exact KPI work serially and publishes values only when complete", async () => {
  const requests: DashboardQueryRequestV1[] = [];
  let active = 0;
  let maximumActive = 0;
  const responses: DashboardQueryResponseV1[] = [
    response({ kind: "bootstrap", limits }),
    response({ kind: "traces", rows: [], pagination: { nextCursor: null, total: 0 } }),
    response({ kind: "facets", rows: [], pagination: { nextCursor: null } }),
    response({ kind: "summary", pagination: { nextCursor: "summary-next" }, work: { state: "pending" } }),
    response({ kind: "summary", pagination: { nextCursor: null }, work: { state: "complete", kpis: completeKpis() } }),
  ];
  const transport: DashboardQueryTransport = async (request) => {
    active += 1;
    maximumActive = Math.max(maximumActive, active);
    requests.push(request);
    await Promise.resolve();
    active -= 1;
    const next = responses.shift();
    if (!next) throw new Error("Unexpected request");
    return next;
  };
  const client = new PagedDashboardClient({ transport, backoffMs: 0 });

  client.start();
  await client.settled();

  assert.equal(maximumActive, 1);
  assert.equal(client.state.summary.state, "complete");
  assert.equal(client.state.summary.kpis?.sessions, 2);
  const summaryRequests = requests.filter((request) => request.kind === "summary") as Array<DashboardQueryRequestV1 & { cursor?: string | null; snapshotId?: string }>;
  assert.equal(summaryRequests.length, 2);
  assert.equal(summaryRequests[1]?.cursor, "summary-next");
  assert.equal(summaryRequests.every((request) => request.snapshotId === snapshot.id), true);
});

test("does not begin queued network work while the document is hidden", async () => {
  let hidden = true;
  let calls = 0;
  const client = new PagedDashboardClient({
    autoLoad: false,
    isHidden: () => hidden,
    transport: async () => {
      calls += 1;
      return response({ kind: "bootstrap", limits });
    },
  });

  client.start();
  await Promise.resolve();
  assert.equal(calls, 0);

  hidden = false;
  client.resume();
  await client.settled();
  assert.equal(calls, 1);
  assert.equal(client.state.availability, "current");
});

test("uses same-origin GET transport and enforces both wire byte bounds", async () => {
  assert.equal(
    dashboardQueryEndpoint({
      href: "http://127.0.0.1:8080/report/capability-token",
      pathname: "/report/capability-token",
    }).href,
    "http://127.0.0.1:8080/report/capability-token/query",
  );
  assert.equal(
    dashboardQueryEndpoint({
      href: "http://127.0.0.1:8080/report/capability-token/",
      pathname: "/report/capability-token/",
    }).pathname,
    "/report/capability-token/query",
  );
  let calledUrl: URL | undefined;
  const validBootstrap = response({ kind: "bootstrap", limits });
  const transport = createDashboardTransport({
    endpoint: "/report/capability/query",
    origin: "http://127.0.0.1:8080",
    validate: validateResponse,
    fetch: async (input, init) => {
      calledUrl = new URL(String(input));
      assert.equal(init?.method, "GET");
      assert.equal(init?.credentials, "same-origin");
      return new Response(JSON.stringify(validBootstrap));
    },
  });

  await transport({ schemaVersion: "agent_observability.dashboard_query.v1", kind: "bootstrap" }, new AbortController().signal);
  assert.equal(calledUrl?.origin, "http://127.0.0.1:8080");
  assert.equal(calledUrl?.pathname, "/report/capability/query");
  assert.equal(JSON.parse(calledUrl?.searchParams.get("request") ?? "null").kind, "bootstrap");

  let oversizedFetchCalled = false;
  const oversizedRequestTransport = createDashboardTransport({
    endpoint: "/report/capability/query",
    origin: "http://127.0.0.1:8080",
    validate: validateResponse,
    fetch: async () => {
      oversizedFetchCalled = true;
      return new Response();
    },
  });
  await assert.rejects(
    oversizedRequestTransport({
      schemaVersion: "agent_observability.dashboard_query.v1",
      kind: "bootstrap",
      filters: { text: "x".repeat(9_000) },
    }, new AbortController().signal),
    (error: unknown) => error instanceof DashboardTransportError && error.code === "request_too_large",
  );
  assert.equal(oversizedFetchCalled, false);

  const oversizedResponseTransport = createDashboardTransport({
    endpoint: "/report/capability/query",
    origin: "http://127.0.0.1:8080",
    validate: validateResponse,
    fetch: async () => new Response("", { headers: { "content-length": "1048577" } }),
  });
  await assert.rejects(
    oversizedResponseTransport({ schemaVersion: "agent_observability.dashboard_query.v1", kind: "bootstrap" }, new AbortController().signal),
    (error: unknown) => error instanceof DashboardTransportError && error.code === "capacity",
  );
});

test("successful headers never conceal a later body-stream failure", async () => {
  const failure = new TypeError("synthetic body transport failure");
  let pulls = 0;
  let validations = 0;
  const transport = createDashboardTransport({
    endpoint: "/report/capability/query",
    origin: "http://127.0.0.1:8080",
    validate: (value): value is DashboardQueryResponseV1 => {
      validations += 1;
      return validateResponse(value);
    },
    fetch: async () => new Response(new ReadableStream<Uint8Array>({
      pull(controller) {
        if (pulls++ === 0) controller.enqueue(new TextEncoder().encode('{"schemaVersion":'));
        else controller.error(failure);
      },
    }), { status: 200 }),
  });
  await assert.rejects(
    transport({ schemaVersion: "agent_observability.dashboard_query.v1", kind: "bootstrap" }, new AbortController().signal),
    (error: unknown) => error === failure,
  );
  assert.equal(validations, 0);
});

test("loads eligible private detail only after explicit request", async () => {
  let privateCalls = 0;
  const turnId = projected("d");
  const detail = privateDetail(turnId);
  const client = new PagedDashboardClient({
    autoLoad: false,
    transport: canonicalTransport(spanDetail(turnId)),
    privateDetailTransport: async (requestedTurnId) => {
      privateCalls += 1;
      assert.equal(requestedTurnId, turnId);
      return { state: "loaded", detail };
    },
  });

  client.start();
  await client.settled();
  client.selectTrace(projected("a"));
  await client.settled();
  client.selectSpan(projected("b"));
  await client.settled();

  const eligibility: string = client.state.privateDetail.state;
  assert.equal(eligibility, "eligible");
  assert.equal(privateCalls, 0);
  await client.loadPrivateDetail();
  assert.equal(privateCalls, 1);
  const loaded = client.state.privateDetail;
  assert.equal(loaded.state, "loaded");
  if (loaded.state === "loaded") {
    assert.deepEqual(loaded.detail, detail);
  }
});

test("aborts and erases late private detail when filters invalidate selection", async () => {
  let resolvePrivate!: (value: { state: "loaded"; detail: PrivateCodexTurnDetailV1 }) => void;
  const pendingPrivate = new Promise<{ state: "loaded"; detail: PrivateCodexTurnDetailV1 }>((resolve) => {
    resolvePrivate = resolve;
  });
  let privateSignal: AbortSignal | undefined;
  const turnId = projected("d");
  const client = new PagedDashboardClient({
    autoLoad: false,
    backoffMs: 0,
    transport: canonicalTransport(spanDetail(turnId)),
    privateDetailTransport: async (_turnId, signal) => {
      privateSignal = signal;
      return pendingPrivate;
    },
  });

  client.start();
  await client.settled();
  client.selectTrace(projected("a"));
  await client.settled();
  client.selectSpan(projected("b"));
  await client.settled();
  const rawRequest = client.loadPrivateDetail();
  await Promise.resolve();

  client.setFilters({ repo: ["repo-new"] });
  assert.equal(privateSignal?.aborted, true);
  assert.equal(client.state.privateDetail.state, "idle");
  assert.equal(client.state.selectedSpan, undefined);
  resolvePrivate({ state: "loaded", detail: privateDetail(turnId) });
  await rawRequest;
  await client.settled();
  assert.equal(client.state.privateDetail.state, "idle");
});

test("private detail transport uses the separate endpoint and 64 KiB bound", async () => {
  const turnId = projected("d");
  let requested: URL | undefined;
  const validatePrivate = (value: unknown): value is PrivateCodexTurnDetailV1 =>
    typeof value === "object" && value !== null && "turnId" in value;
  const transport = createPrivateDetailTransport({
    origin: "http://127.0.0.1:8080",
    reportPath: "/report/capability-token",
    validate: validatePrivate,
    fetch: async (input, init) => {
      requested = new URL(String(input));
      assert.equal(init?.method, "GET");
      assert.equal(init?.credentials, "same-origin");
      return new Response(JSON.stringify(privateDetail(turnId)));
    },
  });

  const result = await transport(turnId, new AbortController().signal);
  assert.equal(result.state, "loaded");
  assert.equal(requested?.pathname, `/report/capability-token/details/${encodeURIComponent(turnId)}`);

  const oversized = createPrivateDetailTransport({
    origin: "http://127.0.0.1:8080",
    reportPath: "/report/capability-token",
    validate: validatePrivate,
    fetch: async () => new Response("", { headers: { "content-length": "65537" } }),
  });
  await assert.rejects(
    oversized(turnId, new AbortController().signal),
    (error: unknown) => error instanceof DashboardTransportError && error.code === "capacity",
  );
});

function scriptedTransport(
  responses: DashboardQueryResponseV1[],
  requests: DashboardQueryRequestV1[],
): DashboardQueryTransport {
  return async (request) => {
    requests.push(request);
    const next = responses.shift();
    if (!next) throw new Error(`Unexpected ${request.kind} request`);
    return next;
  };
}

function cursorPage(cursor: string | null | undefined, prefix: "trace" | "span"): number {
  if (cursor === null || cursor === undefined) return 0;
  const match = new RegExp(`^${prefix}-(\\d+)$`).exec(cursor);
  if (!match?.[1]) throw new Error(`Unexpected ${prefix} cursor: ${cursor}`);
  return Number(match[1]);
}

function canonicalTransport(span: Span): DashboardQueryTransport {
  return async (request) => {
    switch (request.kind) {
      case "bootstrap": return response({ kind: "bootstrap", limits });
      case "traces": return response({ kind: "traces", rows: [], pagination: { nextCursor: null, total: 0 } });
      case "facets": return response({ kind: "facets", rows: [], pagination: { nextCursor: null } });
      case "summary": return response({ kind: "summary", pagination: { nextCursor: null }, work: { state: "complete", kpis: completeKpis() } });
      case "spans": return response({ kind: "spans", rows: [], pagination: { nextCursor: null, total: 0 } });
      case "span": return response({ kind: "span", detail: span });
    }
  };
}

function response(fields: Record<string, unknown>): DashboardQueryResponseV1 {
  return {
    schemaVersion: "agent_observability.dashboard_query.v1",
    snapshot,
    scope,
    work: { state: "pending" },
    ...fields,
  } as DashboardQueryResponseV1;
}

function status(requestKind: string, reason: string): DashboardQueryResponseV1 {
  return {
    schemaVersion: "agent_observability.dashboard_query.v1",
    kind: "status",
    requestKind,
    reason,
  } as DashboardQueryResponseV1;
}

function completeKpis() {
  return {
    sessions: 2,
    turns: 3,
    llm: 4,
    tools: 5,
    errors: 1,
    inputTokens: 100,
    outputTokens: 20,
    totalTokens: 120,
    tokenStatus: "complete",
    estimatedCost: 0.01,
    costStatus: "estimated",
    currency: "USD",
  } as const;
}

function spanDetail(turnId: string): Span {
  return {
    schemaVersion: "agent_observability.v1",
    traceId: projected("a"),
    spanId: projected("b"),
    parentSpanId: null,
    kind: "llm.request",
    name: "Model request",
    status: "ok",
    startTimeUnixMs: 1,
    endTimeUnixMs: 2,
    repo: "repo-a",
    agent: { name: "codex", model: "gpt-test" },
    availability: {
      repository: { state: "available", reason: "reported_by_adapter" },
      turn: { state: "available", reason: "reported_by_adapter" },
      model: { state: "available", reason: "reported_by_adapter" },
      tokens: { state: "available", reason: "reported_by_adapter" },
      latency: { state: "available", reason: "reported_by_adapter" },
      sourceLocation: { state: "private_lookup", reason: "local_opt_in_lookup_required" },
      requestContent: { state: "private_lookup", reason: "local_opt_in_lookup_required" },
      responseContent: { state: "private_lookup", reason: "local_opt_in_lookup_required" },
    },
    turnId,
    attributes: {},
    metrics: {},
    cost: { status: "unknown", rate_table: {}, cost: { assumption: "fixture" } },
  };
}

function privateDetail(turnId: string): PrivateCodexTurnDetailV1 {
  return {
    schemaVersion: "agent_observability.private_turn_detail.v1",
    turnId,
    cwd: "/private/worktree",
    inputMessages: ["private input"],
    lastAssistantMessage: "private response",
  };
}
