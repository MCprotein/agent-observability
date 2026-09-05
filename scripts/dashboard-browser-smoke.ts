import assert from "node:assert/strict";
import { execFile, spawn, type ChildProcessByStdio } from "node:child_process";
import { createHash } from "node:crypto";
import { access, chmod, copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Readable } from "node:stream";
import { promisify } from "node:util";
import { chromium } from "playwright-core";

const execute = promisify(execFile);
const executablePath = chromium.executablePath();
await access(executablePath).catch(() => {
  throw new Error("Pinned Chromium is missing; run npm run setup:browser.");
});

const directory = await mkdtemp(join(tmpdir(), "agent-observability-dashboard-browser-"));
await chmod(directory, 0o700);
const runtimeRoot = join(directory, "runtime");
const binary = join(process.cwd(), "target", "debug", "agent-observability");
const browser = await chromium.launch({ executablePath, headless: true });
const browserContext = await browser.newContext();
let stableOrigin: string | undefined;

try {
  await execute("cargo", ["build", "-q", "-p", "agent-observability-cli"]);
  for (const [command, sourceFixture] of [
    ["codex-ingest", "crates/adapter-codex/tests/fixtures/codex-handoff.jsonl"],
    ["claude-code-ingest", "crates/adapter-claude-code/tests/fixtures/claude-handoff.jsonl"],
    ["cursor-ingest", "crates/adapter-cursor/tests/fixtures/cursor-handoff.jsonl"],
  ] as const) {
    const fixturePath = join(directory, command + ".jsonl");
    await copyFile(sourceFixture, fixturePath);
    await chmod(fixturePath, 0o600);
    await execute(binary, [command, runtimeRoot, fixturePath]);
  }
  await execute(binary, ["config", "set", runtimeRoot, "private-codex-details", "true"]);
  const privateNotify = JSON.stringify({
    type: "agent-turn-complete",
    "thread-id": "conversation-1",
    "turn-id": "turn-1",
    cwd: "/private/project",
    "input-messages": ["PRIVATE_BROWSER_REQUEST"],
    "last-assistant-message": "PRIVATE_BROWSER_RESPONSE",
  });
  const privateNotifyResult = await execute(binary, ["codex-notify", runtimeRoot, privateNotify]);
  assert.match(privateNotifyResult.stdout, /notify=unavailable/);
  const privateTurnDigest = createHash("sha256").update("turn-1").digest("hex");
  const privateDetailDirectory = join(runtimeRoot, "state", "private-codex-turn-details");
  await mkdir(privateDetailDirectory, { recursive: true, mode: 0o700 });
  const privateDetailPath = join(privateDetailDirectory, `${privateTurnDigest}.json`);
  await writeFile(
    privateDetailPath,
    JSON.stringify({
      schemaVersion: "agent_observability.private_turn_detail.v1",
      turnId: `id:sha256:${privateTurnDigest}`,
      cwd: "/private/project",
      inputMessages: ["PRIVATE_BROWSER_REQUEST"],
      lastAssistantMessage: "PRIVATE_BROWSER_RESPONSE",
    }),
    { mode: 0o600 },
  );
  await chmod(privateDetailPath, 0o600);
  const privateStatusDirectory = join(runtimeRoot, "state", "private-codex-turn-detail-statuses");
  await mkdir(privateStatusDirectory, { recursive: true, mode: 0o700 });
  const privateStatusPath = join(privateStatusDirectory, `${privateTurnDigest}.json`);
  await writeFile(
    privateStatusPath,
    JSON.stringify({
      schema_version: "private_codex_turn_detail_status.v1",
      turn_id: `id:sha256:${privateTurnDigest}`,
      state: "available",
      code: "ok",
    }),
    { mode: 0o600 },
  );
  await chmod(privateStatusPath, 0o600);
  await execute(binary, ["report", runtimeRoot]);
  const reportHtml = await readFile(
    join(runtimeRoot, "logs", "agent-observability-report.html"),
    "utf8",
  );
  assert.equal(reportHtml.includes("/private/project"), false);
  assert.equal(reportHtml.includes("PRIVATE_BROWSER_REQUEST"), false);
  assert.equal(reportHtml.includes("PRIVATE_BROWSER_RESPONSE"), false);

  const results = [];
  for (const testCase of [
    { name: "desktop", viewport: { width: 1440, height: 900 } },
    { name: "mobile", viewport: { width: 375, height: 812 } },
  ]) {
    const child = spawn(binary, ["dashboard", runtimeRoot, "--no-open"], {
      stdio: ["ignore", "pipe", "pipe"],
    });
    const stderr: string[] = [];
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk: string) => stderr.push(chunk));
    try {
      const dashboardUrl = await readUrl(child);
      const parsed = new URL(dashboardUrl);
      assert.equal(parsed.hostname, "127.0.0.1");
      assert.notEqual(parsed.port, "");
      assert.match(parsed.pathname, /^\/report\/[0-9a-f]{64}$/);
      stableOrigin ??= parsed.origin;
      assert.equal(parsed.origin, stableOrigin, "dashboard origin must survive process restarts");

      const page = await browserContext.newPage();
      await page.setViewportSize(testCase.viewport);
      const consoleErrors: string[] = [];
      const pageErrors: string[] = [];
      const externalRequests: string[] = [];
      const failedRequests: string[] = [];
      let privateDetailRequests = 0;
      page.on("console", (message) => {
        if (message.type() === "error") consoleErrors.push(message.text());
      });
      page.on("pageerror", (error) => pageErrors.push(error.message));
      page.on("request", (request) => {
        const requestUrl = new URL(request.url());
        if (requestUrl.origin !== parsed.origin) externalRequests.push(request.url());
        if (requestUrl.pathname.includes("/details/")) privateDetailRequests += 1;
      });
      page.on("requestfailed", (request) => failedRequests.push(request.url()));

      await page.route(dashboardUrl, async (route) => {
        const response = await route.fetch();
        let body = addPrivateLookupCoverage(await response.text());
        if (testCase.name === "mobile") body = addPaginationCoverage(body);
        await route.fulfill({ response, body });
      });

      await page.goto(dashboardUrl, { waitUntil: "load" });
      assert.equal(page.url(), dashboardUrl);
      assert.equal(await page.locator("h1").textContent(), "Agent Observability Report");
      assert.notEqual(await page.locator("#span-count").textContent(), "0");
      await page.locator("#agent-filter").selectOption({ label: "codex" });
      await page.locator(".trace-row:visible").first().click();
      assert.equal(await page.locator(".timeline-row").count() > 0, true);
      await page.locator("#span-table .span-open", { hasText: "LLM request" }).first().click();
      await page.locator("#private-detail", { hasText: "not an eligible Codex notify turn" }).waitFor();
      assert.equal(privateDetailRequests, 0, "ineligible spans must not request private detail");
      await page.locator("#span-table .span-open", { hasText: "Private turn" }).first().click();
      await page.locator("#private-detail", { hasText: "PRIVATE_BROWSER_REQUEST" }).waitFor();
      assert.equal(privateDetailRequests, 1);
      assert.match((await page.locator("#private-detail").textContent()) ?? "", /\/private\/project/);
      assert.match((await page.locator("#private-detail").textContent()) ?? "", /PRIVATE_BROWSER_RESPONSE/);
      if (testCase.name === "mobile") {
        const originalOpener = await page.locator("#span-table .span-open", { hasText: "Private turn" }).first().elementHandle();
        const openedSpanId = await originalOpener?.getAttribute("data-span-id");
        assert.ok(originalOpener);
        assert.ok(openedSpanId);
        await page.locator(".trace-row[aria-pressed='true']").click();
        assert.equal(await originalOpener.evaluate((element) => element.isConnected), false);
        const rerenderedOpener = page.locator(`#span-table .span-open[data-span-id="${openedSpanId}"]`);
        assert.equal(await rerenderedOpener.getAttribute("aria-expanded"), "true");
        assert.equal(
          await page.locator(`.span-open[data-span-id="${openedSpanId}"]:not([aria-expanded='true'])`).count(),
          0,
        );
        await page.locator("#details-close").click();
        assert.equal(await rerenderedOpener.getAttribute("aria-expanded"), "false");
        assert.equal(await rerenderedOpener.evaluate((element) => document.activeElement === element), true);

        await rerenderedOpener.click();
        await page.locator("#model-filter").selectOption({ label: "gpt-test" });
        assert.equal(await page.locator(".span-open[aria-expanded='true']").count(), 0);
        await page.locator("#details-close").click();
        assert.equal(await page.locator(".span-open[aria-expanded='true']").count(), 0);
        assert.equal(
          await page.evaluate(() => document.activeElement?.classList.contains("span-open")),
          true,
        );

        await page.locator("#clear-filters").click();
        await page.locator(".trace-row:visible").first().click();
        assert.equal(await page.locator("#span-next").isEnabled(), true);
        const paginatedOpener = page.locator("#span-table .span-open").nth(150);
        const paginatedSpanId = await paginatedOpener.getAttribute("data-span-id");
        assert.ok(paginatedSpanId);
        await paginatedOpener.click();
        await page.locator("#span-next").click();
        assert.equal(
          await page.locator(`.span-open[data-span-id="${paginatedSpanId}"]:visible`).count(),
          0,
        );
        assert.equal(await page.locator(".span-open[aria-expanded='true']").count(), 0);
        await page.locator("#details-close").click();
        assert.equal(await page.locator(".span-open[aria-expanded='true']").count(), 0);
        assert.equal(
          await page.evaluate(() => document.activeElement?.classList.contains("span-open")),
          true,
        );
      }
      assert.equal(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
        ),
        true,
      );
      assert.deepEqual(consoleErrors, []);
      assert.deepEqual(pageErrors, []);
      assert.deepEqual(externalRequests, []);
      assert.deepEqual(failedRequests, []);
      assert.equal(child.exitCode, null, "dashboard must remain available for reload");

      await page.locator("#agent-filter").selectOption({ index: 1 });
      assert.match(
        (await page.locator("#filter-status").textContent()) ?? "",
        /^\d+ spans match the active filters\.$/,
      );
      if (testCase.name === "desktop") {
        await page.locator("#save-filter").click();
        assert.equal(await page.locator("#saved-filter option").count(), 2);
      } else {
        assert.equal(
          await page.locator("#saved-filter option").count(),
          2,
          "saved views must survive a dashboard process restart",
        );
      }
      await page.reload({ waitUntil: "load" });
      assert.notEqual(await page.locator("#span-count").textContent(), "0");
      assert.deepEqual(consoleErrors, []);
      assert.deepEqual(pageErrors, []);
      assert.deepEqual(externalRequests, []);
      assert.deepEqual(failedRequests, []);
      results.push({ name: testCase.name, origin: parsed.origin, reload: true });
      await page.close();
    } finally {
      if (child.exitCode === null) child.kill();
      await waitForExit(child);
    }
  }
  console.log(JSON.stringify({ executablePath, results }));
} finally {
  await browserContext.close();
  await browser.close();
  await rm(directory, { recursive: true, force: true });
}

