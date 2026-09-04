import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { access, chmod, mkdtemp, readFile, realpath, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { promisify } from "node:util";
import { chromium, type Page, type Request } from "playwright-core";

const execute = promisify(execFile);

const executablePath = chromium.executablePath();
await access(executablePath).catch(() => {
  throw new Error("Pinned Chromium is missing; run npm run setup:browser.");
});

const directory = await mkdtemp(join(tmpdir(), "agent-observability-settings-browser-"));
await chmod(directory, 0o700);
const runtimeRoot = join(directory, "runtime");
const binary = join(process.cwd(), "target", "debug", "agent-observability");
const child = spawn(binary, ["settings", runtimeRoot, "--no-open"], {
  stdio: ["ignore", "pipe", "pipe"],
});
type SettingsProcess = typeof child;
const browser = await chromium.launch({ executablePath, headless: true });
let stderr = "";
child.stderr.setEncoding("utf8");
child.stderr.on("data", (chunk) => {
  stderr += chunk;
});

try {
  const url = await readUrl(child);
  const origin = new URL(url).origin;
  const results: Array<Record<string, string | number | boolean>> = [];
  const screenshotDirectory = process.env.SETTINGS_SCREENSHOT_DIR ?? directory;

  const bootstrapFailurePage = await browser.newPage({ viewport: { width: 800, height: 600 } });
  await mockCodexApi(bootstrapFailurePage);
  const bootstrapFailures: Request[] = [];
  const bootstrapPageErrors: string[] = [];
  bootstrapFailurePage.on("requestfailed", (request) => bootstrapFailures.push(request));
  bootstrapFailurePage.on("pageerror", (error) => bootstrapPageErrors.push(error.message));
  await bootstrapFailurePage.route(`${origin}/api/config`, (route) => route.abort("connectionfailed"));
  await bootstrapFailurePage.goto(url);
  await bootstrapFailurePage.locator("text=설정 세션이 종료되었습니다").waitFor();
  assert.equal(await bootstrapFailurePage.evaluate(() => sessionStorage.length), 0);
  assert.equal(bootstrapFailures.length, 1);
  const bootstrapFailure = bootstrapFailures[0];
  assert.ok(bootstrapFailure);
  assert.equal(bootstrapFailure.url(), `${origin}/api/config`);
  assert.equal(bootstrapFailure.method(), "GET");
  assert.deepEqual(bootstrapPageErrors, []);
  await bootstrapFailurePage.close();

  const optionalIntegrationFailurePage = await browser.newPage({ viewport: { width: 900, height: 700 } });
  await mockCodexApi(optionalIntegrationFailurePage, { failStatus: true });
  await optionalIntegrationFailurePage.goto(url, { waitUntil: "networkidle" });
  await optionalIntegrationFailurePage.locator("#settings-form").waitFor();
  await optionalIntegrationFailurePage.locator(".integration-panel[data-state='unavailable']").waitFor();
  assert.equal(
    await optionalIntegrationFailurePage.locator(".integration-identity strong").innerText(),
    "상태 확인 불가",
  );
  assert.equal(await optionalIntegrationFailurePage.locator("#save").isVisible(), true);
  assert.equal(await optionalIntegrationFailurePage.locator("#refresh-integration").isVisible(), true);
  assert.equal(await optionalIntegrationFailurePage.evaluate(() => sessionStorage.length), 1);
  await optionalIntegrationFailurePage.close();

  const recoveringIntegrationPage = await browser.newPage({ viewport: { width: 900, height: 700 } });
  const recoveringIntegration = await mockCodexApi(recoveringIntegrationPage, {
    startConnected: true,
    firstStatusUnavailable: true,
  });
  await recoveringIntegrationPage.goto(url, { waitUntil: "networkidle" });
  await recoveringIntegrationPage.locator(".integration-panel[data-state='ready']").waitFor();
  assert.equal(recoveringIntegration.methods.filter((method) => method === "GET").length, 2);
  assert.equal(recoveringIntegration.getTimestamps.length, 2);
  assert.ok(
    recoveringIntegration.getTimestamps[1]! - recoveringIntegration.getTimestamps[0]! >= 1_450,
  );
  await recoveringIntegrationPage.close();

  const persistentlyUnavailablePage = await browser.newPage({ viewport: { width: 900, height: 700 } });
  await persistentlyUnavailablePage.addInitScript(() => {
    const nativeAddEventListener = window.addEventListener.bind(window);
    window.addEventListener = ((type: string, listener: EventListenerOrEventListenerObject, options?: boolean | AddEventListenerOptions) => {
      if (type !== "focus") nativeAddEventListener(type, listener, options);
    }) as typeof window.addEventListener;
  });
  const persistentlyUnavailable = await mockCodexApi(persistentlyUnavailablePage, {
    startConnected: true,
    unavailableStatusCount: 2,
  });
  await persistentlyUnavailablePage.goto(url, { waitUntil: "networkidle" });
  await persistentlyUnavailablePage
    .locator(".integration-panel[data-config-state='connected'][data-collector-state='unavailable']")
    .waitFor();
  await persistentlyUnavailablePage.waitForTimeout(1_750);
  assert.equal(persistentlyUnavailable.methods.filter((method) => method === "GET").length, 2);
  await persistentlyUnavailablePage.close();

  const outOfOrderPage = await browser.newPage({ viewport: { width: 900, height: 700 } });
  let integrationGetCount = 0;
  let releaseInitialStatus: (() => void) | undefined;
  const initialStatusRelease = new Promise<void>((resolve) => {
    releaseInitialStatus = resolve;
  });
  await outOfOrderPage.route("**/api/integrations/codex", async (route) => {
    integrationGetCount += 1;
    if (integrationGetCount === 1) {
      await initialStatusRelease;
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(disconnectedStatus()) });
      return;
    }
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(connectedStatus()) });
  });
  await outOfOrderPage.goto(url, { waitUntil: "domcontentloaded" });
  while (integrationGetCount < 1) await new Promise((resolve) => setTimeout(resolve, 10));
  await outOfOrderPage.evaluate(() => window.dispatchEvent(new Event("focus")));
  await outOfOrderPage.waitForFunction(() => document.querySelector("#settings-form") !== null);
  if (!releaseInitialStatus) throw new Error("initial status release was not initialized");
  releaseInitialStatus();
  await outOfOrderPage.locator(".integration-panel[data-state='ready']").waitFor();
  await outOfOrderPage.waitForTimeout(100);
  assert.equal(await outOfOrderPage.locator(".integration-panel").getAttribute("data-state"), "ready");
  await outOfOrderPage.close();

  const expiredRacePage = await browser.newPage({ viewport: { width: 900, height: 700 } });
  await expiredRacePage.addInitScript(() => {
    const nativeSetInterval = window.setInterval.bind(window);
    (window as typeof window & { __agentobsIntervalCount?: number }).__agentobsIntervalCount = 0;
    window.setInterval = ((handler: TimerHandler, timeout?: number, ...args: unknown[]) => {
      (window as typeof window & { __agentobsIntervalCount?: number }).__agentobsIntervalCount! += 1;
      return nativeSetInterval(handler, timeout, ...args);
    }) as typeof window.setInterval;
  });
  let expiredRaceGetCount = 0;
  let releaseExpiredRaceInitial: (() => void) | undefined;
  const expiredRaceInitialRelease = new Promise<void>((resolve) => {
    releaseExpiredRaceInitial = resolve;
  });
  await expiredRacePage.route("**/api/integrations/codex", async (route) => {
    expiredRaceGetCount += 1;
    if (expiredRaceGetCount === 1) {
      await expiredRaceInitialRelease;
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(disconnectedStatus()) });
      return;
    }
    await route.fulfill({
      status: 401,
      contentType: "application/json",
      body: JSON.stringify({ code: "invalid_session", message: "expired race session" }),
    });
  });
  await expiredRacePage.goto(url, { waitUntil: "domcontentloaded" });
  while (expiredRaceGetCount < 1) await new Promise((resolve) => setTimeout(resolve, 10));
  await expiredRacePage.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expiredRacePage.locator("text=설정 세션이 종료되었습니다").waitFor();
  if (!releaseExpiredRaceInitial) throw new Error("expired race release was not initialized");
  releaseExpiredRaceInitial();
  await expiredRacePage.waitForTimeout(100);
  assert.equal(await expiredRacePage.locator("text=설정 세션이 종료되었습니다").count(), 1);
  assert.equal(await expiredRacePage.locator("#settings-form").count(), 0);
  assert.equal(await expiredRacePage.evaluate(() => sessionStorage.length), 0);
  assert.equal(
    await expiredRacePage.evaluate(
      () => (window as typeof window & { __agentobsIntervalCount?: number }).__agentobsIntervalCount,
    ),
    0,
  );
  await expiredRacePage.close();

  const mutationFailurePage = await browser.newPage({ viewport: { width: 800, height: 600 } });
  await mockCodexApi(mutationFailurePage);
  await mutationFailurePage.route(`${origin}/api/config`, (route) => {
    if (route.request().method() === "PUT") return route.abort("connectionfailed");
    return route.continue();
  });
  await mutationFailurePage.goto(url, { waitUntil: "networkidle" });
  await mutationFailurePage.locator("#collection-max_batch_records").fill("124");
  await mutationFailurePage.locator("#save").click();
  await mutationFailurePage.locator("text=설정 세션이 종료되었습니다").waitFor();
  assert.equal(await mutationFailurePage.evaluate(() => sessionStorage.length), 0);
  await mutationFailurePage.close();

  const lifecyclePage = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  const lifecycle = await mockCodexApi(lifecyclePage, {
    startConflicted: true,
    failConnect: true,
    conflictDisconnect: true,
  });
  let dashboardFailures = 1;
  let dashboardOpenCount = 0;
  let settingsShutdownCount = 0;
  await lifecyclePage.route(`${origin}/api/dashboard/open`, async (route) => {
    dashboardOpenCount += 1;
    if (dashboardFailures > 0) {
      dashboardFailures -= 1;
      await route.fulfill({
        status: 500,
        contentType: "application/json",
        body: JSON.stringify({ code: "report_unavailable", message: "injected report failure" }),
      });
      return;
    }
    await route.fulfill({ status: 204 });
  });
  await lifecyclePage.route(`${origin}/api/shutdown`, async (route) => {
    settingsShutdownCount += 1;
    await route.fulfill({ status: 204 });
  });
  await lifecyclePage.goto(url, { waitUntil: "networkidle" });
  await lifecyclePage.locator(".integration-panel[data-state='conflict']").waitFor();
  assert.match(await lifecyclePage.locator(".integration-identity strong").innerText(), /설정 충돌/);

  await lifecyclePage.locator("#toggle-integration").click();
  await lifecyclePage.locator("#toast").filter({ hasText: "injected connect failure" }).waitFor();
  assert.equal(await lifecyclePage.locator("#toggle-integration").isEnabled(), true);
  assert.equal(lifecycle.status.config, "conflict");

  await lifecyclePage.locator("#toggle-integration").click();
  await lifecyclePage.locator(".integration-panel[data-state='ready']").waitFor();
  assert.match(await lifecyclePage.locator(".integration-identity strong").innerText(), /수집 중/);
  assert.equal(lifecycle.launchAgentRunning, true);
  assert.equal(await lifecyclePage.locator("#toggle-integration").innerText(), "연결 해제");

  lifecycle.status = degradedStatus();
  const documentIdentity = await lifecyclePage.evaluate(() => performance.timeOrigin);
  const navigationCount = await lifecyclePage.evaluate(
    () => performance.getEntriesByType("navigation").length,
  );
  await lifecyclePage.evaluate(() => window.dispatchEvent(new Event("focus")));
  await lifecyclePage.locator(".integration-panel[data-state='degraded']").waitFor();
  assert.match(
    await lifecyclePage.locator(".integration-identity strong").innerText(),
    /리포트 반영 지연/,
  );
  assert.match(await lifecyclePage.locator(".integration-meta").innerText(), /리포트 지연/);
  assert.doesNotMatch(await lifecyclePage.locator(".integration-meta").innerText(), /정상/);
  assert.equal(await lifecyclePage.locator("#toggle-integration").innerText(), "연결 해제");
  assert.equal(await lifecyclePage.evaluate(() => performance.timeOrigin), documentIdentity);
  assert.equal(
    await lifecyclePage.evaluate(() => performance.getEntriesByType("navigation").length),
    navigationCount,
  );

  const heartbeatPage = await browser.newPage({ viewport: { width: 900, height: 700 } });
  await heartbeatPage.addInitScript(() => {
    const nativeSetInterval = window.setInterval.bind(window);
    window.setInterval = ((handler: TimerHandler, timeout?: number, ...args: unknown[]) => {
      if (timeout === 20_000 && typeof handler === "function") {
        (window as typeof window & { __agentobsHeartbeat?: () => void }).__agentobsHeartbeat = () => {
          handler(...args);
        };
        return 1;
      }
      return nativeSetInterval(handler, timeout, ...args);
    }) as typeof window.setInterval;
  });
  const heartbeatIntegration = await mockCodexApi(heartbeatPage, { startConnected: true });
  let heartbeatCount = 0;
  await heartbeatPage.route(`${origin}/api/heartbeat`, async (route) => {
    heartbeatCount += 1;
    heartbeatIntegration.status = degradedStatus();
    await route.fulfill({ status: 204 });
  });
  await heartbeatPage.goto(url, { waitUntil: "networkidle" });
  const heartbeatDocumentIdentity = await heartbeatPage.evaluate(() => performance.timeOrigin);
  await heartbeatPage.evaluate(() => {
    const callback = (window as typeof window & { __agentobsHeartbeat?: () => void }).__agentobsHeartbeat;
    if (!callback) throw new Error("heartbeat callback was not captured");
    callback();
  });
  await heartbeatPage.locator(".integration-panel[data-state='degraded']").waitFor();
  assert.equal(heartbeatCount, 1);
  assert.equal(await heartbeatPage.evaluate(() => performance.timeOrigin), heartbeatDocumentIdentity);
  assert.equal(
    heartbeatIntegration.methods.filter((method) => method === "GET").length,
    2,
  );
  await heartbeatPage.close();

  await lifecyclePage.locator("#overview-dashboard").click();
  await lifecyclePage.locator("#toast").filter({ hasText: "injected report failure" }).waitFor();
  await lifecyclePage.locator("#open-dashboard").click();
  await lifecyclePage.locator("#toast").filter({ hasText: "모니터링 리포트를 열었습니다" }).waitFor();
  assert.equal(dashboardOpenCount, 2);

  await lifecyclePage.locator("#toggle-integration").click();
  await lifecyclePage.locator("#toast").filter({ hasText: "injected disconnect conflict" }).waitFor();
  assert.equal(lifecycle.status.config, "connected");
  assert.equal(lifecycle.launchAgentRunning, true);
  assert.equal(await lifecyclePage.locator("#toggle-integration").isEnabled(), true);
  await lifecyclePage.locator("#toggle-integration").click();
  await lifecyclePage.locator(".integration-panel[data-state='idle']").waitFor();
  assert.equal(lifecycle.status.config, "disconnected");
  assert.equal(lifecycle.launchAgentRunning, false);

  await lifecyclePage.locator("#toggle-integration").click();
  await lifecyclePage.locator(".integration-panel[data-state='ready']").waitFor();
  assert.equal(lifecycle.launchAgentRunning, true);
  const disconnectCountBeforeClose = lifecycle.methods.filter((method) => method === "DELETE").length;
  await lifecyclePage.locator("#close-session").click();
  await lifecyclePage.locator("text=설정 세션이 종료되었습니다").waitFor();
  assert.equal(settingsShutdownCount, 1);
  assert.equal(lifecycle.launchAgentRunning, true);
  assert.equal(
    lifecycle.methods.filter((method) => method === "DELETE").length,
    disconnectCountBeforeClose,
  );
  assert.equal(await lifecyclePage.evaluate(() => sessionStorage.length), 0);
  await lifecyclePage.close();

  for (const lifecycleMethod of ["POST", "DELETE"] as const) {
    const delayedLifecyclePage = await browser.newPage({ viewport: { width: 1200, height: 800 } });
    const delayedLifecycle = await mockCodexApi(delayedLifecyclePage, {
      startConnected: lifecycleMethod === "DELETE",
      delayedMethod: lifecycleMethod,
    });
    await delayedLifecyclePage.route(`${origin}/api/shutdown`, async (route) => {
      await route.fulfill({ status: 204 });
    });
    await delayedLifecyclePage.goto(url, { waitUntil: "networkidle" });
    await delayedLifecyclePage.locator("#toggle-integration").click();
    await delayedLifecycle.waitForStart();
    await delayedLifecyclePage.locator("#close-session").click();
    await delayedLifecyclePage.locator("text=설정 세션이 종료되었습니다").waitFor();
    delayedLifecycle.complete();
    await delayedLifecycle.waitForCompletion();
    await delayedLifecyclePage.evaluate(() => new Promise<void>((resolve) => {
      requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
    }));
    assert.equal(await delayedLifecyclePage.locator("text=설정 세션이 종료되었습니다").count(), 1);
    assert.equal(await delayedLifecyclePage.locator("#overview-title").count(), 0);
    assert.equal(await delayedLifecyclePage.evaluate(() => sessionStorage.length), 0);
    await delayedLifecyclePage.close();
  }

  const dashboardLauncherPage = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  await dashboardLauncherPage.goto(url, { waitUntil: "networkidle" });
  const expectedDashboardOpenStatus = process.platform === "darwin" ? 204 : 409;
  const firstOpenResponse = dashboardLauncherPage.waitForResponse(
    (response) => response.url() === `${origin}/api/dashboard/open`
      && response.request().method() === "POST",
  );
  await dashboardLauncherPage.locator("#open-dashboard").click();
  assert.equal((await firstOpenResponse).status(), expectedDashboardOpenStatus);
  const dashboardUrl = await waitForDashboardUrl(runtimeRoot);
  const dashboardPid = await readDashboardPid(runtimeRoot);
  const secondOpenResponse = dashboardLauncherPage.waitForResponse(
    (response) => response.url() === `${origin}/api/dashboard/open`
      && response.request().method() === "POST",
  );
  await dashboardLauncherPage.locator("#open-dashboard").click();
  assert.equal((await secondOpenResponse).status(), expectedDashboardOpenStatus);
  assert.equal(await readDashboardPid(runtimeRoot), dashboardPid, "repeated opens must reuse one server");
  const independentDashboardPage = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  await independentDashboardPage.goto(dashboardUrl, { waitUntil: "load" });
  assert.equal(await independentDashboardPage.locator("h1").textContent(), "Agent Observability Report");
  await dashboardLauncherPage.close();

  for (const testCase of [
    { name: "desktop", viewport: { width: 1440, height: 900 } },
    { name: "mobile", viewport: { width: 390, height: 844 } },
    { name: "compact", viewport: { width: 320, height: 800 } },
  ]) {
    const page = await browser.newPage({ viewport: testCase.viewport });
    await mockCodexApi(page);
    const consoleErrors: string[] = [];
    const expectedApiErrors: string[] = [];
    const pageErrors: string[] = [];
    const externalRequests: string[] = [];
    const failedRequests: string[] = [];
    let dirtyReloadDialog = false;
    page.on("console", (message) => {
      if (message.type() !== "error") return;
      if (
        testCase.name === "desktop" &&
        (message.text().includes("409 (Conflict)") ||
          message.text().includes("500 (Internal Server Error)"))
      ) {
        expectedApiErrors.push(message.text());
      } else {
        consoleErrors.push(message.text());
      }
    });
    page.on("pageerror", (error) => pageErrors.push(error.message));
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
      assert.equal(await page.evaluate(() => sessionStorage.length), 1);
      await page.reload({ waitUntil: "networkidle" });
      await page.locator("#overview-title").waitFor();
      assert.equal(await page.evaluate(() => location.hash), "");
      assert.equal(await page.locator("#cadence-visual [data-dual-value]").textContent(), "확인 5초 · 반영 5초");
      const defaultCadenceMarkers = await page.locator("#cadence-visual [data-marker]").evaluateAll(
        (markers) => markers.map((marker) => marker.getBoundingClientRect()),
      );
      const [firstCadenceMarker, secondCadenceMarker] = defaultCadenceMarkers;
      assert.ok(firstCadenceMarker && secondCadenceMarker);
      assert.equal(rectanglesOverlap(firstCadenceMarker, secondCadenceMarker), false);
    }

    if (testCase.name === "desktop") {
      const input = page.locator("#collection-max_batch_records");
      await input.fill("125");
      assert.equal(await page.locator("#save").isEnabled(), true);
      assert.match((await page.locator("#batch-records-visual [data-visual-value]").textContent()) ?? "", /125/);
      await page.locator("#save").click();
      await page.locator("#toast.visible").waitFor();
      assert.match((await page.locator("#toast").textContent()) ?? "", /설정을 저장했습니다/);
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
      await execute(binary, ["config", "set", runtimeRoot, "file-reconcile-ms", "1500"], {
        timeout: 10_000,
      });
      await execute(binary, ["config", "set", runtimeRoot, "retention-days", "45"], {
        timeout: 10_000,
      });
      let failNextConfigRead = true;
      await page.route(`${origin}/api/config`, async (route) => {
        if (failNextConfigRead && route.request().method() === "GET") {
          failNextConfigRead = false;
          await route.fulfill({
            status: 500,
            contentType: "application/json",
            body: JSON.stringify({
              code: "config_unavailable",
              message: "injected config reload failure",
            }),
          });
        } else {
          await route.continue();
        }
      });
      await page.locator("#save").click();
      await page
        .locator("#toast")
        .filter({ hasText: "최신 설정을 불러오지 못했습니다" })
        .waitFor();
      assert.equal(await page.locator("#collection-flush_interval_ms").inputValue(), "6000");
      assert.equal(await page.locator("#save").isEnabled(), true);
      await page.unroute(`${origin}/api/config`);
      await page.locator("#save").click();
      await page
        .locator("#toast")
        .filter({ hasText: "최신 설정을 불러와 내 변경만 다시 적용했습니다" })
        .waitFor();
      assert.equal(await page.locator("#collection-file_reconcile_interval_ms").inputValue(), "1500");
      assert.equal(await page.locator("#retention-max_record_age_days").inputValue(), "45");
      assert.equal(await page.locator("#collection-flush_interval_ms").inputValue(), "6000");
      assert.equal(await page.locator("#save").isEnabled(), true);
      await page.locator("#save").click();
      await page.locator("#toast").filter({ hasText: "설정을 저장했습니다" }).waitFor();
      const rebased = JSON.parse(await readFile(configPath, "utf8"));
      assert.equal(rebased.collection.file_reconcile_interval_ms, 1500);
      assert.equal(rebased.retention.max_record_age_days, 45);
      assert.equal(rebased.collection.flush_interval_ms, 6000);
      await page.locator("#reset").click();
      assert.equal(await page.locator("#reset-dialog").getAttribute("open"), "");
      await page.locator("#cancel-reset").focus();
      await page.keyboard.press("Shift+Tab");
      assert.equal(await page.evaluate(() => document.activeElement?.id), "confirm-reset");
      await page.keyboard.press("Tab");
      assert.equal(await page.evaluate(() => document.activeElement?.id), "cancel-reset");
      await page.locator("#confirm-reset").click();
      await page.locator("#reset").waitFor();
      await page.waitForFunction(() => document.activeElement?.id === "reset");
      assert.equal(await page.locator("#save").isEnabled(), true);
      await page.locator("#discard").click();
      await page.locator("#collection-max_batch_records").fill("126");
      page.once("dialog", async (dialog) => {
        assert.equal(dialog.type(), "beforeunload");
        dirtyReloadDialog = true;
        await dialog.dismiss();
      });
      await page.evaluate(() => location.reload()).catch(() => undefined);
      assert.equal(dirtyReloadDialog, true);
      assert.equal(await page.locator("#collection-max_batch_records").inputValue(), "126");
      await page.locator("#discard").click();
      await page.locator("#collection-max_batch_records").fill("127");
      let confirmedReloadDialog = false;
      page.once("dialog", async (dialog) => {
        assert.equal(dialog.type(), "beforeunload");
        confirmedReloadDialog = true;
        await dialog.accept();
      });
      await page.reload({ waitUntil: "networkidle" });
      await page.locator("#overview-title").waitFor();
      assert.equal(confirmedReloadDialog, true);
      assert.equal(await page.locator("#collection-max_batch_records").inputValue(), "125");
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
      const [activeHeartbeat, idleHeartbeat] = heartbeatMarkers;
      if (activeHeartbeat === undefined || idleHeartbeat === undefined) {
        throw new Error("heartbeat visualization markers are missing");
      }
      assert.equal(activeHeartbeat > idleHeartbeat, true);
      if (testCase.name === "compact") {
        const navigationWidth = await page.locator(".section-nav").evaluate((navigation) => ({
          client: navigation.clientWidth,
          scroll: navigation.scrollWidth,
        }));
        assert.equal(navigationWidth.scroll <= navigationWidth.client, true);
      }
    }

    const screenshotPath = join(screenshotDirectory, `settings-${testCase.name}.png`);
    await page.screenshot({ path: screenshotPath, fullPage: true });
    assert.equal((await stat(screenshotPath)).size > 10_000, true);
    assert.deepEqual(consoleErrors, []);
    assert.equal(
      expectedApiErrors.filter((message) => message.includes("409 (Conflict)")).length,
      testCase.name === "desktop" ? 2 : 0,
    );
    assert.equal(
      expectedApiErrors.filter((message) => message.includes("500 (Internal Server Error)")).length,
      testCase.name === "desktop" ? 1 : 0,
    );
    assert.deepEqual(pageErrors, []);
    assert.deepEqual(externalRequests, []);
    assert.deepEqual(failedRequests, []);
    results.push({
      name: testCase.name,
      screenshot: basename(screenshotPath),
      overflow: false,
      consoleErrors: 0,
      externalRequests: 0,
    });

    if (testCase.name === "compact") {
      await page.locator("#close-session").click();
      assert.equal(await page.locator("#close-dialog").getAttribute("open"), "");
      await page.locator("#cancel-close").focus();
      await page.keyboard.press("Shift+Tab");
      assert.equal(await page.evaluate(() => document.activeElement?.id), "confirm-close");
      await page.keyboard.press("Tab");
      assert.equal(await page.evaluate(() => document.activeElement?.id), "cancel-close");
      await page.locator("#cancel-close").click();
      assert.equal(await page.evaluate(() => document.activeElement?.id), "close-session");
      await page.locator("#close-session").click();
      let failNextShutdown = true;
      await page.route(`${origin}/api/shutdown`, async (route) => {
        if (failNextShutdown) {
          failNextShutdown = false;
          await route.abort("connectionfailed");
        } else {
          await route.continue();
        }
      });
      await page.locator("#confirm-close").click();
      await page.locator("#close-error").filter({ hasText: "다시 시도" }).waitFor();
      assert.equal(await page.evaluate(() => sessionStorage.length), 1);
      assert.equal(await page.locator("#overview-title").count(), 1);
      assert.equal(await page.locator("#confirm-close").isEnabled(), true);
      await page.locator("#confirm-close").click();
      await page.locator("text=설정 세션이 종료되었습니다").waitFor();
      assert.equal(await page.evaluate(() => sessionStorage.length), 0);
      assert.equal(await page.evaluate(() => {
        const event = new Event("beforeunload", { cancelable: true });
        window.dispatchEvent(event);
        return event.defaultPrevented;
      }), false);
    }
    await page.close();
  }

  const exitCode = await waitForExit(child);
  assert.equal(exitCode, 0, stderr);
  await independentDashboardPage.reload({ waitUntil: "load" });
  assert.equal(await independentDashboardPage.locator("h1").textContent(), "Agent Observability Report");
  results.push({ name: "independent-dashboard", reloadAfterSettingsExit: true });
  await independentDashboardPage.close();
  await stopDashboardProcess(runtimeRoot);
  console.log(JSON.stringify({ executablePath, results }));
} finally {
  if (child.exitCode === null) child.kill("SIGTERM");
  await browser.close();
  if (!process.env.SETTINGS_SCREENSHOT_DIR) {
    await rm(directory, { recursive: true, force: true });
  }
}

