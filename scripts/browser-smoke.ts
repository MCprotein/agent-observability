import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { access, chmod, copyFile, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { promisify } from "node:util";
import { chromium } from "playwright-core";

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
  for (const [command, sourceFixture] of [
    ["codex-ingest", "crates/adapter-codex/tests/fixtures/codex-handoff.jsonl"],
    ["claude-code-ingest", "crates/adapter-claude-code/tests/fixtures/claude-handoff.jsonl"],
    ["cursor-ingest", "crates/adapter-cursor/tests/fixtures/cursor-handoff.jsonl"],
  ]) {
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
    assert.equal(await page.locator("h2").count(), 3);
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
