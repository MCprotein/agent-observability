import type {
  Cursor,
  DashboardFacetsResponseV1,
  DashboardFiltersV1,
  DashboardKpisV1,
  DashboardPaginationV1,
  DashboardQueryLimitsV1,
  DashboardQueryRequestV1,
  DashboardQueryResponseV1,
  DashboardQueryScopeV1,
  DashboardRequestKindV1,
  DashboardSnapshotV1,
  DashboardSpanResponseV1,
  DashboardSpanRowV1,
  DashboardSpansResponseV1,
  DashboardSummaryResponseV1,
  DashboardTraceRowV1,
  DashboardTracesResponseV1,
  Span,
} from "./generated/dashboard-query-v1.js";
import type { PrivateCodexTurnDetailV1 } from "./generated/private-codex-turn-detail-v1.js";

export const DASHBOARD_SCHEMA_VERSION = "agent_observability.dashboard_query.v1" as const;
export const DASHBOARD_REQUEST_BYTES = 8_192;
export const DASHBOARD_RESPONSE_BYTES = 1_048_576;
export const PRIVATE_DETAIL_RESPONSE_BYTES = 65_536;
export const DASHBOARD_CURSOR_HISTORY_LIMIT = 16;

export type DashboardAvailability =
  | "idle"
  | "loading"
  | "current"
  | "stale"
  | "building"
  | "refresh_pending"
  | "expired"
  | "capacity"
  | "error";

export interface PageState<Row> {
  readonly rows: readonly Row[];
  readonly total?: number;
  readonly nextCursor: Cursor | null;
  readonly page: number;
  readonly canGoBack: boolean;
}

export interface PagedDashboardState {
  readonly availability: DashboardAvailability;
  readonly reason?: string | undefined;
  readonly snapshot?: DashboardSnapshotV1 | undefined;
  readonly scope?: DashboardQueryScopeV1 | undefined;
  readonly limits?: DashboardQueryLimitsV1 | undefined;
  readonly filters: DashboardFiltersV1;
  readonly summary: { readonly state: "idle" | "pending" | "complete"; readonly kpis?: DashboardKpisV1 };
  readonly traces: PageState<DashboardTraceRowV1>;
  readonly spans: PageState<DashboardSpanRowV1>;
  readonly facets: readonly DashboardFacetsResponseV1["rows"][number][];
  readonly facetsLimited: boolean;
  readonly selectedTraceId?: string | undefined;
  readonly selectedSpan?: Span | undefined;
  readonly privateDetail: PrivateDetailState;
}

export type PrivateDetailState =
  | { readonly state: "idle" | "eligible" | "loading" }
  | { readonly state: "loaded"; readonly detail: PrivateCodexTurnDetailV1 }
  | { readonly state: "not_collected" | "not_applicable" | "unavailable" | "error"; readonly reason: string };

export type DashboardWireValidator = (value: unknown) => value is DashboardQueryResponseV1;
export type DashboardQueryTransport = (
  request: DashboardQueryRequestV1,
  signal: AbortSignal,
) => Promise<DashboardQueryResponseV1>;
export type PrivateDetailValidator = (value: unknown) => value is PrivateCodexTurnDetailV1;
export type PrivateDetailTransport = (
  turnId: string,
  signal: AbortSignal,
) => Promise<PrivateDetailTransportResult>;
export type PrivateDetailTransportResult =
  | { readonly state: "loaded"; readonly detail: PrivateCodexTurnDetailV1 }
  | { readonly state: "not_collected" | "unavailable"; readonly reason: string };

interface ClientOptions {
  readonly transport: DashboardQueryTransport;
  readonly privateDetailTransport?: PrivateDetailTransport;
  readonly onState?: (state: PagedDashboardState) => void;
  readonly isHidden?: () => boolean;
  readonly backoffMs?: number;
  readonly autoLoad?: boolean;
}

interface Task {
  readonly revision: number;
  readonly request: DashboardQueryRequestV1;
  readonly purpose: "bootstrap" | "traces" | "spans" | "summary" | "span" | "facets";
  readonly page: number;
  readonly readyAt: number;
}

