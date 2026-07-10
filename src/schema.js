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

export function createSpanRecord(input) {
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
    requireEnum(record.status, "code", STATUS_CODES, errors, "status.code");
    if (
      record.status.message !== undefined &&
      record.status.message !== null &&
      typeof record.status.message !== "string"
    ) {
      errors.push("status.message must be a string when present");
    }
  }

  for (const key of ["agent", "project", "attributes", "metrics", "content", "redaction"]) {
    if (record[key] === null || typeof record[key] !== "object" || Array.isArray(record[key])) {
      errors.push(`${key} must be an object`);
    } else {
      validateJsonValue(record[key], key, errors);
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
    }
  }

  return errors;
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
    ...(status.message ? { message: status.message } : {}),
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
