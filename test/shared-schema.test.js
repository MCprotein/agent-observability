import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import test from "node:test";
import { chmod, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import Ajv2020 from "ajv/dist/2020.js";
import {
  claudeCodeRecordsFromEvents,
  codexRecordsFromEvents,
  parseClaudeCodeJsonl,
  parseCodexSessionJsonl,
  reportDataFromRecords,
} from "../src/index.js";

const ROOT = new URL("../", import.meta.url);
const execFileAsync = promisify(execFile);

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

test("retention archive schema resolves the durable record contract", async () => {
  const [durableSchema, archiveSchema] = await Promise.all([
    readJson("contracts/durable-record-v1.schema.json"),
    readJson("contracts/retention-archive-entry-v1.schema.json"),
  ]);
  const ajv = new Ajv2020({ strict: true, allErrors: true, allowUnionTypes: true });
  ajv.addSchema(durableSchema);
  assert.doesNotThrow(() => ajv.compile(archiveSchema));
});

test("the Rust cross-agent retention archive satisfies the shared archive schema", async () => {
  const [durableSchema, archiveSchema, codexFixture, claudeFixture, cursorFixture] = await Promise.all([
    readJson("contracts/durable-record-v1.schema.json"),
    readJson("contracts/retention-archive-entry-v1.schema.json"),
    readFile(new URL("crates/adapter-codex/tests/fixtures/codex-handoff.jsonl", ROOT), "utf8"),
    readFile(new URL("crates/adapter-claude-code/tests/fixtures/claude-handoff.jsonl", ROOT), "utf8"),
    readFile(new URL("crates/adapter-cursor/tests/fixtures/cursor-handoff.jsonl", ROOT), "utf8"),
  ]);
  const ajv = new Ajv2020({ strict: true, allErrors: true, allowUnionTypes: true });
  ajv.addSchema(durableSchema);
  const validate = ajv.compile(archiveSchema);
  const root = await mkdtemp(`${tmpdir()}/agent-observability-archive-schema-`);
  const runtime = `${root}/runtime`;
  const archive = `${root}/expired.jsonl`;
  const cargo = async (...args) => execFileAsync(
    "cargo",
    ["run", "-q", "-p", "agent-observability-cli", "--", ...args],
    { cwd: fileURLToPath(ROOT), timeout: 120_000 },
  );

  try {
    await chmod(root, 0o700);
    for (const [command, name, fixture] of [
      ["codex-ingest", "codex", codexFixture],
      ["claude-code-ingest", "claude", claudeFixture],
      ["cursor-ingest", "cursor", cursorFixture],
    ]) {
      const handoff = `${root}/old-${name}-handoff.jsonl`;
      await writeFile(handoff, fixture.replaceAll("178787520", "100000000"), { mode: 0o600 });
      await cargo(command, runtime, handoff);
    }
    const { stdout: planOutput } = await cargo("retention-plan", runtime);
    const planId = planOutput.split("\n").find((line) => line.startsWith("plan_id="))?.slice(8);
    assert.match(planId ?? "", /^[0-9a-f]{64}$/);
    await cargo("retention-apply", runtime, planId, archive);

    const entries = (await readFile(archive, "utf8"))
      .trimEnd()
      .split("\n")
      .map((line) => JSON.parse(line));
    assert.ok(entries.length >= 3);
    for (const entry of entries) {
      assert.equal(validate(entry), true, JSON.stringify(validate.errors));
    }
    const agentNames = new Set(
      entries.filter((entry) => entry.entry_type === "record").map((entry) => entry.record.agent.name),
    );
    assert.deepEqual([...agentNames].sort(), ["claude-code", "codex", "cursor"]);
    assert.equal(validate({ ...entries[0], unexpected: true }), false);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
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