type DashboardProcess = ChildProcessByStdio<null, Readable, Readable>;

function waitForExit(child: DashboardProcess): Promise<number | null> {
  if (child.exitCode !== null) return Promise.resolve(child.exitCode);
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("dashboard process did not exit")), 5_000);
    child.once("exit", (code) => {
      clearTimeout(timer);
      resolve(code);
    });
  });
}

function readUrl(child: DashboardProcess): Promise<string> {
  return new Promise((resolve, reject) => {
    let output = "";
    const timer = setTimeout(() => reject(new Error("dashboard URL was not emitted")), 5_000);
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => {
      output += chunk;
      const match = output.match(/^url=(http:\/\/127\.0\.0\.1:\d+\/\S+)$/m);
      if (!match?.[1]) return;
      clearTimeout(timer);
      resolve(match[1]);
    });
    child.once("exit", (code) => {
      clearTimeout(timer);
      reject(new Error("dashboard exited before URL emission (" + code + ")"));
    });
  });
}

function addPrivateLookupCoverage(html: string): string {
  const pattern = /(<script id="report-data" type="application\/json">)([^<]+)(<\/script>)/;
  const match = html.match(pattern);
  if (!match?.[2]) throw new Error("dashboard report data was not found");
  const report = JSON.parse(match[2]) as {
    spans: Array<Record<string, unknown> & {
      spanId: string;
      traceId: string;
      name: string;
      turnId?: string;
      availability: Record<string, { state: string; reason: string }>;
    }>;
    traces: Array<Record<string, unknown> & { traceId: string; spans: number }>;
  };
  const source = report.spans.find((span) => span.name === "LLM request" && span.turnId !== undefined);
  if (!source) throw new Error("private lookup coverage span was not found");
  const privateLookup = { state: "private_lookup", reason: "local_opt_in_lookup_required" };
  report.spans.push({
    ...source,
    spanId: `${source.spanId}-private-turn`,
    kind: "turn",
    name: "Private turn",
    attributes: { source: "codex", event_type: "turn", turn_id: source.turnId },
    availability: {
      ...source.availability,
      sourceLocation: privateLookup,
      requestContent: privateLookup,
      responseContent: privateLookup,
    },
  });
  const trace = report.traces.find((candidate) => candidate.traceId === source.traceId);
  if (trace) trace.spans += 1;
  return html.replace(pattern, `$1${JSON.stringify(report).replaceAll("<", "\\u003c")}$3`);
}

function addPaginationCoverage(html: string): string {
  const pattern = /(<script id="report-data" type="application\/json">)([^<]+)(<\/script>)/;
  const match = html.match(pattern);
  if (!match?.[2]) throw new Error("dashboard report data was not found");
  const report = JSON.parse(match[2]) as {
    spans: Array<Record<string, unknown> & { spanId: string; traceId: string }>;
    traces: Array<Record<string, unknown> & { traceId: string; spans: number }>;
  };
  const firstByTrace = new Map<string, (typeof report.spans)[number]>();
  for (const span of report.spans) firstByTrace.set(span.traceId, firstByTrace.get(span.traceId) ?? span);
  const clones = [...firstByTrace.values()].flatMap((span) =>
    Array.from({ length: 200 }, (_, index) => ({
      ...span,
      spanId: `${span.spanId}-pagination-${index}`,
    })),
  );
  report.spans.push(...clones);
  for (const trace of report.traces) trace.spans += 200;
  return html.replace(pattern, `$1${JSON.stringify(report).replaceAll("<", "\\u003c")}$3`);
}
