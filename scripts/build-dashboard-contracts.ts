import { mkdir, readFile, writeFile } from "node:fs/promises";
import Ajv2020Module from "ajv/dist/2020.js";
import standaloneCodeModule from "ajv/dist/standalone/index.js";
import { build } from "esbuild";
import { compileFromFile } from "json-schema-to-typescript";

const schemaPath = "contracts/dashboard-query-v1.schema.json";
const reportSchemaPath = "contracts/report-dto-v2.schema.json";
const declarationsPath = "ui/report/generated/dashboard-query-v1.d.ts";
const validatorPath = "ui/report/generated/validate-dashboard-query-v1.ts";
const banner = `Generated from ${schemaPath}. Do not edit.`;
const Ajv2020 = Ajv2020Module as unknown as typeof import("ajv/dist/2020.js").default;
const standaloneCode = standaloneCodeModule as unknown as typeof import("ajv/dist/standalone/index.js").default;

const schema = JSON.parse(await readFile(schemaPath, "utf8"));
const reportSchema = JSON.parse(await readFile(reportSchemaPath, "utf8"));
const requestByteLimit = schema["x-agent-observability-max-request-serialized-utf8-bytes"];
const responseByteLimit = schema["x-agent-observability-max-response-serialized-utf8-bytes"];

const canonicalAvailabilityRules = reportSchema.$defs?.field_availability?.oneOf;
const dashboardAvailabilityRules = schema.$defs?.availability_reason?.oneOf;
if (!Array.isArray(canonicalAvailabilityRules) || !Array.isArray(dashboardAvailabilityRules)) {
  throw new Error("Missing canonical dashboard availability rule sets");
}
const strictCanonicalUnavailableRules = canonicalAvailabilityRules.slice(1).map(
  (rule: Record<string, unknown>) => ({ type: "object", required: ["state", "reason"], ...rule }),
);
if (JSON.stringify(dashboardAvailabilityRules) !== JSON.stringify(strictCanonicalUnavailableRules)) {
  throw new Error("Dashboard availability reasons drifted from canonical FieldAvailabilityV2 rules");
}

if (requestByteLimit !== 8_192 || responseByteLimit !== 1_048_576) {
  throw new Error(`Dashboard wire byte limits drifted in ${schemaPath}`);
}

const ajv = new Ajv2020({
  allowUnionTypes: true,
  code: { esm: true, source: true },
  strict: true,
});
for (const keyword of [
  "x-agent-observability-max-request-serialized-utf8-bytes",
  "x-agent-observability-max-response-serialized-utf8-bytes",
]) {
  ajv.addKeyword({ keyword, schemaType: "number", valid: true });
}

const dashboardSchemaId = schema.$id;
if (typeof dashboardSchemaId !== "string") {
  throw new Error(`Missing dashboard schema ID in ${schemaPath}`);
}
const reportSchemaAlias = new URL("report-dto-v2.schema.json", dashboardSchemaId).href;
delete reportSchema.$id;
ajv.addSchema(reportSchema, reportSchemaAlias);
const validateWire = ajv.compile(schema);

const projectedId = `id:sha256:${"a".repeat(64)}`;
const snapshot = {
  id: "b".repeat(64),
  generation: "42",
  visibilityEpoch: "7",
  generatedAt: "2026-09-07T00:00:00.000Z",
  state: "current",
} as const;
const scope = {
  filters: {},
  selectedTraceId: null,
  coldExcluded: true,
} as const;
const limits = {
  traceRows: 100,
  spanRows: 200,
  timelineRows: 120,
  facetValues: 500,
  requestBytes: requestByteLimit,
  responseBytes: responseByteLimit,
} as const;

const fixtures: ReadonlyArray<unknown> = [
  { schemaVersion: "agent_observability.dashboard_query.v1", kind: "bootstrap" },
  {
    schemaVersion: "agent_observability.dashboard_query.v1",
    kind: "spans",
    snapshotId: snapshot.id,
    traceId: projectedId,
    cursor: "opaque-server-lease",
    filters: { repo: ["workspace"], text: "failed tool" },
  },
  {
    schemaVersion: "agent_observability.dashboard_query.v1",
    kind: "bootstrap",
    snapshot,
    scope,
    work: { state: "pending" },
    limits,
  },
  {
    schemaVersion: "agent_observability.dashboard_query.v1",
    kind: "summary",
    snapshot,
    scope,
    work: {
      state: "complete",
      kpis: {
        sessions: 2,
        turns: 3,
        llm: 4,
        tools: 5,
        errors: 1,
        inputTokens: 100,
        outputTokens: 20,
        totalTokens: 120,
        tokenStatus: "complete",
        estimatedCost: 0.01,
        costStatus: "estimated",
        currency: "USD",
      },
    },
    pagination: { nextCursor: null },
  },
  {
    schemaVersion: "agent_observability.dashboard_query.v1",
    kind: "summary",
    snapshot,
    scope,
    work: { state: "pending" },
    pagination: { nextCursor: "opaque-summary-progress" },
  },
  {
    schemaVersion: "agent_observability.dashboard_query.v1",
    kind: "status",
    requestKind: "traces",
    reason: "snapshot_expired",
  },
];

for (const fixture of fixtures) {
  if (!validateWire(fixture)) {
    throw new Error(`Dashboard fixture failed schema validation: ${ajv.errorsText(validateWire.errors)}`);
  }
}

