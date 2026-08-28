import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import {
  claudeCodeRecordsFromEvents,
  codexRecordsFromEvents,
  parseClaudeCodeJsonl,
  parseCodexSessionJsonl,
  reportDataFromRecords,
} from "../src/index.js";

const ROOT = new URL("../", import.meta.url);

test("shared closed schemas accept the frozen JavaScript durable and report contracts", async () => {
  const [durableSchema, reportSchema, codexText, claudeText] = await Promise.all([
    readJson("contracts/durable-record-v1.schema.json"),
    readJson("contracts/report-dto-v1.schema.json"),
    readFile(new URL("test/fixtures/golden/codex-source.jsonl", ROOT), "utf8"),
    readFile(new URL("test/fixtures/golden/claude-code-source.jsonl", ROOT), "utf8"),
  ]);
  const records = [
    ...codexRecordsFromEvents(parseCodexSessionJsonl(codexText)),
    ...claudeCodeRecordsFromEvents(parseClaudeCodeJsonl(claudeText)),
  ];
  const report = serialized(reportDataFromRecords(records, {
    generated_at: "2026-08-28T00:00:00.000Z",
  }));

  for (const record of records) {
    assert.deepEqual(validateJsonSchema(serialized(record), durableSchema), []);
  }
  assert.deepEqual(validateJsonSchema(report, reportSchema), []);
  const legacyFilters = { ...report.filters };
  delete legacyFilters.agents;
  delete legacyFilters.models;
  assert.deepEqual(validateJsonSchema({ ...report, filters: legacyFilters }, reportSchema), []);

  const wideLegacyRecord = serialized({
    ...records[0],
    trace_id: "x".repeat(513),
    start_time_unix_ms: -2,
    end_time_unix_ms: -1,
  });
  assert.deepEqual(validateJsonSchema(wideLegacyRecord, durableSchema), []);

  assert.match(
    validateJsonSchema({ ...records[0], unexpected: true }, durableSchema).join("; "),
    /durable.unexpected is not declared/,
  );
  assert.match(
    validateJsonSchema({ ...report, schemaVersion: "wrong" }, reportSchema).join("; "),
    /report.schemaVersion must equal/,
  );
  assert.match(
    validateJsonSchema({ ...report, summary: { ...report.summary, sessions: "one" } }, reportSchema).join("; "),
    /report.summary.sessions must have type number/,
  );
});

async function readJson(path) {
  return JSON.parse(await readFile(new URL(path, ROOT), "utf8"));
}

function serialized(value) {
  return JSON.parse(JSON.stringify(value));
}

function validateJsonSchema(value, schema, root = schema, path = schema.$id?.includes("report") ? "report" : "durable") {
  const errors = [];
  validateNode(value, schema, root, path, errors);
  return errors;
}

function validateNode(value, schema, root, path, errors) {
  const resolved = resolveSchema(schema, root);
  if (resolved.anyOf) {
    const matches = resolved.anyOf.some((candidate) => {
      const candidateErrors = [];
      validateNode(value, candidate, root, path, candidateErrors);
      return candidateErrors.length === 0;
    });
    if (!matches) {
      errors.push(`${path} does not match any allowed schema`);
    }
    return;
  }

  if (Object.hasOwn(resolved, "const") && value !== resolved.const) {
    errors.push(`${path} must equal ${JSON.stringify(resolved.const)}`);
  }
  if (resolved.enum && !resolved.enum.includes(value)) {
    errors.push(`${path} must be one of ${resolved.enum.join(", ")}`);
  }
  if (resolved.type && !matchesType(value, resolved.type)) {
    errors.push(`${path} must have type ${[].concat(resolved.type).join(" or ")}`);
    return;
  }
  if (typeof value === "string") {
    if (resolved.minLength !== undefined && value.length < resolved.minLength) {
      errors.push(`${path} is shorter than ${resolved.minLength}`);
    }
    if (resolved.maxLength !== undefined && value.length > resolved.maxLength) {
      errors.push(`${path} is longer than ${resolved.maxLength}`);
    }
    return;
  }
  if (typeof value === "number") {
    if (resolved.minimum !== undefined && value < resolved.minimum) {
      errors.push(`${path} is less than ${resolved.minimum}`);
    }
    return;
  }
  if (Array.isArray(value)) {
    if (resolved.items) {
      value.forEach((item, index) => validateNode(item, resolved.items, root, `${path}.${index}`, errors));
    }
    return;
  }
  if (value === null || typeof value !== "object") {
    return;
  }

  const properties = resolved.properties ?? {};
  if (resolved.additionalProperties === false) {
    for (const key of Object.keys(value)) {
      if (!(key in properties)) {
        errors.push(`${path}.${key} is not declared`);
      }
    }
  }
  for (const key of resolved.required ?? []) {
    if (!(key in value) || value[key] === undefined) {
      errors.push(`${path}.${key} is required`);
    }
  }
  for (const [key, child] of Object.entries(value)) {
    const childSchema = properties[key] ?? resolved.additionalProperties;
    if (childSchema && typeof childSchema === "object") {
      validateNode(child, childSchema, root, `${path}.${key}`, errors);
    }
  }
}

function matchesType(value, type) {
  return [].concat(type).some((candidate) => {
    if (candidate === "null") return value === null;
    if (candidate === "array") return Array.isArray(value);
    if (candidate === "object") return value !== null && typeof value === "object" && !Array.isArray(value);
    if (candidate === "integer") return Number.isInteger(value);
    if (candidate === "number") return typeof value === "number" && Number.isFinite(value);
    return typeof value === candidate;
  });
}

function resolveSchema(schema, root) {
  if (!schema?.$ref) {
    return schema ?? {};
  }
  const segments = schema.$ref.replace(/^#\//, "").split("/");
  return segments.reduce((value, segment) => value[segment], root);
}
