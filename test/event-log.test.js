import assert from "node:assert/strict";
import test from "node:test";
import { chmod, mkdir, mkdtemp, readFile, stat, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  appendEventLog,
  appendEventLogRecords,
  createSpanRecord,
  readEventLog,
  SCHEMA_VERSION,
} from "../src/index.js";

test("writes parent and child spans as append-only JSONL", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-"));
  const artifactDir = join(dir, "private-artifacts");
  const logPath = join(artifactDir, "events.jsonl");
  await mkdir(artifactDir, { mode: 0o700 });

  const session = createSpanRecord({
    trace_id: "trace-1",
    span_id: "session-1",
    span_kind: "agent.session",
    name: "Codex session",
    status: "ok",
    agent: { name: "codex" },
  });

  const tool = createSpanRecord({
    trace_id: "trace-1",
    span_id: "tool-1",
    parent_span_id: "session-1",
    span_kind: "tool.execution",
    name: "exec_command",
    status: "ok",
    metrics: { duration_ms: 12 },
  });

  await appendEventLog(logPath, session);
  await appendEventLog(logPath, tool);

  const records = await readEventLog(logPath);
  assert.equal(records.length, 2);
  assert.equal(records[0].schema_version, SCHEMA_VERSION);
  assert.equal(records[1].parent_span_id, records[0].span_id);
  assert.equal(records[1].trace_id, records[0].trace_id);
  assert.equal((await stat(logPath)).mode & 0o777, 0o600);
  assert.equal((await stat(artifactDir)).mode & 0o777, 0o700);
});

test("treats identical replay as a no-op and rejects conflicting stable identities", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-replay-"));
  const logPath = join(dir, "events.jsonl");
  const original = createSpanRecord({
    trace_id: "trace-replay",
    span_id: "stable-span",
    span_kind: "turn",
    name: "stable turn",
    status: "ok",
    start_time_unix_ms: 1,
    attributes: { session_id: "session-replay", turn_id: "turn-replay" },
  });
  const conflicting = { ...original, status: { code: "error" } };

  assert.equal((await appendEventLogRecords(logPath, [original])).length, 1);
  assert.deepEqual(await appendEventLogRecords(logPath, [original]), []);
  const reordered = {
    ...original,
    attributes: { turn_id: "turn-replay", session_id: "session-replay" },
  };
  assert.deepEqual(await appendEventLogRecords(logPath, [reordered]), []);
  await assert.rejects(
    () => appendEventLogRecords(logPath, [conflicting]),
    /Event log conflict for span_id id:sha256:/,
  );
  assert.equal((await readEventLog(logPath)).length, 1);
});

test("prevalidates batch conflicts before writing any new record", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-batch-conflict-"));
  const logPath = join(dir, "events.jsonl");
  const original = createSpanRecord({
    trace_id: "trace-batch",
    span_id: "existing-span",
    span_kind: "turn",
    name: "existing turn",
    status: "ok",
    start_time_unix_ms: 1,
  });
  const newRecord = createSpanRecord({
    trace_id: "trace-batch",
    span_id: "new-span",
    span_kind: "turn",
    name: "new turn",
    status: "ok",
    start_time_unix_ms: 2,
  });
  const conflicting = { ...original, status: { code: "error" } };
  await appendEventLogRecords(logPath, [original]);

  await assert.rejects(
    () => appendEventLogRecords(logPath, [newRecord, conflicting]),
    /Event log conflict for span_id id:sha256:/,
  );
  assert.deepEqual(
    (await readEventLog(logPath)).map((record) => record.span_id),
    ["id:sha256:52b81b16cf01922fdf4144ccc30e215a0a744a50d0cbd2cf5e703468027502eb"],
  );
});

test("single-record append uses replay no-op and conflict semantics", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-single-replay-"));
  const logPath = join(dir, "events.jsonl");
  const original = createSpanRecord({
    trace_id: "single-trace",
    span_id: "single-span",
    span_kind: "turn",
    name: "single",
    status: "ok",
    start_time_unix_ms: 1,
  });
  await appendEventLog(logPath, original);
  await appendEventLog(logPath, original);
  await assert.rejects(
    () => appendEventLog(logPath, { ...original, status: { code: "error" } }),
    /Event log conflict/,
  );
  assert.equal((await readEventLog(logPath)).length, 1);
});

