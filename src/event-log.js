import { appendFile, rename, readFile, writeFile } from "node:fs/promises";
import { isDeepStrictEqual } from "node:util";
import { assertValidSpanRecord, migrateLegacyV1Record } from "./schema.js";
import { redactRecord } from "./redaction.js";
import { enforcePrivateFile, preparePrivateArtifact } from "./private-artifact.js";

export async function appendEventLog(filePath, record, options = {}) {
  const written = await appendEventLogRecords(filePath, [record], options);
  return written[0] ?? sanitizeRecord(record, options);
}

export async function appendEventLogRecords(filePath, records, options = {}) {
  const existing = await existingRecords(filePath);
  const planned = new Map(existing);
  const pending = [];

  for (const record of records) {
    const sanitized = sanitizeRecord(record, options);
    const previous = planned.get(sanitized.span_id);
    if (previous) {
      if (!isDeepStrictEqual(previous, sanitized)) {
        throw new Error(`Event log conflict for span_id ${sanitized.span_id}`);
      }
      continue;
    }
    planned.set(sanitized.span_id, sanitized);
    pending.push(sanitized);
  }

  for (const sanitized of pending) {
    await appendSanitizedRecord(filePath, sanitized);
  }

  return pending;
}

export async function readEventLog(filePath) {
  const body = await readFile(filePath, "utf8");
  let migrationRequired = false;
  const records = body
    .split("\n")
    .filter(Boolean)
    .map((line, index) => {
      const parsed = JSON.parse(line);
      try {
        assertValidSpanRecord(parsed);
        return parsed;
      } catch (error) {
        const migrated = redactRecord(migrateLegacyV1Record(parsed));
        try {
          assertValidSpanRecord(migrated);
          migrationRequired = true;
          return migrated;
        } catch {
          throw new Error(`Invalid event log record at line ${index + 1}: ${error.message}`);
        }
      }
    });

  if (migrationRequired) {
    await rewriteMigratedEventLog(filePath, records);
  }
  return records;
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

async function rewriteMigratedEventLog(filePath, records) {
  await preparePrivateArtifact(filePath);
  const temporaryPath = `${filePath}.migrate-${process.pid}-${Date.now()}`;
  const body = records.map((record) => JSON.stringify(record)).join("\n") + "\n";
  await writeFile(temporaryPath, body, { encoding: "utf8", mode: 0o600, flag: "wx" });
  await enforcePrivateFile(temporaryPath);
  await rename(temporaryPath, filePath);
  await enforcePrivateFile(filePath);
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
