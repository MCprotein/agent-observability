import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import Ajv2020Module from "ajv/dist/2020.js";

const fixture = JSON.parse(await readFile("contracts/report-dto-v2.fixture.json", "utf8"));
const schema = JSON.parse(await readFile("contracts/report-dto-v2.schema.json", "utf8"));
const parityCases = JSON.parse(
  await readFile("contracts/report-dto-v2.parity.json", "utf8"),
) as ParityCase[];
const Ajv2020 = Ajv2020Module as unknown as typeof import("ajv/dist/2020.js").default;
const validateReport = new Ajv2020({ allowUnionTypes: true, strict: true }).compile(schema);

interface ParityCase {
  name: string;
  path: string[];
  operation: "set" | "remove" | "none";
  value?: unknown;
  valid: boolean;
}

test("authoritative report schema accepts the current v2 fixture", () => {
  assert.equal(validateReport(structuredClone(fixture)), true);
});

test("authoritative report schema matches the Rust availability parity corpus", () => {
  for (const parityCase of parityCases) {
    const document = structuredClone(fixture);
    applyParityCase(document, parityCase);
    assert.equal(validateReport(document), parityCase.valid, parityCase.name);
  }
});

test("partial token metrics require the explicit incomplete availability reason", () => {
  const partial = structuredClone(fixture);
  partial.spans[0].metrics = { totalInputTokens: 10 };
  partial.spans[0].availability.tokens = {
    state: "source_unavailable",
    reason: "partial_token_metrics",
  };
  assert.equal(validateReport(partial), true);

  const wrongState = structuredClone(partial);
  wrongState.spans[0].availability.tokens.state = "withheld";
  assert.equal(validateReport(wrongState), false);

  const wrongReason = structuredClone(partial);
  wrongReason.spans[0].availability.tokens.reason = "source_not_provided";
  assert.equal(validateReport(wrongReason), false);
});

function applyParityCase(document: unknown, parityCase: ParityCase): void {
  if (parityCase.operation === "none") return;
  let parent = document as Record<string, unknown> | unknown[];
  for (const segment of parityCase.path.slice(0, -1)) {
    parent = Array.isArray(parent)
      ? parent[Number(segment)] as Record<string, unknown> | unknown[]
      : parent[segment] as Record<string, unknown> | unknown[];
  }
  const field = parityCase.path.at(-1);
  assert.ok(field);
  if (Array.isArray(parent)) {
    const index = Number(field);
    if (parityCase.operation === "set") parent[index] = parityCase.value;
    else parent.splice(index, 1);
  } else if (parityCase.operation === "set") {
    parent[field] = parityCase.value;
  } else {
    delete parent[field];
  }
}
