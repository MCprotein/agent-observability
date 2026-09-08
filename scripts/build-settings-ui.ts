import { mkdir, readFile, writeFile } from "node:fs/promises";
import Ajv2020Module from "ajv/dist/2020.js";
import standaloneCodeModule from "ajv/dist/standalone/index.js";
import { build } from "esbuild";
import { compileFromFile } from "json-schema-to-typescript";

const configSchemaPath = "contracts/local-runtime-config-v5.schema.json";
const integrationStatusSchemaPath = "contracts/codex-integration-status-v1.schema.json";
const generatedUiPath = "ui/settings/generated";
const generatedRustPath = "crates/local-ui/src/generated";
const configBanner = "Generated from contracts/local-runtime-config-v5.schema.json. Do not edit.";
const integrationStatusBanner = "Generated from contracts/codex-integration-status-v1.schema.json. Do not edit.";
const Ajv2020 = Ajv2020Module as unknown as typeof import("ajv/dist/2020.js").default;
const standaloneCode = standaloneCodeModule as unknown as typeof import("ajv/dist/standalone/index.js").default;

await Promise.all([
  mkdir(generatedUiPath, { recursive: true }),
  mkdir(generatedRustPath, { recursive: true }),
]);

const configDeclarations = await compileFromFile(configSchemaPath, {
  bannerComment: `/* ${configBanner} */`,
  style: { singleQuote: false },
});
const integrationStatusDeclarations = await compileFromFile(integrationStatusSchemaPath, {
  bannerComment: `/* ${integrationStatusBanner} */`,
  style: { singleQuote: false },
});
await Promise.all([
  writeFile(`${generatedUiPath}/local-runtime-config-v5.d.ts`, configDeclarations, "utf8"),
  writeFile(
    `${generatedUiPath}/codex-integration-status-v1.d.ts`,
    integrationStatusDeclarations,
    "utf8",
  ),
]);

const ajv = new Ajv2020({ code: { esm: true, source: true }, strict: true });
const compileBrowserValidator = async (path: string) => {
  const schema = JSON.parse(await readFile(path, "utf8"));
  delete schema.$schema;
  delete schema.$id;
  return ajv.compile(schema);
};
const validateConfig = await compileBrowserValidator(configSchemaPath);
const validateIntegrationStatus = await compileBrowserValidator(integrationStatusSchemaPath);
const integrationErrorPath = "contracts/codex-integration-error-v1.schema.json";
await writeFile(`${generatedUiPath}/codex-integration-error-v1.d.ts`, await compileFromFile(integrationErrorPath), "utf8");
await writeFile(`${generatedUiPath}/validate-codex-integration-error-v1.js`,
  standaloneCode(ajv, await compileBrowserValidator(integrationErrorPath)), "utf8");
await writeFile(`${generatedUiPath}/validate-codex-integration-error-v1.d.ts`,
  "declare const validate: (value: unknown) => boolean;\nexport default validate;\n", "utf8");
await Promise.all([
  writeFile(
    `${generatedUiPath}/validate-local-runtime-config-v5.js`,
    standaloneCode(ajv, validateConfig),
    "utf8",
  ),
  writeFile(
    `${generatedUiPath}/validate-local-runtime-config-v5.d.ts`,
    "declare const validate: ((value: unknown) => boolean) & { errors?: Array<{ instancePath?: string; message?: string }> | null };\nexport default validate;\n",
    "utf8",
  ),
  writeFile(
    `${generatedUiPath}/validate-codex-integration-status-v1.js`,
    standaloneCode(ajv, validateIntegrationStatus),
    "utf8",
  ),
  writeFile(
    `${generatedUiPath}/validate-codex-integration-status-v1.d.ts`,
    "declare const validate: ((value: unknown) => boolean) & { errors?: Array<{ instancePath?: string; message?: string }> | null };\nexport default validate;\n",
    "utf8",
  ),
]);

await build({
  entryPoints: ["ui/settings/main.ts"],
  outfile: `${generatedRustPath}/settings-ui.js`,
  bundle: true,
  format: "iife",
  platform: "browser",
  target: ["es2022"],
  legalComments: "none",
  banner: { js: `/* ${configBanner} */` },
});

await Promise.all([
  writeFile(
    `${generatedRustPath}/settings-ui.css`,
    await readFile("ui/settings/main.css", "utf8"),
    "utf8",
  ),
  writeFile(
    `${generatedRustPath}/settings-shell.html`,
    await readFile("ui/settings/index.html", "utf8"),
    "utf8",
  ),
]);
