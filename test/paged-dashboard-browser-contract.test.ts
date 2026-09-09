import assert from "node:assert/strict";
import { chmod, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { consumeExpectedSmokeCancellation, generateSyntheticFixtureSet, isCompletedResponseCancellation, isExpectedSmokeCancellation } from "../scripts/paged-dashboard-browser-smoke.ts";

test("completed-response cancellation requires exact version, body and network proof", () => {
  const valid: Parameters<typeof isCompletedResponseCancellation>[0] = {
    browserVersion: "151.0.7922.34",
    error: "net::ERR_ABORTED", token: "1", tokenMultiplicity: 1,
    sourceAborted: false, dispatchedAborted: false, fetchRejected: false,
    body: { token: "1", status: 200, declared: "458", encoding: null, bytes: 458, terminal: "complete", schemaValidated: true, responseKindMatches: true },
    network: [{ startStage: "bootstrap", events: ["request", "aborted"], canceled: true }],
  };
  assert.equal(isCompletedResponseCancellation(valid), true);
  for (const patch of [
    { browserVersion: "152.0.0.0" }, { error: "net::ERR_CONNECTION_RESET" },
    { error: undefined }, { token: undefined }, { token: "" }, { token: "2" },
    { tokenMultiplicity: 0 }, { tokenMultiplicity: 2 },
    { sourceAborted: true }, { dispatchedAborted: true }, { fetchRejected: true },
    { body: undefined }, { network: undefined }, { network: [] },
    { network: [...valid.network!, ...valid.network!] },
  ]) assert.equal(isCompletedResponseCancellation({ ...valid, ...patch }), false);
  for (const patch of [
    { token: "2" }, { status: 500 }, { schemaValidated: false }, { responseKindMatches: false },
    { declared: null }, { declared: "0458" }, { declared: "458.0" },
    { declared: "458x" }, { declared: "459" }, { encoding: "gzip" },
    { bytes: 457 }, { bytes: Number.NaN }, { bytes: 0, declared: "0" },
    { bytes: 1048577, declared: "1048577" },
    { terminal: "reading" }, { terminal: "cancel" }, { terminal: "error" },
  ]) assert.equal(isCompletedResponseCancellation({ ...valid, body: { ...valid.body!, ...patch } }), false);
  for (const entry of [
    { events: ["request", "finished"], canceled: true },
    { events: ["request", "aborted", "finished"], canceled: true },
    { events: ["request", "request_restarted", "aborted"], canceled: true },
    { events: ["request", "aborted"], canceled: false },
    { events: ["request", "aborted"] },
  ]) assert.equal(isCompletedResponseCancellation({ ...valid, network: [{ startStage: "bootstrap", ...entry }] }), false);
});

test("smoke cancellation consumes only the exact causal token once", () => {
  const expected = new Set(["1", "2"]);
  assert.equal(consumeExpectedSmokeCancellation("net::ERR_ABORTED", "3", expected), false);
  assert.equal(consumeExpectedSmokeCancellation("net::ERR_ABORTED", undefined, expected), false);
  assert.equal(consumeExpectedSmokeCancellation("net::ERR_CONNECTION_RESET", "1", expected), false);
  assert.equal(expected.has("1"), true);
  assert.equal(consumeExpectedSmokeCancellation("net::ERR_ABORTED", "1", expected), true);
  assert.equal(consumeExpectedSmokeCancellation("net::ERR_ABORTED", "1", expected), false);
  assert.deepEqual([...expected], ["2"]);
});

test("smoke accepts only explicitly marked request aborts, not ordinary network failures", () => {
  assert.equal(isExpectedSmokeCancellation("net::ERR_ABORTED", true), true);
  assert.equal(isExpectedSmokeCancellation("net::ERR_ABORTED", false), false);
  assert.equal(isExpectedSmokeCancellation("net::ERR_CONNECTION_RESET", true), false);
  assert.equal(isExpectedSmokeCancellation(undefined, true), false);
});

test("CI paged smoke explicitly budgets setup and browser traversal without changing product limits", async () => {
  const manifest = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8")) as {
    scripts: Record<string, string>;
  };
  assert.match(manifest.scripts["test:paged-dashboard-browser"]!, /--profile=small --timeout-ms=180000$/);
});

