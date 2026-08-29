import { mkdir, readFile, writeFile } from "node:fs/promises";
import Ajv2020 from "ajv/dist/2020.js";
import standaloneCode from "ajv/dist/standalone/index.js";
import { build } from "esbuild";
import { compileFromFile } from "json-schema-to-typescript";

const schemaPath = "contracts/report-dto-v1.schema.json";
const typePath = "ui/report/generated/report-dto-v1.d.ts";
const browserSchemaPath = "ui/report/generated/report-dto-v1.schema.json";
const validatorPath = "ui/report/generated/validate-report-dto-v1.js";
const bundlePath = "src/report/generated/report-ui.js";
const shellPath = "src/report/generated/report-shell.html";
const viewSummaryPath = "ui/report/generated/view-summary.js";
const banner = "Generated from contracts/report-dto-v1.schema.json. Do not edit.";

await Promise.all([
  mkdir("ui/report/generated", { recursive: true }),
  mkdir("src/report/generated", { recursive: true }),
]);

const declarations = await compileFromFile(schemaPath, {
  bannerComment: `/* ${banner} */`,
  style: { singleQuote: false },
});
await writeFile(typePath, declarations, "utf8");
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
await Promise.all([
  writeFile(validatorPath, standaloneCode(ajv, validate), "utf8"),
  writeFile(
    "ui/report/generated/validate-report-dto-v1.d.ts",
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

const { reportDocumentTemplate } = await import("../src/report/html.js");
await writeFile(shellPath, reportDocumentTemplate(), "utf8");

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