async function waitForDashboardUrl(root: string): Promise<string> {
  const canonicalRoot = await realpath(root);
  const digest = createHash("sha256").update(canonicalRoot).digest();
  const port = 49_152 + (digest.readUInt16BE(0) % 12_000);
  const capabilityPath = join(canonicalRoot, "runtime", "dashboard-ui", "capability");
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    const token = await readFile(capabilityPath, "utf8").then((value) => value.trim()).catch(() => "");
    if (/^[0-9a-f]{64}$/.test(token)) {
      return `http://127.0.0.1:${port}/report/${token}`;
    }
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  throw new Error("settings-launched dashboard capability was not created");
}

async function stopDashboardProcess(root: string): Promise<void> {
  const pid = await readDashboardPid(root);
  process.kill(pid, "SIGTERM");
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    try {
      process.kill(pid, 0);
      await new Promise((resolve) => setTimeout(resolve, 25));
    } catch {
      return;
    }
  }
  throw new Error("settings-launched dashboard process did not stop");
}

async function readDashboardPid(root: string): Promise<number> {
  const canonicalRoot = await realpath(root);
  const metadata = await readFile(
    join(canonicalRoot, "runtime", "dashboard-ui", "runtime.meta"),
    "utf8",
  );
  const pid = Number.parseInt(metadata.match(/^pid=(\d+)$/m)?.[1] ?? "", 10);
  if (!Number.isSafeInteger(pid) || pid <= 1) throw new Error("invalid dashboard process metadata");
  return pid;
}

