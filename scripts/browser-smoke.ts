import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { access, chmod, copyFile, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { promisify } from "node:util";
import { chromium } from "playwright-core";
import type {
  AgentObservabilityReportV1,
  Span,
  Trace,
} from "../ui/report/generated/report-dto-v1.js";

const execute = promisify(execFile);

const executablePath = chromium.executablePath();
try {
  await access(executablePath);
} catch {
  throw new Error("Pinned Chromium is missing; run npm run setup:browser.");
}
const directory = await mkdtemp(join(tmpdir(), "agent-observability-browser-"));
const runtimeRoot = join(directory, "runtime");
const reportPath = join(runtimeRoot, "logs", "agent-observability-report.html");
const rateTablePath = join(directory, "rate-table.json");
const browser = await chromium.launch({ executablePath, headless: true });

try {
  await writeFile(rateTablePath, `${JSON.stringify(rateTable(), null, 2)}\n`, { mode: 0o600 });
  await execute("cargo", ["build", "-q", "-p", "agent-observability-cli"]);
  const binary = join(process.cwd(), "target", "debug", "agent-observability");
  const sources: ReadonlyArray<readonly [string, string]> = [
    ["codex-ingest", "crates/adapter-codex/tests/fixtures/codex-handoff.jsonl"],
    ["claude-code-ingest", "crates/adapter-claude-code/tests/fixtures/claude-handoff.jsonl"],
    ["cursor-ingest", "crates/adapter-cursor/tests/fixtures/cursor-handoff.jsonl"],
  ];
  for (const [command, sourceFixture] of sources) {
    const fixturePath = join(directory, `${command}.jsonl`);
    await copyFile(sourceFixture, fixturePath);
    await chmod(fixturePath, 0o600);
    await execute(binary, [command, runtimeRoot, fixturePath]);
  }
  const { stdout } = await execute(binary, ["report", runtimeRoot, rateTablePath]);
  assert.match(stdout, /cost_status=(estimated|incomplete)/);

  const html = await readFile(reportPath, "utf8");
  for (const sentinel of [
    "RAW_PROMPT_SECRET",
    "RAW_TOOL_OUTPUT_SECRET",
    "RAW_ASSISTANT_SECRET",
    "RAW_RESPONSE_SECRET",
    "RAW_COMMAND_SECRET",
    "RAW_EMAIL",
    "RAW_PATH",
    "RAW_OUTPUT",
    "RAW_MCP",
    "RAW_UNOWNED_MODEL",
  ]) {
    assert.equal(html.includes(sentinel), false, `report leaked ${sentinel}`);
  }
  const results = [];
  for (const testCase of [
    { name: "desktop", viewport: { width: 1440, height: 900 } },
    { name: "mobile", viewport: { width: 375, height: 812 } },
  ]) {
    const page = await browser.newPage({ viewport: testCase.viewport });
    const consoleErrors: string[] = [];
    const externalRequests: string[] = [];
    const failedRequests: string[] = [];
    page.on("console", (message) => {
      if (message.type() === "error") consoleErrors.push(message.text());
    });
    page.on("request", (request) => {
      if (!request.url().startsWith("file://")) externalRequests.push(request.url());
    });
    page.on("requestfailed", (request) => failedRequests.push(request.url()));
    await page.goto(pathToFileURL(reportPath).href);

    assert.equal(await page.locator("h1").textContent(), "Agent Observability Report");
    assert.equal(await page.locator("h2").count(), 4);
    assert.equal(await page.locator('[aria-labelledby="timeline-heading"]').count(), 1);
    assert.equal(await page.locator('[aria-labelledby="traces-heading"]').count(), 1);
    assert.equal(await page.locator('[aria-labelledby="spans-heading"]').count(), 1);
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth),
      true,
    );

    await page.keyboard.press("Tab");
    assert.equal(await page.evaluate(() => document.activeElement?.id), "repo-filter");
    await page.locator("#model-filter").selectOption({ label: "gpt-test" });
    assert.match((await page.locator("#filter-status").textContent()) ?? "", /^[1-9]\d* spans match the active filters\.$/);
    assert.equal(await page.locator("#span-table tr").count() <= 200, true);
    await page.locator("#save-filter").click();
    assert.equal(await page.locator("#saved-filter option").count(), 2);
    const storedView = await page.evaluate(() =>
      localStorage.getItem("agent-observability.report.v1.saved-filters"),
    );
    assert.deepEqual(JSON.parse(storedView ?? "null"), {
      version: 1,
      filters: [{ model: "gpt-test" }],
    });
    await page.reload();
    assert.equal(await page.locator("#saved-filter option").count(), 2);
    await page.locator("#saved-filter").selectOption("0");
    assert.equal(await page.locator("#model-filter option:checked").textContent(), "gpt-test");
    await page.locator("#delete-filter").click();
    assert.equal(await page.locator("#saved-filter option").count(), 1);
    await page.locator(".trace-row:visible").first().click();
    assert.equal(await page.locator(".trace-row:visible").first().getAttribute("aria-pressed"), "true");
    assert.equal(await page.locator(".timeline-row").count() > 0, true);
    await page.locator("#span-table .span-open:visible").first().click();
    assert.match((await page.locator("#span-details").textContent()) ?? "", /Source & privacy/);
    assert.match((await page.locator("#span-details").textContent()) ?? "", /Location/);
    assert.match(
      (await page.locator("#private-detail").textContent()) ?? "",
      /Unavailable in a file:\/\/ report\. Open the localhost dashboard\./,
    );

    if (testCase.name === "mobile") {
      const heights = await page.locator("select, input, button").evaluateAll((elements) =>
        elements.map((element) => element.getBoundingClientRect().height),
      );
      assert.equal(heights.every((height) => height >= 44), true);
    }

    const screenshotPath = join(directory, `${testCase.name}.png`);
    await page.screenshot({ path: screenshotPath, fullPage: true });
    assert.equal((await stat(screenshotPath)).size > 0, true);
    assert.deepEqual(consoleErrors, []);
    assert.deepEqual(externalRequests, []);
    assert.deepEqual(failedRequests, []);
    results.push({ name: testCase.name, overflow: false, consoleErrors: 0, externalRequests: 0 });
    await page.close();
  }

  const largeReportPath = join(directory, "large-report.html");
  const largeReport = largeReportFixture();
  const reportShell = await readFile("src/report/generated/report-shell.html", "utf8");
  await writeFile(largeReportPath, renderReportDto(reportShell, largeReport), { mode: 0o600 });
  const largePage = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const largeConsoleErrors: string[] = [];
  const largeExternalRequests: string[] = [];
  const largeFailedRequests: string[] = [];
  largePage.on("console", (message) => {
    if (message.type() === "error") largeConsoleErrors.push(message.text());
  });
  largePage.on("request", (request) => {
    if (!request.url().startsWith("file://")) largeExternalRequests.push(request.url());
  });
  largePage.on("requestfailed", (request) => largeFailedRequests.push(request.url()));
  await largePage.goto(pathToFileURL(largeReportPath).href);
  assert.equal(await largePage.locator("#span-count").textContent(), "4096");
  assert.equal(await largePage.locator("#span-table tr").count(), 200);
  assert.equal(await largePage.locator("#trace-list .trace-row").count(), 100);
  assert.equal(await largePage.locator("#session-filter option").count(), 502);
  await largePage.locator("#trace-list .trace-row").first().click();
  assert.equal(await largePage.locator(".timeline-row").count(), 120);
  await largePage.locator("#span-table .span-open").first().click();
  assert.match((await largePage.locator("#span-details").textContent()) ?? "", /Private local detail/);
  assert.deepEqual(largeConsoleErrors, []);
  assert.deepEqual(largeExternalRequests, []);
  assert.deepEqual(largeFailedRequests, []);
  results.push({ name: "large", spans: 4_096, traceRows: 100, spanRows: 200, timelineRows: 120 });
  await largePage.close();
  console.log(JSON.stringify({ executablePath, results }));
} finally {
  await browser.close();
  await rm(directory, { recursive: true, force: true });
}

