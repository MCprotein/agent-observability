import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { build } from "esbuild";

test("tracked settings JavaScript matches the TypeScript bundle", async () => {
  const result = await build({
    entryPoints: ["ui/settings/main.ts"],
    bundle: true,
    format: "iife",
    platform: "browser",
    target: ["es2022"],
    legalComments: "none",
    banner: {
      js: "/* Generated from contracts/local-runtime-config-v3.schema.json. Do not edit. */",
    },
    write: false,
  });
  assert.equal(result.outputFiles.length, 1);
  assert.equal(
    result.outputFiles[0]?.text,
    await readFile("crates/local-ui/src/generated/settings-ui.js", "utf8"),
  );
});
