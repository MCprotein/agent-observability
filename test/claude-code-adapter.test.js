import assert from "node:assert/strict";
import test from "node:test";
import { mkdtemp, readFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  appendClaudeCodeJsonl,
  claudeCodeRecordsFromEvents,
  normalizeClaudeCodeHookPayload,
  parseClaudeCodeJsonl,
  readEventLog,
} from "../src/index.js";

const CLAUDE_CODE_JSONL = [
  JSON.stringify({
    hook_event_name: "SessionStart",
    session_id: "cc-s1",
    timestamp: "2026-07-10T00:00:00.000Z",
    cwd: "/repo",
    transcript_path: "/Users/example/.claude/projects/raw-transcript.jsonl",
    model: "claude-test",
  }),
  JSON.stringify({
    hook_event_name: "UserPromptSubmit",
    session_id: "cc-s1",
    timestamp: "2026-07-10T00:00:01.000Z",
    prompt_id: "turn-1",
    prompt: "RAW_PROMPT_SECRET",
  }),
  JSON.stringify({
    type: "assistant",
    sessionId: "cc-s1",
    uuid: "assistant-message-1",
    timestamp: "2026-07-10T00:00:03.000Z",
    message: {
      id: "assistant-message-1",
      model: "claude-test",
      usage: {
        input_tokens: 120,
        output_tokens: 34,
        cache_read_input_tokens: 9,
        cache_creation_input_tokens: 3,
      },
      content: [
        {
          type: "text",
          text: "RAW_ASSISTANT_SECRET",
        },
        {
          type: "tool_use",
          id: "toolu_1",
          name: "Bash",
          input: {
            command: "echo RAW_TOOL_INPUT_SECRET",
          },
        },
      ],
    },
  }),
  JSON.stringify({
    hook_event_name: "PostToolUse",
    session_id: "cc-s1",
    timestamp: "2026-07-10T00:00:04.000Z",
    tool_use_id: "toolu_1",
    tool_name: "Bash",
    duration_ms: 42,
    tool_response: {
      stdout: "RAW_STDOUT_SECRET",
      stderr: "RAW_STDERR_SECRET",
      exit_code: 0,
    },
  }),
  JSON.stringify({
    hook_event_name: "PermissionRequest",
    session_id: "cc-s1",
    timestamp: "2026-07-10T00:00:05.000Z",
    permission_id: "perm-1",
    tool_name: "Bash",
    command_kind: "shell",
    decision: "denied",
    command: "rm -rf RAW_PERMISSION_COMMAND_SECRET",
  }),
  JSON.stringify({
    hook_event_name: "PreCompact",
    session_id: "cc-s1",
    timestamp: "2026-07-10T00:00:06.000Z",
    compaction_id: "compact-1",
    before_tokens: 120000,
    after_tokens: 64000,
    trigger: "manual",
  }),
].join("\n");

const TRANSCRIPT_ONLY_MULTI_TURN_JSONL = [
  JSON.stringify({
    hook_event_name: "SessionStart",
    session_id: "cc-transcript-s1",
    timestamp: "2026-07-10T00:00:00.000Z",
  }),
  JSON.stringify({
    type: "user",
    sessionId: "cc-transcript-s1",
    uuid: "user-turn-1",
    timestamp: "2026-07-10T00:00:01.000Z",
    message: {
      role: "user",
      content: "RAW_MULTI_TURN_PROMPT_1",
    },
  }),
  JSON.stringify({
    type: "assistant",
    sessionId: "cc-transcript-s1",
    uuid: "assistant-turn-1",
    parent_uuid: "user-turn-1",
    timestamp: "2026-07-10T00:00:02.000Z",
    message: {
      id: "assistant-turn-1",
      model: "claude-test",
      usage: {
        input_tokens: 10,
        output_tokens: 4,
      },
      content: [
        {
          type: "text",
          text: "RAW_MULTI_TURN_OUTPUT_1",
        },
      ],
    },
  }),
  JSON.stringify({
    hook_event_name: "Stop",
    session_id: "cc-transcript-s1",
    timestamp: "2026-07-10T00:00:03.000Z",
    transcript_path: "/Users/example/.claude/projects/raw-stop-transcript.jsonl",
  }),
  JSON.stringify({
    type: "user",
    sessionId: "cc-transcript-s1",
    uuid: "user-turn-2",
    timestamp: "2026-07-10T00:00:04.000Z",
    message: {
      role: "user",
      content: "RAW_MULTI_TURN_PROMPT_2",
    },
  }),
  JSON.stringify({
    type: "assistant",
    sessionId: "cc-transcript-s1",
    uuid: "assistant-turn-2",
    parent_uuid: "user-turn-2",
    timestamp: "2026-07-10T00:00:05.000Z",
    message: {
      id: "assistant-turn-2",
      model: "claude-test",
      usage: {
        input_tokens: 12,
        output_tokens: 6,
      },
      content: [
        {
          type: "text",
          text: "RAW_MULTI_TURN_OUTPUT_2",
        },
      ],
    },
  }),
].join("\n");

