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

const FIXTURE_ROOT = new URL("./fixtures/golden/", import.meta.url);

test("keeps Codex and Claude Code durable and report contracts in golden parity", async () => {
  const [codexSource, claudeSource, expectedText] = await Promise.all([
    readFile(new URL("codex-source.jsonl", FIXTURE_ROOT), "utf8"),
    readFile(new URL("claude-code-source.jsonl", FIXTURE_ROOT), "utf8"),
    readFile(new URL("cross-agent-contract.json", FIXTURE_ROOT), "utf8"),
  ]);
  const expected = JSON.parse(expectedText);
  const codex = codexRecordsFromEvents(parseCodexSessionJsonl(codexSource), {
    project_name: "golden-project",
  });
  const claudeCode = claudeCodeRecordsFromEvents(parseClaudeCodeJsonl(claudeSource), {
    project_name: "golden-project",
  });

  assert.deepEqual(durableContract(codex), expected.durable.codex);
  assert.deepEqual(durableContract(claudeCode), expected.durable.claude_code);
  assert.deepEqual(codex, expected.durable_full.codex);
  assert.deepEqual(claudeCode, expected.durable_full.claude_code);

  const report = reportDataFromRecords([...codex, ...claudeCode], {
    generated_at: "2026-08-01T01:00:00.000Z",
    rate_table: goldenRateTable(),
  });
  assert.deepEqual(reportContract(report), expected.report);
  assert.deepEqual(JSON.parse(JSON.stringify(report)), expected.report_full);
  assert.equal(JSON.stringify({ codex, claudeCode, report }).includes("RAW_GOLDEN_"), false);
});

function durableContract(records) {
  return records.map((record) => ({
    span_kind: record.span_kind,
    span_id: record.span_id,
    parent_span_id: record.parent_span_id,
    agent_name: record.agent.name,
    model: record.agent.model ?? null,
    session_id: record.attributes.session_id,
    turn_id: record.attributes.turn_id ?? null,
    request_id: record.attributes.request_id ?? null,
    start_time_unix_ms: record.start_time_unix_ms,
    metrics: record.metrics,
  }));
}

function reportContract(report) {
  return {
    schemaVersion: report.schemaVersion,
    sessions: report.summary.sessions,
    turns: report.summary.turns,
    llmRequests: report.summary.llmRequests,
    inputTokens: report.summary.inputTokens,
    outputTokens: report.summary.outputTokens,
    cachedInputTokens: report.summary.cachedInputTokens,
    cacheCreationInputTokens: report.summary.cacheCreationInputTokens,
    reasoningOutputTokens: report.summary.reasoningOutputTokens,
    estimatedCost: report.summary.estimatedCost,
    costStatus: report.cost.status,
    sessionsFilter: report.filters.sessions,
    turnsFilter: report.filters.turns,
    traceIds: report.traces.map((trace) => trace.traceId),
    spanAgents: report.spans.map((span) => span.agent.name),
  };
}

function goldenRateTable() {
  return {
    version: "golden-rates",
    currency: "USD",
    unit: "per_1m_tokens",
    assumption: "Golden fixture rates.",
    models: {
      "gpt-golden": {
        input_tokens: 2,
        output_tokens: 8,
        cached_input_tokens: 1,
        reasoning_output_tokens: 20,
        token_semantics: {
          cached_input_tokens: "included_in_total",
          reasoning_output_tokens: "included_in_total",
        },
      },
      "claude-golden": {
        input_tokens: 3,
        output_tokens: 9,
        cached_input_tokens: 1,
        cache_creation_input_tokens: 4,
        token_semantics: {
          cached_input_tokens: "included_in_total",
          cache_creation_input_tokens: "included_in_total",
        },
      },
    },
  };
}
