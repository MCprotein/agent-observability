import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import validateConfig from "../ui/settings/generated/validate-local-runtime-config-v2.js";

const fixture = JSON.parse(
  await readFile("contracts/local-runtime-config-v2.fixture.json", "utf8"),
);
const parityCases = JSON.parse(
  await readFile("contracts/local-runtime-config-v2.parity.json", "utf8"),
);

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

function applyParityCase(document, parityCase) {
  if (parityCase.path.length === 0) return;
  let parent = document;
  for (const segment of parityCase.path.slice(0, -1)) parent = parent[segment];
  const field = parityCase.path.at(-1);
  if (parityCase.operation === "set") parent[field] = parityCase.value;
  else if (parityCase.operation === "remove") delete parent[field];
  else throw new Error(`unsupported parity operation: ${parityCase.operation}`);
}
