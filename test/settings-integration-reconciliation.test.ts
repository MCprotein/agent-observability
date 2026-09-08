import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { createContext, runInContext } from "node:vm";
import { transform } from "esbuild";
import validateIntegrationError from "../ui/settings/generated/validate-codex-integration-error-v1.js";
import { validateCodexIntegrationStatus } from "../ui/settings/integration-status-validation.js";

test("failed integration mutation stays locked until status reconciles, or offers retry", async () => {
  const source = await readFile("ui/settings/main.ts", "utf8");
  const functions = source.slice(source.indexOf("async function toggleIntegration()"), source.indexOf("async function refreshIntegrationStatus()"));
  for (const method of ["POST", "DELETE"]) {
    for (const unavailable of [false, true]) {
      let finishStatus!: (value: unknown) => void;
      const status = new Promise((resolve) => { finishStatus = resolve; });
      const calls: string[] = [];
      const context = createContext({
        busy: false, token: "session", integrationRequestGeneration: 0,
        integration: { config: method === "POST" ? "disconnected" : "connected" },
        integrationUnavailable: false, disabled: false,
        integrationApi: async (_path: string, init?: RequestInit) => {
          calls.push(init?.method ?? "GET");
          if (init?.method) throw Object.assign(new Error("committed; verification unavailable"), { code: "integration_connect_committed_unverified" });
          await status;
          if (unavailable) throw new Error("status unavailable");
          return { config: method === "POST" ? "connected" : "disconnected" };
        },
        messageOf: (error: Error) => error.message, showToast: () => {}, expireSession: () => {},
      });
      runInContext("function setBusy(value) { disabled = value; } function renderSettings() { disabled = busy; }", context);
      runInContext((await transform(functions, { loader: "ts", target: "es2022" })).code, context);
      const pending = runInContext("toggleIntegration()", context) as Promise<void>;
      await new Promise((resolve) => setImmediate(resolve));
      assert.deepEqual(calls, [method, "GET"]);
      assert.equal(context.disabled, true);
      assert.equal(context.integration, null);
      await runInContext("toggleIntegration()", context);
      assert.equal(calls.length, 2);
      finishStatus(undefined);
      await pending;
      assert.equal(context.busy, false);
      assert.equal(context.disabled, false);
      assert.equal(context.integrationUnavailable, unavailable);
      if (unavailable) {
        assert.equal(context.integration, null);
        await runInContext("toggleIntegration()", context);
        assert.equal(calls.length, 2);
        context.integrationApi = async () => {
          calls.push("GET");
          return { config: "connected" };
        };
        await runInContext("refreshIntegration()", context);
        assert.equal(context.integrationUnavailable, false);
        assert.equal(calls.at(-1), "GET");
      } else {
        assert.equal((context.integration as { config: string } | null)?.config, method === "POST" ? "connected" : "disconnected");
      }
    }
  }
});

test("integration error contract is closed and unavailable panel offers status only", async () => {
  const schema = JSON.parse(await readFile("contracts/codex-integration-error-v1.schema.json", "utf8"));
  for (const code of schema.properties.code.enum) {
    assert.equal(validateIntegrationError({ code, message: "sanitized outcome" }), true);
    assert.equal(validateIntegrationError({ code, message: "sanitized outcome", token: "secret" }), false);
  }
  assert.equal(validateIntegrationError({ code: "unknown_outcome", message: "unknown" }), false);
  const source = await readFile("ui/settings/main.ts", "utf8");
  const panel = source.slice(source.indexOf("function integrationPanel()"), source.indexOf("function integrationDegradedCopy("));
  const context = createContext({ integration: null, integrationUnavailable: true,
    integrationDegradedCopy: () => ({}), escapeHtml: (value: string) => value });
  runInContext((await transform(panel, { loader: "ts" })).code, context);
  const html = runInContext("integrationPanel()", context) as string;
  assert.match(html, /id="refresh-integration"/);
  assert.doesNotMatch(html, /id="toggle-integration"/);
  assert.match(html, /연결 변경을 잠갔습니다/);
});

test("status reconciliation rejects malformed status and bounds request time", async () => {
  const source = await readFile("ui/settings/main.ts", "utf8");
  const fn = source.slice(source.indexOf("async function integrationApi("), source.indexOf("function applyEnvelope("));
  let timeout = 0;
  const signal = new AbortController().signal;
  const context = createContext({ validateCodexIntegrationStatus, validateIntegrationError,
    AbortSignal: { timeout: (milliseconds: number) => { timeout = milliseconds; return signal; } },
    api: async (_path: string, init: RequestInit) => {
      assert.equal(init.signal, signal);
      return { config: "connected" };
    },
  });
  runInContext((await transform(fn, { loader: "ts" })).code, context);
  await assert.rejects(runInContext('integrationApi("/api/integrations/codex")', context), /상태 응답이 올바르지/);
  assert.equal(timeout, 5000);
});