function rateTable() {
  return {
    schema_version: "agent_observability.rate_table.v1",
    version: "browser-smoke",
    currency: "USD",
    unit: "per_1m_tokens",
    assumption: "Browser smoke fixture.",
    models: {
      "gpt-test": { input_tokens: 2, output_tokens: 8, reasoning_output_tokens: 8 },
      "claude-sonnet-5": {
        input_tokens: 3,
        output_tokens: 15,
        cached_input_tokens: 0.3,
        cache_creation_input_tokens: 3.75,
      },
      "cursor-test": { input_tokens: 2, output_tokens: 8 },
    },
  };
}

function renderReportDto(shell: string, report: AgentObservabilityReportV1): string {
  return shell
    .replaceAll("__AGENT_OBSERVABILITY_REPORT_TITLE__", report.title)
    .replaceAll("__AGENT_OBSERVABILITY_REPORT_GENERATED_AT__", report.generatedAt)
    .replace(
      "__AGENT_OBSERVABILITY_REPORT_DATA__",
      JSON.stringify(report).replaceAll("<", "\\u003c"),
    );
}

function largeReportFixture(): AgentObservabilityReportV1 {
  const spans: Span[] = Array.from({ length: 4_096 }, (_, index) => ({
    schemaVersion: "agent_observability.v1",
    traceId: index < 200 ? "trace-large-0" : `trace-large-${1 + (index % 255)}`,
    spanId: `span-large-${index}`,
    parentSpanId: null,
    kind: "tool.execution",
    name: `operation-${index}`,
    status: index % 31 === 0 ? "error" : "ok",
    startTimeUnixMs: 1_000 + index * 10,
    endTimeUnixMs: 1_005 + index * 10,
    repo: "agent-observability",
    agent: { name: "codex", model: index % 2 === 0 ? "gpt-test" : "other-model" },
    availability: {
      repository: { state: "available", reason: "fixture" },
      turn: { state: "source_unavailable", reason: "fixture" },
      model: { state: "available", reason: "fixture" },
      tokens: { state: "source_unavailable", reason: "fixture" },
      latency: { state: "available", reason: "fixture" },
      sourceLocation: { state: "private_lookup", reason: "fixture" },
      requestContent: { state: "private_lookup", reason: "fixture" },
      responseContent: { state: "private_lookup", reason: "fixture" },
    },
    sessionId: `session-${index % 501}`,
    toolName: "exec_command",
    attributes: { session_id: `session-${index % 501}`, tool_name: "exec_command" },
    metrics: { durationMs: 5 },
    cost: { status: "unknown", rate_table: {}, cost: { assumption: "fixture" } },
  }));
  const traceIds = [...new Set(spans.map((span) => span.traceId))];
  const traces: Trace[] = traceIds.map((traceId) => {
    const traceSpans = spans.filter((span) => span.traceId === traceId);
    const first = traceSpans[0];
    const last = traceSpans.at(-1);
    if (!first || !last) throw new Error(`empty trace fixture: ${traceId}`);
    return {
      traceId,
      repo: "agent-observability",
      spans: traceSpans.length,
      errors: traceSpans.filter((span) => span.status === "error").length,
      inputTokens: 0,
      outputTokens: 0,
      estimatedCost: 0,
      startTimeUnixMs: first.startTimeUnixMs,
      endTimeUnixMs: last.endTimeUnixMs,
      sessions: [...new Set(traceSpans.flatMap((span) => span.sessionId ?? []))],
      turns: [],
    };
  });
  return {
    schemaVersion: "agent_observability.report.v1",
    generatedAt: "2026-07-10T00:00:00.000Z",
    title: "Agent Observability Report",
    summary: {
      generatedSpans: spans.length,
      sessions: 501,
      turns: 0,
      llmRequests: 0,
      toolExecutions: spans.length,
      errors: spans.filter((span) => span.status === "error").length,
      inputTokens: 0,
      outputTokens: 0,
      cachedInputTokens: 0,
      cacheCreationInputTokens: 0,
      reasoningOutputTokens: 0,
      latencyMs: 0,
      durationMs: spans.length * 5,
      estimatedCost: 0,
    },
    cost: { status: "unknown", rate_table: {}, cost: { assumption: "fixture" } },
    filters: {
      repos: ["agent-observability"],
      sessions: Array.from({ length: 501 }, (_, index) => `session-${index}`),
      turns: [],
      agents: ["codex"],
      models: ["gpt-test", "other-model"],
    },
    traces,
    spans,
  };
}