type BoundRequest = DashboardQueryRequestV1 & {
  readonly snapshotId?: string;
  readonly cursor?: Cursor | null;
};

const emptyPage = <Row>(): PageState<Row> => ({
  rows: [],
  nextCursor: null,
  page: 0,
  canGoBack: false,
});

export class PagedDashboardClient {
  private readonly transport: DashboardQueryTransport;
  private readonly privateDetailTransport: PrivateDetailTransport | undefined;
  private readonly onState: ((state: PagedDashboardState) => void) | undefined;
  private readonly isHidden: () => boolean;
  private readonly backoffMs: number;
  private readonly autoLoad: boolean;
  private stateValue: PagedDashboardState = initialState({});
  private queue: Task[] = [];
  private revision = 0;
  private draining = false;
  private controller: AbortController | undefined;
  private privateDetailController: AbortController | undefined;
  private wakeTimer: ReturnType<typeof setTimeout> | undefined;
  private traceCursors: Array<Cursor | null> = [null];
  private spanCursors: Array<Cursor | null> = [null];
  private selectedSpanRequestId: string | undefined;
  private privateDetailRevision = 0;
  private idleWaiters: Array<() => void> = [];

  constructor(options: ClientOptions) {
    this.transport = options.transport;
    this.privateDetailTransport = options.privateDetailTransport;
    this.onState = options.onState;
    this.isHidden = options.isHidden ?? (() => false);
    this.backoffMs = options.backoffMs ?? 250;
    this.autoLoad = options.autoLoad ?? true;
  }

  get state(): PagedDashboardState {
    return this.stateValue;
  }

  start(): void {
    this.restartWithBootstrap(this.stateValue.filters);
  }

  refresh(): void {
    this.restartWithBootstrap(this.stateValue.filters);
  }

  setFilters(filters: DashboardFiltersV1): void {
    const normalized = normalizeFilters(filters);
    this.invalidatePendingWork();
    this.traceCursors = [null];
    this.spanCursors = [null];
    this.selectedSpanRequestId = undefined;
    this.update({
      filters: normalized,
      summary: { state: "idle" },
      traces: emptyPage(),
      spans: emptyPage(),
      facets: [],
      facetsLimited: false,
      selectedTraceId: undefined,
      selectedSpan: undefined,
      privateDetail: { state: "idle" },
      availability: this.stateValue.snapshot === undefined ? "loading" : availabilityOf(this.stateValue.snapshot),
      reason: undefined,
    });
    if (this.stateValue.snapshot === undefined || this.stateValue.availability === "expired") {
      this.enqueueBootstrap();
    } else {
      this.enqueueInitialQueries();
    }
  }

  selectTrace(traceId: string): void {
    const snapshot = this.stateValue.snapshot;
    if (!snapshot || !isProjectedId(traceId)) return;
    this.spanCursors = [null];
    this.selectedSpanRequestId = undefined;
    this.clearPrivateDetail();
    this.update({
      selectedTraceId: traceId,
      selectedSpan: undefined,
      spans: emptyPage(),
      availability: availabilityOf(snapshot),
      reason: undefined,
    });
    this.enqueueBound("spans", { kind: "spans", traceId }, null, 0);
  }

  selectSpan(spanId: string): void {
    const traceId = this.stateValue.selectedTraceId;
    if (!traceId || !isProjectedId(spanId)) return;
    this.clearPrivateDetail();
    this.selectedSpanRequestId = spanId;
    this.enqueueBound("span", { kind: "span", traceId, spanId }, null, 0);
  }

  clearSelectedDetail(): void {
    this.selectedSpanRequestId = undefined;
    this.clearPrivateDetail();
    this.update({ selectedSpan: undefined });
  }

