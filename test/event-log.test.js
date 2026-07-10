import assert from "node:assert/strict";
import test from "node:test";
import { mkdtemp, readFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { appendEventLog, createSpanRecord, readEventLog, SCHEMA_VERSION } from "../src/index.js";

test("writes parent and child spans as append-only JSONL", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-"));
  const logPath = join(dir, "events.jsonl");

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
});

test("redacts content and secrets before durable write", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-"));
  const logPath = join(dir, "events.jsonl");

  const span = createSpanRecord({
    trace_id: "trace-2",
    span_id: "turn-1",
    span_kind: "turn",
    name: "user turn",
    content: {
      prompt: "deploy with super-secret prompt",
      output: "the password is hunter2",
      tool_input: { command: "cat .env" },
    },
    attributes: {
      api_key: "sk-test-secret",
      apiKey: "sk-camel-secret",
      accessToken: "access-token-secret",
      config_path: "/repo/.env",
      terraform_file: "/repo/prod.tfvars",
      harmless: "kept",
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
  assert.equal(sanitized.attributes.api_key, "[redacted]");
  assert.equal(sanitized.attributes.apiKey, "[redacted]");
  assert.equal(sanitized.attributes.accessToken, "[redacted]");
  assert.equal(sanitized.attributes.config_path, "[redacted path]");
  assert.equal(sanitized.attributes.terraform_file, "[redacted path]");
  assert.equal(sanitized.attributes.harmless, "kept");
  assert.equal(sanitized.redaction.applied, true);
  assert.deepEqual(sanitized.redaction.fields.sort(), [
    "attributes.accessToken",
    "attributes.apiKey",
    "attributes.api_key",
    "attributes.config_path",
    "attributes.terraform_file",
    "content.output",
    "content.prompt",
    "content.tool_input",
  ]);

  const raw = await readFile(logPath, "utf8");
  assert.equal(raw.includes("super-secret prompt"), false);
  assert.equal(raw.includes("hunter2"), false);
  assert.equal(raw.includes("sk-test-secret"), false);
  assert.equal(raw.includes("sk-camel-secret"), false);
  assert.equal(raw.includes("access-token-secret"), false);
  assert.equal(raw.includes("/repo/.env"), false);
  assert.equal(raw.includes("/repo/prod.tfvars"), false);
});

test("rejects nested values that cannot be represented safely in JSONL", () => {
  assert.throws(
    () =>
      createSpanRecord({
        trace_id: "trace-3",
        span_id: "turn-2",
        span_kind: "turn",
        name: "invalid nested bigint",
        attributes: { bad: 1n },
      }),
    /attributes.bad must contain only JSON-serializable values/,
  );

  assert.throws(
    () =>
      createSpanRecord({
        trace_id: "trace-4",
        span_id: "turn-3",
        span_kind: "turn",
        name: "invalid nested undefined",
        attributes: { bad: undefined },
      }),
    /attributes.bad must contain only JSON-serializable values/,
  );
});
