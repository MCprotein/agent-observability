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
      const cancelledDashboardRequests: string[] = [];
      let privateDetailRequests = 0;
      let dashboardQueryRequests = 0;
      let activeDashboardQueries = 0;
      let lastDashboardQueryAt = 0;
      let firstSpanPage: Record<string, unknown> | undefined;
      let paginationExcludedSpanId: string | undefined;
      page.on("console", (message) => {
        if (message.type() === "error") consoleErrors.push(message.text());
      });
      page.on("pageerror", (error) => pageErrors.push(error.message));
      page.on("request", (request) => {
        const requestUrl = new URL(request.url());
        if (requestUrl.origin !== parsed.origin) externalRequests.push(request.url());
        if (requestUrl.pathname.includes("/details/")) privateDetailRequests += 1;
        if (requestUrl.pathname.endsWith("/query")) {
          dashboardQueryRequests += 1;
          activeDashboardQueries += 1;
          lastDashboardQueryAt = Date.now();
        }
      });
      page.on("requestfinished", (request) => {
        if (new URL(request.url()).pathname.endsWith("/query")) activeDashboardQueries -= 1;
      });
      page.on("requestfailed", (request) => {
        const requestUrl = new URL(request.url());
        if (requestUrl.pathname.endsWith("/query")) activeDashboardQueries -= 1;
        if (
          requestUrl.origin === parsed.origin
          && requestUrl.pathname.endsWith("/query")
          && /ERR_ABORTED/.test(request.failure()?.errorText ?? "")
        ) {
          cancelledDashboardRequests.push(request.url());
          return;
        }
        failedRequests.push(request.url());
      });
      const waitForDashboardIdle = async (): Promise<void> => {
        const deadline = Date.now() + 5_000;
        while (Date.now() < deadline) {
          if (activeDashboardQueries === 0 && Date.now() - lastDashboardQueryAt >= 50) return;
          await page.waitForTimeout(25);
        }
        throw new Error(`dashboard queries did not become idle: active=${activeDashboardQueries}`);
      };
      if (testCase.name === "mobile") {
        await page.route("**/*", async (route) => {
          const request = dashboardRequest(route.request().url());
          if (request?.kind !== "spans") {
            await route.continue();
            return;
          }
          if (request.cursor === "browser-smoke-next") {
            assert.ok(firstSpanPage);
            assert.ok(paginationExcludedSpanId);
            const rows = (firstSpanPage.rows as Array<{ spanId: string }>).filter(
              (row) => row.spanId !== paginationExcludedSpanId,
            );
            assert.ok(rows.length > 0, "synthetic next page needs a deterministic focus fallback");
            await route.fulfill({
              json: { ...firstSpanPage, rows, pagination: { nextCursor: null, total: rows.length + 1 } },
            });
            return;
          }
          const response = await route.fetch();
          const body = await response.json() as Record<string, unknown>;
          firstSpanPage = body;
          await route.fulfill({
            response,
            json: { ...body, pagination: { nextCursor: "browser-smoke-next", total: 201 } },
          });
        });
      }

      await page.goto(dashboardUrl, { waitUntil: "load" });
      assert.equal(page.url(), dashboardUrl);
      assert.equal(await page.locator("h1").textContent(), "Agent Observability");
      await page.waitForFunction(() => document.querySelectorAll(".trace-row").length > 0);
      assert.match((await page.locator("#filter-status").textContent()) ?? "", /Current snapshot/);
      assert.match((await page.locator(".timestamp").textContent()) ?? "", /Current snapshot/);
      await page.locator("#agent-filter option", { hasText: "codex" }).waitFor({ state: "attached" });
      await page.waitForFunction(() => document.getElementById("kpi-sessions")?.textContent?.startsWith("Exact"));
      await waitForDashboardIdle();
      await page.locator("#agent-filter").selectOption({ label: "codex" });
      await page.waitForFunction(() => document.querySelectorAll(".trace-row").length > 0);
      await page.locator(".trace-row:visible").first().click();
      await page.waitForFunction(() => document.querySelectorAll("#span-table .span-open").length > 0);
      assert.equal(await page.locator(".timeline-row").count() > 0, true);
      await page.locator("#span-table .span-open", { hasText: "LLM request" }).first().click();
      await page.locator("#private-detail", { hasText: "not eligible for private local detail" }).waitFor();
      assert.match((await page.locator("#details-body").textContent()) ?? "", /Input tokens/);
      assert.match((await page.locator("#details-body").textContent()) ?? "", /Estimated API cost/);
      assert.equal(
        await page.locator("#details-heading").evaluate((element) => document.activeElement === element),
        true,
        "opening span details must focus the details heading",
      );
      assert.equal(privateDetailRequests, 0, "ineligible spans must not request private detail");
      const privateTurnOpener = page.locator("#span-table .span-open", { hasText: "Turn" }).first();
      const openedSpanId = await privateTurnOpener.getAttribute("data-span-id");
      const originalOpener = await privateTurnOpener.elementHandle();
      assert.ok(openedSpanId);
      assert.ok(originalOpener);
      await privateTurnOpener.click();
      await page.locator("#private-detail button", { hasText: "Load private detail" }).waitFor();
      assert.equal(
        await page.locator("#details-heading").evaluate((element) => document.activeElement === element),
        true,
        "opening span details must focus the details heading",
      );
      await page.locator("#private-detail button", { hasText: "Load private detail" }).click();
      await page.locator("#private-detail", { hasText: "PRIVATE_BROWSER_REQUEST" }).waitFor();
      assert.equal(privateDetailRequests, 1);
      assert.match((await page.locator("#private-detail").textContent()) ?? "", /\/private\/project/);
      assert.match((await page.locator("#private-detail").textContent()) ?? "", /PRIVATE_BROWSER_RESPONSE/);
      if (testCase.name === "mobile") {
        assert.equal(await originalOpener.evaluate((element) => element.isConnected), false);
        await page.locator("#details-close").click();
        assert.equal(await page.locator("#span-details").evaluate((element) => element.classList.contains("open")), false);
        assert.equal(await page.locator(".span-open[aria-expanded='true']").count(), 0);
        assert.equal((await page.locator("#details-body").textContent())?.includes("PRIVATE_BROWSER_REQUEST"), false);
        assert.equal(
          await page.locator(`#span-table .span-open[data-span-id="${openedSpanId}"]`).evaluate(
            (element) => document.activeElement === element,
          ),
          true,
          "close must restore the rerendered table opener",
        );

        const timelineOpener = page.locator(`.timeline-row .span-open[data-span-id="${openedSpanId}"]`);
        await timelineOpener.click();
        await page.locator("#span-details.open").waitFor();
        assert.equal(
          await page.locator("#details-heading").evaluate((element) => document.activeElement === element),
          true,
        );
        await page.locator("#details-close").click();
        assert.equal(
          await page.locator(`.timeline-row .span-open[data-span-id="${openedSpanId}"]`).evaluate(
            (element) => document.activeElement === element,
          ),
          true,
          "close must restore the initiating timeline surface",
        );

        await page.locator(`#span-table .span-open[data-span-id="${openedSpanId}"]`).click();
        await page.locator("#span-details.open").waitFor();
        paginationExcludedSpanId = openedSpanId;
        const nextSpanPage = page.waitForResponse((response) =>
          dashboardRequest(response.url())?.cursor === "browser-smoke-next",
        );
        await page.locator("#span-next").click();
        await nextSpanPage;
        await page.waitForFunction(
          (spanId) => ![...document.querySelectorAll<HTMLElement>(".span-open")]
            .some((element) => element.dataset.spanId === spanId),
          openedSpanId,
        );
        await page.locator("#details-close").click();
        assert.equal(await page.locator(".span-open[aria-expanded='true']").count(), 0);
        assert.equal(
          await page.locator(".span-open:visible").first().evaluate((element) => document.activeElement === element),
          true,
          "close after pagination must focus the first visible span opener",
        );

        await page.locator("#model-filter option", { hasText: "gpt-test" }).waitFor({ state: "attached" });
        await page.locator("#model-filter").selectOption({ label: "gpt-test" });
        assert.equal(await page.locator(".span-open[aria-expanded='true']").count(), 0);
        await page.waitForFunction(() => document.getElementById("kpi-sessions")?.textContent?.startsWith("Exact"));
        await waitForDashboardIdle();
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

      if (testCase.name === "desktop") {
        assert.equal(await page.locator("#save-filter").isEnabled(), true);
        await page.locator("#save-filter").click();
        assert.equal(await page.locator("#saved-filter option").count(), 2);
        const saved = await page.evaluate(() =>
          localStorage.getItem("agent-observability.report.v1.saved-filters") ?? "",
        );
        assert.equal(saved.includes("PRIVATE_BROWSER_REQUEST"), false);
        assert.equal(saved.includes("PRIVATE_BROWSER_RESPONSE"), false);
      } else {
        assert.equal(
          await page.locator("#saved-filter option").count(),
          2,
          "saved views must survive a dashboard process restart",
        );
        await page.locator("#clear-filters").click();
        await page.locator("#saved-filter").selectOption("0");
        assert.equal(await page.locator("#agent-filter").inputValue(), "codex");
      }
      await page.waitForFunction(() => document.getElementById("kpi-sessions")?.textContent?.startsWith("Exact"));
      await waitForDashboardIdle();
      const queriesBeforeRefresh = dashboardQueryRequests;
      const refreshRequest = page.waitForRequest((request) => dashboardRequestKind(request.url()) === "bootstrap");
      await page.locator("#refresh-dashboard").click();
      await refreshRequest;
      assert.equal(dashboardQueryRequests > queriesBeforeRefresh, true);
      await page.waitForFunction(() => document.getElementById("filter-status")?.textContent?.includes("Current snapshot"));
      await page.waitForFunction(() => document.getElementById("kpi-sessions")?.textContent?.startsWith("Exact"));
      await waitForDashboardIdle();
      assert.equal(page.url(), dashboardUrl, "refresh must not reload or escape the dashboard URL");
      await page.reload({ waitUntil: "load" });
      await page.waitForFunction(() => document.querySelectorAll(".trace-row").length > 0);
      await page.waitForFunction(() => document.getElementById("kpi-sessions")?.textContent?.startsWith("Exact"));
      await waitForDashboardIdle();
      assert.deepEqual(consoleErrors, []);
      assert.deepEqual(pageErrors, []);
      assert.deepEqual(externalRequests, []);
      assert.deepEqual(failedRequests, []);
      results.push({
        name: testCase.name,
        origin: parsed.origin,
        reload: true,
        cancelledDashboardQueries: cancelledDashboardRequests.length,
      });
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

function dashboardRequestKind(url: string): string | undefined {
  return dashboardRequest(url)?.kind;
}

function dashboardRequest(url: string): { kind: string; cursor?: string | null } | undefined {
  const encoded = new URL(url).searchParams.get("request");
  if (!encoded) return undefined;
  try {
    const value = JSON.parse(encoded) as unknown;
    if (typeof value !== "object" || value === null || !("kind" in value) || typeof value.kind !== "string") {
      return undefined;
    }
    const cursor = "cursor" in value && (typeof value.cursor === "string" || value.cursor === null)
      ? value.cursor
      : undefined;
    return { kind: value.kind, ...(cursor === undefined ? {} : { cursor }) };
  } catch {
    return undefined;
  }
}