  async loadPrivateDetail(): Promise<void> {
    const span = this.stateValue.selectedSpan;
    const transport = this.privateDetailTransport;
    if (!span || !transport || !privateDetailEligible(span)) {
      this.update({ privateDetail: { state: "not_applicable", reason: "This span is not eligible for private local detail." } });
      return;
    }
    const turnId = span.turnId;
    if (!turnId) {
      this.update({ privateDetail: { state: "not_applicable", reason: "Turn correlation is unavailable." } });
      return;
    }
    this.privateDetailController?.abort();
    const revision = ++this.privateDetailRevision;
    const controller = new AbortController();
    this.privateDetailController = controller;
    this.update({ privateDetail: { state: "loading" } });
    try {
      const result = await transport(turnId, controller.signal);
      if (revision !== this.privateDetailRevision || this.stateValue.selectedSpan?.turnId !== turnId) return;
      this.update({ privateDetail: result });
    } catch (error) {
      if (revision !== this.privateDetailRevision || isAbortError(error)) return;
      this.update({ privateDetail: { state: "error", reason: errorMessage(error) } });
    } finally {
      if (revision === this.privateDetailRevision) this.privateDetailController = undefined;
    }
  }

  nextTraces(): void {
    const cursor = this.stateValue.traces.nextCursor;
    if (!cursor) return;
    this.traceCursors.push(cursor);
    if (this.traceCursors.length > DASHBOARD_CURSOR_HISTORY_LIMIT) this.traceCursors.shift();
    this.enqueueBound("traces", { kind: "traces", cursor }, cursor, this.stateValue.traces.page + 1);
  }

  previousTraces(): void {
    if (this.traceCursors.length <= 1) return;
    this.traceCursors.pop();
    const cursor = this.traceCursors.at(-1) ?? null;
    this.enqueueBound("traces", { kind: "traces", cursor }, cursor, Math.max(0, this.stateValue.traces.page - 1));
  }

  nextSpans(): void {
    const cursor = this.stateValue.spans.nextCursor;
    const traceId = this.stateValue.selectedTraceId;
    if (!cursor || !traceId) return;
    this.spanCursors.push(cursor);
    if (this.spanCursors.length > DASHBOARD_CURSOR_HISTORY_LIMIT) this.spanCursors.shift();
    this.enqueueBound("spans", { kind: "spans", traceId, cursor }, cursor, this.stateValue.spans.page + 1);
  }

  previousSpans(): void {
    const traceId = this.stateValue.selectedTraceId;
    if (!traceId || this.spanCursors.length <= 1) return;
    this.spanCursors.pop();
    const cursor = this.spanCursors.at(-1) ?? null;
    this.enqueueBound("spans", { kind: "spans", traceId, cursor }, cursor, Math.max(0, this.stateValue.spans.page - 1));
  }

  resume(): void {
    this.scheduleDrain();
  }

  async settled(): Promise<void> {
    if (!this.draining && this.queue.length === 0 && this.wakeTimer === undefined) return;
    await new Promise<void>((resolve) => this.idleWaiters.push(resolve));
  }

  dispose(): void {
    this.invalidatePendingWork();
    this.resolveIdle();
  }

  private restartWithBootstrap(filters: DashboardFiltersV1): void {
    this.invalidatePendingWork();
    this.traceCursors = [null];
    this.spanCursors = [null];
    this.selectedSpanRequestId = undefined;
    this.clearPrivateDetail();
    this.stateValue = initialState(normalizeFilters(filters));
    this.emit();
    this.enqueueBootstrap();
  }

  private enqueueBootstrap(): void {
    this.enqueue({
      revision: this.revision,
      request: baseRequest("bootstrap", this.stateValue.filters),
      purpose: "bootstrap",
      page: 0,
      readyAt: Date.now(),
    });
  }

  private enqueueInitialQueries(): void {
    this.enqueueBound("traces", { kind: "traces" }, null, 0);
    this.enqueueBound("facets", { kind: "facets" }, null, 0);
    this.update({ summary: { state: "pending" } });
    this.enqueueBound("summary", { kind: "summary" }, null, 0);
  }

