const DEFAULT_CONTENT_LOGGING = Object.freeze({
  prompts: false,
  outputs: false,
  tool_inputs: false,
  tool_outputs: false,
});

const SENSITIVE_KEY_PATTERN = /(^|_)(authorization|bearer|cookie|credential|credentials|password|passwd|secret|token|api_key|access_key|client_secret|id_token|private_key|refresh_token|session_key)($|_)/i;
const SENSITIVE_PATH_PATTERN = /(^|\/)(\.env(\.|$)|.*\.pem$|.*\.key$|.*\.tfstate(\.backup)?$|.*\.tfvars(\.json)?$)/i;

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

  result.redaction = {
    ...(result.redaction ?? {}),
    applied: redactions.length > 0 || Boolean(result.redaction?.applied),
    count: (result.redaction?.count ?? 0) + redactions.length,
    fields: [...(result.redaction?.fields ?? []), ...redactions],
  };

  return result;
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

    if (typeof value === "string" && looksLikeSensitivePath(key, value)) {
      node[key] = "[redacted path]";
      redactions.push(pointer);
      continue;
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
