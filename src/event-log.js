import { appendFile, mkdir, readFile } from "node:fs/promises";
import { dirname } from "node:path";
import { assertValidSpanRecord } from "./schema.js";
import { redactRecord } from "./redaction.js";

export async function appendEventLog(filePath, record, options = {}) {
  assertValidSpanRecord(record);
  const sanitized = redactRecord(record, options);
  assertValidSpanRecord(sanitized);

  await mkdir(dirname(filePath), { recursive: true });
  await appendLine(filePath, JSON.stringify(sanitized));
  return sanitized;
}

export async function readEventLog(filePath) {
  const body = await readFile(filePath, "utf8");
  return body
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function appendLine(filePath, line) {
  return appendFile(filePath, `${line}\n`, "utf8");
}