  private enqueueBound(
    purpose: Task["purpose"],
    fields: Record<string, unknown>,
    cursor: Cursor | null,
    page: number,
    delay = 0,
  ): void {
    const snapshot = this.stateValue.snapshot;
    if (!snapshot) return;
    const { cursor: requestCursor, ...nonCursorFields } = fields;
    const request = {
      ...baseRequest(purpose, this.stateValue.filters),
      snapshotId: snapshot.id,
      ...nonCursorFields,
      ...(requestCursor === null || requestCursor === undefined ? {} : { cursor: requestCursor }),
    } as BoundRequest;
    this.enqueue({ revision: this.revision, request, purpose, page, readyAt: Date.now() + delay });
  }

  private enqueue(task: Task): void {
    this.queue.push(task);
    this.scheduleDrain();
  }

  private scheduleDrain(): void {
    if (this.draining || this.isHidden() || this.queue.length === 0) return;
    const readyAt = Math.min(...this.queue.map((task) => task.readyAt));
    const delay = Math.max(0, readyAt - Date.now());
    if (delay > 0) {
      if (this.wakeTimer === undefined) {
        this.wakeTimer = setTimeout(() => {
          this.wakeTimer = undefined;
          this.scheduleDrain();
        }, delay);
      }
      return;
    }
    void this.drain();
  }

  private async drain(): Promise<void> {
    if (this.draining) return;
    this.draining = true;
    try {
      while (!this.isHidden()) {
        const taskIndex = this.queue.findIndex((candidate) => candidate.readyAt <= Date.now());
        if (taskIndex < 0) break;
        const [task] = this.queue.splice(taskIndex, 1);
        if (!task) break;
        if (task.revision !== this.revision) continue;
        this.controller = new AbortController();
        try {
          const response = await this.transport(task.request, this.controller.signal);
          if (task.revision !== this.revision) continue;
          this.applyResponse(task, response);
        } catch (error) {
          if (task.revision !== this.revision || isAbortError(error)) continue;
          this.update({ availability: error instanceof DashboardTransportError && error.code === "capacity" ? "capacity" : "error", reason: errorMessage(error) });
          this.queue = [];
        } finally {
          this.controller = undefined;
        }
      }
    } finally {
      this.draining = false;
      if (this.queue.length === 0) this.resolveIdle();
      else this.scheduleDrain();
    }
  }

  private applyResponse(task: Task, response: DashboardQueryResponseV1): void {
    if (response.kind === "status") {
      this.applyStatus(task, response.reason);
      return;
    }
    if (response.kind !== task.purpose) {
      throw new DashboardTransportError("invalid_response", `Expected ${task.purpose}, received ${response.kind}`);
    }
    const requestedSnapshotId = (task.request as BoundRequest).snapshotId;
    if (requestedSnapshotId !== undefined && response.snapshot.id !== requestedSnapshotId) {
      throw new DashboardTransportError("invalid_response", "Dashboard response changed the bound snapshot");
    }
    this.update({
      snapshot: response.snapshot,
      scope: response.scope,
      availability: availabilityOf(response.snapshot),
      reason: response.scope.coldExcluded ? "cold_excluded" : undefined,
    });
    switch (response.kind) {
      case "bootstrap":
        this.update({ limits: response.limits });
        if (this.autoLoad) this.enqueueInitialQueries();
        break;
      case "traces":
        this.applyTracePage(response, task.page);
        break;
      case "spans":
        if ((task.request as BoundRequest & { traceId?: string }).traceId !== this.stateValue.selectedTraceId) break;
        this.applySpanPage(response, task.page);
        break;
      case "summary":
        this.applySummary(response);
        break;
      case "facets":
        this.applyFacets(response, task);
        break;
      case "span":
        if ((task.request as BoundRequest & { spanId?: string }).spanId === this.selectedSpanRequestId) {
          const detail = (response as DashboardSpanResponseV1).detail;
          this.update({
            selectedSpan: detail,
            privateDetail: privateDetailEligible(detail)
              ? { state: "eligible" }
              : { state: "not_applicable", reason: "This span is not eligible for private local detail." },
          });
        }
        break;
    }
  }

