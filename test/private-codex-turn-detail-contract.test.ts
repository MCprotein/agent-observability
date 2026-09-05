import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import validatePrivateCodexTurnDetailV1 from "../ui/report/generated/validate-private-codex-turn-detail-v1.js";

const corpus = JSON.parse(
  await readFile("contracts/private-codex-turn-detail-v1.parity.json", "utf8"),
) as ParityCorpus;

interface ParityCorpus {
  base: Record<string, unknown>;
  cases: ParityCase[];
}

interface ParityCase {
  name: string;
  operation: "none" | "set" | "remove" | "set_serialized_size" | "set_repeated_string";
  path: string[];
  value?: unknown;
  size?: number;
  count?: number;
  valid: boolean;
}

test("generated private-detail validator matches the shared Rust parity corpus", () => {
  for (const parityCase of corpus.cases) {
    const document = structuredClone(corpus.base);
    applyParityCase(document, parityCase);
    assert.equal(validatePrivateCodexTurnDetailV1(document), parityCase.valid, parityCase.name);
  }
});

function applyParityCase(document: Record<string, unknown>, parityCase: ParityCase): void {
  if (parityCase.operation === "none") return;
  assert.equal(parityCase.path.length, 1);
  const field = parityCase.path[0];
  assert.ok(field);
  if (parityCase.operation === "set") {
    document[field] = parityCase.value;
  } else if (parityCase.operation === "remove") {
    delete document[field];
  } else if (parityCase.operation === "set_serialized_size") {
    if (typeof parityCase.size !== "number") throw new Error("serialized-size case is missing size");
    const targetSize = parityCase.size;
    document[field] = "";
    const emptySize = Buffer.byteLength(JSON.stringify(document), "utf8");
    assert.ok(targetSize >= emptySize);
    document[field] = "x".repeat(targetSize - emptySize);
    assert.equal(Buffer.byteLength(JSON.stringify(document), "utf8"), targetSize);
  } else {
    if (typeof parityCase.value !== "string" || typeof parityCase.count !== "number") {
      throw new Error("repeated-string case is missing string value or count");
    }
    document[field] = parityCase.value.repeat(parityCase.count);
  }
}
