import { appendEventLog } from "../event-log.js";
import { createSpanRecord } from "../schema.js";

const SESSION_EVENT_TYPES = new Set(["session.started", "session_start", "session.created"]);
const TURN_EVENT_TYPES = new Set(["turn.started", "turn_start", "turn"]);
const LLM_EVENT_TYPES = new Set(["llm.completed", "llm_complete", "assistant.completed"]);
const TOOL_EVENT_TYPES = new Set([
  "tool.call",
  "tool.output",
  "tool.completed",
  "tool_complete",
  "tool_call.completed",
]);
const PERMISSION_EVENT_TYPES = new Set(["permission.requested", "permission.denied", "permission.approved"]);
const TOOL_CALL_PAYLOAD_TYPES = new Set([
  "function_call",
  "custom_tool_call",
  "tool_search_call",
]);
const TOOL_OUTPUT_PAYLOAD_TYPES = new Set([
  "function_call_output",
  "custom_tool_call_output",
  "tool_search_output",
  "patch_apply_end",
]);

export function parseCodexSessionJsonl(text) {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line, index) => parseJsonLine(line, index + 1));
}

export function codexRecordsFromEvents(events, options = {}) {
  const state = {
    sessionId: options.session_id ?? inferSessionId(events),
    traceId: options.trace_id ?? `codex:${options.session_id ?? inferSessionId(events)}`,
    project: compactObject({
      name: options.project_name,
      repo_path: options.repo_path,
    }),
    agent: compactObject({
      name: "codex",
      version: options.agent_version,
    }),
    sessionSpanId: null,
    turnSpans: new Map(),
    currentTurnId: options.turn_id ?? null,
    currentModel: options.model,
    sequence: 0,
    toolNamesByCallId: new Map(),
  };

  const records = [];

  for (const event of events) {
    const normalizedEvent = normalizeCodexEvent(event, state);
    if (!normalizedEvent) {
      continue;
    }

    const type = normalizedEvent.type ?? normalizedEvent.event ?? normalizedEvent.kind;

    if (SESSION_EVENT_TYPES.has(type)) {
      records.push(sessionRecord(normalizedEvent, state));
      continue;
    }

    if (TURN_EVENT_TYPES.has(type)) {
      records.push(ensureSessionRecord(normalizedEvent, state));
      records.push(ensureTurnRecord(normalizedEvent, state));
      continue;
    }

    if (LLM_EVENT_TYPES.has(type)) {
      records.push(ensureSessionRecord(normalizedEvent, state));
      records.push(ensureTurnRecord(normalizedEvent, state));
      records.push(llmRecord(normalizedEvent, state));
      continue;
    }

    if (TOOL_EVENT_TYPES.has(type)) {
      records.push(ensureSessionRecord(normalizedEvent, state));
      records.push(ensureTurnRecord(normalizedEvent, state));
      records.push(toolRecord(normalizedEvent, state));
      continue;
    }

    if (PERMISSION_EVENT_TYPES.has(type)) {
      records.push(ensureSessionRecord(normalizedEvent, state));
      records.push(ensureTurnRecord(normalizedEvent, state));
      records.push(permissionRecord(normalizedEvent, state));
    }
  }

  return uniqueBySpanId(records);
}

export function normalizeCodexNotifyPayload(payload, options = {}) {
  return codexRecordsFromEvents([payload], options);
}

export async function appendCodexSessionJsonl(eventLogPath, sessionJsonl, options = {}) {
  const events = parseCodexSessionJsonl(sessionJsonl);
  const records = codexRecordsFromEvents(events, options);
  const written = [];

  for (const record of records) {
    written.push(await appendEventLog(eventLogPath, record, options));
  }

  return written;
}

function sessionRecord(event, state) {
  state.sessionId = event.session_id ?? event.sessionId ?? state.sessionId;
  state.traceId = event.trace_id ?? event.traceId ?? state.traceId;
  state.sessionSpanId = `codex-session:${state.sessionId}`;
  state.currentModel = event.model ?? state.currentModel;

  return createSpanRecord({
    trace_id: state.traceId,
    span_id: state.sessionSpanId,
    span_kind: "agent.session",
    name: "Codex session",
    start_time_unix_ms: timestampMs(event.timestamp, event.start_time_unix_ms),
    end_time_unix_ms: timestampMs(event.end_timestamp, event.end_time_unix_ms, null),
    status: statusFromEvent(event),
    agent: compactObject({
      ...state.agent,
      model: event.model,
    }),
    project: compactObject({
      ...state.project,
      repo_path: event.cwd ?? event.repo_path ?? state.project.repo_path,
    }),
    attributes: compactObject({
      source: event.source ?? "codex.session_jsonl",
      event_type: event.type,
      envelope_type: event.envelope_type,
      session_id: state.sessionId,
    }),
  });
}