const rejectedFixtures: ReadonlyArray<unknown> = [
  { schemaVersion: "agent_observability.dashboard_query.v1", kind: "bootstrap", sql: "select 1" },
  {
    schemaVersion: "agent_observability.dashboard_query.v1",
    kind: "spans",
    snapshotId: snapshot.id,
    traceId: "/tmp/store.db",
  },
  { schemaVersion: "agent_observability.dashboard_query.v1", kind: "traces" },
  {
    schemaVersion: "agent_observability.dashboard_query.v1",
    kind: "summary",
    snapshot: { ...snapshot, generation: 42 },
    scope,
    work: { state: "pending", kpis: {} },
    pagination: { nextCursor: "opaque-summary-progress" },
  },
  {
    schemaVersion: "agent_observability.dashboard_query.v1",
    kind: "spans",
    snapshot,
    scope,
    work: { state: "pending" },
    rows: [{
      traceId: projectedId,
      spanId: projectedId,
      parentSpanId: null,
      kind: "tool",
      name: "shell",
      status: "ok",
      startTimeUnixMs: 1,
      endTimeUnixMs: 2,
      repo: "workspace",
      availabilityReasons: [],
      rawContent: "forbidden",
    }],
    pagination: { nextCursor: null },
  },
];

for (const fixture of rejectedFixtures) {
  if (validateWire(fixture)) {
    throw new Error("Closed dashboard schema accepted a rejected fixture");
  }
}

const encoder = new TextEncoder();
for (const fixture of fixtures) {
  const bytes = encoder.encode(JSON.stringify(fixture)).byteLength;
  const isRequest = !Object.hasOwn(fixture as object, "snapshot") &&
    (fixture as { kind?: string }).kind !== "status";
  const limit = isRequest ? requestByteLimit : responseByteLimit;
  if (bytes > limit) {
    throw new Error("Dashboard parity fixture exceeds its serialized UTF-8 wire limit");
  }
}

await mkdir("ui/report/generated", { recursive: true });
const declarations = await compileFromFile(schemaPath, {
  additionalProperties: false,
  bannerComment: `/* ${banner} */`,
  style: { singleQuote: false },
});
const standalone = standaloneCode(ajv, validateWire);
const validatorSource = standalone.replace(
  /^"use strict";export const validate = ([A-Za-z_$][A-Za-z0-9_$]*);export default \1;/,
  '"use strict";const validateDashboardWireShape = $1;',
);
if (validatorSource === standalone) {
  throw new Error("Unable to wrap the generated dashboard validator");
}
const validatorHelpersSource =
  `function isRecord(value: unknown): value is Record<string, unknown> {\n` +
  `  return typeof value === "object" && value !== null && !Array.isArray(value);\n` +
  `}\n` +
  `function isResponseEnvelope(value: unknown): boolean {\n` +
  `  return isRecord(value) && (value.kind === "status" || Object.hasOwn(value, "snapshot"));\n` +
  `}\n` +
  `function withinUtf8Limit(value: unknown, limit: number): boolean {\n` +
  `  try {\n` +
  `    const serialized = JSON.stringify(value);\n` +
  `    return typeof serialized === "string" && new TextEncoder().encode(serialized).byteLength <= limit;\n` +
  `  } catch {\n` +
  `    return false;\n` +
  `  }\n` +
  `}\n` +
  `function isSemanticU64(value: unknown): boolean {\n` +
  `  if (typeof value !== "string" || !/^(0|[1-9][0-9]{0,19})$/.test(value)) return false;\n` +
  `  try {\n` +
  `    return BigInt(value) <= 18446744073709551615n;\n` +
  `  } catch {\n` +
  `    return false;\n` +
  `  }\n` +
  `}\n` +
  `function hasValidSnapshotCounters(value: unknown): boolean {\n` +
  `  if (!isRecord(value)) return false;\n` +
  `  if (value.kind === "status" && value.snapshot === undefined) return true;\n` +
  `  if (!isRecord(value.snapshot)) return false;\n` +
  `  return isSemanticU64(value.snapshot.generation) && isSemanticU64(value.snapshot.visibilityEpoch);\n` +
  `}\n` +
  `export function validateDashboardQueryRequestV1(value: unknown): value is import("./dashboard-query-v1.js").DashboardQueryRequestV1 {\n` +
  `  return validateDashboardWireShape(value) && !isResponseEnvelope(value) && withinUtf8Limit(value, ${requestByteLimit});\n` +
  `}\n` +
  `export default function validateDashboardQueryResponseV1(value: unknown): value is import("./dashboard-query-v1.js").DashboardQueryResponseV1 {\n` +
  `  return validateDashboardWireShape(value) && isResponseEnvelope(value) && hasValidSnapshotCounters(value) && withinUtf8Limit(value, ${responseByteLimit});\n` +
  `}\n`;
const validatorBundle = await build({
  stdin: {
    contents: validatorSource,
    resolveDir: process.cwd(),
    sourcefile: "validate-dashboard-query-v1.runtime.generated.js",
    loader: "js",
  },
  bundle: true,
  format: "esm",
  platform: "neutral",
  target: ["es2022"],
  legalComments: "none",
  treeShaking: false,
  write: false,
});
const bundledValidator = validatorBundle.outputFiles[0];
if (!bundledValidator) {
  throw new Error("Dashboard validator bundling produced no output");
}
await Promise.all([
  writeFile(declarationsPath, declarations, "utf8"),
  writeFile(
    validatorPath,
    `/* ${banner} */\n` +
      `// @ts-nocheck -- Ajv standalone output is bundled generated JavaScript embedded in TypeScript.\n` +
      bundledValidator.text +
      validatorHelpersSource,
    "utf8",
  ),
]);
