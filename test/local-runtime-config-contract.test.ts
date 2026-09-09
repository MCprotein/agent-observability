import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { validateLocalRuntimeConfig } from "../ui/settings/config-validation.js";

const fixture = JSON.parse(
  await readFile("contracts/local-runtime-config-v5.fixture.json", "utf8"),
);
const parityCases = JSON.parse(
  await readFile("contracts/local-runtime-config-v4.parity.json", "utf8"),
) as ParityCase[];

interface ParityCase {
  name: string;
  path: string[];
  operation: "set" | "remove";
  value?: unknown;
  valid: boolean;
}

test("composite v5 settings validator accepts the default fixture", () => {
  assert.equal(validateLocalRuntimeConfig(structuredClone(fixture)).valid, true);
});

test("composite v5 validator preserves every inherited v4 parity constraint", () => {
  for (const parityCase of parityCases) {
    const document = structuredClone(fixture);
    applyParityCase(document, parityCase);
    assert.equal(validateLocalRuntimeConfig(document).valid, parityCase.valid, parityCase.name);
  }
});

function applyParityCase(document: Record<string, unknown>, parityCase: ParityCase): void {
  if (parityCase.path.length === 0) return;
  let parent: Record<string, unknown> = document;
  for (const segment of parityCase.path.slice(0, -1)) {
    const child = parent[segment];
    assert.equal(typeof child, "object");
    assert.notEqual(child, null);
    parent = child as Record<string, unknown>;
  }
  const field = parityCase.path.at(-1);
  assert.ok(field);
  if (parityCase.operation === "set") parent[field] = parityCase.value;
  else if (parityCase.operation === "remove") delete parent[field];
  else throw new Error(`unsupported parity operation: ${parityCase.operation}`);
}
