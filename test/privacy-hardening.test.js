import assert from "node:assert/strict";
import test from "node:test";
import { mkdtemp, readFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  appendEventLog,
  createSpanRecord,
  readEventLog,
  redactedSnapshotFromRecords,
  renderStaticHtmlReport,
  writeRedactedJsonSnapshot,
} from "../src/index.js";

const RAW_PROMPT = "RAW_PROMPT_SHOULD_NOT_LEAK";
const RAW_OUTPUT = "RAW_OUTPUT_SHOULD_NOT_LEAK";
const RAW_TOOL_OUTPUT = "RAW_TOOL_OUTPUT_SHOULD_NOT_LEAK";
const RAW_TOKEN = "RAW_TOKEN_SHOULD_NOT_LEAK";
const RAW_BEARER = "RAW_BEARER_SHOULD_NOT_LEAK";
const RAW_REFRESH = "RAW_REFRESH_SHOULD_NOT_LEAK";
const RAW_PRIVATE_KEY = [
  "-----BEGIN PRIVATE KEY-----",
  "RAW_PRIVATE_KEY_SHOULD_NOT_LEAK",
  "-----END PRIVATE KEY-----",
].join("\n");

const SENTINELS = [
  RAW_PROMPT,
  RAW_OUTPUT,
  RAW_TOOL_OUTPUT,
  RAW_TOKEN,
  RAW_BEARER,
  RAW_REFRESH,
  "RAW_PRIVATE_KEY_SHOULD_NOT_LEAK",
  "/repo/.env",
  "/repo/id_rsa.key",
];

function privacyFixtureSpan() {
  return createSpanRecord({
    trace_id: "privacy-trace",
    span_id: "privacy-turn",
    span_kind: "turn",
    name: `${RAW_PROMPT} ${RAW_OUTPUT}`,
    status: "ok",
    content: {
      prompt: RAW_PROMPT,
      output: RAW_OUTPUT,
      tool_input: {
        command: `cat /repo/.env && echo token=${RAW_TOKEN}`,
      },
      tool_output: RAW_TOOL_OUTPUT,
    },
    attributes: {
      turn_id: "turn-privacy",
      command: `cat /repo/.env && echo token=${RAW_TOKEN}`,
      note: `Authorization: Bearer ${RAW_BEARER}`,
      nested: {
        refreshToken: RAW_REFRESH,
      },
      private_key: RAW_PRIVATE_KEY,
      key_file: "/repo/id_rsa.key",
      harmless: "kept",
    },
  });
}

function workstreamSpan() {
  return createSpanRecord({
    trace_id: "privacy-trace",
    span_id: "privacy-workstream",
    span_kind: "workstream",
    name: `Workstream ${RAW_PROMPT} ${RAW_OUTPUT}`,
    status: "ok",
    content: {
      prompt: RAW_PROMPT,
      output: RAW_OUTPUT,
    },
  });
}

test("keeps raw prompt, output, and secrets out of local log, report, and export", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-privacy-"));
  const logPath = join(dir, "events.jsonl");
  const exportPath = join(dir, "snapshot.json");
  const span = privacyFixtureSpan();
  const workstream = workstreamSpan();

  const sanitized = await appendEventLog(logPath, span);
  const rawLog = await readFile(logPath, "utf8");
  const logRecords = await readEventLog(logPath);

  assert.equal(sanitized.name, "Turn turn-privacy");
  assert.equal(logRecords[0].content.prompt, "[content omitted]");
  assert.equal(logRecords[0].content.output, "[content omitted]");
  assert.equal(logRecords[0].content.tool_input, "[content omitted]");
  assert.equal(logRecords[0].content.tool_output, "[content omitted]");
  assert.equal(logRecords[0].attributes.nested.refreshToken, "[redacted]");
  assert.equal(logRecords[0].attributes.private_key, "[redacted]");
  assert.equal(logRecords[0].attributes.key_file, "[redacted path]");
  assert.equal(logRecords[0].attributes.harmless, "kept");
  assertNoSentinels(rawLog);

  const html = renderStaticHtmlReport([span, workstream], {
    title: "Privacy Report",
    generated_at: "2026-07-10T00:00:00.000Z",
  });
  assertNoSentinels(html);

  const snapshot = redactedSnapshotFromRecords([span, workstream], {
    generated_at: "2026-07-10T00:00:00.000Z",
    content_logging: {
      prompts: true,
      outputs: true,
      tool_inputs: true,
      tool_outputs: true,
    },
  });
  assertNoSentinels(JSON.stringify(snapshot));

  await writeRedactedJsonSnapshot(exportPath, [span, workstream], {
    generated_at: "2026-07-10T00:00:00.000Z",
    content_logging: {
      prompts: true,
      outputs: true,
      tool_inputs: true,
      tool_outputs: true,
    },
  });
  assertNoSentinels(await readFile(exportPath, "utf8"));
});

function assertNoSentinels(text) {
  for (const sentinel of SENTINELS) {
    assert.equal(text.includes(sentinel), false, `${sentinel} leaked`);
  }
}
