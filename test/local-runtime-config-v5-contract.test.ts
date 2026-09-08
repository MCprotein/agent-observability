import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { validateLocalRuntimeConfig } from "../ui/settings/config-validation.js";
import validateLocalRuntimeConfigV5 from "../ui/settings/generated/validate-local-runtime-config-v5.js";
import type { LocalRuntimeConfigV5 } from "../ui/settings/generated/local-runtime-config-v5.js";

const fixture = JSON.parse(
  await readFile("contracts/local-runtime-config-v5.fixture.json", "utf8"),
) as LocalRuntimeConfigV5;
const parityCases = JSON.parse(
  await readFile("contracts/local-runtime-config-v5.parity.json", "utf8"),
) as ParityCase[];

interface ParityCase {
  name: string;
  path: string[];
  operation: "none" | "set" | "remove";
  value?: unknown;
  valid: boolean;
}

test("generated v5 validator accepts the legacy default fixture", () => {
  assert.equal(validateLocalRuntimeConfigV5(structuredClone(fixture)), true);
  assert.deepEqual(fixture.storage_budget, {
    mode: "legacy",
    retained_target_bytes: 1_073_741_824,
    workspace_budget_bytes: 1_073_741_824,
    minimum_free_bytes: 1_073_741_824,
  });
});

test("generated v5 validator matches the additive strict parity corpus", () => {
  for (const parityCase of parityCases) {
    const document = structuredClone(fixture) as unknown as Record<string, unknown>;
    applyParityCase(document, parityCase);
    assert.equal(validateLocalRuntimeConfigV5(document), parityCase.valid, parityCase.name);
    assert.equal(validateLocalRuntimeConfig(document).valid, parityCase.valid, parityCase.name);
  }
});

test("the UI full-object serialization shape retains inactive storage-budget fields", () => {
  const draft = structuredClone(fixture);
  draft.enabled = !draft.enabled;
  const submitted = JSON.parse(JSON.stringify({ config: draft, revision: "revision-v5" })) as {
    config: LocalRuntimeConfigV5;
    revision: string;
  };
  assert.deepEqual(submitted.config.storage_budget, fixture.storage_budget);
  assert.equal(submitted.revision, "revision-v5");
});

test("composite v5 validation retains lifecycle ordering checks", () => {
  const invalidWarm = structuredClone(fixture);
  invalidWarm.lifecycle.warm_days = invalidWarm.lifecycle.hot_days - 1;
  assert.equal(validateLocalRuntimeConfig(invalidWarm).valid, false);

  const invalidExpiry = structuredClone(fixture);
  invalidExpiry.lifecycle.delete_after_days = invalidExpiry.lifecycle.warm_days;
  assert.equal(validateLocalRuntimeConfig(invalidExpiry).valid, false);
});

function applyParityCase(document: Record<string, unknown>, parityCase: ParityCase): void {
  if (parityCase.operation === "none") return;
  let parent = document;
  for (const segment of parityCase.path.slice(0, -1)) {
    const child = parent[segment];
    assert.equal(typeof child, "object");
    assert.notEqual(child, null);
    parent = child as Record<string, unknown>;
  }
  const field = parityCase.path.at(-1);
  assert.ok(field);
  if (parityCase.operation === "set") parent[field] = parityCase.value;
  else delete parent[field];
}
