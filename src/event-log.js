import { appendFile, readFile } from "node:fs/promises";
import { assertValidSpanRecord } from "./schema.js";
import { redactRecord } from "./redaction.js";
import { enforcePrivateFile, preparePrivateArtifact } from "./private-artifact.js";

export async function appendEventLog(filePath, record, options = {}) {
  const sanitized = sanitizeRecord(record, options);
  await appendSanitizedRecord(filePath, sanitized);
  return sanitized;
}

export async function appendEventLogRecords(filePath, records, options = {}) {
  const existing = await existingRecords(filePath);
  const written = [];

  for (const record of records) {
    const sanitized = sanitizeRecord(record, options);
    const previous = existing.get(sanitized.span_id);
    if (previous) {
      if (JSON.stringify(previous) !== JSON.stringify(sanitized)) {
        throw new Error(`Event log conflict for span_id ${sanitized.span_id}`);
      }
      continue;
    }
    await appendSanitizedRecord(filePath, sanitized);
    existing.set(sanitized.span_id, sanitized);
    written.push(sanitized);
  }

  return written;
}

export async function readEventLog(filePath) {
  const body = await readFile(filePath, "utf8");
  return body
    .split("\n")
    .filter(Boolean)
    .map((line, index) => {
      const record = JSON.parse(line);
      try {
        assertValidSpanRecord(record);
      } catch (error) {
        throw new Error(`Invalid event log record at line ${index + 1}: ${error.message}`);
      }
      return record;
    });
}

function sanitizeRecord(record, options) {
  assertValidSpanRecord(record);
  const sanitized = redactRecord(record, options);
  assertValidSpanRecord(sanitized);
  return sanitized;
}

async function appendSanitizedRecord(filePath, record) {
  await preparePrivateArtifact(filePath);
  await appendLine(filePath, JSON.stringify(record));
  await enforcePrivateFile(filePath);
}

function appendLine(filePath, line) {
  return appendFile(filePath, `${line}\n`, { encoding: "utf8", mode: 0o600 });
}

async function existingRecords(filePath) {
  try {
    return new Map((await readEventLog(filePath)).map((record) => [record.span_id, record]));
  } catch (error) {
    if (error?.code === "ENOENT") {
      return new Map();
    }
    throw error;
  }
}
