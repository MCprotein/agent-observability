import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { createContext, runInContext } from "node:vm";
import { transform } from "esbuild";

function deferred<T = void>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, reject, resolve };
}

async function saveCloseRace(scenario: "success" | "error" | "rebase") {
  const source = await readFile("ui/settings/main.ts", "utf8");
  const functions = [
    source.slice(
      source.indexOf("async function saveDraft()"),
      source.indexOf("function discardChanges()"),
    ),
    source.slice(
      source.indexOf("async function closeSession()"),
      source.indexOf("function requestCloseSession()"),
    ),
    source.slice(
      source.indexOf("function expireSession()"),
      source.indexOf("function readSessionToken()"),
    ),
  ].join("\n");
  const put = deferred<Record<string, unknown>>();
  const latest = deferred<Record<string, unknown>>();
  const calls: string[] = [];
  const mutationsAfterExpiry: string[] = [];
  let expired = false;
  let expiredRenders = 0;
  const persisted = {
    enabled: true,
    capture_private_codex_turn_details: false,
    collection: { max_batch_records: 10 },
    lifecycle: { enabled: true },
  };
  const draft = structuredClone(persisted);
  draft.collection.max_batch_records = 11;
  const recordMutation = (name: string) => {
    if (expired) mutationsAfterExpiry.push(name);
  };
  const context = createContext({
    busy: false,
    closeInFlight: false,
    closeFailureMessage: "",
    token: "session",
    sessionGeneration: 0,
    integrationRequestGeneration: 0,
    persisted,
    draft,
    defaults: structuredClone(persisted),
    revision: "revision-1",
    conflicted: false,
    heartbeatTimer: 7,
    fields: {},
    structuredClone,
    changedPaths: () => ["collection.max_batch_records"],
    getValue: (value: typeof draft) => value.collection.max_batch_records,
    setValue: (value: typeof draft, _path: string, next: number) => {
      value.collection.max_batch_records = next;
    },
    validateLocalRuntimeConfig: () => ({ valid: true, errors: [] }),
    clearErrors: () => {},
    showFieldError: () => {},
    focusFirstInvalid: () => {},
    messageOf: (error: Error) => error.message,
    sessionIsCurrent: (identity: { generation: number; token: string }) =>
      identity.generation === context.sessionGeneration &&
      identity.token === context.token &&
      context.token !== "",
    api: async (path: string, init: RequestInit = {}) => {
      calls.push(`${init.method ?? "GET"} ${path}`);
      if (path === "/api/shutdown") return undefined;
      if (init.method === "PUT") return put.promise;
      return latest.promise;
    },
    applyEnvelope: (envelope: { config: typeof persisted; revision: string }) => {
      recordMutation("applyEnvelope");
      context.persisted = structuredClone(envelope.config);
      context.draft = structuredClone(envelope.config);
      context.revision = envelope.revision;
    },
    renderSettings: () => recordMutation("renderSettings"),
    showToast: () => recordMutation("showToast"),
    setBusy: () => recordMutation("setBusy"),
    setText: () => recordMutation("setText"),
    updateDirtyState: () => recordMutation("updateDirtyState"),
    clearSessionToken: () => {},
    renderExpired: () => {
      expired = true;
      expiredRenders += 1;
    },
    document: { querySelector: () => null },
    window: { clearInterval: () => {} },
  });
  runInContext(
    (await transform(functions, { loader: "ts", target: "es2022" })).code,
    context,
  );

  const save = runInContext("saveDraft()", context) as Promise<void>;
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(calls, ["PUT /api/config"]);

  if (scenario === "rebase") {
    const conflict = new Error("conflict") as Error & { code?: string };
    conflict.code = "config_conflict";
    put.reject(conflict);
    await new Promise((resolve) => setImmediate(resolve));
    assert.deepEqual(calls, ["PUT /api/config", "GET /api/config"]);
  }

  await runInContext("closeSession()", context);
  assert.equal(context.token, "");
  assert.equal(expiredRenders, 1);

  if (scenario === "success") {
    put.resolve({ config: persisted, defaults: persisted, revision: "revision-2" });
  } else if (scenario === "error") {
    const error = new Error("late save failure") as Error & { code?: string };
    error.code = "save_failed";
    put.reject(error);
  } else {
    latest.resolve({ config: persisted, defaults: persisted, revision: "revision-2" });
  }
  await save;

  assert.deepEqual(mutationsAfterExpiry, []);
  assert.equal(expiredRenders, 1);
  assert.equal(context.busy, false);
  assert.equal(context.revision, "revision-1");
  return calls;
}

test("closing during save ignores a late successful PUT", async () => {
  assert.deepEqual(await saveCloseRace("success"), ["PUT /api/config", "POST /api/shutdown"]);
});

test("closing during save ignores a late failed PUT", async () => {
  assert.deepEqual(await saveCloseRace("error"), ["PUT /api/config", "POST /api/shutdown"]);
});

