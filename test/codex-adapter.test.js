import assert from "node:assert/strict";
import test from "node:test";
import { mkdtemp, readFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  appendCodexSessionJsonl,
  codexRecordsFromEvents,
  normalizeCodexNotifyPayload,
  parseCodexSessionJsonl,
  readEventLog,
} from "../src/index.js";

const SESSION_JSONL = [
  JSON.stringify({
    type: "session.started",
    session_id: "s1",
    timestamp: "2026-07-10T00:00:00.000Z",
    model: "gpt-test",
    cwd: "/repo",
  }),
  JSON.stringify({
    type: "turn.started",
    session_id: "s1",
    turn_id: "t1",
    timestamp: "2026-07-10T00:00:01.000Z",
  }),
  JSON.stringify({
    type: "llm.completed",
    session_id: "s1",
    turn_id: "t1",
    request_id: "r1",
    model: "gpt-test",
    duration_ms: 1200,
    usage: {
      input_tokens: 100,
      output_tokens: 25,
      cached_input_tokens: 10,
      reasoning_output_tokens: 5,
    },
  }),
  JSON.stringify({
    type: "tool.completed",
    session_id: "s1",
    turn_id: "t1",
    call_id: "c1",
    tool_name: "exec_command",
    duration_ms: 30,
    exit_code: 0,
    status: "ok",
  }),
].join("\n");

const REAL_CODEX_SESSION_JSONL = [
  JSON.stringify({
    type: "session_meta",
    payload: {
      session_id: "real-s1",
      id: "rollout-real-s1",
      timestamp: "2026-07-10T00:00:00.000Z",
      cwd: "/repo",
      model: "gpt-test",
      model_provider: "provider-test",
    },
  }),
  JSON.stringify({
    type: "event_msg",
    payload: {
      type: "task_started",
      turn_id: "turn-real-1",
      started_at: "2026-07-10T00:00:01.000Z",
    },
  }),
  JSON.stringify({
    type: "turn_context",
    payload: {
      turn_id: "turn-real-1",
      model: "gpt-test",
      cwd: "/repo",
      sandbox_policy: "workspace-write",
    },
  }),
  JSON.stringify({
    type: "response_item",
    payload: {
      type: "message",
      role: "user",
      content: "RAW_USER_MESSAGE_SECRET",
    },
  }),
  JSON.stringify({
    type: "response_item",
    payload: {
      type: "function_call",
      name: "exec_command",
      call_id: "call-real-1",
      arguments: "{\"cmd\":\"echo RAW_ARGUMENT_SECRET\"}",
    },
  }),
  JSON.stringify({
    type: "response_item",
    payload: {
      type: "function_call_output",
      call_id: "call-real-1",
      output: "RAW_OUTPUT_SECRET",
      stdout: "RAW_STDOUT_SECRET",
      stderr: "RAW_STDERR_SECRET",
    },
  }),
  JSON.stringify({
    type: "event_msg",
    payload: {
      type: "token_count",
      info: {
        last_token_usage: {
          input_tokens: 11,
          output_tokens: 7,
          cached_input_tokens: 2,
          reasoning_output_tokens: 1,
          total_tokens: 18,
        },
        total_token_usage: {
          input_tokens: 21,
          output_tokens: 13,
          cached_input_tokens: 3,
          reasoning_output_tokens: 2,
          total_tokens: 34,
        },
        model_context_window: 128000,
      },
      rate_limits: {
        limit_id: "test-limit",
        limit_name: "test",
        plan_type: "test-plan",
      },
    },
  }),
].join("\n");

test("parses Codex session JSONL into session, turn, LLM, and tool spans", () => {
  const events = parseCodexSessionJsonl(SESSION_JSONL);
  const records = codexRecordsFromEvents(events, { project_name: "agent-observability" });

  assert.equal(records.length, 4);

  const session = records.find((record) => record.span_kind === "agent.session");
  const turn = records.find((record) => record.span_kind === "turn");
  const llm = records.find((record) => record.span_kind === "llm.request");
  const tool = records.find((record) => record.span_kind === "tool.execution");

  assert.equal(turn.parent_span_id, session.span_id);
  assert.equal(llm.parent_span_id, turn.span_id);
  assert.equal(tool.parent_span_id, turn.span_id);
  assert.equal(llm.metrics.input_tokens, 100);
  assert.equal(llm.metrics.output_tokens, 25);
  assert.equal(llm.metrics.cached_input_tokens, 10);
  assert.equal(llm.metrics.reasoning_output_tokens, 5);
  assert.equal(llm.metrics.latency_ms, 1200);
  assert.equal(tool.metrics.duration_ms, 30);
  assert.equal(tool.attributes.tool_name, "exec_command");
});

