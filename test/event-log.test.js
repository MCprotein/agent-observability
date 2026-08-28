import assert from "node:assert/strict";
import test from "node:test";
import { mkdtemp, readFile, stat } from "node:fs/promises";
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
  await assert.rejects(
    () => appendEventLogRecords(logPath, [conflicting]),
    /Event log conflict for span_id stable-span/,
  );
  assert.equal((await readEventLog(logPath)).length, 1);
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
    "attributes.source",
    "content.output",
    "content.prompt",
    "content.tool_input",
    "name",
    "project.repo_path",
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
