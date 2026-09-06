import { mkdir, readFile, writeFile } from "node:fs/promises";
import Ajv2020Module from "ajv/dist/2020.js";
import standaloneCodeModule from "ajv/dist/standalone/index.js";
import { build } from "esbuild";
import { compileFromFile } from "json-schema-to-typescript";

const schemaPath = "contracts/local-runtime-config-v3.schema.json";
const generatedUiPath = "ui/settings/generated";
const generatedRustPath = "crates/local-ui/src/generated";
const banner = "Generated from contracts/local-runtime-config-v3.schema.json. Do not edit.";
const Ajv2020 = Ajv2020Module as unknown as typeof import("ajv/dist/2020.js").default;
const standaloneCode = standaloneCodeModule as unknown as typeof import("ajv/dist/standalone/index.js").default;

await Promise.all([
  mkdir(generatedUiPath, { recursive: true }),
  mkdir(generatedRustPath, { recursive: true }),
]);

const declarations = await compileFromFile(schemaPath, {
  bannerComment: `/* ${banner} */`,
  style: { singleQuote: false },
});
await writeFile(`${generatedUiPath}/local-runtime-config-v3.d.ts`, declarations, "utf8");

const browserSchema = JSON.parse(await readFile(schemaPath, "utf8"));
delete browserSchema.$schema;
delete browserSchema.$id;
const ajv = new Ajv2020({ code: { esm: true, source: true }, strict: true });
const validate = ajv.compile(browserSchema);
await Promise.all([
  writeFile(
    `${generatedUiPath}/validate-local-runtime-config-v3.js`,
    standaloneCode(ajv, validate),
    "utf8",
  ),
  writeFile(
    `${generatedUiPath}/validate-local-runtime-config-v3.d.ts`,
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
  banner: { js: `/* ${banner} */` },
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