function readUrl(process: SettingsProcess): Promise<string> {
  return new Promise<string>((resolve, reject) => {
    let stdout = "";
    const timeout = setTimeout(() => reject(new Error(`settings URL timed out: ${stderr}`)), 20_000);
    process.once("error", reject);
    process.stdout.setEncoding("utf8");
    process.stdout.on("data", (chunk: string) => {
      stdout += chunk;
      const match = stdout.match(/^url=(.+)$/m);
      const matchedUrl = match?.[1];
      if (matchedUrl) {
        clearTimeout(timeout);
        resolve(matchedUrl.trim());
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

function waitForExit(process: SettingsProcess): Promise<number | null> {
  if (process.exitCode !== null) return Promise.resolve(process.exitCode);
  return new Promise<number | null>((resolve, reject) => {
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

function rectanglesOverlap(first: DOMRect, second: DOMRect): boolean {
  return !(
    first.right <= second.left ||
    second.right <= first.left ||
    first.bottom <= second.top ||
    second.bottom <= first.top
  );
}

type IntegrationStatus = {
  config: "connected" | "disconnected" | "conflict";
  collector: "ready" | "degraded" | "unavailable";
  endpoint?: string;
  service?: string;
  data_retained: boolean;
};

type MockCodexOptions = {
  startConflicted?: boolean;
  startConnected?: boolean;
  failConnect?: boolean;
  conflictDisconnect?: boolean;
  delayedMethod?: "POST" | "DELETE";
  failStatus?: boolean;
  firstStatusUnavailable?: boolean;
  unavailableStatusCount?: number;
};

async function mockCodexApi(page: Page, options: MockCodexOptions = {}) {
  let startLifecycle: (() => void) | undefined;
  const lifecycleStarted = new Promise<void>((resolve) => {
    startLifecycle = resolve;
  });
  let completeLifecycle: (() => void) | undefined;
  const lifecycleCompletion = new Promise<void>((resolve) => {
    completeLifecycle = resolve;
  });
  let releaseLifecycle: (() => void) | undefined;
  const lifecycleRelease = new Promise<void>((resolve) => {
    releaseLifecycle = resolve;
  });
  const lifecycle = {
    status: options.startConflicted
      ? conflictStatus()
      : options.startConnected
        ? connectedStatus()
        : disconnectedStatus(),
    launchAgentRunning: options.startConnected ?? false,
    methods: [] as string[],
    getTimestamps: [] as number[],
    waitForStart: () => lifecycleStarted,
    complete: () => releaseLifecycle?.(),
    waitForCompletion: () => lifecycleCompletion,
  };
  let failConnect = options.failConnect ?? false;
  let conflictDisconnect = options.conflictDisconnect ?? false;
  let firstStatusUnavailable = options.firstStatusUnavailable ?? false;
  let unavailableStatusCount = options.unavailableStatusCount ?? 0;
  await page.route("**/api/integrations/codex", async (route) => {
    const method = route.request().method();
    lifecycle.methods.push(method);
    if (method === "GET") lifecycle.getTimestamps.push(Date.now());
    if (method === "GET" && options.failStatus) {
      await route.fulfill({
        status: 500,
        contentType: "application/json",
        body: JSON.stringify({ code: "integration_failed", message: "fixed integration failure" }),
      });
      return;
    }
    if (method === "GET" && firstStatusUnavailable) {
      firstStatusUnavailable = false;
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ ...connectedStatus(), collector: "unavailable" }),
      });
      return;
    }
    if (method === "GET" && unavailableStatusCount > 0) {
      unavailableStatusCount -= 1;
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ ...connectedStatus(), collector: "unavailable" }),
      });
      return;
    }
    if (method === options.delayedMethod) {
      startLifecycle?.();
      await lifecycleRelease;
    }
    if (method === "POST" && failConnect) {
      failConnect = false;
      await route.fulfill({
        status: 500,
        contentType: "application/json",
        body: JSON.stringify({ code: "integration_failed", message: "injected connect failure" }),
      });
      return;
    }
    if (method === "DELETE" && conflictDisconnect) {
      conflictDisconnect = false;
      await route.fulfill({
        status: 409,
        contentType: "application/json",
        body: JSON.stringify({ code: "integration_conflict", message: "injected disconnect conflict" }),
      });
      return;
    }
    if (method === "POST") {
      lifecycle.launchAgentRunning = true;
      lifecycle.status = connectedStatus();
    } else if (method === "DELETE") {
      lifecycle.launchAgentRunning = false;
      lifecycle.status = disconnectedStatus();
    }
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(lifecycle.status),
    });
    if (method === options.delayedMethod) completeLifecycle?.();
  });
  return lifecycle;
}

function connectedStatus(): IntegrationStatus {
  return {
    config: "connected",
    collector: "ready",
    endpoint: "https://127.0.0.1:4318/v1/logs",
    service: "dev.agent-observability.collector",
    data_retained: true,
  };
}

function degradedStatus(): IntegrationStatus {
  return {
    config: "connected",
    collector: "degraded",
    endpoint: "https://127.0.0.1:4318/v1/logs",
    service: "dev.agent-observability.collector",
    data_retained: true,
  };
}

function disconnectedStatus(): IntegrationStatus {
  return {
    config: "disconnected",
    collector: "unavailable",
    data_retained: true,
  };
}

function conflictStatus(): IntegrationStatus {
  return {
    config: "conflict",
    collector: "unavailable",
    data_retained: true,
  };
}