test("parses real Codex session envelopes without copying raw content", () => {
  const events = parseCodexSessionJsonl(REAL_CODEX_SESSION_JSONL);
  const records = codexRecordsFromEvents(events, { project_name: "agent-observability" });

  const session = records.find((record) => record.span_kind === "agent.session");
  const turn = records.find((record) => record.span_kind === "turn");
  const llm = records.find((record) => record.span_kind === "llm.request");
  const tools = records.filter((record) => record.span_kind === "tool.execution");

  assert.ok(session);
  assert.ok(turn);
  assert.ok(llm);
  assert.equal(tools.length, 2);
  assert.equal(turn.parent_span_id, session.span_id);
  assert.equal(llm.parent_span_id, turn.span_id);
  assert.equal(tools[0].parent_span_id, turn.span_id);
  assert.equal(tools[0].attributes.tool_name, "exec_command");
  assert.equal(tools[0].attributes.phase, "call");
  assert.equal(tools[1].attributes.tool_name, "exec_command");
  assert.equal(tools[1].attributes.phase, "output");
  assert.equal(llm.metrics.input_tokens, 11);
  assert.equal(llm.metrics.output_tokens, 7);
  assert.equal(llm.metrics.cached_input_tokens, 2);
  assert.equal(llm.metrics.reasoning_output_tokens, 1);
  assert.equal(llm.metrics.total_tokens, 18);
  assert.equal(llm.metrics.total_input_tokens, 21);
  assert.equal(llm.metrics.total_output_tokens, 13);
  assert.equal(llm.metrics.context_window_tokens, 128000);

  const serialized = JSON.stringify(records);
  assert.equal(serialized.includes("RAW_USER_MESSAGE_SECRET"), false);
  assert.equal(serialized.includes("RAW_ARGUMENT_SECRET"), false);
  assert.equal(serialized.includes("RAW_OUTPUT_SECRET"), false);
  assert.equal(serialized.includes("RAW_STDOUT_SECRET"), false);
  assert.equal(serialized.includes("RAW_STDERR_SECRET"), false);
});

test("normalizes Codex notify payloads into tool spans without raw content", () => {
  const records = normalizeCodexNotifyPayload({
    type: "tool.completed",
    session_id: "s2",
    turn_id: "t2",
    call_id: "c2",
    tool_name: "exec_command",
    duration_ms: 12,
    status: "ok",
    output: "raw output should not be copied",
  });

  const session = records.find((record) => record.span_kind === "agent.session");
  const turn = records.find((record) => record.span_kind === "turn");
  const tool = records.find((record) => record.span_kind === "tool.execution");
  assert.equal(records.length, 3);
  assert.equal(turn.parent_span_id, session.span_id);
  assert.equal(tool.parent_span_id, turn.span_id);
  assert.equal(tool.metrics.duration_ms, 12);
  assert.equal(tool.content.output, undefined);
});

test("does not copy raw Codex error strings into status messages", () => {
  const records = normalizeCodexNotifyPayload({
    type: "tool.completed",
    session_id: "s3",
    turn_id: "t3",
    call_id: "c3",
    tool_name: "exec_command",
    status: "error",
    error: "failed with token sk-raw-secret",
  });

  const tool = records.find((record) => record.span_kind === "tool.execution");
  assert.equal(tool.status.code, "error");
  assert.equal(tool.status.message, undefined);
  assert.equal(JSON.stringify(records).includes("sk-raw-secret"), false);
});

test("appends parsed synthetic Codex session spans to the local event log", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-codex-"));
  const logPath = join(dir, "events.jsonl");

  const written = await appendCodexSessionJsonl(logPath, SESSION_JSONL, {
    project_name: "agent-observability",
    content_logging: {
      prompts: false,
      outputs: false,
      tool_inputs: false,
      tool_outputs: false,
    },
  });

  const records = await readEventLog(logPath);
  const raw = await readFile(logPath, "utf8");

  assert.equal(written.length, 4);
  assert.equal(records.length, 4);
  assert.equal(raw.includes("raw output should not be copied"), false);
});

test("appends real Codex envelope spans without raw content", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-codex-real-"));
  const logPath = join(dir, "events.jsonl");

  const written = await appendCodexSessionJsonl(logPath, REAL_CODEX_SESSION_JSONL, {
    project_name: "agent-observability",
    content_logging: {
      prompts: false,
      outputs: false,
      tool_inputs: false,
      tool_outputs: false,
    },
  });

  const records = await readEventLog(logPath);
  const raw = await readFile(logPath, "utf8");

  assert.equal(written.length, 5);
  assert.equal(records.length, 5);
  assert.equal(records.some((record) => record.span_kind === "llm.request"), true);
  assert.equal(records.filter((record) => record.span_kind === "tool.execution").length, 2);
  assert.equal(raw.includes("RAW_USER_MESSAGE_SECRET"), false);
  assert.equal(raw.includes("RAW_ARGUMENT_SECRET"), false);
  assert.equal(raw.includes("RAW_OUTPUT_SECRET"), false);
  assert.equal(raw.includes("RAW_STDOUT_SECRET"), false);
  assert.equal(raw.includes("RAW_STDERR_SECRET"), false);
});
