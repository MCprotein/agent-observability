import assert from "node:assert/strict";
import test from "node:test";
import { mkdtemp, readFile, stat } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { Script, createContext } from "node:vm";
import {
  createSpanRecord,
  reportDataFromRecords,
  renderStaticHtmlReport,
  writeStaticHtmlReport,
} from "../src/index.js";

function reportFixture() {
  const session = createSpanRecord({
    trace_id: "trace-report-1",
    span_id: "codex-session:session-report-1",
    span_kind: "agent.session",
    name: "Codex session",
    status: "ok",
    agent: { name: "codex", model: "gpt-test" },
    project: { name: "agent-observability", repo_path: "/private/repo/agent-observability" },
    attributes: { session_id: "session-report-1" },
  });

  const turn = createSpanRecord({
    trace_id: "trace-report-1",
    span_id: "codex-turn:session-report-1:turn-report-1",
    parent_span_id: session.span_id,
    span_kind: "turn",
    name: "Codex turn </script><img src=x>",
    status: "ok",
    project: { name: "agent-observability" },
    attributes: { session_id: "session-report-1", turn_id: "turn-report-1" },
    content: {
      prompt: "RAW_PROMPT_SECRET",
      output: "RAW_OUTPUT_SECRET",
    },
  });

  const llm = createSpanRecord({
    trace_id: "trace-report-1",
    span_id: "codex-llm:session-report-1:turn-report-1:r1",
    parent_span_id: turn.span_id,
    span_kind: "llm.request",
    name: "Codex LLM gpt-test",
    status: "ok",
    agent: { name: "codex", model: "gpt-test" },
    project: { name: "agent-observability" },
    metrics: {
      input_tokens: 42,
      output_tokens: 17,
      cached_input_tokens: 6,
      reasoning_output_tokens: 3,
      latency_ms: 1200,
    },
    attributes: { session_id: "session-report-1", turn_id: "turn-report-1", request_id: "r1" },
  });

  const tool = createSpanRecord({
    trace_id: "trace-report-1",
    span_id: "codex-tool:session-report-1:turn-report-1:call-1:output",
    parent_span_id: turn.span_id,
    span_kind: "tool.execution",
    name: "exec_command",
    status: "error",
    project: { name: "agent-observability" },
    metrics: { duration_ms: 35 },
    attributes: {
      session_id: "session-report-1",
      turn_id: "turn-report-1",
      call_id: "call-1",
      tool_name: "exec_command",
      phase: "output",
    },
  });

  return [session, turn, llm, tool];
}

test("builds report data with summaries and without raw content", () => {
  const data = reportDataFromRecords(reportFixture(), {
    generated_at: "2026-07-10T00:00:00.000Z",
    rate_table: reportRateTable(),
  });

  assert.equal(data.summary.sessions, 1);
  assert.equal(data.summary.turns, 1);
  assert.equal(data.summary.llmRequests, 1);
  assert.equal(data.summary.toolExecutions, 1);
  assert.equal(data.summary.errors, 1);
  assert.equal(data.summary.inputTokens, 42);
  assert.equal(data.summary.outputTokens, 17);
  assert.equal(data.summary.estimatedCost, 0.00025);
  assert.equal(data.cost.status, "estimated");
  assert.equal(data.cost.rate_table.version, "report-test");
  assert.deepEqual(data.filters.repos, ["agent-observability"]);
  assert.equal(data.filters.sessions.length, 1);
  assert.match(data.filters.sessions[0], /^id:sha256:[a-f0-9]{64}$/);
  assert.equal(data.filters.turns.length, 1);
  assert.match(data.filters.turns[0], /^id:sha256:[a-f0-9]{64}$/);

  const serialized = JSON.stringify(data);
  assert.equal(serialized.includes("RAW_PROMPT_SECRET"), false);
  assert.equal(serialized.includes("RAW_OUTPUT_SECRET"), false);
  assert.equal(serialized.includes("RAW_ARGUMENT_SECRET"), false);
  assert.equal(serialized.includes("/private/repo"), false);
});

test("marks report cost unknown when no rate table is supplied", () => {
  const data = reportDataFromRecords(reportFixture(), {
    generated_at: "2026-07-10T00:00:00.000Z",
  });

  assert.equal(data.cost.status, "unknown");
  assert.equal(data.cost.reason, "missing_rate_table");
  assert.equal(data.summary.estimatedCost, 0);
});