  private applyTracePage(response: DashboardTracesResponseV1, page: number): void {
    this.update({ traces: { ...pageState(response.rows, response.pagination, page), canGoBack: this.traceCursors.length > 1 } });
  }

  private applySpanPage(response: DashboardSpansResponseV1, page: number): void {
    this.update({ spans: { ...pageState(response.rows, response.pagination, page), canGoBack: this.spanCursors.length > 1 } });
  }

  private applySummary(response: DashboardSummaryResponseV1): void {
    if (response.work.state === "complete") {
      this.update({ summary: { state: "complete", kpis: response.work.kpis } });
      return;
    }
    this.update({ summary: { state: "pending" } });
    const pagination = (response as DashboardSummaryResponseV1 & { pagination: DashboardPaginationV1 }).pagination;
    if (pagination.nextCursor) {
      this.enqueueBound("summary", { kind: "summary", cursor: pagination.nextCursor }, pagination.nextCursor, 0, Math.min(this.backoffMs, 16));
    }
  }

  private applyFacets(response: DashboardFacetsResponseV1, task: Task): void {
    const limit = this.stateValue.limits?.facetValues ?? 500;
    // Reserve equal bounded capacity for all dimensions; a large session list must not
    // prevent later agent/model dimensions from ever becoming selectable.
    const perDimension = Math.floor(limit / 4);
    const counts = { repo: 0, session: 0, agent: 0, model: 0 };
    const rows = [...this.stateValue.facets];
    for (const row of rows) counts[row.dimension] += 1;
    let limited = this.stateValue.facetsLimited;
    for (const row of response.rows) {
      if (counts[row.dimension] >= perDimension) {
        limited = true;
        continue;
      }
      rows.push(row);
      counts[row.dimension] += 1;
    }
    this.update({ facets: rows, facetsLimited: limited });
    if (response.pagination.nextCursor) {
      this.enqueueBound("facets", { kind: "facets", cursor: response.pagination.nextCursor }, response.pagination.nextCursor, task.page + 1, Math.min(this.backoffMs, 16));
    }
  }

  private applyStatus(task: Task, reason: string): void {
    if (reason === "busy" || reason === "building") {
      this.update({ availability: reason === "building" ? "building" : this.stateValue.availability, reason });
      this.enqueue({ ...task, readyAt: Date.now() + this.backoffMs });
      return;
    }
    if (reason === "refresh_pending") {
      this.clearPrivateDetail();
      this.stateValue = { ...initialState(this.stateValue.filters), availability: "refresh_pending", reason };
      this.emit();
      this.queue = [];
      this.enqueueBootstrapAfter(this.backoffMs);
      return;
    }
    this.queue = [];
    if (reason === "snapshot_expired") {
      this.clearPrivateDetail();
      this.stateValue = { ...initialState(this.stateValue.filters), availability: "expired", reason };
      this.emit();
    } else if (reason === "capacity") {
      this.update({ availability: "capacity", reason });
    } else {
      this.update({ availability: "error", reason });
    }
  }

  private enqueueBootstrapAfter(delay: number): void {
    this.enqueue({
      revision: this.revision,
      request: baseRequest("bootstrap", this.stateValue.filters),
      purpose: "bootstrap",
      page: 0,
      readyAt: Date.now() + delay,
    });
  }
  private invalidatePendingWork(): void {
    this.revision += 1;
    this.controller?.abort();
    this.controller = undefined;
    this.queue = [];
    if (this.wakeTimer !== undefined) clearTimeout(this.wakeTimer);
    this.wakeTimer = undefined;
    this.clearPrivateDetail();
  }

  private clearPrivateDetail(): void {
    this.privateDetailRevision += 1;
    this.privateDetailController?.abort();
    this.privateDetailController = undefined;
    if (this.stateValue.privateDetail.state !== "idle") {
      this.stateValue = { ...this.stateValue, privateDetail: { state: "idle" } };
      this.emit();
    }
  }

