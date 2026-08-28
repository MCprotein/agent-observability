import assert from "node:assert/strict";
import { access, mkdtemp, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { chromium } from "playwright-core";
import { createSpanRecord, writeStaticHtmlReport } from "../src/index.js";

const executablePath = chromium.executablePath();
try {
  await access(executablePath);
} catch {
  throw new Error("Pinned Chromium is missing; run npm run setup:browser.");
}
const directory = await mkdtemp(join(tmpdir(), "agent-observability-browser-"));
const reportPath = join(directory, "report.html");
const browser = await chromium.launch({ executablePath, headless: true });

try {
  await writeStaticHtmlReport(reportPath, fixture(), {
    title: "Local Agent Observability",
    generated_at: "2026-08-28T00:00:00.000Z",
    rate_table: rateTable(),
  });
  const results = [];
  for (const testCase of [
    { name: "desktop", viewport: { width: 1440, height: 900 } },
    { name: "mobile", viewport: { width: 375, height: 812 } },
  ]) {
    const page = await browser.newPage({ viewport: testCase.viewport });
    const consoleErrors = [];
    const externalRequests = [];
    page.on("console", (message) => {
      if (message.type() === "error") consoleErrors.push(message.text());
    });
    page.on("request", (request) => {
      if (!request.url().startsWith("file://")) externalRequests.push(request.url());
    });
    await page.goto(pathToFileURL(reportPath).href);

    assert.equal(await page.locator("h1").textContent(), "Local Agent Observability");
    assert.equal(await page.locator("h2").count(), 2);
    assert.equal(await page.locator('[aria-labelledby="traces-heading"]').count(), 1);
    assert.equal(await page.locator('[aria-labelledby="spans-heading"]').count(), 1);
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth),
      true,
    );

    await page.keyboard.press("Tab");
    assert.equal(await page.evaluate(() => document.activeElement?.id), "repo-filter");
    await page.locator("#model-filter").selectOption({ label: "gpt-test" });
    assert.equal(await page.locator("#filter-status").textContent(), "1 spans match the active filters.");
    await page.locator(".trace-row").click();
    assert.equal(await page.locator(".trace-row").getAttribute("aria-pressed"), "true");

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
    results.push({ name: testCase.name, overflow: false, consoleErrors: 0, externalRequests: 0 });
    await page.close();
  }
  console.log(JSON.stringify({ executablePath, results }));
} finally {
  await browser.close();
  await rm(directory, { recursive: true, force: true });
}

function fixture() {
  const session = createSpanRecord({
    trace_id: "browser-trace",
    span_id: "browser-session",
    span_kind: "agent.session",
    name: "Session",
    status: "ok",
    agent: { name: "codex" },
    project: { name: "agent-observability" },
    attributes: { session_id: "browser-session" },
  });
  const llm = createSpanRecord({
    trace_id: "browser-trace",
    span_id: "browser-llm",
    parent_span_id: session.span_id,
    span_kind: "llm.request",
    name: "LLM request",
    status: "ok",
    agent: { name: "codex", model: "gpt-test" },
    project: { name: "agent-observability" },
    metrics: { input_tokens: 12, output_tokens: 3, latency_ms: 20 },
    attributes: { session_id: "browser-session", request_id: "browser-request" },
  });
  return [session, llm];
}

function rateTable() {
  return {
    version: "browser-smoke",
    currency: "USD",
    unit: "per_1m_tokens",
    assumption: "Browser smoke fixture.",
    models: { "gpt-test": { input_tokens: 2, output_tokens: 8 } },
  };
}
