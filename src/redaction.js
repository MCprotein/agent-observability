const DEFAULT_CONTENT_LOGGING = Object.freeze({
  prompts: false,
  outputs: false,
  tool_inputs: false,
  tool_outputs: false,
});

const SENSITIVE_KEY_PATTERN = /(^|_)(authorization|bearer|cookie|credential|credentials|password|passwd|secret|token|api_key|access_key|client_secret|id_token|private_key|refresh_token|session_key)($|_)/i;
const SENSITIVE_PATH_PATTERN = /(^|\/)(\.env(\.|$)|.*\.pem$|.*\.key$|.*\.tfstate(\.backup)?$|.*\.tfvars(\.json)?$)/i;
const SENSITIVE_PATH_FRAGMENT_PATTERN = /(?:^|\s)(\S*(?:\/|^)(?:\.env(?:\.\S*)?|\S+\.pem|\S+\.key|\S+\.tfstate(?:\.backup)?|\S+\.tfvars(?:\.json)?))(?:\s|$)/gi;
const SECRET_ASSIGNMENT_PATTERN = /\b((?:api[_-]?key|access[_-]?token|refresh[_-]?token|id[_-]?token|client[_-]?secret|password|passwd|secret|token)\s*[:=]\s*)[^\s,;&]+/gi;
const AUTHORIZATION_PATTERN = /\b(authorization\s*[:=]\s*(?:bearer\s+)?)\S+/gi;
const BEARER_PATTERN = /\b(bearer\s+)[A-Za-z0-9._~+/=-]+/gi;
const PRIVATE_KEY_BLOCK_PATTERN = /-----BEGIN [^-]*PRIVATE KEY-----[\s\S]*?-----END [^-]*PRIVATE KEY-----/gi;

const CONTENT_FIELD_POLICY = Object.freeze({
  prompt: "prompts",
  output: "outputs",
  tool_input: "tool_inputs",
  tool_output: "tool_outputs",
});

export function redactRecord(record, options = {}) {
  const contentLogging = {
    ...DEFAULT_CONTENT_LOGGING,
    ...(options.content_logging ?? {}),
  };
  const result = structuredClone(record);
  const redactions = [];

  redactNode(result, [], redactions, contentLogging);
  sanitizeSpanDisplayFields(result, redactions);

  result.redaction = {
    ...(result.redaction ?? {}),
    applied: redactions.length > 0 || Boolean(result.redaction?.applied),
    count: (result.redaction?.count ?? 0) + redactions.length,
    fields: [...(result.redaction?.fields ?? []), ...redactions],
  };

  return result;
}

function sanitizeSpanDisplayFields(record, redactions) {
  if (record?.record_type !== "span") {
    return;
  }

  const safeName = safeSpanName(record);
  if (record.name !== safeName) {
    record.name = safeName;
    redactions.push("name");
  }
}

function safeSpanName(record) {
  if (record.span_kind === "agent.session") {
    return `${redactText(record.agent?.name ?? "Agent", "agent.name")} session`;
  }
  if (record.span_kind === "turn") {
    return record.attributes?.turn_id
      ? `Turn ${redactText(String(record.attributes.turn_id), "turn_id")}`
      : "Turn";
  }
  if (record.span_kind === "llm.request") {
    return record.agent?.model
      ? `LLM ${redactText(record.agent.model, "agent.model")}`
      : "LLM request";
  }
  if (record.span_kind === "tool.execution") {
    return redactText(record.attributes?.tool_name ?? "Tool execution", "tool_name");
  }
  if (record.span_kind === "permission") {
    return "Permission";
  }
  if (record.span_kind === "compaction") {
    return "Compaction";
  }
  if (record.span_kind === "workstream") {
    return "Workstream";
  }
  return redactText(String(record.name), "name");
}

export function redactText(value, key = "") {
  if (typeof value !== "string") {
    return value;
  }

  if (isSensitiveKey(key)) {
    return "[redacted]";
  }

  if (looksLikeSensitivePath(key, value)) {
    return "[redacted path]";
  }

  return value
    .replace(PRIVATE_KEY_BLOCK_PATTERN, "[redacted]")
    .replace(AUTHORIZATION_PATTERN, "$1[redacted]")
    .replace(BEARER_PATTERN, "$1[redacted]")
    .replace(SECRET_ASSIGNMENT_PATTERN, "$1[redacted]")
    .replace(SENSITIVE_PATH_FRAGMENT_PATTERN, (match, sensitivePath) =>
      match.replace(sensitivePath, "[redacted path]"),
    );
}

function redactNode(node, path, redactions, contentLogging) {
  if (!node || typeof node !== "object") {
    return;
  }

  if (Array.isArray(node)) {
    for (let index = 0; index < node.length; index += 1) {
      redactNode(node[index], [...path, String(index)], redactions, contentLogging);
    }
    return;
  }

  for (const [key, value] of Object.entries(node)) {
    const nextPath = [...path, key];
    const pointer = nextPath.join(".");
    const contentPolicy = path[path.length - 1] === "content" ? CONTENT_FIELD_POLICY[key] : null;

    if (contentPolicy && contentLogging[contentPolicy] !== true) {
      node[key] = "[content omitted]";
      redactions.push(pointer);
      continue;
    }

    if (isSensitiveKey(key)) {
      node[key] = "[redacted]";
      redactions.push(pointer);
      continue;
    }

    if (typeof value === "string") {
      const redactedText = redactText(value, key);
      if (redactedText !== value) {
        node[key] = redactedText;
        redactions.push(pointer);
        continue;
      }
    }

    redactNode(value, nextPath, redactions, contentLogging);
  }
}

function isSensitiveKey(key) {
  return SENSITIVE_KEY_PATTERN.test(normalizeKey(key));
}

function normalizeKey(key) {
  return key.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();
}

function looksLikeSensitivePath(key, value) {
  if (!/(path|file|filename)$/i.test(key)) {
    return false;
  }
  return SENSITIVE_PATH_PATTERN.test(value);
}