  private update(patch: Partial<PagedDashboardState>): void {
    this.stateValue = { ...this.stateValue, ...patch } as PagedDashboardState;
    this.emit();
  }

  private emit(): void {
    this.onState?.(this.stateValue);
  }

  private resolveIdle(): void {
    const waiters = this.idleWaiters.splice(0);
    for (const resolve of waiters) resolve();
  }
}

export function createDashboardTransport(options: {
  readonly endpoint?: string;
  readonly fetch?: typeof fetch;
  readonly validate: DashboardWireValidator;
  readonly origin?: string;
}): DashboardQueryTransport {
  const fetchValue = options.fetch ?? globalThis.fetch;
  const endpoint = dashboardQueryEndpoint({
    href: globalThis.location?.href ?? `${options.origin ?? "http://localhost"}/`,
    pathname: globalThis.location?.pathname ?? "/report",
  }, options.endpoint);
  const origin = options.origin ?? globalThis.location?.origin ?? endpoint.origin;
  if (endpoint.origin !== origin) throw new DashboardTransportError("cross_origin", "Dashboard query endpoint must be same-origin");

  return async (request, signal) => {
    const serialized = JSON.stringify(request);
    if (utf8Bytes(serialized) > DASHBOARD_REQUEST_BYTES) {
      throw new DashboardTransportError("request_too_large", "Dashboard query exceeds the 8 KiB request bound");
    }
    const encoded = encodeURIComponent(serialized);
    if (utf8Bytes(encoded) > DASHBOARD_REQUEST_BYTES) {
      throw new DashboardTransportError("request_too_large", "Encoded dashboard query exceeds the 8 KiB request bound");
    }
    const url = new URL(endpoint);
    url.search = new URLSearchParams({ request: serialized }).toString();
    const response = await fetchValue(url, {
      method: "GET",
      cache: "no-store",
      credentials: "same-origin",
      headers: { Accept: "application/json" },
      signal,
    });
    const body = await readBoundedResponse(response, DASHBOARD_RESPONSE_BYTES);
    if (response.status >= 500) {
      throw new DashboardTransportError("http_error", `Dashboard storage query failed with HTTP ${response.status}`);
    }
    let value: unknown;
    try {
      value = JSON.parse(body);
    } catch {
      throw new DashboardTransportError("invalid_response", "Dashboard returned invalid JSON");
    }
    if (!options.validate(value)) {
      throw new DashboardTransportError("invalid_response", "Dashboard response failed schema validation");
    }
    if (!response.ok && value.kind !== "status") {
      throw new DashboardTransportError("http_error", `Dashboard query failed with HTTP ${response.status}`);
    }
    return value;
  };
}

export function dashboardQueryEndpoint(
  location: { readonly href: string; readonly pathname: string },
  endpoint?: string,
): URL {
  const currentPath = location.pathname.replace(/\/$/, "");
  return new URL(endpoint ?? `${currentPath}/query`, location.href);
}

export function createPrivateDetailTransport(options: {
  readonly fetch?: typeof fetch;
  readonly validate: PrivateDetailValidator;
  readonly origin?: string;
  readonly reportPath?: string;
}): PrivateDetailTransport {
  const fetchValue = options.fetch ?? globalThis.fetch;
  const origin = new URL(options.origin ?? globalThis.location?.origin ?? "http://localhost").origin;
  const reportPath = (options.reportPath ?? globalThis.location?.pathname ?? "/report").replace(/\/$/, "");
  return async (turnId, signal) => {
    if (!isProjectedId(turnId)) throw new DashboardTransportError("invalid_query", "Invalid private-detail turn identifier");
    const endpoint = new URL(`${reportPath}/details/${encodeURIComponent(turnId)}`, `${origin}/`);
    if (endpoint.origin !== origin) throw new DashboardTransportError("cross_origin", "Private-detail endpoint must be same-origin");
    const response = await fetchValue(endpoint, {
      method: "GET",
      cache: "no-store",
      credentials: "same-origin",
      headers: { Accept: "application/json" },
      signal,
    });
    const body = await readBoundedResponse(response, PRIVATE_DETAIL_RESPONSE_BYTES, "Private detail");
    if (response.status === 404) return { state: "not_collected", reason: "Not collected. Enable private Codex turn details for future turns." };
    if (response.status === 503) return { state: "unavailable", reason: boundedFailureReason(body) };
    if (!response.ok) throw new DashboardTransportError("http_error", `Private-detail lookup failed with HTTP ${response.status}`);
    let value: unknown;
    try {
      value = JSON.parse(body);
    } catch {
      throw new DashboardTransportError("invalid_response", "Private detail returned invalid JSON");
    }
    if (!options.validate(value) || value.turnId !== turnId) {
      throw new DashboardTransportError("invalid_response", "Private detail failed contract validation");
    }
    return { state: "loaded", detail: value };
  };
}

