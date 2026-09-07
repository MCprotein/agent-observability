import assert from "node:assert/strict";
import { spawn, type ChildProcessByStdio } from "node:child_process";
import { constants } from "node:fs";
import {
  access,
  chmod,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import type { Readable } from "node:stream";
import { chromium, type BrowserContext, type Page } from "playwright-core";

const HANDOFF_SCHEMA_VERSION = "codex_handoff.v1";
const DASHBOARD_SCHEMA_VERSION = "agent_observability.dashboard_query.v1";
const REPORT_LIMIT_BYTES = 32 * 1024 * 1024;
const MAX_HANDOFF_BYTES = 960 * 1024;
const MAX_HANDOFF_LINES = 500;
const DEFAULT_TRACE_COUNT = 2_000;
const LARGE_TRACE_COUNT = 16_000;
const DEFAULT_EXTRA_TOOL_SPANS = 450;
const SYNTHETIC_REPOSITORY = `synthetic-repository-${"x".repeat(235)}`;
const LARGE_SYNTHETIC_REPOSITORY = "synthetic-repository";
const RATE_TABLE_ASSUMPTION_BYTES = 9 * 1024;
const DEFAULT_SMOKE_TIMEOUT_MS = 60_000;
const LARGE_SMOKE_TIMEOUT_MS = 180_000;
const MAX_SMOKE_TIMEOUT_MS = 300_000;
const COMMAND_OUTPUT_LIMIT = 128 * 1024;

type SmokeProfile = "small" | "large";

interface SmokeOptions {
  profile: SmokeProfile;
  timeoutMs: number;
}

interface Snapshot {
  id: string;
  generation: string;
  visibilityEpoch: string;
}

interface DashboardResponse {
  schemaVersion: string;
  kind: string;
  requestKind?: string;
  reason?: string;
  snapshot?: Snapshot;
  rows?: Array<Record<string, unknown>>;
  pagination?: { nextCursor: string | null; total?: number };
  work?: { state: string; kpis?: Record<string, unknown> };
  detail?: Record<string, unknown>;
}

interface FixtureManifest {
  schemaVersion: "agent_observability.paged_dashboard_smoke_fixture.v1";
  traceCount: number;
  expectedSpanCount: number;
  expectedToolCount: number;
  expectedTokenCount: number;
  oldTraceSession: string;
  pagedTraceSession: string;
  files: Array<{ path: string; bytes: number; records: number }>;
  rateTable: { path: string; bytes: number; assumptionBytes: number } | null;
  totalBytes: number;
  totalRecords: number;
}

interface CommandResult {
  code: number;
  stdout: string;
  stderr: string;
}

interface TraversalResult {
  rows: Array<Record<string, unknown>>;
  pages: number;
}

interface DirectQueryEvidence {
  snapshotId: string;
  tracePages: number;
  spanPages: number;
  allTraceRows: number;
}

type RunningChild = ChildProcessByStdio<null, Readable, Readable>;

export async function generateSyntheticFixtureSet(
  fixtureDirectory: string,
  options: {
    traceCount?: number;
    extraToolSpans?: number;
    nowUnixMs?: number;
    repositoryName?: string;
    rateTableAssumptionBytes?: number | null;
  } = {},
): Promise<FixtureManifest> {
  const traceCount = options.traceCount ?? DEFAULT_TRACE_COUNT;
  const extraToolSpans = options.extraToolSpans ?? DEFAULT_EXTRA_TOOL_SPANS;
  const nowUnixMs = options.nowUnixMs ?? Date.now();
  const repositoryName = options.repositoryName ?? SYNTHETIC_REPOSITORY;
  const rateTableAssumptionBytes = options.rateTableAssumptionBytes === undefined
    ? RATE_TABLE_ASSUMPTION_BYTES
    : options.rateTableAssumptionBytes;
  assert.ok(Number.isSafeInteger(traceCount) && traceCount >= 2 && traceCount <= 30_000);
  assert.ok(Number.isSafeInteger(extraToolSpans) && extraToolSpans >= 201 && extraToolSpans <= 500);
  assert.ok(repositoryName.length >= 1 && repositoryName.length <= 256);
  assert.ok(rateTableAssumptionBytes === null || (
    Number.isSafeInteger(rateTableAssumptionBytes)
    && rateTableAssumptionBytes >= 128
    && rateTableAssumptionBytes < 1024 * 1024
  ));
  await mkdir(fixtureDirectory, { recursive: true, mode: 0o700 });
  await chmod(fixtureDirectory, 0o700);

  const files: FixtureManifest["files"] = [];
  let batch = 0;
  let lines: string[] = [];
  let bytes = 0;
  let cursor = 0;
  let totalBytes = 0;
  let totalRecords = 0;

  const flush = async (): Promise<void> => {
    if (lines.length === 0) return;
    const body = `${lines.join("\n")}\n`;
    const path = join(fixtureDirectory, `codex-${String(batch).padStart(3, "0")}.jsonl`);
    await writeFile(path, body, { mode: 0o600, flag: "wx" });
    await chmod(path, 0o600);
    const file = await stat(path);
    assert.ok(file.size <= 1024 * 1024, "synthetic handoff exceeds the adapter byte limit");
    assert.ok(lines.length <= MAX_HANDOFF_LINES, "synthetic handoff exceeds the configured ingest limit");
    files.push({ path: basename(path), bytes: file.size, records: lines.length });
    totalBytes += file.size;
    totalRecords += lines.length;
    batch += 1;
    lines = [];
    bytes = 0;
    cursor = 0;
  };

  const appendTrace = async (records: Array<{
    event: string;
    surface: "notify" | "otel_log";
    attributes: Record<string, unknown>;
    at: number;
  }>): Promise<void> => {
    const encoded = records.map((record, index) => {
      const nextCursor = cursor + index + 1;
      return JSON.stringify({
        schema_version: HANDOFF_SCHEMA_VERSION,
        source_generation: `paged-browser-${batch}`,
        previous_cursor: nextCursor === 1 ? null : String(nextCursor - 1),
        cursor: String(nextCursor),
        surface: record.surface,
        received_at_unix_ms: record.at,
        event_name: record.event,
        attributes: record.attributes,
      });
    });
    const encodedBytes = encoded.reduce((sum, line) => sum + Buffer.byteLength(line) + 1, 0);
    if (lines.length > 0 && (bytes + encodedBytes > MAX_HANDOFF_BYTES || lines.length + encoded.length > MAX_HANDOFF_LINES)) {
      await flush();
      return appendTrace(records);
    }
    assert.ok(encodedBytes <= MAX_HANDOFF_BYTES, "one synthetic trace exceeds a fixture batch");
    assert.ok(encoded.length <= MAX_HANDOFF_LINES, "one synthetic trace exceeds the configured ingest limit");
    lines.push(...encoded);
    bytes += encodedBytes;
    cursor += encoded.length;
  };

  const oldTraceSession = "synthetic-session-00000";
  const pagedTraceSession = `synthetic-session-${String(traceCount - 1).padStart(5, "0")}`;
  for (let index = 0; index < traceCount; index += 1) {
    const session = `synthetic-session-${String(index).padStart(5, "0")}`;
    const turn = `synthetic-turn-${String(index).padStart(5, "0")}`;
    const base = index === 0 ? nowUnixMs - 3 * 86_400_000 : nowUnixMs + index * 2;
    const records: Array<{
      event: string;
      surface: "notify" | "otel_log";
      attributes: Record<string, unknown>;
      at: number;
    }> = [];
    records.push({
      event: "codex.conversation_starts",
      surface: "otel_log",
      at: base,
      attributes: { conversation_id: session, model: "gpt-test" },
    });
    records.push({
      event: "agent-turn-complete",
      surface: "notify",
      at: base + 1,
      attributes: { thread_id: session, turn_id: turn, project_name: repositoryName },
    });
    if (index === traceCount - 1) {
      for (let extra = 0; extra < extraToolSpans; extra += 1) {
        records.push({
          event: "codex.tool_result",
          surface: "otel_log",
          at: base + 2 + extra,
          attributes: {
            conversation_id: session,
            turn_id: turn,
            call_id: `synthetic-extra-${String(extra).padStart(3, "0")}`,
            tool_name: "synthetic-tool",
            success: true,
            duration_ms: 1,
          },
        });
      }
    }
    await appendTrace(records);
  }
  await flush();

  let rateTable: FixtureManifest["rateTable"] = null;
  if (rateTableAssumptionBytes !== null) {
    const assumption = "Synthetic capacity padding only; not billing truth. ".padEnd(
      rateTableAssumptionBytes,
      "x",
    );
    const rateTablePath = join(fixtureDirectory, "synthetic-rate-table.json");
    const rateTableBody = `${JSON.stringify({
      schema_version: "agent_observability.rate_table.v1",
      version: "paged-dashboard-smoke-capacity-only",
      currency: "USD",
      unit: "per_1m_tokens",
      assumption,
      models: {},
    })}\n`;
    await writeFile(rateTablePath, rateTableBody, { mode: 0o600, flag: "wx" });
    await chmod(rateTablePath, 0o600);
    const rateTableBytes = (await stat(rateTablePath)).size;
    assert.ok(rateTableBytes < 1024 * 1024, "synthetic rate table exceeds the CLI byte limit");
    rateTable = {
      path: basename(rateTablePath),
      bytes: rateTableBytes,
      assumptionBytes: Buffer.byteLength(assumption),
    };
  }

  const manifest: FixtureManifest = {
    schemaVersion: "agent_observability.paged_dashboard_smoke_fixture.v1",
    traceCount,
    expectedSpanCount: traceCount * 2 + extraToolSpans,
    expectedToolCount: extraToolSpans,
    expectedTokenCount: 0,
    oldTraceSession,
    pagedTraceSession,
    files,
    rateTable,
    totalBytes,
    totalRecords,
  };
  await writeFile(join(fixtureDirectory, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`, {
    mode: 0o600,
    flag: "wx",
  });
  return manifest;
}

function parseSmokeOptions(arguments_: string[]): SmokeOptions {
  let profile: SmokeProfile = "small";
  let timeoutOverride: number | undefined;
  for (const argument of arguments_) {
    if (argument === "--profile=small") profile = "small";
    else if (argument === "--profile=large") profile = "large";
    else if (argument.startsWith("--timeout-ms=")) {
      timeoutOverride = Number(argument.slice("--timeout-ms=".length));
    } else {
      throw new Error(`unknown paged dashboard smoke option: ${argument}`);
    }
  }
  const timeoutMs = timeoutOverride
    ?? (profile === "large" ? LARGE_SMOKE_TIMEOUT_MS : DEFAULT_SMOKE_TIMEOUT_MS);
  assert.ok(
    Number.isSafeInteger(timeoutMs)
      && timeoutMs >= 30_000
      && timeoutMs <= MAX_SMOKE_TIMEOUT_MS,
    `smoke timeout must be between 30000 and ${MAX_SMOKE_TIMEOUT_MS} ms`,
  );
  return { profile, timeoutMs };
}

async function main(): Promise<void> {
  const startedAt = Date.now();
  const stage = (name: string): void => {
    console.error(`stage=${name} elapsed_ms=${Date.now() - startedAt}`);
  };
  const candidateArgument = process.argv[2];
  if (!candidateArgument) {
    throw new Error("usage: tsx scripts/paged-dashboard-browser-smoke.ts /absolute/path/to/candidate-agentobs [--profile=small|large] [--timeout-ms=N]");
  }
  const smokeOptions = parseSmokeOptions(process.argv.slice(3));
  const binary = resolve(candidateArgument);
  assert.equal(binary, candidateArgument, "candidate binary path must be absolute");
  await access(binary, constants.X_OK);
  const executablePath = chromium.executablePath();
  await access(executablePath, constants.X_OK).catch(() => {
    throw new Error("Pinned Chromium is missing; run npm run setup:browser.");
  });

  const directory = await mkdtemp(join(tmpdir(), "agent-observability-paged-browser-"));
  await chmod(directory, 0o700);
  const runtimeRoot = join(directory, "runtime");
  const fixtureDirectory = join(directory, "fixtures");
  const isolatedHome = join(directory, "home");
  await mkdir(isolatedHome, { mode: 0o700 });
  const environment = {
    ...process.env,
    HOME: isolatedHome,
    XDG_CACHE_HOME: join(isolatedHome, "cache"),
    XDG_CONFIG_HOME: join(isolatedHome, "config"),
    XDG_DATA_HOME: join(isolatedHome, "data"),
    TMPDIR: directory,
  };
  const deadline = startedAt + smokeOptions.timeoutMs;
  let dashboard: RunningChild | undefined;
  let browser: Awaited<ReturnType<typeof chromium.launch>> | undefined;

  try {
    const manifest = await generateSyntheticFixtureSet(fixtureDirectory, smokeOptions.profile === "large" ? {
      traceCount: LARGE_TRACE_COUNT,
      repositoryName: LARGE_SYNTHETIC_REPOSITORY,
      rateTableAssumptionBytes: null,
    } : {});
    if (smokeOptions.profile === "large") {
      assert.ok(manifest.expectedSpanCount > 32_317);
      assert.equal(manifest.rateTable, null);
    }
    stage("fixtures_ready");
    await runChecked(binary, ["init", runtimeRoot], environment, deadline);
    await runChecked(binary, ["config", "set", runtimeRoot, "batch-records", String(MAX_HANDOFF_LINES)], environment, deadline);
    let ingestedObservations = 0;
    for (const file of manifest.files) {
      const ingest = await runChecked(binary, ["codex-ingest", runtimeRoot, join(fixtureDirectory, file.path)], environment, deadline);
      assert.match(ingest.stdout, /collection_disabled=0(?:\n|$)/);
      assert.match(ingest.stdout, /policy_blocked=0(?:\n|$)/);
      assert.match(ingest.stdout, /pressure_blocked=0(?:\n|$)/);
      assert.match(ingest.stdout, /storage_blocked=0(?:\n|$)/);
      const observations = Number(/^observations=(\d+)$/m.exec(ingest.stdout)?.[1]);
      assert.equal(observations, file.records, `fixture admission mismatch for ${file.path}`);
      ingestedObservations += observations;
    }
    assert.equal(ingestedObservations, manifest.totalRecords);
    stage("ingest_complete");

    const reportArguments = manifest.rateTable
      ? ["report", runtimeRoot, join(fixtureDirectory, manifest.rateTable.path)]
      : ["report", runtimeRoot];
    const report = await runCommand(binary, reportArguments, environment, deadline);
    const reportPath = join(runtimeRoot, "logs", "agent-observability-report.html");
    const reportBytes = await stat(reportPath).then((value) => value.size).catch(() => 0);
    assert.notEqual(
      report.code,
      0,
      `the synthetic fixture must exceed the manual HTML export contract; exporter produced ${reportBytes} bytes`,
    );
    assert.match(`${report.stdout}\n${report.stderr}`, /report artifact exceeds the 32 MiB contract/);
    assert.ok(reportBytes <= REPORT_LIMIT_BYTES, "failed export must not publish an oversized artifact");
    stage("manual_export_capacity_verified");

    dashboard = spawn(binary, ["dashboard", runtimeRoot, "--no-open"], {
      env: environment,
      stdio: ["ignore", "pipe", "pipe"],
    });
    const dashboardUrl = await readDashboardUrl(dashboard, deadline);
    stage("dashboard_ready");
    const parsed = new URL(dashboardUrl);
    assert.equal(parsed.hostname, "127.0.0.1");
    assert.match(parsed.pathname, /^\/report\/[0-9a-f]{64}$/);
    const directBootstrap = await queryHttp(dashboardUrl, { kind: "bootstrap" }, deadline);
    assert.ok(directBootstrap.snapshot);
    const directSummary = await completeSummaryHttp(dashboardUrl, directBootstrap.snapshot.id, {}, deadline);
    assert.equal(directSummary.sessions, manifest.traceCount);
    assert.equal(directSummary.turns, manifest.traceCount);
    assert.equal(directSummary.tools, manifest.expectedToolCount);
    stage("api_summary_complete");
    const directTraces = await traverseRowsHttp(dashboardUrl, {
      kind: "traces",
      snapshotId: directBootstrap.snapshot.id,
    }, deadline);
    assert.equal(directTraces.rows.length, manifest.traceCount);
    assert.equal(new Set(directTraces.rows.map((row) => String(row.traceId))).size, manifest.traceCount);
    assert.equal(
      directTraces.rows.reduce((sum, row) => sum + Number(row.spanCount), 0),
      manifest.expectedSpanCount,
    );
    const pagedTrace = directTraces.rows.find((row) => Number(row.spanCount) > 200);
    assert.ok(pagedTrace, "synthetic paged trace was not returned");
    const directSpans = await traverseRowsHttp(dashboardUrl, {
      kind: "spans",
      snapshotId: directBootstrap.snapshot.id,
      traceId: String(pagedTrace.traceId),
    }, deadline);
    assert.equal(directSpans.rows.length, 2 + DEFAULT_EXTRA_TOOL_SPANS);
    assert.equal(new Set(directSpans.rows.map((row) => String(row.spanId))).size, directSpans.rows.length);
    assert.ok(directSpans.pages >= 2, "span traversal must cross a server cursor");
    const selected = directSpans.rows[0];
    assert.ok(selected);
    const detail = await queryHttp(dashboardUrl, {
      kind: "span",
      snapshotId: directBootstrap.snapshot.id,
      traceId: String(pagedTrace.traceId),
      spanId: String(selected.spanId),
    }, deadline);
    assert.equal(detail.kind, "span");
    assert.equal(detail.detail?.spanId, selected.spanId);
    const filteredSummary = await completeSummaryHttp(
      dashboardUrl,
      directBootstrap.snapshot.id,
      { agent: ["codex"] },
      deadline,
    );
    assert.equal(filteredSummary.sessions, manifest.traceCount);
    assert.equal(filteredSummary.turns, manifest.traceCount);
    assert.equal(filteredSummary.llm, 0);
    assert.equal(filteredSummary.tools, manifest.expectedToolCount);
    assert.equal(filteredSummary.totalTokens, null);
    const directEvidence: DirectQueryEvidence = {
      snapshotId: directBootstrap.snapshot.id,
      tracePages: directTraces.pages,
      spanPages: directSpans.pages,
      allTraceRows: directTraces.rows.length,
    };
    stage("api_traversal_complete");

    browser = await chromium.launch({ executablePath, headless: true, env: environment });
    const context = await browser.newContext();
    context.setDefaultTimeout(15_000);
    const page = await context.newPage();
    const browserEvidence = await withDeadline(
      exerciseBrowser(page, context, dashboardUrl, manifest, directEvidence, binary, runtimeRoot, directory, environment, deadline),
      deadline,
    );
    stage("browser_complete");

    const manifestBody = await readFile(join(fixtureDirectory, "manifest.json"), "utf8");
    assert.equal(manifestBody.includes("prompt"), false);
    assert.equal(manifestBody.includes("assistant"), false);
    console.log(JSON.stringify({
      executablePath,
      candidateBinary: binary,
      fixture: {
        traces: manifest.traceCount,
        spans: manifest.expectedSpanCount,
        records: manifest.totalRecords,
        handoffBytes: manifest.totalBytes,
        files: manifest.files.length,
        rateTableBytes: manifest.rateTable?.bytes ?? null,
        rateTableAssumptionBytes: manifest.rateTable?.assumptionBytes ?? null,
      },
      profile: smokeOptions.profile,
      manualExport: manifest.rateTable === null
        ? "32_mib_capacity_rejected_without_rate_table"
        : "32_mib_capacity_rejected_with_synthetic_rate_table",
      dashboard: browserEvidence,
      elapsedMs: Date.now() - startedAt,
    }));
  } finally {
    if (dashboard && dashboard.exitCode === null) dashboard.kill("SIGTERM");
    if (dashboard) await waitForExit(dashboard).catch(() => undefined);
    await browser?.close().catch(() => undefined);
    await rm(directory, { recursive: true, force: true });
  }
}

async function completeSummaryHttp(
  dashboardUrl: string,
  snapshotId: string,
  filters: Record<string, unknown>,
  deadline: number,
): Promise<Record<string, unknown>> {
  let cursor: string | null = null;
  for (let slice = 0; slice < 256; slice += 1) {
    const response = await queryHttp(dashboardUrl, { kind: "summary", snapshotId, cursor, filters }, deadline);
    assert.equal(response.kind, "summary");
    if (response.work?.state === "complete") {
      assert.ok(response.work.kpis);
      return response.work.kpis;
    }
    cursor = response.pagination?.nextCursor ?? null;
    assert.ok(cursor, "pending HTTP summary must provide a continuation cursor");
  }
  throw new Error("HTTP summary did not complete within 256 bounded slices");
}

async function queryHttp(
  dashboardUrl: string,
  request: Record<string, unknown>,
  deadline: number,
): Promise<DashboardResponse> {
  for (let attempt = 0; attempt < 9; attempt += 1) {
    const payload = dashboardPayload(request);
    const endpoint = `${dashboardUrl}/query?request=${encodeURIComponent(JSON.stringify(payload))}`;
    const response = await fetch(endpoint, {
      headers: { Accept: "application/json" },
      method: "GET",
    });
    const body = await response.json() as DashboardResponse;
    assert.equal(body.schemaVersion, DASHBOARD_SCHEMA_VERSION);
    assert.ok(response.ok || body.kind === "status", `HTTP query failed with status ${response.status}`);
    if (body.kind !== "status" || body.reason !== "busy" || attempt === 8) return body;
    const remaining = deadline - Date.now();
    if (remaining <= 0) throw new Error("paged dashboard smoke exceeded its configured deadline");
    await new Promise((resolvePromise) => setTimeout(resolvePromise, Math.min(250, remaining)));
  }
  throw new Error("bounded dashboard busy retry exhausted");
}

async function traverseRowsHttp(
  dashboardUrl: string,
  request: Record<string, unknown>,
  deadline: number,
): Promise<TraversalResult> {
  const rows: Array<Record<string, unknown>> = [];
  let cursor: string | null = null;
  for (let pageNumber = 0; pageNumber < 512; pageNumber += 1) {
    const response = await queryHttp(dashboardUrl, { ...request, cursor }, deadline);
    if (response.kind !== request.kind) {
      throw new Error(
        `dashboard traversal expected kind=${safeStatusValue(request.kind)}, received kind=${safeStatusValue(response.kind)}, reason=${safeStatusValue(response.reason)}, page=${pageNumber + 1}`,
      );
    }
    rows.push(...(response.rows ?? []));
    const next = response.pagination?.nextCursor ?? null;
    if (!next) return { rows, pages: pageNumber + 1 };
    cursor = next;
  }
  throw new Error(`${safeStatusValue(request.kind)} traversal exceeded 512 pages`);
}

function safeStatusValue(value: unknown): string {
  return typeof value === "string" && /^[a-z_]{1,64}$/.test(value) ? value : "unknown";
}

async function withDeadline<T>(work: Promise<T>, deadline: number): Promise<T> {
  const remaining = deadline - Date.now();
  if (remaining <= 0) throw new Error("paged dashboard smoke exceeded its configured deadline");
  let timer: NodeJS.Timeout | undefined;
  try {
    return await Promise.race([
      work,
      new Promise<never>((_resolve, reject) => {
        timer = setTimeout(
          () => reject(new Error("paged dashboard smoke exceeded its configured deadline")),
          remaining,
        );
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

async function exerciseBrowser(
  page: Page,
  context: BrowserContext,
  dashboardUrl: string,
  manifest: FixtureManifest,
  directEvidence: DirectQueryEvidence,
  binary: string,
  runtimeRoot: string,
  temporaryRoot: string,
  environment: NodeJS.ProcessEnv,
  deadline: number,
): Promise<Record<string, unknown>> {
  const origin = new URL(dashboardUrl).origin;
  const consoleErrors: string[] = [];
  const pageErrors: string[] = [];
  const externalRequests: string[] = [];
  const failedRequests: string[] = [];
  const requestedPaths: string[] = [];
  const queryFailures: Array<{ kind: string; reason: string }> = [];
  let popupCount = 0;
  let delayedRequestSeenResolve!: () => void;
  let delayedRouteFinishedResolve!: () => void;
  const delayedRequestSeen = new Promise<void>((resolvePromise) => { delayedRequestSeenResolve = resolvePromise; });
  const delayedRouteFinished = new Promise<void>((resolvePromise) => { delayedRouteFinishedResolve = resolvePromise; });
  let delayArmed = true;

  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  page.on("pageerror", (error) => pageErrors.push(error.message));
  page.on("popup", () => { popupCount += 1; });
  context.on("page", (openedPage) => {
    if (openedPage !== page) popupCount += 1;
  });
  page.on("request", (request) => {
    const url = new URL(request.url());
    requestedPaths.push(url.pathname);
    if (url.origin !== origin) externalRequests.push(request.url());
  });
  page.on("requestfailed", (request) => {
    if (decodeDashboardRequest(request.url()).filters?.text !== "delayed-no-match") {
      failedRequests.push(request.url());
    }
  });
  page.on("response", async (response) => {
    if (!new URL(response.url()).pathname.endsWith("/query")) return;
    const body = await response.json().catch(() => undefined) as DashboardResponse | undefined;
    if (body?.kind === "status" && body.reason !== "busy" && body.reason !== "building") {
      const request = decodeDashboardRequest(response.url());
      queryFailures.push({
        kind: safeStatusValue(request.kind),
        reason: safeStatusValue(body.reason),
      });
    }
  });
  await page.route("**/query?**", async (route) => {
    const request = decodeDashboardRequest(route.request().url());
    if (delayArmed && request.filters?.text === "delayed-no-match") {
      delayArmed = false;
      delayedRequestSeenResolve();
      await new Promise((resolvePromise) => setTimeout(resolvePromise, 250));
      await route.continue().catch(() => undefined);
      delayedRouteFinishedResolve();
      return;
    }
    await route.continue();
  });

  await page.goto(dashboardUrl, { waitUntil: "load" });
  assert.equal(await page.locator("h1").textContent(), "Agent Observability");
  await assertKpi(page, "kpi-sessions", manifest.traceCount).catch((error: unknown) => {
    throw new Error(`initial dashboard KPI failed; queryFailures=${JSON.stringify(queryFailures)}`, {
      cause: error,
    });
  });
  await assertKpi(page, "kpi-turns", manifest.traceCount);
  await assertKpi(page, "kpi-llm", 0);
  await assertKpi(page, "kpi-tools", manifest.expectedToolCount);
  await page.waitForFunction(() => document.getElementById("kpi-tokens")?.textContent === "Unavailable");
  assert.match((await page.locator("#quality-summary").textContent()) ?? "", /Hot\/warm data; cold excluded/);

  await page.locator("#text-filter").fill("delayed-no-match");
  await delayedRequestSeen;
  await page.locator("#text-filter").fill("");
  await page.locator("#agent-filter").selectOption("codex");
  await delayedRouteFinished;
  await assertKpi(page, "kpi-sessions", manifest.traceCount);
  assert.equal(await page.locator("#agent-filter").inputValue(), "codex");
  assert.equal(await page.locator(".trace-row").count() > 0, true);

  const expectedPagedTraceSpans = 2 + DEFAULT_EXTRA_TOOL_SPANS;
  const uiTracePages = await selectTraceAcrossPages(page, `${expectedPagedTraceSpans} spans`);
  await page.waitForFunction(() => {
    const button = document.getElementById("span-next") as HTMLButtonElement | null;
    return button !== null && !button.disabled;
  });
  await page.locator("#span-table .span-open").first().click();
  await page.locator("#span-details.open").waitFor();
  assert.match((await page.locator("#details-body").textContent()) ?? "", /Safe projected detail/);
  await page.locator("#span-next").click();
  await page.waitForFunction(() => document.getElementById("span-page-status")?.textContent?.startsWith("Page 2"));
  assert.equal(await page.locator("#span-table .span-open").count(), expectedPagedTraceSpans - 200);

  const deletionPage = await queryHttp(dashboardUrl, {
    kind: "traces",
    snapshotId: directEvidence.snapshotId,
  }, deadline);
  assert.equal(deletionPage.kind, "traces");
  const staleCursor = deletionPage.pagination?.nextCursor ?? null;
  assert.ok(staleCursor, "large trace result must produce a cursor for deletion invalidation");
  await runChecked(binary, ["config", "set", runtimeRoot, "retention-days", "1"], environment, deadline);
  const plan = await runChecked(binary, ["retention-plan", runtimeRoot], environment, deadline);
  assert.match(plan.stdout, /traces=1(?:\n|$)/);
  const planId = /^plan_id=(\S+)$/m.exec(plan.stdout)?.[1];
  assert.ok(planId);
  const archive = join(temporaryRoot, "deleted-trace-archive.jsonl");
  const applied = await runChecked(binary, ["retention-apply", runtimeRoot, planId, archive], environment, deadline);
  assert.match(applied.stdout, /applied=1(?:\n|$)/);
  const revoked = await queryHttp(dashboardUrl, {
    kind: "traces",
    snapshotId: directEvidence.snapshotId,
    cursor: staleCursor,
  }, deadline);
  assert.equal(revoked.kind, "status");
  assert.equal(revoked.reason, "refresh_pending");

  assert.equal(requestedPaths.includes("/api/dashboard/open"), false);
  assert.equal(context.pages().length, 1);
  assert.equal(popupCount, 0);
  assert.deepEqual(consoleErrors, []);
  assert.deepEqual(pageErrors, []);
  assert.deepEqual(externalRequests, []);
  assert.deepEqual(failedRequests, []);
  return {
    origin,
    tracePages: directEvidence.tracePages,
    uiTracePages,
    spanPages: directEvidence.spanPages,
    allTraceRows: directEvidence.allTraceRows,
    allProjectedSpans: manifest.expectedSpanCount,
    filteredKpis: "exact",
    lateFilterResponse: "ignored",
    deletionCursor: revoked.reason,
    externalRequests: 0,
    escapedOpenRequests: 0,
  };
}

async function selectTraceAcrossPages(page: Page, expectedText: string): Promise<number> {
  for (let pageNumber = 1; pageNumber <= 32; pageNumber += 1) {
    const matchingTrace = page.locator(".trace-row", { hasText: expectedText });
    if (await matchingTrace.count() > 0) {
      await matchingTrace.first().click();
      return pageNumber;
    }
    const next = page.locator("#trace-next");
    if (await next.isDisabled()) break;
    const priorStatus = await page.locator("#trace-page-status").textContent();
    await next.click();
    await page.waitForFunction(
      (prior) => document.getElementById("trace-page-status")?.textContent !== prior,
      priorStatus,
    );
  }
  throw new Error("paged synthetic trace was not found within 32 bounded UI pages");
}

async function assertKpi(page: Page, id: string, expected: number): Promise<void> {
  await page.waitForFunction(
    ({ elementId, value }) => {
      const text = document.getElementById(elementId)?.textContent ?? "";
      return text.startsWith("Exact") && Number(text.replace(/\D/g, "")) === value;
    },
    { elementId: id, value: expected },
  ).catch(async (error: unknown) => {
    const actual = (await page.locator(`#${id}`).textContent()) ?? "<missing>";
    const quality = (await page.locator("#quality-summary").textContent()) ?? "<missing>";
    const status = (await page.locator("#filter-status").textContent()) ?? "<missing>";
    throw new Error(`${id} expected Exact ${expected}, received ${actual}; status=${status}; quality=${quality}`, {
      cause: error,
    });
  });
  const text = (await page.locator(`#${id}`).textContent()) ?? "";
  assert.equal(Number(text.replace(/\D/g, "")), expected, `${id} must cover the complete filtered dataset`);
}

async function query(page: Page, dashboardUrl: string, request: Record<string, unknown>): Promise<DashboardResponse> {
  const payload = dashboardPayload(request);
  const response = await page.evaluate(async ({ url, encoded }) => {
    const result = await fetch(`${url}/query?request=${encodeURIComponent(encoded)}`, {
      credentials: "same-origin",
      method: "GET",
    });
    return { ok: result.ok, status: result.status, body: await result.text() };
  }, { url: dashboardUrl, encoded: JSON.stringify(payload) });
  const body = JSON.parse(response.body) as DashboardResponse;
  assert.equal(body.schemaVersion, DASHBOARD_SCHEMA_VERSION);
  assert.ok(response.ok || body.kind === "status", `query failed with HTTP ${response.status}`);
  return body;
}

function dashboardPayload(request: Record<string, unknown>): Record<string, unknown> {
  const payload: Record<string, unknown> = { schemaVersion: DASHBOARD_SCHEMA_VERSION, ...request };
  if (payload.cursor === null) delete payload.cursor;
  return payload;
}

function decodeDashboardRequest(url: string): Record<string, any> {
  const encoded = new URL(url).searchParams.get("request");
  return encoded ? JSON.parse(encoded) as Record<string, any> : {};
}

async function runChecked(
  binary: string,
  arguments_: string[],
  environment: NodeJS.ProcessEnv,
  deadline: number,
): Promise<CommandResult> {
  const result = await runCommand(binary, arguments_, environment, deadline);
  assert.equal(result.code, 0, `${basename(binary)} ${arguments_[0] ?? ""} failed: ${result.stderr || result.stdout}`);
  return result;
}

function runCommand(
  binary: string,
  arguments_: string[],
  environment: NodeJS.ProcessEnv,
  deadline: number,
): Promise<CommandResult> {
  return new Promise((resolvePromise, reject) => {
    const remaining = deadline - Date.now();
    if (remaining <= 0) {
      reject(new Error("paged dashboard smoke exceeded its configured deadline"));
      return;
    }
    const child = spawn(binary, arguments_, { env: environment, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    const append = (current: string, chunk: string): string =>
      (current + chunk).slice(-COMMAND_OUTPUT_LIMIT);
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => { stdout = append(stdout, chunk); });
    child.stderr.on("data", (chunk: string) => { stderr = append(stderr, chunk); });
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error(`${basename(binary)} ${arguments_[0] ?? ""} exceeded the smoke deadline`));
    }, remaining);
    child.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.once("exit", (code) => {
      clearTimeout(timer);
      resolvePromise({ code: code ?? -1, stdout, stderr });
    });
  });
}

function readDashboardUrl(child: RunningChild, deadline: number): Promise<string> {
  return new Promise((resolvePromise, reject) => {
    let output = "";
    const timer = setTimeout(() => reject(new Error("dashboard URL was not emitted")), Math.max(1, deadline - Date.now()));
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => {
      output += chunk;
      const match = /^url=(http:\/\/127\.0\.0\.1:\d+\/\S+)$/m.exec(output);
      if (!match?.[1] || !/^status=dashboard_ready$/m.test(output) || !/^opened=false$/m.test(output)) return;
      clearTimeout(timer);
      resolvePromise(match[1]);
    });
    child.once("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(`dashboard exited before URL emission (${String(code)})`));
    });
  });
}

function waitForExit(child: RunningChild): Promise<number | null> {
  if (child.exitCode !== null) return Promise.resolve(child.exitCode);
  return new Promise((resolvePromise, reject) => {
    const timer = setTimeout(() => reject(new Error("dashboard process did not exit")), 5_000);
    child.once("exit", (code) => {
      clearTimeout(timer);
      resolvePromise(code);
    });
  });
}

const invokedPath = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : undefined;
if (invokedPath === import.meta.url) {
  await main();
}