test("closing during conflict rebase ignores a late config GET", async () => {
  assert.deepEqual(
    await saveCloseRace("rebase"),
    ["PUT /api/config", "GET /api/config", "POST /api/shutdown"],
  );
});

test("busy release preserves derived controls and close blocks a reconciliation render", async () => {
  const source = await readFile("ui/settings/main.ts", "utf8");
  const setBusySource = source.slice(
    source.indexOf("function setBusy("),
    source.indexOf("function showFieldError("),
  );
  const buttons = [
    { id: "save", disabled: true },
    { id: "discard", disabled: true },
    { id: "reset", disabled: false },
    { id: "toggle-integration", disabled: true },
    { id: "confirm-close", disabled: false },
    { id: "close-session", disabled: false },
  ];
  const inputs = [{ disabled: false }];
  const form = { setAttribute: () => {} };
  const context = createContext({
    closeInFlight: false,
    buttonDisabledBeforeBusy: new WeakMap(),
    inputDisabledBeforeClose: new WeakMap(),
    document: {
      querySelector: () => form,
      querySelectorAll: (selector: string) => selector === "button" ? buttons : inputs,
    },
    setText: () => {},
  });
  runInContext(
    (await transform(setBusySource, { loader: "ts", target: "es2022" })).code,
    context,
  );

  runInContext("setBusy(false)", context);
  assert.equal(buttons.find((button) => button.id === "save")?.disabled, true);
  assert.equal(buttons.find((button) => button.id === "discard")?.disabled, true);
  assert.equal(buttons.find((button) => button.id === "reset")?.disabled, false);

  context.closeInFlight = true;
  runInContext("setBusy(false)", context);
  assert.equal(
    buttons.filter((button) => button.id !== "close-session").every((button) => button.disabled),
    true,
  );
  assert.equal(inputs[0]?.disabled, true);
  assert.equal(buttons.find((button) => button.id === "confirm-close")?.disabled, true);

  context.closeInFlight = false;
  runInContext("setBusy(false)", context);
  assert.equal(buttons.find((button) => button.id === "save")?.disabled, true);
  assert.equal(buttons.find((button) => button.id === "discard")?.disabled, true);
  assert.equal(buttons.find((button) => button.id === "toggle-integration")?.disabled, true);
  assert.equal(buttons.find((button) => button.id === "confirm-close")?.disabled, false);
  assert.equal(inputs[0]?.disabled, false);
});

test("closing during an integration mutation expires the UI without disconnecting the collector", async () => {
  const source = await readFile("ui/settings/main.ts", "utf8");
  const functions = [
    source.slice(
      source.indexOf("async function toggleIntegration()"),
      source.indexOf("async function refreshIntegration()"),
    ),
    source.slice(
      source.indexOf("async function closeSession()"),
      source.indexOf("function requestCloseSession()"),
    ),
    source.slice(
      source.indexOf("function expireSession()"),
      source.indexOf("function readSessionToken()"),
    ),
  ].join("\n");
  let finishIntegration!: () => void;
  const integrationResponse = new Promise<void>((resolve) => {
    finishIntegration = resolve;
  });
  let finishShutdown!: () => void;
  const shutdownResponse = new Promise<void>((resolve) => {
    finishShutdown = resolve;
  });
  const calls: string[] = [];
  let expiredRenders = 0;
  let settingsRenders = 0;
  let toasts = 0;
  const context = createContext({
    busy: false,
    closeInFlight: false,
    closeFailureMessage: "",
    token: "session",
    sessionGeneration: 0,
    integrationRequestGeneration: 0,
    integration: { config: "disconnected" },
    integrationUnavailable: false,
    persisted: null,
    draft: null,
    conflicted: false,
    heartbeatTimer: 7,
    integrationApi: async () => {
      calls.push("POST /api/integrations/codex");
      await integrationResponse;
      return { config: "connected" };
    },
    api: async (path: string, init: RequestInit) => {
      calls.push(`${init.method} ${path}`);
      await shutdownResponse;
    },
    setBusy: () => {},
    setText: () => {},
    updateDirtyState: () => {},
    clearSessionToken: () => {},
    renderExpired: () => {
      expiredRenders += 1;
    },
    renderSettings: () => {
      settingsRenders += 1;
    },
    showToast: () => {
      toasts += 1;
    },
    messageOf: (error: Error) => error.message,
    sessionIsCurrent: (session: { generation: number; token: string }) =>
      session.generation === context.sessionGeneration &&
      session.token === context.token &&
      context.token !== "",
    window: { clearInterval: () => {} },
  });
  runInContext(
    (await transform(functions, { loader: "ts", target: "es2022" })).code,
    context,
  );

  const mutation = runInContext("toggleIntegration()", context) as Promise<void>;
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(context.busy, true);
  assert.deepEqual(calls, ["POST /api/integrations/codex"]);

  const close = runInContext("closeSession()", context) as Promise<void>;
  const duplicateClose = runInContext("closeSession()", context) as Promise<void>;
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(context.closeInFlight, true);
  assert.deepEqual(calls, ["POST /api/integrations/codex", "POST /api/shutdown"]);

  finishShutdown();
  await Promise.all([close, duplicateClose]);
  assert.equal(context.token, "");
  assert.equal(context.sessionGeneration, 1);
  assert.equal(context.integrationRequestGeneration, 2);
  assert.equal(context.closeInFlight, false);
  assert.equal(expiredRenders, 1);

  finishIntegration();
  await mutation;
  assert.equal(settingsRenders, 0);
  assert.equal(toasts, 0);
  assert.equal((context.integration as { config: string }).config, "disconnected");
  assert.equal(calls.some((call) => call.startsWith("DELETE ")), false);
});

