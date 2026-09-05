import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import validateConfig from "../ui/settings/generated/validate-local-runtime-config-v3.js";

const fixture = JSON.parse(
  await readFile("contracts/local-runtime-config-v3.fixture.json", "utf8"),
);
const parityCases = JSON.parse(
  await readFile("contracts/local-runtime-config-v3.parity.json", "utf8"),
) as ParityCase[];

interface ParityCase {
  name: string;
  path: string[];
  operation: "set" | "remove";
  value?: unknown;
  valid: boolean;
}

test("generated settings validator accepts the Rust default fixture", () => {
  assert.equal(validateConfig(structuredClone(fixture)), true, JSON.stringify(validateConfig.errors));
});

test("generated settings validator matches the shared Rust parity corpus", () => {
  for (const parityCase of parityCases) {
    const document = structuredClone(fixture);
    applyParityCase(document, parityCase);
    assert.equal(validateConfig(document), parityCase.valid, parityCase.name);
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