export class DashboardTransportError extends Error {
  constructor(readonly code: string, message: string) {
    super(message);
    this.name = "DashboardTransportError";
  }
}

async function readBoundedResponse(response: Response, limit: number, label = "Dashboard"): Promise<string> {
  const declared = Number(response.headers.get("content-length"));
  if (Number.isFinite(declared) && declared > limit) {
    await response.body?.cancel();
    throw new DashboardTransportError("capacity", `${label} response exceeds its byte bound`);
  }
  if (!response.body) return "";
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > limit) {
      await reader.cancel();
      throw new DashboardTransportError("capacity", `${label} response exceeds its byte bound`);
    }
    chunks.push(value);
  }
  const joined = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    joined.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return new TextDecoder("utf-8", { fatal: true }).decode(joined);
}

function initialState(filters: DashboardFiltersV1): PagedDashboardState {
  return {
    availability: "loading",
    filters,
    summary: { state: "idle" },
    traces: emptyPage(),
    spans: emptyPage(),
    facets: [],
    facetsLimited: false,
    privateDetail: { state: "idle" },
  };
}

function baseRequest(kind: DashboardRequestKindV1, filters: DashboardFiltersV1): DashboardQueryRequestV1 {
  return { schemaVersion: DASHBOARD_SCHEMA_VERSION, kind, ...(Object.keys(filters).length === 0 ? {} : { filters }) } as DashboardQueryRequestV1;
}

function pageState<Row>(rows: readonly Row[], pagination: DashboardPaginationV1, page: number): PageState<Row> {
  return {
    rows,
    ...(pagination.total === undefined ? {} : { total: pagination.total }),
    nextCursor: pagination.nextCursor,
    page,
    canGoBack: page > 0,
  };
}

function availabilityOf(snapshot: DashboardSnapshotV1): "current" | "stale" {
  return snapshot.state;
}

function normalizeFilters(filters: DashboardFiltersV1): DashboardFiltersV1 {
  const result: Record<string, unknown> = {};
  for (const key of ["repo", "session", "agent", "model"] as const) {
    const values = filters[key];
    if (values && values.length > 0) result[key] = [...new Set(values)].slice(0, 16);
  }
  const text = filters.text?.trim();
  if (text) result.text = text.slice(0, 512);
  return result as DashboardFiltersV1;
}

function isProjectedId(value: string): boolean {
  return /^id:sha256:[a-f0-9]{64}$/.test(value);
}

function privateDetailEligible(span: Span): boolean {
  return span.turnId !== undefined && [
    span.availability.sourceLocation,
    span.availability.requestContent,
    span.availability.responseContent,
  ].every((field) => field.state === "private_lookup");
}

function boundedFailureReason(body: string): string {
  try {
    const value = JSON.parse(body) as { code?: unknown };
    return typeof value.code === "string" && /^[a-z0-9_]{1,64}$/.test(value.code)
      ? `Capture failed (${value.code}).`
      : "Private detail is temporarily unavailable.";
  } catch {
    return "Private detail is temporarily unavailable.";
  }
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Dashboard query failed";
}

function utf8Bytes(value: string): number {
  return new TextEncoder().encode(value).byteLength;
}