test("paged browser smoke fixture stays bounded, synthetic, and cursor-valid by construction", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-observability-paged-contract-"));
  await chmod(root, 0o700);
  try {
    const manifest = await generateSyntheticFixtureSet(root, {
      traceCount: 2,
      extraToolSpans: 201,
      nowUnixMs: 1_800_000_000_000,
    });
    assert.equal(manifest.traceCount, 2);
    assert.equal(manifest.expectedSpanCount, 205);
    assert.equal(manifest.expectedToolCount, 201);
    assert.equal(manifest.files.length, 1);
    assert.ok(manifest.totalBytes < 1024 * 1024);
    assert.ok(manifest.totalRecords <= 500);
    assert.ok(manifest.rateTable);
    assert.equal(manifest.rateTable.assumptionBytes, 9 * 1024);
    assert.ok(manifest.rateTable.bytes < 1024 * 1024);

    const body = await readFile(join(root, manifest.files[0]!.path), "utf8");
    const records = body.trimEnd().split("\n").map((line) => JSON.parse(line) as Record<string, unknown>);
    assert.equal(records[0]?.previous_cursor, null);
    for (let index = 1; index < records.length; index += 1) {
      assert.equal(records[index]?.previous_cursor, String(index));
      assert.equal(records[index]?.cursor, String(index + 1));
    }
    for (const record of records) {
      const attributes = record.attributes as Record<string, unknown>;
      for (const forbidden of ["prompt", "output", "input_messages", "last_assistant_message", "cwd"]) {
        assert.equal(forbidden in attributes, false);
      }
      if (typeof attributes.project_name === "string") {
        assert.equal(attributes.project_name.length, 256);
      }
    }
    assert.equal(body.includes("/Users/"), false);
    const rateTable = JSON.parse(
      await readFile(join(root, manifest.rateTable.path), "utf8"),
    ) as Record<string, unknown>;
    assert.equal(rateTable.schema_version, "agent_observability.rate_table.v1");
    assert.equal(rateTable.version, "paged-dashboard-smoke-capacity-only");
    assert.deepEqual(rateTable.models, {});
    assert.match(String(rateTable.assumption), /^Synthetic capacity padding only; not billing truth\./);
    assert.equal(Buffer.byteLength(String(rateTable.assumption)), 9 * 1024);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("paged browser smoke requires a candidate binary and forbids real-browser or opener paths", async () => {
  const source = await readFile(new URL("../scripts/paged-dashboard-browser-smoke.ts", import.meta.url), "utf8");
  assert.match(source, /const candidateArgument = process\.argv\[2\]/);
  assert.match(source, /chromium\.executablePath\(\)/);
  assert.match(source, /headless: true/);
  assert.match(source, /mkdtemp\(join\(tmpdir\(\), "agent-observability-paged-browser-"\)\)/);
  assert.match(source, /\["dashboard", runtimeRoot, "--no-open"\]/);
  assert.match(source, /HOME: isolatedHome/);
  assert.match(source, /const DEFAULT_SMOKE_TIMEOUT_MS = 60_000/);
  assert.match(source, /const LARGE_SMOKE_TIMEOUT_MS = 180_000/);
  assert.match(source, /const LARGE_TRACE_COUNT = 16_000/);
  assert.match(source, /assert\.ok\(manifest\.expectedSpanCount > 32_317\)/);
  assert.match(source, /rateTableAssumptionBytes: null/);
  assert.match(source, /await rm\(directory, \{ recursive: true, force: true \}\)/);
  assert.match(source, /requestedPaths\.includes\("\/api\/dashboard\/open"\)/);
  assert.match(source, /received kind=.*reason=.*page=/);
  assert.match(source, /selectTraceAcrossPages\(page, `\$\{expectedPagedTraceSpans\} spans`, directEvidence\.tracePages\)/);
  assert.match(source, /verifiedPageCount <= 512/);
  assert.match(source, /pageNumber <= verifiedPageCount/);
  assert.doesNotMatch(source, /queryFailures\.push\(\{ request:/);
  assert.doesNotMatch(source, /cargo["'], \["build/);
  assert.doesNotMatch(source, /channel:\s*["']chrome["']/);
  assert.doesNotMatch(source, /\/Applications\/Google Chrome/);
  assert.doesNotMatch(source, /open_local_target|osascript|xdg-open|cmd\.exe/);
});