function ensureTurnRecord(event, state) {
  ensureSessionRecord(event, state);

  const turnId = turnIdFromEvent(event, state);
  state.currentTurnId = turnId;
  state.currentModel = event.model ?? state.currentModel;

  const spanId = `codex-turn:${state.sessionId}:${turnId}`;
  if (state.turnSpans.has(spanId)) {
    return state.turnSpans.get(spanId);
  }

  const record = createSpanRecord({
    trace_id: state.traceId,
    span_id: spanId,
    parent_span_id: state.sessionSpanId,
    span_kind: "turn",
    name: `Codex turn ${turnId}`,
    start_time_unix_ms: timestampMs(event.timestamp, event.start_time_unix_ms),
    end_time_unix_ms: timestampMs(event.end_timestamp, event.end_time_unix_ms, null),
    status: statusFromEvent(event),
    agent: state.agent,
    project: state.project,
    attributes: compactObject({
      source: event.source ?? "codex.session_jsonl",
      event_type: event.type,
      envelope_type: event.envelope_type,
      turn_id: turnId,
    }),
  });

  state.turnSpans.set(spanId, record);
  return record;
}

function llmRecord(event, state) {
  const turnId = turnIdFromEvent(event, state);
  const usage = event.usage ?? event.token_usage ?? tokenUsageFromInfo(event.info);
  const totalUsage = event.total_usage ?? event.total_token_usage ?? totalTokenUsageFromInfo(event.info);
  const requestId = event.request_id ?? `response:${nextSequence(state)}`;

  return createSpanRecord({
    trace_id: state.traceId,
    span_id: `codex-llm:${state.sessionId}:${turnId}:${requestId}`,
    parent_span_id: `codex-turn:${state.sessionId}:${turnId}`,
    span_kind: "llm.request",
    name: event.model ?? state.currentModel ? `Codex LLM ${event.model ?? state.currentModel}` : "Codex LLM request",
    start_time_unix_ms: timestampMs(event.timestamp, event.start_time_unix_ms),
    end_time_unix_ms: timestampMs(event.end_timestamp, event.end_time_unix_ms, null),
    status: statusFromEvent(event),
    agent: compactObject({
      ...state.agent,
      model: event.model,
    }),
    project: state.project,
    metrics: compactObject({
      input_tokens: metricNumber(usage.input_tokens ?? usage.input ?? usage.prompt_tokens),
      output_tokens: metricNumber(usage.output_tokens ?? usage.output ?? usage.completion_tokens),
      cached_input_tokens: metricNumber(usage.cached_input_tokens ?? usage.cached_input),
      reasoning_output_tokens: metricNumber(usage.reasoning_output_tokens ?? usage.reasoning_output),
      total_tokens: metricNumber(usage.total_tokens),
      total_input_tokens: metricNumber(totalUsage.input_tokens),
      total_output_tokens: metricNumber(totalUsage.output_tokens),
      total_cached_input_tokens: metricNumber(totalUsage.cached_input_tokens),
      total_reasoning_output_tokens: metricNumber(totalUsage.reasoning_output_tokens),
      total_accumulated_tokens: metricNumber(totalUsage.total_tokens),
      context_window_tokens: metricNumber(event.model_context_window ?? event.context_window),
      latency_ms: metricNumber(event.latency_ms ?? event.duration_ms),
    }),
    attributes: compactObject({
      source: event.source ?? "codex.session_jsonl",
      event_type: event.type,
      envelope_type: event.envelope_type,
      turn_id: turnId,
      request_id: requestId,
    }),
  });
}

function toolRecord(event, state) {
  const turnId = turnIdFromEvent(event, state);
  const callId = event.call_id ?? event.tool_call_id ?? event.id ?? `tool:${nextSequence(state)}`;
  const toolKey = `${turnId}:${callId}`;
  const incomingToolName = event.tool_name ?? event.name;
  if (incomingToolName) {
    state.toolNamesByCallId.set(toolKey, incomingToolName);
  }

  const toolName = incomingToolName ?? state.toolNamesByCallId.get(toolKey) ?? "tool";
  const phaseSuffix = event.phase ? `:${event.phase}` : "";

  return createSpanRecord({
    trace_id: state.traceId,
    span_id: `codex-tool:${state.sessionId}:${turnId}:${callId}${phaseSuffix}`,
    parent_span_id: `codex-turn:${state.sessionId}:${turnId}`,
    span_kind: "tool.execution",
    name: toolName,
    start_time_unix_ms: timestampMs(event.timestamp, event.start_time_unix_ms),
    end_time_unix_ms: timestampMs(event.end_timestamp, event.end_time_unix_ms, null),
    status: statusFromEvent(event),
    agent: state.agent,
    project: state.project,
    metrics: compactObject({
      duration_ms: metricNumber(event.duration_ms),
    }),
    attributes: compactObject({
      source: event.source ?? "codex.notify_or_session_jsonl",
      event_type: event.type,
      envelope_type: event.envelope_type,
      turn_id: turnId,
      call_id: callId,
      tool_name: toolName,
      phase: event.phase,
      exit_code: event.exit_code,
      sandbox: event.sandbox,
      approval: event.approval,
    }),
  });
}