test("parses Claude Code hook and transcript JSONL into shared schema spans", () => {
  const events = parseClaudeCodeJsonl(CLAUDE_CODE_JSONL);
  const records = claudeCodeRecordsFromEvents(events, { project_name: "agent-observability" });

  const session = records.find((record) => record.span_kind === "agent.session");
  const turn = records.find((record) => record.span_kind === "turn");
  const llm = records.find((record) => record.span_kind === "llm.request");
  const tools = records.filter((record) => record.span_kind === "tool.execution");
  const permission = records.find((record) => record.span_kind === "permission");
  const compaction = records.find((record) => record.span_kind === "compaction");

  assert.equal(records.length, 7);
  assert.ok(session);
  assert.ok(turn);
  assert.ok(llm);
  assert.equal(tools.length, 2);
  assert.ok(permission);
  assert.ok(compaction);
  assert.equal(turn.parent_span_id, session.span_id);
  assert.equal(llm.parent_span_id, turn.span_id);
  assert.equal(tools[0].parent_span_id, turn.span_id);
  assert.equal(permission.parent_span_id, turn.span_id);
  assert.equal(compaction.parent_span_id, turn.span_id);
  assert.equal(llm.metrics.input_tokens, 120);
  assert.equal(llm.metrics.output_tokens, 34);
  assert.equal(llm.metrics.cached_input_tokens, 9);
  assert.equal(llm.metrics.cache_creation_input_tokens, 3);
  assert.equal(tools[0].attributes.tool_name, "Bash");
  assert.equal(tools[0].attributes.phase, "call");
  assert.equal(tools[1].attributes.tool_name, "Bash");
  assert.equal(tools[1].attributes.phase, "finish");
  assert.equal(tools[1].metrics.duration_ms, 42);
  assert.equal(permission.attributes.decision, "denied");
  assert.equal(permission.status.code, "error");
  assert.equal(compaction.metrics.input_tokens_before, 120000);
  assert.equal(compaction.metrics.input_tokens_after, 64000);

  const serialized = JSON.stringify(records);
  assert.equal(serialized.includes("RAW_PROMPT_SECRET"), false);
  assert.equal(serialized.includes("RAW_ASSISTANT_SECRET"), false);
  assert.equal(serialized.includes("RAW_TOOL_INPUT_SECRET"), false);
  assert.equal(serialized.includes("RAW_STDOUT_SECRET"), false);
  assert.equal(serialized.includes("RAW_STDERR_SECRET"), false);
  assert.equal(serialized.includes("RAW_PERMISSION_COMMAND_SECRET"), false);
  assert.equal(serialized.includes("raw-transcript.jsonl"), false);
});

test("keeps transcript-only turns separate and ignores stop as an LLM span", () => {
  const events = parseClaudeCodeJsonl(TRANSCRIPT_ONLY_MULTI_TURN_JSONL);
  const records = claudeCodeRecordsFromEvents(events, { project_name: "agent-observability" });

  const turns = records.filter((record) => record.span_kind === "turn");
  const llms = records.filter((record) => record.span_kind === "llm.request");

  assert.equal(records.length, 5);
  assert.equal(turns.length, 2);
  assert.equal(llms.length, 2);
  assert.equal(turns[0].span_id.endsWith(":user-turn-1"), true);
  assert.equal(turns[1].span_id.endsWith(":user-turn-2"), true);
  assert.equal(llms[0].parent_span_id, turns[0].span_id);
  assert.equal(llms[1].parent_span_id, turns[1].span_id);
  assert.equal(llms[0].metrics.input_tokens, 10);
  assert.equal(llms[1].metrics.input_tokens, 12);

  const serialized = JSON.stringify(records);
  assert.equal(serialized.includes("RAW_MULTI_TURN_PROMPT_1"), false);
  assert.equal(serialized.includes("RAW_MULTI_TURN_OUTPUT_1"), false);
  assert.equal(serialized.includes("RAW_MULTI_TURN_PROMPT_2"), false);
  assert.equal(serialized.includes("RAW_MULTI_TURN_OUTPUT_2"), false);
  assert.equal(serialized.includes("raw-stop-transcript.jsonl"), false);
});

test("normalizes Claude Code hook payloads without raw tool output", () => {
  const records = normalizeClaudeCodeHookPayload({
    hook_event_name: "PostToolUse",
    session_id: "cc-s2",
    tool_use_id: "toolu_2",
    tool_name: "Read",
    duration_ms: 9,
    tool_response: {
      content: "raw output should not be copied",
      is_error: false,
    },
  });

  const session = records.find((record) => record.span_kind === "agent.session");
  const turn = records.find((record) => record.span_kind === "turn");
  const tool = records.find((record) => record.span_kind === "tool.execution");

  assert.equal(records.length, 3);
  assert.equal(turn.parent_span_id, session.span_id);
  assert.equal(tool.parent_span_id, turn.span_id);
  assert.equal(tool.attributes.tool_name, "Read");
  assert.equal(tool.attributes.phase, "finish");
  assert.equal(tool.metrics.duration_ms, 9);
  assert.equal(JSON.stringify(records).includes("raw output should not be copied"), false);
});

test("appends Claude Code spans to the local event log without raw content", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-claude-code-"));
  const logPath = join(dir, "events.jsonl");

  const written = await appendClaudeCodeJsonl(logPath, CLAUDE_CODE_JSONL, {
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

  assert.equal(written.length, 7);
  assert.equal(records.length, 7);
  assert.equal(records.some((record) => record.span_kind === "permission"), true);
  assert.equal(records.some((record) => record.span_kind === "compaction"), true);
  assert.equal(raw.includes("RAW_PROMPT_SECRET"), false);
  assert.equal(raw.includes("RAW_ASSISTANT_SECRET"), false);
  assert.equal(raw.includes("RAW_TOOL_INPUT_SECRET"), false);
  assert.equal(raw.includes("RAW_STDOUT_SECRET"), false);
  assert.equal(raw.includes("RAW_STDERR_SECRET"), false);
  assert.equal(raw.includes("RAW_PERMISSION_COMMAND_SECRET"), false);
});
