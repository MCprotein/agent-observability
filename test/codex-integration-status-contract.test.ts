import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import Ajv2020Module from "ajv/dist/2020.js";
import validateIntegrationStatus from "../ui/settings/generated/validate-codex-integration-status-v1.js";

const Ajv2020 = Ajv2020Module as unknown as typeof import("ajv/dist/2020.js").default;

const integrationFixture = JSON.parse(
  await readFile("contracts/codex-integration-status-v1.fixture.json", "utf8"),
);
const healthFixture = JSON.parse(
  await readFile("contracts/local-collector-health-v1.fixture.json", "utf8"),
);

test("generated integration status validator accepts the canonical fixture", () => {
  assert.equal(validateIntegrationStatus(structuredClone(integrationFixture)), true);
});

test("integration status contract rejects malformed versions, reasons, and open fields", () => {
  for (const document of [
    { ...structuredClone(integrationFixture), schema_version: "codex_integration_status.v2" },
    { ...structuredClone(integrationFixture), collector_degradation_reasons: ["unknown"] },
    { ...structuredClone(integrationFixture), unexpected: true },
    { ...structuredClone(integrationFixture), collector: "ready" },
  ]) {
    assert.equal(validateIntegrationStatus(document), false);
  }
});

test("collector health schema matches the current serialized health fixture", async () => {
  const schema = JSON.parse(
    await readFile("contracts/local-collector-health-v1.schema.json", "utf8"),
  );
  const validateHealth = new Ajv2020({ strict: true }).compile(schema);
  assert.equal(validateHealth(structuredClone(healthFixture)), true);
  assert.equal(validateHealth({ ...structuredClone(healthFixture), report_failure: "unknown" }), false);
  assert.equal(validateHealth({ ...structuredClone(healthFixture), degradation_reasons: ["unknown"] }), false);
  assert.equal(validateHealth({ ...structuredClone(healthFixture), unexpected: true }), false);
});