function permissionRecord(event, state) {
  const turnId = turnIdFromEvent(event, state);
  const permissionId = event.permission_id ?? event.id ?? event.type ?? "permission";

  return createSpanRecord({
    trace_id: state.traceId,
    span_id: `codex-permission:${state.sessionId}:${turnId}:${permissionId}`,
    parent_span_id: `codex-turn:${state.sessionId}:${turnId}`,
    span_kind: "permission",
    name: event.type ?? "Codex permission event",
    start_time_unix_ms: timestampMs(event.timestamp, event.start_time_unix_ms),
    end_time_unix_ms: timestampMs(event.end_timestamp, event.end_time_unix_ms, null),
    status: statusFromEvent(event),
    agent: state.agent,
    project: state.project,
    attributes: compactObject({
      source: "codex.notify_or_session_jsonl",
      event_type: event.type,
      turn_id: turnId,
      permission_id: permissionId,
      decision: event.decision,
      command_kind: event.command_kind,
    }),
  });
}

function parseJsonLine(line, lineNumber) {
  try {
    return JSON.parse(line);
  } catch (error) {
    throw new Error(`Invalid Codex session JSONL at line ${lineNumber}: ${error.message}`);
  }
}

function inferSessionId(events) {
  for (const event of events) {
    const payload = objectValue(event.payload);
    const sessionId = event.session_id ?? event.sessionId ?? payload?.session_id ?? payload?.sessionId;
    if (sessionId) {
      return sessionId;
    }
    if (event.type === "session_meta" && payload?.id) {
      return payload.id;
    }
  }
  return "unknown-session";
}

function normalizeCodexEvent(event, state) {
  const payload = objectValue(event.payload);
  if (!payload) {
    return compactObject({
      ...event,
      source: event.source ?? "codex.notify_or_session_jsonl",
    });
  }

  const envelopeType = event.type ?? event.event ?? event.kind;
  const payloadType = payload.type ?? payload.kind ?? envelopeType;
  const base = compactObject({
    session_id: payload.session_id ?? event.session_id ?? state.sessionId,
    trace_id: payload.trace_id ?? event.trace_id ?? state.traceId,
    envelope_type: envelopeType,
    source: "codex.session_jsonl",
    timestamp: event.timestamp ?? payload.timestamp ?? payload.started_at ?? payload.completed_at,
  });

  if (envelopeType === "session_meta") {
    return compactObject({
      ...base,
      type: "session.started",
      session_id: payload.session_id ?? payload.id ?? base.session_id,
      model: payload.model ?? payload.model_slug,
      cwd: payload.cwd,
    });
  }

  if (envelopeType === "turn_context") {
    return compactObject({
      ...base,
      type: "turn.started",
      turn_id: payload.turn_id ?? state.currentTurnId,
      model: payload.model,
      cwd: payload.cwd,
      sandbox: payload.sandbox_policy,
    });
  }

  if (envelopeType === "event_msg" && payloadType === "task_started") {
    return compactObject({
      ...base,
      type: "turn.started",
      turn_id: payload.turn_id ?? state.currentTurnId,
      timestamp: payload.started_at ?? base.timestamp,
      model_context_window: payload.model_context_window,
    });
  }

  if (envelopeType === "event_msg" && payloadType === "token_count") {
    return compactObject({
      ...base,
      type: "llm.completed",
      turn_id: payload.turn_id ?? state.currentTurnId,
      request_id: `token_count:${nextSequence(state)}`,
      usage: tokenUsageFromInfo(payload.info),
      total_usage: totalTokenUsageFromInfo(payload.info),
      model_context_window: payload.info?.model_context_window,
      rate_limit_names: rateLimitNames(payload.rate_limits),
    });
  }

  if (envelopeType === "response_item" && TOOL_CALL_PAYLOAD_TYPES.has(payloadType)) {
    return compactObject({
      ...base,
      type: "tool.call",
      turn_id: payload.turn_id ?? state.currentTurnId,
      call_id: payload.call_id ?? payload.id,
      tool_name: payload.name ?? payload.tool_name ?? payloadType,
      status: payload.status,
      phase: "call",
    });
  }

  if (envelopeType === "response_item" && TOOL_OUTPUT_PAYLOAD_TYPES.has(payloadType)) {
    return compactObject({
      ...base,
      type: "tool.output",
      turn_id: payload.turn_id ?? state.currentTurnId,
      call_id: payload.call_id ?? payload.id,
      tool_name: payload.name ?? payload.tool_name,
      status: payload.status ?? "ok",
      duration_ms: payload.duration_ms,
      exit_code: payload.exit_code,
      phase: "output",
    });
  }

  if (payloadType && PERMISSION_EVENT_TYPES.has(payloadType)) {
    return compactObject({
      ...base,
      type: payloadType,
      turn_id: payload.turn_id ?? state.currentTurnId,
      permission_id: payload.permission_id ?? payload.id,
      decision: payload.decision,
      command_kind: payload.command_kind,
    });
  }

  return null;
}

