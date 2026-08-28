export const SCHEMA_VERSION = "agent_observability.v1";

export const RECORD_TYPES = Object.freeze(["span"]);

export const SPAN_KINDS = Object.freeze([
  "workstream",
  "agent.session",
  "turn",
  "llm.request",
  "tool.execution",
  "permission",
  "compaction",
]);

export const STATUS_CODES = Object.freeze(["unset", "ok", "error"]);

const RECORD_KEYS = new Set([
  "schema_version",
  "record_type",
  "trace_id",
  "span_id",
  "parent_span_id",
  "span_kind",
  "name",
  "start_time_unix_ms",
  "end_time_unix_ms",
  "status",
  "agent",
  "project",
  "attributes",
  "metrics",
  "content",
  "redaction",
]);
const INPUT_KEYS = new Set([...RECORD_KEYS].filter((key) => !["schema_version", "record_type"].includes(key)));
const STATUS_KEYS = new Set(["code"]);
const AGENT_KEYS = new Set(["name", "version", "model"]);
const PROJECT_KEYS = new Set(["name", "repo_path"]);
const ATTRIBUTE_KEYS = new Set([
  "source",
  "event_type",
  "envelope_type",
  "session_id",
  "turn_id",
  "request_id",
  "call_id",
  "tool_name",
  "phase",
  "exit_code",
  "sandbox",
  "approval",
  "permission_id",
  "decision",
  "command_kind",
  "compaction_id",
  "trigger",
]);
const METRIC_KEYS = new Set([
  "input_tokens",
  "output_tokens",
  "cached_input_tokens",
  "cache_creation_input_tokens",
  "reasoning_output_tokens",
  "total_tokens",
  "total_input_tokens",
  "total_output_tokens",
  "total_cached_input_tokens",
  "total_reasoning_output_tokens",
  "total_accumulated_tokens",
  "context_window_tokens",
  "input_tokens_before",
  "input_tokens_after",
  "latency_ms",
  "duration_ms",
]);
const CONTENT_KEYS = new Set(["prompt", "output", "tool_input", "tool_output"]);
const REDACTION_KEYS = new Set(["applied", "count", "fields"]);

export function createSpanRecord(input) {
  assertKnownKeys(input, INPUT_KEYS, "input");
  const now = Date.now();
  const record = {
    schema_version: SCHEMA_VERSION,
    record_type: "span",
    trace_id: input.trace_id,
    span_id: input.span_id,
    parent_span_id: input.parent_span_id ?? null,
    span_kind: input.span_kind,
    name: input.name,
    start_time_unix_ms: input.start_time_unix_ms ?? now,
    end_time_unix_ms: input.end_time_unix_ms ?? null,
    status: normalizeStatus(input.status),
    agent: input.agent ?? {},
    project: input.project ?? {},
    attributes: input.attributes ?? {},
    metrics: input.metrics ?? {},
    content: input.content ?? {},
    redaction: input.redaction ?? { applied: false, count: 0, fields: [] },
  };

  assertValidSpanRecord(record);
  return record;
}

export function assertValidSpanRecord(record) {
  const errors = validateSpanRecord(record);
  if (errors.length > 0) {
    throw new Error(`Invalid span record: ${errors.join("; ")}`);
  }
}

export function validateSpanRecord(record) {
  const errors = [];

  if (!record || typeof record !== "object" || Array.isArray(record)) {
    return ["record must be an object"];
  }

  rejectUnknownKeys(record, RECORD_KEYS, "record", errors);

  requireString(record, "schema_version", errors);
  requireEnum(record, "schema_version", [SCHEMA_VERSION], errors);
  requireEnum(record, "record_type", RECORD_TYPES, errors);
  requireString(record, "trace_id", errors);
  requireString(record, "span_id", errors);
  requireNullableString(record, "parent_span_id", errors);
  requireEnum(record, "span_kind", SPAN_KINDS, errors);
  requireString(record, "name", errors);
  requireNumber(record, "start_time_unix_ms", errors);
  requireNullableNumber(record, "end_time_unix_ms", errors);

  if (record.end_time_unix_ms !== null && record.end_time_unix_ms < record.start_time_unix_ms) {
    errors.push("end_time_unix_ms must be >= start_time_unix_ms");
  }

  if (!record.status || typeof record.status !== "object" || Array.isArray(record.status)) {
    errors.push("status must be an object");
  } else {
    rejectUnknownKeys(record.status, STATUS_KEYS, "status", errors);
    requireEnum(record.status, "code", STATUS_CODES, errors, "status.code");
  }

  for (const key of ["agent", "project", "attributes", "metrics", "content", "redaction"]) {
    if (record[key] === null || typeof record[key] !== "object" || Array.isArray(record[key])) {
      errors.push(`${key} must be an object`);
    } else {
      validateJsonValue(record[key], key, errors);
    }
  }

  validateKnownObject(record.agent, AGENT_KEYS, "agent", errors);
  validateKnownObject(record.project, PROJECT_KEYS, "project", errors);
  validateKnownObject(record.attributes, ATTRIBUTE_KEYS, "attributes", errors);
  validateKnownObject(record.metrics, METRIC_KEYS, "metrics", errors);
  validateKnownObject(record.content, CONTENT_KEYS, "content", errors);
  validateKnownObject(record.redaction, REDACTION_KEYS, "redaction", errors);

  for (const [key, value] of Object.entries(record.agent ?? {})) {
    requireOptionalStringValue(value, `agent.${key}`, errors);
  }
  for (const [key, value] of Object.entries(record.project ?? {})) {
    requireOptionalStringValue(value, `project.${key}`, errors);
  }
  for (const [key, value] of Object.entries(record.attributes ?? {})) {
    if (!["string", "number", "boolean"].includes(typeof value) || (typeof value === "number" && !Number.isFinite(value))) {
      errors.push(`attributes.${key} must be a finite scalar value`);
    }
  }
  for (const [key, value] of Object.entries(record.metrics ?? {})) {
    if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
      errors.push(`metrics.${key} must be a non-negative finite number`);
    }
  }

  if (record.redaction && typeof record.redaction === "object") {
    if (typeof record.redaction.applied !== "boolean") {
      errors.push("redaction.applied must be a boolean");
    }
    if (!Number.isInteger(record.redaction.count) || record.redaction.count < 0) {
      errors.push("redaction.count must be a non-negative integer");
    }
    if (!Array.isArray(record.redaction.fields)) {
      errors.push("redaction.fields must be an array");
    } else if (record.redaction.fields.some((field) => typeof field !== "string")) {
      errors.push("redaction.fields must contain only strings");
    }
  }

  return errors;
}

