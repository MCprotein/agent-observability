import { appendEventLog } from "../event-log.js";
import { createSpanRecord } from "../schema.js";

const SESSION_EVENT_TYPES = new Set(["session.started", "session_start", "session.created"]);
const TURN_EVENT_TYPES = new Set([
  "turn.started",
  "turn_start",
  "turn",
  "user_prompt_submit",
  "user.prompt.submitted",
]);
const LLM_EVENT_TYPES = new Set([
  "llm.completed",
  "llm_complete",
  "assistant.completed",
  "assistant_message",
  "assistant.response",
]);
const TOOL_EVENT_TYPES = new Set([
  "tool.call",
  "tool.output",
  "tool.completed",
  "tool_complete",
  "tool_use",
  "tool_result",
]);
const PERMISSION_EVENT_TYPES = new Set([
  "permission.requested",
  "permission.denied",
  "permission.approved",
  "permission_request",
]);
const COMPACTION_EVENT_TYPES = new Set([
  "compaction",
  "compaction.completed",
  "compact",
  "context.compacted",
  "pre_compact",
]);

export function parseClaudeCodeJsonl(text) {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line, index) => parseJsonLine(line, index + 1));
}

export function claudeCodeRecordsFromEvents(events, options = {}) {
  const sessionId = options.session_id ?? inferSessionId(events);
  const state = {
    sessionId,
    traceId: options.trace_id ?? `claude-code:${sessionId}`,
    project: compactObject({
      name: options.project_name,
      repo_path: options.repo_path,
    }),
    agent: compactObject({
      name: "claude-code",
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
    const normalizedEvents = normalizeClaudeCodeEvent(event, state);

    for (const normalizedEvent of normalizedEvents) {
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
        continue;
      }

      if (COMPACTION_EVENT_TYPES.has(type)) {
        records.push(ensureSessionRecord(normalizedEvent, state));
        records.push(ensureTurnRecord(normalizedEvent, state));
        records.push(compactionRecord(normalizedEvent, state));
      }
    }
  }

  return uniqueBySpanId(records);
}

export function normalizeClaudeCodeHookPayload(payload, options = {}) {
  return claudeCodeRecordsFromEvents([payload], options);
}

export async function appendClaudeCodeJsonl(eventLogPath, jsonl, options = {}) {
  const events = parseClaudeCodeJsonl(jsonl);
  const records = claudeCodeRecordsFromEvents(events, options);
  const written = [];

  for (const record of records) {
    written.push(await appendEventLog(eventLogPath, record, options));
  }

  return written;
}

function sessionRecord(event, state) {
  state.sessionId = event.session_id ?? event.sessionId ?? state.sessionId;
  state.traceId = event.trace_id ?? event.traceId ?? state.traceId;
  state.sessionSpanId = `claude-code-session:${state.sessionId}`;
  state.currentModel = event.model ?? state.currentModel;

  return createSpanRecord({
    trace_id: state.traceId,
    span_id: state.sessionSpanId,
    span_kind: "agent.session",
    name: "Claude Code session",
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
      source: event.source ?? "claude_code.hook_or_transcript",
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

  const spanId = `claude-code-turn:${state.sessionId}:${turnId}`;
  if (state.turnSpans.has(spanId)) {
    return state.turnSpans.get(spanId);
  }

  const record = createSpanRecord({
    trace_id: state.traceId,
    span_id: spanId,
    parent_span_id: state.sessionSpanId,
    span_kind: "turn",
    name: `Claude Code turn ${turnId}`,
    start_time_unix_ms: timestampMs(event.timestamp, event.start_time_unix_ms),
    end_time_unix_ms: timestampMs(event.end_timestamp, event.end_time_unix_ms, null),
    status: statusFromEvent(event),
    agent: state.agent,
    project: state.project,
    attributes: compactObject({
      source: event.source ?? "claude_code.hook_or_transcript",
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
  const usage = event.usage ?? event.token_usage ?? {};
  const requestId = event.request_id ?? event.message_id ?? `response:${nextSequence(state)}`;

  return createSpanRecord({
    trace_id: state.traceId,
    span_id: `claude-code-llm:${state.sessionId}:${turnId}:${requestId}`,
    parent_span_id: `claude-code-turn:${state.sessionId}:${turnId}`,
    span_kind: "llm.request",
    name: event.model ?? state.currentModel ? `Claude Code LLM ${event.model ?? state.currentModel}` : "Claude Code LLM request",
    start_time_unix_ms: timestampMs(event.timestamp, event.start_time_unix_ms),
    end_time_unix_ms: timestampMs(event.end_timestamp, event.end_time_unix_ms, null),
    status: statusFromEvent(event),
    agent: compactObject({
      ...state.agent,
      model: event.model ?? state.currentModel,
    }),
    project: state.project,
    metrics: compactObject({
      input_tokens: metricNumber(usage.input_tokens ?? usage.input ?? usage.prompt_tokens),
      output_tokens: metricNumber(usage.output_tokens ?? usage.output ?? usage.completion_tokens),
      cached_input_tokens: metricNumber(
        usage.cached_input_tokens ?? usage.cached_input ?? usage.cache_read_input_tokens,
      ),
      cache_creation_input_tokens: metricNumber(usage.cache_creation_input_tokens),
      reasoning_output_tokens: metricNumber(
        usage.reasoning_output_tokens ?? usage.reasoning_output ?? usage.thinking_tokens,
      ),
      total_tokens: metricNumber(usage.total_tokens),
      latency_ms: metricNumber(event.latency_ms ?? event.duration_ms ?? event.elapsed_ms),
    }),
    attributes: compactObject({
      source: event.source ?? "claude_code.hook_or_transcript",
      event_type: event.type,
      envelope_type: event.envelope_type,
      turn_id: turnId,
      request_id: requestId,
    }),
  });
}

function toolRecord(event, state) {
  const turnId = turnIdFromEvent(event, state);
  const callId = event.call_id ?? event.tool_call_id ?? event.tool_use_id ?? event.id ?? `tool:${nextSequence(state)}`;
  const toolKey = `${turnId}:${callId}`;
  const incomingToolName = event.tool_name ?? event.name;
  if (incomingToolName) {
    state.toolNamesByCallId.set(toolKey, incomingToolName);
  }

  const toolName = incomingToolName ?? state.toolNamesByCallId.get(toolKey) ?? "tool";
  const phaseSuffix = event.phase ? `:${event.phase}` : "";

  return createSpanRecord({
    trace_id: state.traceId,
    span_id: `claude-code-tool:${state.sessionId}:${turnId}:${callId}${phaseSuffix}`,
    parent_span_id: `claude-code-turn:${state.sessionId}:${turnId}`,
    span_kind: "tool.execution",
    name: toolName,
    start_time_unix_ms: timestampMs(event.timestamp, event.start_time_unix_ms),
    end_time_unix_ms: timestampMs(event.end_timestamp, event.end_time_unix_ms, null),
    status: statusFromEvent(event),
    agent: state.agent,
    project: state.project,
    metrics: compactObject({
      duration_ms: metricNumber(event.duration_ms ?? event.elapsed_ms),
    }),
    attributes: compactObject({
      source: event.source ?? "claude_code.hook_or_transcript",
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
  const permissionId = event.permission_id ?? event.id ?? event.type ?? `permission:${nextSequence(state)}`;

  return createSpanRecord({
    trace_id: state.traceId,
    span_id: `claude-code-permission:${state.sessionId}:${turnId}:${permissionId}`,
    parent_span_id: `claude-code-turn:${state.sessionId}:${turnId}`,
    span_kind: "permission",
    name: "Claude Code permission event",
    start_time_unix_ms: timestampMs(event.timestamp, event.start_time_unix_ms),
    end_time_unix_ms: timestampMs(event.end_timestamp, event.end_time_unix_ms, null),
    status: statusFromEvent(event),
    agent: state.agent,
    project: state.project,
    attributes: compactObject({
      source: event.source ?? "claude_code.hook",
      event_type: event.type,
      turn_id: turnId,
      permission_id: permissionId,
      decision: event.decision,
      command_kind: event.command_kind,
      tool_name: event.tool_name,
    }),
  });
}

function compactionRecord(event, state) {
  const turnId = turnIdFromEvent(event, state);
  const compactionId = event.compaction_id ?? event.id ?? `compaction:${nextSequence(state)}`;

  return createSpanRecord({
    trace_id: state.traceId,
    span_id: `claude-code-compaction:${state.sessionId}:${turnId}:${compactionId}`,
    parent_span_id: `claude-code-turn:${state.sessionId}:${turnId}`,
    span_kind: "compaction",
    name: "Claude Code compaction",
    start_time_unix_ms: timestampMs(event.timestamp, event.start_time_unix_ms),
    end_time_unix_ms: timestampMs(event.end_timestamp, event.end_time_unix_ms, null),
    status: statusFromEvent(event),
    agent: state.agent,
    project: state.project,
    metrics: compactObject({
      input_tokens_before: metricNumber(event.input_tokens_before ?? event.before_tokens),
      input_tokens_after: metricNumber(event.input_tokens_after ?? event.after_tokens),
      context_window_tokens: metricNumber(event.context_window_tokens ?? event.model_context_window),
    }),
    attributes: compactObject({
      source: event.source ?? "claude_code.hook_or_transcript",
      event_type: event.type,
      turn_id: turnId,
      compaction_id: compactionId,
      trigger: event.trigger,
    }),
  });
}

function normalizeClaudeCodeEvent(event, state) {
  const payload = objectValue(event.payload);
  const view = payload ?? event;
  const eventName = event.hook_event_name ?? event.hookEventName ?? view.hook_event_name ?? view.hookEventName;
  const envelopeType = eventName ?? event.type ?? event.event ?? event.kind ?? view.type ?? view.event ?? view.kind;
  const normalizedType = normalizeEventType(envelopeType);
  const base = baseEvent(event, view, normalizedType, envelopeType, state);

  if (normalizedType === "assistant.transcript") {
    return assistantTranscriptEvents(event, view, base, state);
  }

  if (normalizedType === "user.transcript") {
    return userTranscriptEvents(event, view, base, state);
  }

  if (SESSION_EVENT_TYPES.has(normalizedType)) {
    return [
      compactObject({
        ...base,
        type: "session.started",
        model: view.model ?? view.model_slug,
        cwd: view.cwd ?? view.workspace_dir,
      }),
    ];
  }

  if (TURN_EVENT_TYPES.has(normalizedType)) {
    return [
      compactObject({
        ...base,
        type: "turn.started",
        turn_id: turnIdCandidate(event, view, state),
        model: view.model,
      }),
    ];
  }

  if (LLM_EVENT_TYPES.has(normalizedType)) {
    const message = objectValue(view.message);
    return [
      compactObject({
        ...base,
        type: "llm.completed",
        turn_id: turnIdCandidate(event, view, state),
        request_id: view.request_id ?? view.requestId ?? view.message_id ?? message?.id,
        model: view.model ?? message?.model,
        usage: usageFromSource(view.usage ?? view.token_usage ?? message?.usage),
        duration_ms: view.duration_ms ?? view.elapsed_ms,
      }),
    ];
  }

  if (TOOL_EVENT_TYPES.has(normalizedType)) {
    return [
      compactObject({
        ...base,
        type: normalizeToolType(normalizedType),
        turn_id: turnIdCandidate(event, view, state),
        call_id: toolCallId(event, view, state),
        tool_name: view.tool_name ?? view.name,
        phase: toolPhase(normalizedType, view),
        duration_ms: view.duration_ms ?? view.elapsed_ms,
        exit_code: exitCodeFromToolResponse(view),
        status: toolStatus(view),
        sandbox: view.sandbox,
        approval: view.approval,
      }),
    ];
  }

  if (PERMISSION_EVENT_TYPES.has(normalizedType)) {
    return [
      compactObject({
        ...base,
        type: permissionType(view, normalizedType),
        turn_id: turnIdCandidate(event, view, state),
        permission_id: view.permission_id ?? view.permissionId ?? view.id,
        decision: permissionDecision(view, normalizedType),
        command_kind: view.command_kind ?? view.commandKind,
        tool_name: view.tool_name ?? view.name,
      }),
    ];
  }

  if (COMPACTION_EVENT_TYPES.has(normalizedType)) {
    return [
      compactObject({
        ...base,
        type: "compaction",
        turn_id: turnIdCandidate(event, view, state),
        compaction_id: view.compaction_id ?? view.compactionId ?? view.id,
        before_tokens: view.before_tokens ?? view.input_tokens_before,
        after_tokens: view.after_tokens ?? view.input_tokens_after,
        context_window_tokens: view.context_window_tokens ?? view.model_context_window,
        trigger: view.trigger,
      }),
    ];
  }

  return [];
}

function assistantTranscriptEvents(event, view, base, state) {
  const message = objectValue(view.message) ?? {};
  const turnId = childTranscriptTurnId(event, view, state);
  const events = [
    compactObject({
      ...base,
      type: "llm.completed",
      turn_id: turnId,
      request_id: view.request_id ?? view.uuid ?? message.id,
      model: view.model ?? message.model,
      usage: usageFromSource(view.usage ?? message.usage),
      duration_ms: view.duration_ms ?? view.elapsed_ms,
    }),
  ];

  for (const block of contentBlocks(message.content ?? view.content)) {
    if (block.type !== "tool_use") {
      continue;
    }
    events.push(
      compactObject({
        ...base,
        type: "tool.call",
        turn_id: turnId,
        call_id: block.id,
        tool_name: block.name,
        status: "ok",
        phase: "call",
      }),
    );
  }

  return events;
}

function userTranscriptEvents(event, view, base, state) {
  const message = objectValue(view.message) ?? {};
  const turnId = newTranscriptTurnId(event, view, state);
  const toolResults = contentBlocks(message.content ?? view.content).filter(
    (block) => block.type === "tool_result",
  );

  if (toolResults.length > 0) {
    const childTurnId = childTranscriptTurnId(event, view, state);
    return toolResults.map((block) =>
      compactObject({
        ...base,
        type: "tool.output",
        turn_id: childTurnId,
        call_id: block.tool_use_id ?? block.id,
        status: block.is_error ? "error" : "ok",
        phase: "output",
      }),
    );
  }

  return [
    compactObject({
      ...base,
      type: "turn.started",
      turn_id: turnId,
    }),
  ];
}

function baseEvent(event, view, normalizedType, envelopeType, state) {
  return compactObject({
    session_id: sessionIdFromEvent(event, view, state),
    trace_id: event.trace_id ?? event.traceId ?? view.trace_id ?? view.traceId ?? state.traceId,
    envelope_type: envelopeType,
    source: sourceFromEvent(event, view, normalizedType),
    timestamp:
      event.timestamp ??
      event.created_at ??
      event.started_at ??
      event.completed_at ??
      view.timestamp ??
      view.created_at ??
      view.started_at ??
      view.completed_at,
  });
}

function normalizeEventType(type) {
  if (!type || typeof type !== "string") {
    return undefined;
  }

  const key = type.replace(/[A-Z]/g, (match, index) => `${index === 0 ? "" : "_"}${match.toLowerCase()}`);

  switch (key) {
    case "session_start":
      return "session.started";
    case "user_prompt_submit":
      return "user_prompt_submit";
    case "pre_tool_use":
      return "tool.call";
    case "post_tool_use":
      return "tool.completed";
    case "permission_request":
      return "permission.requested";
    case "permission_denied":
      return "permission.denied";
    case "permission_approved":
      return "permission.approved";
    case "pre_compact":
      return "pre_compact";
    case "stop":
    case "subagent_stop":
      return "stop";
    case "assistant":
      return "assistant.transcript";
    case "user":
      return "user.transcript";
    default:
      return type.toLowerCase();
  }
}

function sourceFromEvent(event, view, normalizedType) {
  if (event.hook_event_name || event.hookEventName || view.hook_event_name || view.hookEventName) {
    return "claude_code.hook";
  }
  if (normalizedType === "assistant.transcript" || normalizedType === "user.transcript") {
    return "claude_code.transcript";
  }
  return "claude_code.hook_or_transcript";
}

function normalizeToolType(type) {
  if (type === "tool.output" || type === "tool_result") {
    return "tool.output";
  }
  if (type === "tool.completed" || type === "tool_complete") {
    return "tool.completed";
  }
  return "tool.call";
}

function toolPhase(type, view) {
  if (view.phase) {
    return view.phase;
  }
  if (type === "tool.output" || type === "tool_result") {
    return "output";
  }
  if (type === "tool.completed" || type === "tool_complete") {
    return "finish";
  }
  return "call";
}

function permissionType(view, type) {
  const decision = permissionDecision(view, type);
  if (decision === "denied") {
    return "permission.denied";
  }
  if (decision === "approved" || decision === "allowed") {
    return "permission.approved";
  }
  return "permission.requested";
}

function permissionDecision(view, type) {
  if (view.decision) {
    return view.decision;
  }
  if (view.allowed === true || view.approved === true) {
    return "approved";
  }
  if (view.denied === true || type === "permission.denied") {
    return "denied";
  }
  return "requested";
}

function toolStatus(view) {
  const response = objectValue(view.tool_response ?? view.toolResponse ?? view.response);
  if (view.status) {
    return view.status;
  }
  if (view.is_error === true || response?.is_error === true || response?.error) {
    return "error";
  }
  const exitCode = exitCodeFromToolResponse(view);
  if (exitCode !== undefined && exitCode !== 0) {
    return "error";
  }
  return "ok";
}

function exitCodeFromToolResponse(view) {
  const response = objectValue(view.tool_response ?? view.toolResponse ?? view.response);
  return metricNumber(view.exit_code ?? view.exitCode ?? response?.exit_code ?? response?.exitCode);
}

function toolCallId(event, view, state) {
  return (
    view.tool_use_id ??
    view.toolUseId ??
    view.tool_call_id ??
    view.toolCallId ??
    view.call_id ??
    view.callId ??
    view.id ??
    event.uuid ??
    `${view.tool_name ?? view.name ?? "tool"}:${nextSequence(state)}`
  );
}

function turnIdCandidate(event, view, state) {
  return (
    view.turn_id ??
    view.turnId ??
    view.prompt_id ??
    view.promptId ??
    view.generation_id ??
    view.generationId ??
    state.currentTurnId ??
    event.parent_uuid ??
    event.parentUuid ??
    event.uuid
  );
}

function newTranscriptTurnId(event, view, state) {
  return (
    view.turn_id ??
    view.turnId ??
    view.prompt_id ??
    view.promptId ??
    view.generation_id ??
    view.generationId ??
    event.uuid ??
    event.parent_uuid ??
    event.parentUuid ??
    state.currentTurnId
  );
}

function childTranscriptTurnId(event, view, state) {
  return (
    view.turn_id ??
    view.turnId ??
    view.prompt_id ??
    view.promptId ??
    view.generation_id ??
    view.generationId ??
    event.parent_uuid ??
    event.parentUuid ??
    state.currentTurnId ??
    event.uuid
  );
}

function sessionIdFromEvent(event, view, state) {
  return (
    view.session_id ??
    view.sessionId ??
    event.session_id ??
    event.sessionId ??
    objectValue(view.session)?.id ??
    objectValue(event.session)?.id ??
    state.sessionId
  );
}

function contentBlocks(value) {
  if (Array.isArray(value)) {
    return value.filter((item) => item && typeof item === "object" && !Array.isArray(item));
  }
  return [];
}

function usageFromSource(source) {
  const usage = objectValue(source);
  if (!usage) {
    return {};
  }
  return compactObject({
    input_tokens: metricNumber(usage.input_tokens ?? usage.input ?? usage.prompt_tokens),
    output_tokens: metricNumber(usage.output_tokens ?? usage.output ?? usage.completion_tokens),
    cached_input_tokens: metricNumber(
      usage.cached_input_tokens ?? usage.cached_input ?? usage.cache_read_input_tokens,
    ),
    cache_creation_input_tokens: metricNumber(usage.cache_creation_input_tokens),
    reasoning_output_tokens: metricNumber(
      usage.reasoning_output_tokens ?? usage.reasoning_output ?? usage.thinking_tokens,
    ),
    total_tokens: metricNumber(usage.total_tokens),
  });
}

function parseJsonLine(line, lineNumber) {
  try {
    return JSON.parse(line);
  } catch (error) {
    throw new Error(`Invalid Claude Code JSONL at line ${lineNumber}: ${error.message}`);
  }
}

function inferSessionId(events) {
  for (const event of events) {
    const payload = objectValue(event.payload);
    const view = payload ?? event;
    const sessionId =
      event.session_id ??
      event.sessionId ??
      view.session_id ??
      view.sessionId ??
      objectValue(event.session)?.id ??
      objectValue(view.session)?.id;
    if (sessionId) {
      return sessionId;
    }
  }
  return "unknown-session";
}

function turnIdFromEvent(event, state) {
  return event.turn_id ?? event.turnId ?? state.currentTurnId ?? "unknown-turn";
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
  if (
    event.status === "error" ||
    event.error ||
    event.is_error === true ||
    (event.exit_code !== undefined && event.exit_code !== 0) ||
    event.decision === "denied" ||
    event.type === "permission.denied"
  ) {
    return { code: "error" };
  }
  if (
    event.status === "ok" ||
    event.status === "completed" ||
    event.status === "success" ||
    event.type?.endsWith(".completed") ||
    event.type === "tool.output" ||
    event.type === "permission.approved" ||
    event.type === "compaction"
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
      name: "Claude Code session",
      status: "unset",
      agent: state.agent,
      project: state.project,
      attributes: {
        source: "claude_code.synthetic_session",
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

function objectValue(value) {
  if (value && typeof value === "object" && !Array.isArray(value)) {
    return value;
  }
  return undefined;
}

function nextSequence(state) {
  state.sequence += 1;
  return state.sequence;
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