test("renders a self-contained static HTML report", () => {
  const html = renderStaticHtmlReport(reportFixture(), {
    title: "Agent Report",
    generated_at: "2026-07-10T00:00:00.000Z",
    rate_table: reportRateTable(),
  });

  assert.equal(html.startsWith("<!doctype html>"), true);
  assert.equal(html.includes('<script id="report-data" type="application/json">'), true);
  assert.equal(html.includes("Agent Report"), true);
  assert.equal(html.includes("RAW_PROMPT_SECRET"), false);
  assert.equal(html.includes("RAW_OUTPUT_SECRET"), false);
  assert.equal(html.includes("RAW_ARGUMENT_SECRET"), false);
  assert.equal(html.includes("</script><img"), false);
  assert.equal(/https?:\/\//.test(html), false);
  assert.equal(/<(script|link|img|iframe)\b[^>]+\s(src|href)=/i.test(html), false);
});

test("writes an HTML report file and executes the inline renderer", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-report-"));
  const reportPath = join(dir, "report.html");

  const result = await writeStaticHtmlReport(reportPath, reportFixture(), {
    title: "Local Agent Report",
    generated_at: "2026-07-10T00:00:00.000Z",
    rate_table: reportRateTable(),
  });
  const html = await readFile(reportPath, "utf8");

  assert.equal(result.filePath, reportPath);
  assert.equal(result.bytes, Buffer.byteLength(html, "utf8"));
  assert.equal((await stat(reportPath)).mode & 0o777, 0o600);
  assert.equal(html.includes("<title>Local Agent Report</title>"), true);
  assert.equal(html.includes('id="report-data"'), true);
  assert.equal(new URL(`file://${reportPath}`).protocol, "file:");

  const dataJson = extractReportDataJson(html);
  const data = JSON.parse(dataJson);
  assert.equal(data.summary.inputTokens, 42);
  assert.equal(data.summary.outputTokens, 17);
  assert.equal(data.summary.estimatedCost, 0.00025);

  const dom = createReportDom(dataJson);
  new Script(extractRendererScript(html)).runInContext(createContext({ document: dom.document }));

  assert.equal(dom.element("kpi-sessions").textContent, "1");
  assert.equal(dom.element("kpi-turns").textContent, "1");
  assert.equal(dom.element("kpi-llm").textContent, "1");
  assert.equal(dom.element("kpi-tools").textContent, "1");
  assert.equal(dom.element("kpi-tokens").textContent, "59");
  assert.equal(dom.element("kpi-cost").textContent, "USD 0.00025");
  assert.equal(dom.element("kpi-errors").textContent, "1");
  assert.equal(dom.element("trace-list").children.length, 1);
  assert.equal(dom.element("span-table").children.length, 4);
  assert.equal(dom.element("span-table").innerHTML.includes("LLM gpt-test"), true);
  assert.equal(dom.element("span-table").innerHTML.includes("exec_command"), true);
});

test("renders incomplete cost status with the partial amount", async () => {
  const dir = await mkdtemp(join(tmpdir(), "agent-observability-report-incomplete-"));
  const reportPath = join(dir, "report.html");
  const partialRateTable = {
    ...reportRateTable(),
    models: {
      "gpt-test": {
        input_tokens: 2,
        token_semantics: {
          cached_input_tokens: "included_in_total",
          reasoning_output_tokens: "included_in_total",
        },
      },
    },
  };

  await writeStaticHtmlReport(reportPath, reportFixture(), {
    title: "Incomplete Cost Report",
    generated_at: "2026-07-10T00:00:00.000Z",
    rate_table: partialRateTable,
  });

  const html = await readFile(reportPath, "utf8");
  const dataJson = extractReportDataJson(html);
  const data = JSON.parse(dataJson);
  assert.equal(data.cost.status, "incomplete");
  assert.equal(data.cost.estimated_cost, 0.000072);

  const dom = createReportDom(dataJson);
  new Script(extractRendererScript(html)).runInContext(createContext({ document: dom.document }));

  assert.equal(dom.element("kpi-cost").textContent, "USD 0.000072 incomplete");
});

function extractReportDataJson(html) {
  const match = /<script id="report-data" type="application\/json">([\s\S]*?)<\/script>/.exec(html);
  assert.ok(match, "report data script should exist");
  return match[1];
}

function reportRateTable() {
  return {
    version: "report-test",
    currency: "USD",
    unit: "per_1m_tokens",
    assumption: "Fixture report rates.",
    models: {
      "gpt-test": {
        input_tokens: 2,
        output_tokens: 8,
        cached_input_tokens: 1,
        reasoning_output_tokens: 20,
        token_semantics: {
          cached_input_tokens: "included_in_total",
          reasoning_output_tokens: "included_in_total",
        },
      },
    },
  };
}

function extractRendererScript(html) {
  const scripts = [...html.matchAll(/<script(?![^>]*type="application\/json")[^>]*>([\s\S]*?)<\/script>/g)];
  assert.ok(scripts.length > 0, "inline renderer script should exist");
  return scripts.at(-1)[1];
}

function createReportDom(reportDataJson) {
  const elements = new Map();
  const ids = [
    "repo-filter",
    "session-filter",
    "turn-filter",
    "text-filter",
    "trace-list",
    "span-table",
    "trace-count",
    "span-count",
    "kpi-sessions",
    "kpi-turns",
    "kpi-llm",
    "kpi-tools",
    "kpi-tokens",
    "kpi-cost",
    "kpi-errors",
  ];

  for (const id of ids) {
    elements.set(id, new MiniElement(tagNameForId(id), id));
  }
  const reportData = new MiniElement("script", "report-data");
  reportData.textContent = reportDataJson;
  elements.set("report-data", reportData);

  const document = {
    createElement(tagName) {
      return new MiniElement(tagName);
    },
    getElementById(id) {
      assert.equal(elements.has(id), true, `missing DOM id ${id}`);
      return elements.get(id);
    },
  };

  return {
    document,
    element(id) {
      return document.getElementById(id);
    },
  };
}

function tagNameForId(id) {
  if (id.endsWith("-filter") && id !== "text-filter") {
    return "select";
  }
  if (id === "text-filter") {
    return "input";
  }
  if (id === "span-table") {
    return "tbody";
  }
  return "div";
}

class MiniElement {
  constructor(tagName, id = "") {
    this.tagName = tagName.toUpperCase();
    this.id = id;
    this.children = [];
    this.listeners = {};
    this.value = "";
    this.className = "";
    this.type = "";
    this._innerHTML = "";
    this.textContent = "";
  }

  set innerHTML(value) {
    this._innerHTML = String(value);
    this.children = [];
  }

  get innerHTML() {
    if (this.children.length > 0) {
      return this.children.map((child) => child.innerHTML || child.textContent).join("");
    }
    return this._innerHTML;
  }

  addEventListener(eventName, callback) {
    this.listeners[eventName] = callback;
  }

  replaceChildren(...children) {
    this.children = children;
    this._innerHTML = "";
    if (this.tagName === "SELECT") {
      this.value = children[0]?.value ?? "";
    }
  }
}