function turnIdFromEvent(event, state) {
  return event.turn_id ?? event.turnId ?? state.currentTurnId ?? "unknown-turn";
}

function tokenUsageFromInfo(info) {
  const source = objectValue(info?.last_token_usage) ?? objectValue(info) ?? {};
  return compactObject({
    input_tokens: metricNumber(source.input_tokens ?? source.input ?? source.prompt_tokens),
    output_tokens: metricNumber(source.output_tokens ?? source.output ?? source.completion_tokens),
    cached_input_tokens: metricNumber(source.cached_input_tokens ?? source.cached_input),
    reasoning_output_tokens: metricNumber(source.reasoning_output_tokens ?? source.reasoning_output),
    total_tokens: metricNumber(source.total_tokens),
  });
}

function totalTokenUsageFromInfo(info) {
  const source = objectValue(info?.total_token_usage) ?? {};
  return compactObject({
    input_tokens: metricNumber(source.input_tokens),
    output_tokens: metricNumber(source.output_tokens),
    cached_input_tokens: metricNumber(source.cached_input_tokens),
    reasoning_output_tokens: metricNumber(source.reasoning_output_tokens),
    total_tokens: metricNumber(source.total_tokens),
  });
}

function rateLimitNames(rateLimits) {
  const rateLimit = objectValue(rateLimits);
  if (!rateLimit) {
    return undefined;
  }
  return compactObject({
    limit_id: rateLimit.limit_id,
    limit_name: rateLimit.limit_name,
    plan_type: rateLimit.plan_type,
  });
}

function nextSequence(state) {
  state.sequence += 1;
  return state.sequence;
}

function objectValue(value) {
  if (value && typeof value === "object" && !Array.isArray(value)) {
    return value;
  }
  return undefined;
}

function timestampMs(value, fallback, defaultValue = Date.now()) {
  const candidate = value ?? fallback;
  if (candidate === null) {
    return defaultValue;
  }
  if (typeof candidate === "number" && Number.isFinite(candidate)) {
    return candidate;
  }
  if (typeof candidate === "string") {
    const parsed = Date.parse(candidate);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }
  return defaultValue;
}

function statusFromEvent(event) {
  if (event.status === "error" || event.error) {
    return { code: "error" };
  }
  if (
    event.status === "ok" ||
    event.status === "completed" ||
    event.status === "success" ||
    event.type?.endsWith(".completed")
  ) {
    return { code: "ok" };
  }
  return { code: "unset" };
}

function ensureSessionRecord(event, state) {
  if (state.sessionSpanId) {
    return createSpanRecord({
      trace_id: state.traceId,
      span_id: state.sessionSpanId,
      span_kind: "agent.session",
      name: "Codex session",
      status: "unset",
      agent: state.agent,
      project: state.project,
      attributes: {
        source: "codex.synthetic_session",
      },
    });
  }

  return sessionRecord(
    {
      type: "session.started",
      session_id: event.session_id ?? event.sessionId ?? state.sessionId,
      trace_id: event.trace_id ?? event.traceId ?? state.traceId,
      model: event.model ?? state.currentModel,
      cwd: event.cwd ?? event.repo_path,
      timestamp: event.timestamp,
      start_time_unix_ms: event.start_time_unix_ms,
      source: event.source,
      envelope_type: event.envelope_type,
    },
    state,
  );
}

function compactObject(object) {
  return Object.fromEntries(
    Object.entries(object).filter(([, value]) => value !== undefined && value !== null),
  );
}

function metricNumber(value) {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string" && value.trim() !== "") {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }
  return undefined;
}

function uniqueBySpanId(records) {
  const seen = new Set();
  const unique = [];
  for (const record of records) {
    if (!seen.has(record.span_id)) {
      seen.add(record.span_id);
      unique.push(record);
    }
  }
  return unique;
}
