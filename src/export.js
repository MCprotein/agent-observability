import { writeFile } from "node:fs/promises";
import { assertValidSpanRecord } from "./schema.js";
import { redactRecord, redactText } from "./redaction.js";
import { enforcePrivateFile, preparePrivateArtifact } from "./private-artifact.js";

export function redactedSnapshotFromRecords(records, options = {}) {
  return {
    schema_version: "agent_observability.snapshot.v1",
    generated_at: options.generated_at ?? new Date().toISOString(),
    records: records.map((record) => redactedSnapshotRecord(record, options)),
  };
}

export async function writeRedactedJsonSnapshot(filePath, records, options = {}) {
  const snapshot = redactedSnapshotFromRecords(records, options);
  const body = `${JSON.stringify(snapshot, null, 2)}\n`;
  await preparePrivateArtifact(filePath);
  await writeFile(filePath, body, { encoding: "utf8", mode: 0o600 });
  await enforcePrivateFile(filePath);
  return {
    filePath,
    bytes: Buffer.byteLength(body, "utf8"),
    records: snapshot.records.length,
  };
}

function redactedSnapshotRecord(record, options) {
  assertValidSpanRecord(record);
  const sanitized = redactRecord(record, {
    ...options,
    content_logging: {
      prompts: false,
      outputs: false,
      tool_inputs: false,
      tool_outputs: false,
    },
  });

  return {
    ...sanitized,
    name: safeDisplayName(sanitized),
    attributes: safeObjectStrings(sanitized.attributes),
    agent: safeObjectStrings(sanitized.agent),
    project: safeObjectStrings(sanitized.project),
  };
}

function safeDisplayName(record) {
  if (record.span_kind === "agent.session") {
    return `${record.agent?.name ?? "Agent"} session`;
  }
  if (record.span_kind === "turn") {
    return record.attributes?.turn_id ? `Turn ${record.attributes.turn_id}` : "Turn";
  }
  if (record.span_kind === "llm.request") {
    return record.agent?.model ? `LLM ${record.agent.model}` : "LLM request";
  }
  if (record.span_kind === "tool.execution") {
    return record.attributes?.tool_name ?? "Tool execution";
  }
  return redactText(String(record.name), "name");
}

function safeObjectStrings(object) {
  return Object.fromEntries(
    Object.entries(object ?? {}).map(([key, value]) => [
      key,
      typeof value === "string" ? redactText(value, key) : value,
    ]),
  );
}
