/* Generated from contracts/report-dto-v2.schema.json. Do not edit. */

// ui/report/view-state.ts
var UNKNOWN = "unknown";
function buildFilteredView(spans, traces, state) {
  const filteredSpans = [];
  const spansByTrace = /* @__PURE__ */ new Map();
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
    spansByTrace
  };
}
function paginate(items, requestedIndex, pageSize) {
  if (!Number.isInteger(pageSize) || pageSize <= 0) throw new Error("pageSize must be positive");
  const count = Math.max(1, Math.ceil(items.length / pageSize));
  const index = Math.min(Math.max(0, requestedIndex), count - 1);
  return {
    items: items.slice(index * pageSize, (index + 1) * pageSize),
    index,
    count,
    total: items.length
  };
}
function buildTimeline(spans, limit) {
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
    const leftPercent = Math.min(99, (span.startTimeUnixMs - start) / range * 100);
    const available = Math.max(0, 100 - leftPercent);
    const durationPercent = (spanEnd(span) - span.startTimeUnixMs) / range * 100;
    return {
      span,
      leftPercent,
      widthPercent: Math.min(available, Math.max(0.8, durationPercent))
    };
  });
}
function parseSavedFilters(value, limit) {
  if (!value || value.length > 32768) return [];
  try {
    const candidate = JSON.parse(value);
    if (!isSavedFilterEnvelope(candidate)) return [];
    const result = [];
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
function serializeSavedFilters(filters) {
  if (filters.some((candidate) => !isPersistableDimensions(candidate))) {
    throw new Error("Saved filters contain a non-persistable value");
  }
  const envelope = { version: 1, filters };
  return JSON.stringify(envelope);
}
function isPersistableDimensions(filters) {
  if (!hasDimensions(filters)) return false;
  const allowed = /* @__PURE__ */ new Set(["repo", "session", "agent", "model"]);
  if (Object.keys(filters).some((key) => !allowed.has(key))) return false;
  const safeName = /^[A-Za-z0-9._:-]{1,128}$/;
  const safeModel = /^[A-Za-z0-9._:-]{1,128}(?:\/[A-Za-z0-9._:-]{1,128}){0,2}$/;
  const opaqueSession = /^id:sha256:[a-f0-9]{64}$/;
  return (filters.repo === void 0 || safeName.test(filters.repo)) && (filters.session === void 0 || opaqueSession.test(filters.session)) && (filters.agent === void 0 || safeName.test(filters.agent)) && (filters.model === void 0 || safeModel.test(filters.model));
}
function sameDimensions(left, right) {
  return left.repo === right.repo && left.session === right.session && left.agent === right.agent && left.model === right.model;
}
function matchesFilters(span, state) {
  if (state.repo !== void 0 && span.repo !== state.repo) return false;
  if (state.session !== void 0 && span.sessionId !== state.session) return false;
  if (state.agent !== void 0 && (span.agent.name ?? UNKNOWN) !== state.agent) return false;
  if (state.model !== void 0 && (span.agent.model ?? UNKNOWN) !== state.model) return false;
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
    span.agent.model
  ].join(" ").toLowerCase().includes(state.text);
}
function spanEnd(span) {
  const inferredDuration = span.metrics.latencyMs ?? span.metrics.durationMs ?? 0;
  return Math.max(span.startTimeUnixMs, span.endTimeUnixMs ?? span.startTimeUnixMs + inferredDuration);
}
function isDimensionFilters(value) {
  if (!isObject(value)) return false;
  const allowed = /* @__PURE__ */ new Set(["repo", "session", "agent", "model"]);
  if (Object.keys(value).some((key) => !allowed.has(key))) return false;
  return ["repo", "session", "agent", "model"].every((key) => {
    const candidate = value[key];
    return candidate === void 0 || typeof candidate === "string" && candidate.length <= 512;
  });
}
function hasDimensions(filters) {
  return filters.repo !== void 0 || filters.session !== void 0 || filters.agent !== void 0 || filters.model !== void 0;
}
function isSavedFilterEnvelope(value) {
  if (!isObject(value) || value.version !== 1 || !Array.isArray(value.filters)) return false;
  if (Object.keys(value).some((key) => key !== "version" && key !== "filters")) return false;
  return value.filters.every(isDimensionFilters);
}
function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
export {
  buildFilteredView,
  buildTimeline,
  isPersistableDimensions,
  paginate,
  parseSavedFilters,
  sameDimensions,
  serializeSavedFilters
};
