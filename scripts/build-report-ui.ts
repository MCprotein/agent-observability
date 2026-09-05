import { mkdir, readFile, writeFile } from "node:fs/promises";
import Ajv2020Module from "ajv/dist/2020.js";
import standaloneCodeModule from "ajv/dist/standalone/index.js";
import { build } from "esbuild";
import { compileFromFile } from "json-schema-to-typescript";

const schemaPath = "contracts/report-dto-v2.schema.json";
const typePath = "ui/report/generated/report-dto-v2.d.ts";
const browserSchemaPath = "ui/report/generated/report-dto-v2.schema.json";
const validatorPath = "ui/report/generated/validate-report-dto-v2.js";
const privateDetailSchemaPath = "contracts/private-codex-turn-detail-v1.schema.json";
const privateDetailTypePath = "ui/report/generated/private-codex-turn-detail-v1.d.ts";
const privateDetailValidatorPath = "ui/report/generated/validate-private-codex-turn-detail-v1.ts";
const bundlePath = "src/report/generated/report-ui.js";
const shellPath = "src/report/generated/report-shell.html";
const viewSummaryPath = "ui/report/generated/view-summary.js";
const viewStatePath = "ui/report/generated/view-state.js";
const banner = "Generated from contracts/report-dto-v2.schema.json. Do not edit.";
const Ajv2020 = Ajv2020Module as unknown as typeof import("ajv/dist/2020.js").default;
const standaloneCode = standaloneCodeModule as unknown as typeof import("ajv/dist/standalone/index.js").default;

await Promise.all([
  mkdir("ui/report/generated", { recursive: true }),
  mkdir("src/report/generated", { recursive: true }),
]);

const declarations = await compileFromFile(schemaPath, {
  bannerComment: `/* ${banner} */`,
  style: { singleQuote: false },
});
await writeFile(typePath, declarations, "utf8");
const privateDetailDeclarations = await compileFromFile(privateDetailSchemaPath, {
  bannerComment: `/* Generated from ${privateDetailSchemaPath}. Local-only; do not promote or edit. */`,
  style: { singleQuote: false },
});
await writeFile(privateDetailTypePath, privateDetailDeclarations, "utf8");
const browserSchema = JSON.parse(await readFile(schemaPath, "utf8"));
delete browserSchema.$schema;
delete browserSchema.$id;
await writeFile(browserSchemaPath, `${JSON.stringify(browserSchema, null, 2)}\n`, "utf8");
const ajv = new Ajv2020({
  allowUnionTypes: true,
  code: { esm: true, source: true },
  strict: true,
});
const validate = ajv.compile(browserSchema);
const privateDetailSchema = JSON.parse(await readFile(privateDetailSchemaPath, "utf8"));
delete privateDetailSchema.$schema;
delete privateDetailSchema.$id;
const privateDetailMaxBytes = privateDetailSchema["x-agent-observability-max-serialized-utf8-bytes"];
if (!Number.isSafeInteger(privateDetailMaxBytes) || privateDetailMaxBytes <= 0) {
  throw new Error(`Invalid private-detail UTF-8 byte bound in ${privateDetailSchemaPath}`);
}
const privateDetailAjv = new Ajv2020({
  allowUnionTypes: true,
  code: { esm: true, source: true },
  strict: true,
});
privateDetailAjv.addKeyword({
  keyword: "x-agent-observability-max-serialized-utf8-bytes",
  schemaType: "number",
  valid: true,
});
const validatePrivateDetailShape = privateDetailAjv.compile(privateDetailSchema);
const privateDetailStandalone = standaloneCode(privateDetailAjv, validatePrivateDetailShape);
const privateDetailValidatorSource = privateDetailStandalone.replace(
  /^"use strict";export const validate = ([A-Za-z_$][A-Za-z0-9_$]*);export default \1;/,
  '"use strict";const validatePrivateDetailShape = $1;',
);
if (privateDetailValidatorSource === privateDetailStandalone) {
  throw new Error("Unable to wrap the generated private-detail validator");
}
await writeFile(
  privateDetailValidatorPath,
  `/* Generated from ${privateDetailSchemaPath}. Local-only; do not promote or edit. */\n` +
    `// @ts-nocheck -- Ajv standalone output is generated JavaScript embedded in TypeScript.\n` +
    `${privateDetailValidatorSource}\n` +
    `export default function validatePrivateCodexTurnDetailV1(value: unknown): value is import("./private-codex-turn-detail-v1.js").PrivateCodexTurnDetailV1 {\n` +
    `  if (!validatePrivateDetailShape(value)) return false;\n` +
    `  try {\n` +
    `    return new TextEncoder().encode(JSON.stringify(value)).byteLength <= ${privateDetailMaxBytes};\n` +
    `  } catch {\n` +
    `    return false;\n` +
    `  }\n` +
    `}\n`,
  "utf8",
);
await Promise.all([
  build({
    stdin: {
      contents: standaloneCode(ajv, validate),
      resolveDir: process.cwd(),
      sourcefile: "validate-report-dto-v2.generated.js",
    },
    outfile: validatorPath,
    bundle: true,
    format: "esm",
    platform: "neutral",
    target: ["es2022"],
    legalComments: "none",
  }),
  writeFile(
    "ui/report/generated/validate-report-dto-v2.d.ts",
    "declare const validate: (value: unknown) => boolean;\nexport default validate;\n",
    "utf8",
  ),
]);

await build({
  entryPoints: ["ui/report/main.ts"],
  outfile: bundlePath,
  bundle: true,
  format: "iife",
  platform: "browser",
  target: ["es2022"],
  legalComments: "none",
  banner: { js: `/* ${banner} */` },
});

const reportUi = await readFile(bundlePath, "utf8");
const currentShell = await readFile(shellPath, "utf8");
const scriptPattern = /(<script id="report-data"[^>]*>[^<]*<\/script>\s*<script>)[\s\S]*?(<\/script>\s*<\/body>)/;
if (!scriptPattern.test(currentShell)) {
  throw new Error(`Unable to locate the generated report UI block in ${shellPath}`);
}
await writeFile(shellPath, currentShell.replace(scriptPattern, `$1${reportUi}$2`), "utf8");

await build({
  entryPoints: ["ui/report/view-summary.ts"],
  outfile: viewSummaryPath,
  bundle: true,
  format: "esm",
  platform: "node",
  target: ["node20"],
  legalComments: "none",
  banner: { js: `/* ${banner} */` },
});

await build({
  entryPoints: ["ui/report/view-state.ts"],
  outfile: viewStatePath,
  bundle: true,
  format: "esm",
  platform: "node",
  target: ["node20"],
  legalComments: "none",
  banner: { js: `/* ${banner} */` },
});
