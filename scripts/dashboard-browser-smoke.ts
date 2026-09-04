import assert from "node:assert/strict";
import { execFile, spawn, type ChildProcessByStdio } from "node:child_process";
import { access, chmod, copyFile, mkdtemp, rm } from "node:fs/promises";
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

      const page = await browser.newPage({ viewport: testCase.viewport });
      const consoleErrors: string[] = [];
      const pageErrors: string[] = [];
      const externalRequests: string[] = [];
      const failedRequests: string[] = [];
      page.on("console", (message) => {
        if (message.type() === "error") consoleErrors.push(message.text());
      });
      page.on("pageerror", (error) => pageErrors.push(error.message));
      page.on("request", (request) => {
        if (new URL(request.url()).origin !== parsed.origin) externalRequests.push(request.url());
      });
      page.on("requestfailed", (request) => failedRequests.push(request.url()));

      await page.goto(dashboardUrl, { waitUntil: "load" });
      assert.equal(page.url(), dashboardUrl);
      assert.equal(await page.locator("h1").textContent(), "Agent Observability Report");
      assert.notEqual(await page.locator("#span-count").textContent(), "0");
      await page.locator(".trace-row:visible").first().click();
      assert.equal(await page.locator(".timeline-row").count() > 0, true);
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
    }
  }
  console.log(JSON.stringify({ executablePath, results }));
} finally {
  await browser.close();
  await rm(directory, { recursive: true, force: true });
}

type DashboardProcess = ChildProcessByStdio<null, Readable, Readable>;

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