test("reads legacy v1 metadata through the strict compatibility projection", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-legacy-v1-"));
  const logPath = join(dir, "events.jsonl");
  const legacy = createSpanRecord({
    trace_id: "legacy-trace",
    span_id: "legacy-span",
    span_kind: "turn",
    name: "legacy",
    start_time_unix_ms: 1,
  });
  legacy.status.message = "RAW_LEGACY_STATUS_SECRET";
  legacy.attributes.legacy_extension = "RAW_LEGACY_METADATA_SECRET";
  await writeFile(logPath, `${JSON.stringify(legacy)}\n`, { mode: 0o600 });

  const [migrated] = await readEventLog(logPath);
  assert.deepEqual(migrated.status, { code: "unset" });
  assert.equal("legacy_extension" in migrated.attributes, false);
  assert.equal(JSON.stringify(migrated).includes("RAW_LEGACY_"), false);
  const rewritten = await readFile(logPath, "utf8");
  assert.equal(rewritten.includes("RAW_LEGACY_"), false);
  assert.match(migrated.span_id, /^id:sha256:[a-f0-9]{64}$/);
  assert.equal((await stat(logPath)).mode & 0o777, 0o600);
});

test("rejects malformed legacy v1 containers without rewriting the source", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-malformed-v1-"));
  const logPath = join(dir, "events.jsonl");
  const malformed = createSpanRecord({
    trace_id: "malformed-trace",
    span_id: "malformed-span",
    span_kind: "turn",
    name: "malformed",
    start_time_unix_ms: 1,
  });
  malformed.attributes = [];
  const original = `${JSON.stringify(malformed)}\n`;
  await writeFile(logPath, original, { mode: 0o600 });

  await assert.rejects(() => readEventLog(logPath), /attributes must be an object/);
  assert.equal(await readFile(logPath, "utf8"), original);
});

test("rejects permissive caller-owned artifact directories without changing them", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-shared-dir-"));
  await chmod(dir, 0o755);
  const logPath = join(dir, "events.jsonl");
  const record = createSpanRecord({
    trace_id: "shared-trace",
    span_id: "shared-span",
    span_kind: "turn",
    name: "shared",
  });
  await assert.rejects(() => appendEventLog(logPath, record), /must use mode 0700/);
  assert.equal((await stat(dir)).mode & 0o777, 0o755);
});

test("redacts content and secrets before durable write", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-"));
  const logPath = join(dir, "events.jsonl");

  const span = createSpanRecord({
    trace_id: "trace-2",
    span_id: "turn-1",
    span_kind: "turn",
    name: "user turn",
    project: { repo_path: "/repo/.env" },
    content: {
      prompt: "deploy with super-secret prompt",
      output: "the password is hunter2",
      tool_input: { command: "cat .env" },
    },
    attributes: {
      source: "Authorization: Bearer access-token-secret",
      event_type: "turn.started",
      session_id: "session-2",
      turn_id: "turn-1",
    },
  });

  const sanitized = await appendEventLog(logPath, span, {
    content_logging: {
      prompts: false,
      outputs: false,
      tool_inputs: false,
      tool_outputs: false,
    },
  });

  assert.equal(sanitized.content.prompt, "[content omitted]");
  assert.equal(sanitized.content.output, "[content omitted]");
  assert.equal(sanitized.content.tool_input, "[content omitted]");
  assert.equal(sanitized.attributes.source, "Authorization: Bearer [redacted]");
  assert.equal(sanitized.project.repo_path, "[redacted path]");
  assert.equal(sanitized.redaction.applied, true);
  assert.deepEqual(sanitized.redaction.fields.sort(), [
    "attributes.session_id",
    "attributes.source",
    "attributes.turn_id",
    "content.output",
    "content.prompt",
    "content.tool_input",
    "name",
    "project.repo_path",
    "span_id",
    "trace_id",
  ]);

  const raw = await readFile(logPath, "utf8");
  assert.equal(raw.includes("super-secret prompt"), false);
  assert.equal(raw.includes("hunter2"), false);
  assert.equal(raw.includes("access-token-secret"), false);
  assert.equal(raw.includes("/repo/.env"), false);
});

test("rejects unknown metadata and nested values that cannot be represented safely in JSONL", () => {
  assert.throws(
    () =>
      createSpanRecord({
        trace_id: "trace-unknown",
        span_id: "turn-unknown",
        span_kind: "turn",
        name: "unknown metadata",
        attributes: { harmless: "must not persist" },
      }),
    /attributes.harmless is not allowed/,
  );

  assert.throws(
    () =>
      createSpanRecord({
        trace_id: "trace-3",
        span_id: "turn-2",
        span_kind: "turn",
        name: "invalid nested bigint",
        content: { tool_input: { bad: 1n } },
      }),
    /content.tool_input.bad must contain only JSON-serializable values/,
  );

  assert.throws(
    () =>
      createSpanRecord({
        trace_id: "trace-4",
        span_id: "turn-3",
        span_kind: "turn",
        name: "invalid nested undefined",
        content: { tool_input: { bad: undefined } },
      }),
    /content.tool_input.bad must contain only JSON-serializable values/,
  );
});
