import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import validateConfig from "../ui/settings/generated/validate-local-runtime-config-v2.js";

const fixture = JSON.parse(
  await readFile("contracts/local-runtime-config-v2.fixture.json", "utf8"),
);

test("generated settings validator accepts the Rust default fixture", () => {
  assert.equal(validateConfig(structuredClone(fixture)), true, JSON.stringify(validateConfig.errors));
});

test("generated settings validator locks bounds, version, and unknown fields", () => {
  const cases = [
    (config) => { config.collection.file_reconcile_interval_ms = 999; },
    (config) => { config.collection.local_storage_budget_bytes = 21_474_836_481; },
    (config) => { config.retention.max_record_age_days = 3_651; },
    (config) => { config.schema_version = "local_runtime.v3"; },
    (config) => { config.unknown = true; },
  ];
  for (const mutate of cases) {
    const config = structuredClone(fixture);
    mutate(config);
    assert.equal(validateConfig(config), false);
  }
});
