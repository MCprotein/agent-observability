import type { Span, Trace } from "./generated/report-dto-v1.js";

export type FilterKey = "repo" | "session" | "agent" | "model";
export type DimensionFilters = Record<FilterKey, string | undefined>;
export type FilterState = DimensionFilters & { text: string; trace: string | undefined };

export interface FilteredView {
  spans: Span[];
  traces: Trace[];
  spansByTrace: Map<string, Span[]>;
}

export interface Page<T> {
  items: T[];
  index: number;
  count: number;
  total: number;
}

export interface TimelineItem {
  span: Span;
  leftPercent: number;
  widthPercent: number;
}

interface SavedFilterEnvelope {
  version: 1;
  filters: DimensionFilters[];
}

const UNKNOWN = "unknown";

export function buildFilteredView(
  spans: Span[],
  traces: Trace[],
  state: FilterState,
): FilteredView {
  const filteredSpans: Span[] = [];
  const spansByTrace = new Map<string, Span[]>();

  for (const span of spans) {
    if (!matchesFilters(span, state)) continue;
    filteredSpans.push(span);
    const traceSpans = spansByTrace.get(span.traceId);
    if (traceSpans) traceSpans.push(span);
    else spansByTrace.set(span.traceId, [span]);
  }

  return {
    spans: filteredSpans,
    traces: traces.filter((trace) => spansByTrace.has(trace.traceId)),
    spansByTrace,
  };
}

export function paginate<T>(items: T[], requestedIndex: number, pageSize: number): Page<T> {
  if (!Number.isInteger(pageSize) || pageSize <= 0) throw new Error("pageSize must be positive");
  const count = Math.max(1, Math.ceil(items.length / pageSize));
  const index = Math.min(Math.max(0, requestedIndex), count - 1);
  return {
    items: items.slice(index * pageSize, (index + 1) * pageSize),
    index,
    count,
    total: items.length,
  };
}

export function buildTimeline(spans: Span[], limit: number): TimelineItem[] {
  if (!Number.isInteger(limit) || limit <= 0) throw new Error("limit must be positive");
  if (spans.length === 0) return [];

  let start = Number.POSITIVE_INFINITY;
  let end = Number.NEGATIVE_INFINITY;
  for (const span of spans) {
    start = Math.min(start, span.startTimeUnixMs);
    end = Math.max(end, spanEnd(span));
  }
  const range = Math.max(1, end - start);

  return spans.slice(0, limit).map((span) => {
    const leftPercent = Math.min(99, ((span.startTimeUnixMs - start) / range) * 100);
    const available = Math.max(0, 100 - leftPercent);
    const durationPercent = ((spanEnd(span) - span.startTimeUnixMs) / range) * 100;
    return {
      span,
      leftPercent,
      widthPercent: Math.min(available, Math.max(0.8, durationPercent)),
    };
  });
}

export function parseSavedFilters(value: string | null, limit: number): DimensionFilters[] {
  if (!value || value.length > 32_768) return [];
  try {
    const candidate = JSON.parse(value) as unknown;
    if (!isSavedFilterEnvelope(candidate)) return [];
    const result: DimensionFilters[] = [];
    for (const filters of candidate.filters) {
      if (!isPersistableDimensions(filters) || result.some((saved) => sameDimensions(saved, filters))) continue;
      result.push(filters);
      if (result.length >= limit) break;
    }
    return result;
  } catch {
    return [];
  }
}

export function serializeSavedFilters(filters: DimensionFilters[]): string {
  if (filters.some((candidate) => !isPersistableDimensions(candidate))) {
    throw new Error("Saved filters contain a non-persistable value");
  }
  const envelope: SavedFilterEnvelope = { version: 1, filters };
  return JSON.stringify(envelope);
}

export function isPersistableDimensions(filters: DimensionFilters): boolean {
  if (!hasDimensions(filters)) return false;
  const allowed = new Set(["repo", "session", "agent", "model"]);
  if (Object.keys(filters).some((key) => !allowed.has(key))) return false;
  const safeName = /^[A-Za-z0-9._:-]{1,128}$/;
  const safeModel = /^[A-Za-z0-9._:-]{1,128}(?:\/[A-Za-z0-9._:-]{1,128}){0,2}$/;
  const opaqueSession = /^id:sha256:[a-f0-9]{64}$/;
  return (filters.repo === undefined || safeName.test(filters.repo))
    && (filters.session === undefined || opaqueSession.test(filters.session))
    && (filters.agent === undefined || safeName.test(filters.agent))
    && (filters.model === undefined || safeModel.test(filters.model));
}

export function sameDimensions(left: DimensionFilters, right: DimensionFilters): boolean {
  return left.repo === right.repo
    && left.session === right.session
    && left.agent === right.agent
    && left.model === right.model;
}

function matchesFilters(span: Span, state: FilterState): boolean {
  if (state.repo !== undefined && span.repo !== state.repo) return false;
  if (state.session !== undefined && span.sessionId !== state.session) return false;
  if (state.agent !== undefined && (span.agent.name ?? UNKNOWN) !== state.agent) return false;
  if (state.model !== undefined && (span.agent.model ?? UNKNOWN) !== state.model) return false;
  if (!state.text) return true;
  return [
    span.name,
    span.kind,
    span.status,
    span.toolName,
    span.traceId,
    span.spanId,
    span.repo,
    span.sessionId,
    span.agent.name,
    span.agent.model,
  ].join(" ").toLowerCase().includes(state.text);
}

function spanEnd(span: Span): number {
  const inferredDuration = span.metrics.latencyMs ?? span.metrics.durationMs ?? 0;
  return Math.max(span.startTimeUnixMs, span.endTimeUnixMs ?? span.startTimeUnixMs + inferredDuration);
}

function isDimensionFilters(value: unknown): value is DimensionFilters {
  if (!isObject(value)) return false;
  const allowed = new Set(["repo", "session", "agent", "model"]);
  if (Object.keys(value).some((key) => !allowed.has(key))) return false;
  return ["repo", "session", "agent", "model"].every((key) => {
    const candidate = value[key];
    return candidate === undefined || (typeof candidate === "string" && candidate.length <= 512);
  });
}

function hasDimensions(filters: DimensionFilters): boolean {
  return filters.repo !== undefined
    || filters.session !== undefined
    || filters.agent !== undefined
    || filters.model !== undefined;
}

function isSavedFilterEnvelope(value: unknown): value is SavedFilterEnvelope {
  if (!isObject(value) || value.version !== 1 || !Array.isArray(value.filters)) return false;
  if (Object.keys(value).some((key) => key !== "version" && key !== "filters")) return false;
  return value.filters.every(isDimensionFilters);
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
