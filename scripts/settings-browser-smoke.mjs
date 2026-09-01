import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { access, chmod, mkdtemp, readFile, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { promisify } from "node:util";
import { chromium } from "playwright-core";

const execute = promisify(execFile);

const executablePath = chromium.executablePath();
await access(executablePath).catch(() => {
  throw new Error("Pinned Chromium is missing; run npm run setup:browser.");
});

const directory = await mkdtemp(join(tmpdir(), "agent-observability-settings-browser-"));
await chmod(directory, 0o700);
const runtimeRoot = join(directory, "runtime");
const binary = join(process.cwd(), "target", "debug", "agent-observability");
const child = spawn(binary, ["ui", runtimeRoot, "--no-open"], {
  stdio: ["ignore", "pipe", "pipe"],
});
const browser = await chromium.launch({ executablePath, headless: true });
let stderr = "";
child.stderr.setEncoding("utf8");
child.stderr.on("data", (chunk) => {
  stderr += chunk;
});

try {
  const url = await readUrl(child);
  const origin = new URL(url).origin;
  const results = [];
  const screenshotDirectory = process.env.SETTINGS_SCREENSHOT_DIR ?? directory;

  for (const testCase of [
    { name: "desktop", viewport: { width: 1440, height: 900 } },
    { name: "mobile", viewport: { width: 390, height: 844 } },
  ]) {
    const page = await browser.newPage({ viewport: testCase.viewport });
    const consoleErrors = [];
    const expectedConflictErrors = [];
    const externalRequests = [];
    const failedRequests = [];
    page.on("console", (message) => {
      if (message.type() !== "error") return;
      if (testCase.name === "desktop" && message.text().includes("409 (Conflict)")) {
        expectedConflictErrors.push(message.text());
      } else {
        consoleErrors.push(message.text());
      }
    });
    page.on("request", (request) => {
      if (!request.url().startsWith(origin)) externalRequests.push(request.url());
    });
    page.on("requestfailed", (request) => failedRequests.push(request.url()));

    const response = await page.goto(url, { waitUntil: "networkidle" });
    assert.equal(response?.status(), 200);
    assert.match(response?.headers()["content-security-policy"] ?? "", /default-src 'none'/);
    assert.equal(response?.headers()["cache-control"], "no-store, max-age=0");
    await page.locator("#overview-title").waitFor();
    assert.equal(await page.evaluate(() => location.hash), "");
    assert.equal(await page.locator("main").count(), 1);
    assert.equal(await page.locator("nav[aria-label='설정 영역']").count(), 1);
    assert.equal(await page.locator("#settings-form input[type=number]").count(), 10);
    assert.equal(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
      true,
    );

    if (testCase.name === "desktop") {
      const input = page.locator("#collection-max_batch_records");
      await input.fill("125");
      assert.equal(await page.locator("#save").isEnabled(), true);
      assert.match(await page.locator("#batch-records-visual [data-visual-value]").textContent(), /125/);
      await page.locator("#save").click();
      await page.locator("#toast.visible").waitFor();
      assert.match(await page.locator("#toast").textContent(), /설정을 저장했습니다/);
      assert.equal(await page.locator("#save").isDisabled(), true);
      await page.waitForFunction(() => document.activeElement?.id === "save-title");
      const configPath = join(runtimeRoot, "config.json");
      const config = JSON.parse(await readFile(configPath, "utf8"));
      assert.equal(config.collection.max_batch_records, 125);
      await page.locator("#collection-max_batch_records").fill("");
      await page.locator("#collection-flush_interval_ms").fill("6000");
      await page.locator("#save").click();
      await page.locator("#toast").filter({ hasText: "비어 있거나 허용 범위" }).waitFor();
      assert.equal(await page.evaluate(() => document.activeElement?.id), "collection-max_batch_records");
      assert.equal(
        JSON.parse(await readFile(configPath, "utf8")).collection.flush_interval_ms,
        5000,
      );
      await page.locator("#collection-max_batch_records").fill("125");
      await execute(binary, ["config", "set", runtimeRoot, "retention-days", "45"], {
        timeout: 10_000,
      });
      await page.locator("#save").click();
      await page
        .locator("#toast")
        .filter({ hasText: "최신 설정을 불러와 내 변경만 다시 적용했습니다" })
        .waitFor();
      assert.equal(await page.locator("#retention-max_record_age_days").inputValue(), "45");
      assert.equal(await page.locator("#collection-flush_interval_ms").inputValue(), "6000");
      assert.equal(await page.locator("#save").isEnabled(), true);
      await page.locator("#save").click();
      await page.locator("#toast").filter({ hasText: "설정을 저장했습니다" }).waitFor();
      const rebased = JSON.parse(await readFile(configPath, "utf8"));
      assert.equal(rebased.retention.max_record_age_days, 45);
      assert.equal(rebased.collection.flush_interval_ms, 6000);
      await page.locator("#reset").click();
      assert.equal(await page.locator("#reset-dialog").getAttribute("open"), "");
      await page.locator("#confirm-reset").click();
      await page.locator("#reset").waitFor();
      await page.waitForFunction(() => document.activeElement?.id === "reset");
      assert.equal(await page.locator("#save").isEnabled(), true);
      await page.locator("#discard").click();
    } else {
      const controlHeights = await page
        .locator("button, .section-nav a, input[type=number]")
        .evaluateAll((elements) =>
          elements
            .map((element) => element.getBoundingClientRect().height)
            .filter((height) => height > 0),
        );
      assert.equal(controlHeights.every((height) => height >= 44), true);
      assert.equal(await page.locator("#collection-max_batch_records").inputValue(), "125");
      await page.locator('.section-nav a[href="#storage"]').click();
      await page.waitForTimeout(150);
      const anchorPosition = await page.evaluate(() => ({
        heading: document.querySelector("#storage-title")?.getBoundingClientRect().top ?? -1,
        navigation: document.querySelector(".section-nav")?.getBoundingClientRect().bottom ?? -1,
      }));
      assert.equal(anchorPosition.heading >= anchorPosition.navigation, true);
      await page.locator("#collection-active_heartbeat_interval_ms").fill("300000");
      await page.locator("#collection-idle_heartbeat_interval_ms").fill("120000");
      const heartbeatMarkers = await page.locator("#heartbeat-visual [data-marker]").evaluateAll(
        (markers) => markers.map((marker) => Number.parseFloat(marker.style.left)),
      );
      assert.equal(heartbeatMarkers[0] > heartbeatMarkers[1], true);
    }

    const screenshotPath = join(screenshotDirectory, `settings-${testCase.name}.png`);
    await page.screenshot({ path: screenshotPath, fullPage: true });
    assert.equal((await stat(screenshotPath)).size > 10_000, true);
    assert.deepEqual(consoleErrors, []);
    assert.equal(expectedConflictErrors.length, testCase.name === "desktop" ? 1 : 0);
    assert.deepEqual(externalRequests, []);
    assert.deepEqual(failedRequests, []);
    results.push({
      name: testCase.name,
      screenshot: basename(screenshotPath),
      overflow: false,
      consoleErrors: 0,
      externalRequests: 0,
    });

    if (testCase.name === "mobile") {
      await page.locator("#close-session").click();
      assert.equal(await page.locator("#close-dialog").getAttribute("open"), "");
      await page.locator("#cancel-close").click();
      assert.equal(await page.evaluate(() => document.activeElement?.id), "close-session");
      await page.locator("#discard").click();
      await page.locator("#close-session").click();
      await page.locator("text=설정 세션이 종료되었습니다").waitFor();
    }
    await page.close();
  }

  const exitCode = await waitForExit(child);
  assert.equal(exitCode, 0, stderr);
  console.log(JSON.stringify({ executablePath, results }));
} finally {
  if (child.exitCode === null) child.kill("SIGTERM");
  await browser.close();
  if (!process.env.SETTINGS_SCREENSHOT_DIR) {
    await rm(directory, { recursive: true, force: true });
  }
}

function readUrl(process) {
  return new Promise((resolve, reject) => {
    let stdout = "";
    const timeout = setTimeout(() => reject(new Error(`settings URL timed out: ${stderr}`)), 20_000);
    process.once("error", reject);
    process.stdout.setEncoding("utf8");
    process.stdout.on("data", (chunk) => {
      stdout += chunk;
      const match = stdout.match(/^url=(.+)$/m);
      if (match) {
        clearTimeout(timeout);
        resolve(match[1].trim());
      }
    });
    process.once("exit", (code) => {
      if (!stdout.match(/^url=/m)) {
        clearTimeout(timeout);
        reject(new Error(`settings process exited ${code}: ${stderr}`));
      }
    });
  });
}

function waitForExit(process) {
  if (process.exitCode !== null) return Promise.resolve(process.exitCode);
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      process.kill("SIGTERM");
      reject(new Error("settings process did not stop"));
    }, 10_000);
    process.once("exit", (code) => {
      clearTimeout(timeout);
      resolve(code);
    });
  });
}
