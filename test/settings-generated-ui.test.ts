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
      js: "/* Generated from contracts/local-runtime-config-v4.schema.json. Do not edit. */",
    },
    write: false,
  });
  assert.equal(result.outputFiles.length, 1);
  assert.equal(
    result.outputFiles[0]?.text,
    await readFile("crates/local-ui/src/generated/settings-ui.js", "utf8"),
  );
});

test("degraded collector copy does not claim one unavailable failure reason", async () => {
  const source = await readFile("ui/settings/main.ts", "utf8");
  assert.match(source, /수집기 상태 저하/);
  assert.match(source, /리포트 반영 또는 데이터 보관 정리가 지연될 수 있습니다/);
  assert.doesNotMatch(source, /리포트 반영 지연/);
  assert.doesNotMatch(source, /collector 실행 중 · 리포트 지연/);
});