test("failed shutdown keeps a visible enabled retry across both integration completion orders", async () => {
  const source = await readFile("ui/settings/main.ts", "utf8");
  const functions = [
    source.slice(
      source.indexOf("async function toggleIntegration()"),
      source.indexOf("async function refreshIntegration()"),
    ),
    source.slice(
      source.indexOf("async function closeSession()"),
      source.indexOf("function requestCloseSession()"),
    ),
  ].join("\n");
  for (const order of ["integration-first", "shutdown-first"] as const) {
    const integrationResponse = deferred();
    const shutdownResponse = deferred();
    const calls: string[] = [];
    const busyStates: boolean[] = [];
    const derivedButton = { disabled: true };
    let focusedRetries = 0;
    const makeDialog = () => {
      const confirm = {
        disabled: false,
        focus: () => {
          focusedRetries += 1;
        },
      };
      return {
        confirm,
        error: "",
        open: false,
        showModal() {
          this.open = true;
        },
      };
    };
    const context = createContext({
      busy: false,
      closeInFlight: false,
      closeFailureMessage: "",
      token: "session",
      sessionGeneration: 0,
      integrationRequestGeneration: 0,
      integration: { config: "disconnected" },
      integrationUnavailable: false,
      persisted: null,
      draft: null,
      conflicted: false,
      settingsRenders: 0,
      currentDialog: makeDialog(),
      replaceDialog: () => {
        context.currentDialog = makeDialog();
      },
      integrationApi: async () => {
        calls.push("POST /api/integrations/codex");
        await integrationResponse.promise;
        return { config: "connected" };
      },
      api: async (path: string, init: RequestInit) => {
        calls.push(`${init.method} ${path}`);
        await shutdownResponse.promise;
        throw new Error("injected shutdown failure");
      },
      busyStates,
      derivedButton,
      setText: (id: string, value: string) => {
        if (id === "close-error") context.currentDialog.error = value;
      },
      updateDirtyState: () => {
        derivedButton.disabled = true;
      },
      showToast: () => {},
      messageOf: (error: Error) => error.message,
      sessionIsCurrent: (session: { generation: number; token: string }) =>
        session.generation === context.sessionGeneration &&
        session.token === context.token &&
        context.token !== "",
      document: {
        querySelector: (selector: string) => {
          if (selector === "#close-dialog") return context.currentDialog;
          if (selector === "#confirm-close") return context.currentDialog.confirm;
          return null;
        },
      },
    });
    runInContext(
      (await transform(functions, { loader: "ts", target: "es2022" })).code,
      context,
    );
    runInContext(`
      function setBusy(value) {
        busyStates.push(value);
        derivedButton.disabled = true;
        currentDialog.confirm.disabled = closeInFlight;
      }
      function renderSettings() {
        settingsRenders += 1;
        replaceDialog();
        renderCloseFailure();
      }
    `, context);

    const mutation = runInContext("toggleIntegration()", context) as Promise<void>;
    await new Promise((resolve) => setImmediate(resolve));
    const close = runInContext("closeSession()", context) as Promise<void>;
    await new Promise((resolve) => setImmediate(resolve));

    if (order === "integration-first") {
      integrationResponse.resolve();
      await mutation;
      shutdownResponse.resolve();
      await close;
    } else {
      shutdownResponse.resolve();
      await close;
      assert.equal(context.busy, true);
      assert.equal(busyStates.at(-1), true);
      await runInContext("toggleIntegration()", context);
      assert.equal(calls.length, 2);
      integrationResponse.resolve();
      await mutation;
    }

    assert.equal(context.closeInFlight, false);
    assert.equal(context.busy, false);
    assert.equal(context.settingsRenders, 1);
    assert.equal(context.currentDialog.open, true);
    assert.match(context.currentDialog.error, /다시 시도/);
    assert.equal(context.currentDialog.confirm.disabled, false);
    assert.equal(focusedRetries > 0, true);
    assert.equal(derivedButton.disabled, true);
    assert.deepEqual(calls, ["POST /api/integrations/codex", "POST /api/shutdown"]);
  }
});
