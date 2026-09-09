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
      js: "/* Generated from contracts/local-runtime-config-v5.schema.json. Do not edit. */",
    },
    write: false,
  });
  assert.equal(result.outputFiles.length, 1);
  assert.equal(
    result.outputFiles[0]?.text,
    await readFile("crates/local-ui/src/generated/settings-ui.js", "utf8"),
  );
});

test("settings v5 keeps separated-budget fields outside the activation controls and save projection", async () => {
  const source = await readFile("ui/settings/main.ts", "utf8");
  assert.match(source, /local-runtime-config-v5/);
  assert.match(source, /body: JSON\.stringify\(\{ config: draft, revision \}\)/);
  assert.match(source, /draft = structuredClone\(envelope\.config\)/);
  assert.match(source, /storage_budget: structuredClone\(draft\.storage_budget\)/);
  assert.doesNotMatch(source, /storage_budget\.(mode|retained_target_bytes|workspace_budget_bytes|minimum_free_bytes)/);
});

test("degraded collector copy maps typed reasons and retains a generic fallback", async () => {
  const source = await readFile("ui/settings/main.ts", "utf8");
  assert.match(source, /수집기 상태 저하/);
  assert.match(source, /리포트 반영 또는 데이터 보관 정리가 지연될 수 있습니다/);
  assert.doesNotMatch(source, /이벤트 수집은 가능하지만/);
  assert.match(source, /데이터 보관 정리 미완료/);
  assert.match(source, /정리용 임시 저장 공간 부족/);
  assert.match(source, /만료된 세션 데이터 제외/);
  assert.match(source, /에이전트에서 새 세션을 시작해야 합니다/);
});