export function migrateLegacyV1Record(record) {
  if (record?.schema_version !== SCHEMA_VERSION || record?.record_type !== "span") {
    return record;
  }

  for (const key of ["status", "agent", "project", "attributes", "metrics", "content", "redaction"]) {
    assertLegacyObject(record[key], key);
  }
  const migrated = pickKnown(record, RECORD_KEYS);
  migrated.status = { code: record.status?.code ?? "unset" };
  migrated.agent = pickKnown(record.agent, AGENT_KEYS);
  migrated.project = pickKnown(record.project, PROJECT_KEYS);
  migrated.attributes = pickKnown(record.attributes, ATTRIBUTE_KEYS);
  migrated.metrics = pickKnown(record.metrics, METRIC_KEYS);
  migrated.content = pickKnown(record.content, CONTENT_KEYS);
  migrated.redaction = pickKnown(record.redaction, REDACTION_KEYS);
  return migrated;
}

function assertLegacyObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`Invalid legacy v1 record: ${label} must be an object`);
  }
  const prototype = Object.getPrototypeOf(value);
  if (prototype !== Object.prototype && prototype !== null) {
    throw new Error(`Invalid legacy v1 record: ${label} must be a plain object`);
  }
}

function pickKnown(value, allowed) {
  return Object.fromEntries(Object.entries(value).filter(([key]) => allowed.has(key)));
}

function assertKnownKeys(object, allowed, label) {
  const errors = [];
  if (!object || typeof object !== "object" || Array.isArray(object)) {
    throw new Error(`${label} must be an object`);
  }
  rejectUnknownKeys(object, allowed, label, errors);
  if (errors.length > 0) {
    throw new Error(`Invalid span record: ${errors.join("; ")}`);
  }
}

function validateKnownObject(object, allowed, label, errors) {
  if (object && typeof object === "object" && !Array.isArray(object)) {
    rejectUnknownKeys(object, allowed, label, errors);
  }
}

function rejectUnknownKeys(object, allowed, label, errors) {
  for (const key of Object.keys(object)) {
    if (!allowed.has(key)) {
      errors.push(`${label}.${key} is not allowed`);
    }
  }
}

function requireOptionalStringValue(value, label, errors) {
  if (typeof value !== "string" || value.length === 0) {
    errors.push(`${label} must be a non-empty string`);
  }
}

function normalizeStatus(status) {
  if (!status) {
    return { code: "unset" };
  }

  if (typeof status === "string") {
    return { code: status };
  }

  return {
    code: status.code ?? "unset",
  };
}

function requireString(object, key, errors, label = key) {
  if (typeof object[key] !== "string" || object[key].length === 0) {
    errors.push(`${label} must be a non-empty string`);
  }
}

function requireNullableString(object, key, errors, label = key) {
  if (object[key] !== null && object[key] !== undefined && typeof object[key] !== "string") {
    errors.push(`${label} must be a string or null`);
  }
}

function requireNumber(object, key, errors, label = key) {
  if (typeof object[key] !== "number" || !Number.isFinite(object[key])) {
    errors.push(`${label} must be a finite number`);
  }
}

function requireNullableNumber(object, key, errors, label = key) {
  if (object[key] !== null && object[key] !== undefined) {
    requireNumber(object, key, errors, label);
  }
}

function requireEnum(object, key, allowed, errors, label = key) {
  if (!allowed.includes(object[key])) {
    errors.push(`${label} must be one of ${allowed.join(", ")}`);
  }
}

function validateJsonValue(value, path, errors) {
  if (value === null) {
    return;
  }

  const type = typeof value;
  if (type === "string" || type === "boolean") {
    return;
  }

  if (type === "number") {
    if (!Number.isFinite(value)) {
      errors.push(`${path} must contain only finite JSON numbers`);
    }
    return;
  }

  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1) {
      validateJsonValue(value[index], `${path}.${index}`, errors);
    }
    return;
  }

  if (type === "object") {
    const prototype = Object.getPrototypeOf(value);
    if (prototype !== Object.prototype && prototype !== null) {
      errors.push(`${path} must contain only plain JSON objects`);
      return;
    }

    for (const [key, child] of Object.entries(value)) {
      validateJsonValue(child, `${path}.${key}`, errors);
    }
    return;
  }

  errors.push(`${path} must contain only JSON-serializable values`);
}
