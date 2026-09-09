import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const launcher = resolve("scripts/test-rust-cli.sh");

test("both Rust CI jobs replace default CLI execution with the mandatory complete-package launcher", () => {
  const workflow = readFileSync(resolve(".github/workflows/ci.yml"), "utf8");
  const paired = /cargo \+1\.97\.0 test --workspace --exclude agent-observability-cli --no-fail-fast -- --skip "\$timing_test"\n\s+bash scripts\/test-rust-cli\.sh/g;
  assert.equal([...workflow.matchAll(paired)].length, 2);
});

function runHarness(soft: number, hard: number) {
  const fixture = mkdtempSync(join(tmpdir(), "agentobs-test-harness-"));
  try {
    writeFileSync(
      join(fixture, "cargo"),
      '#!/bin/bash\nprintf "child_limit=%s\\n" "$(ulimit -Sn)"\nprintf "arg=%s\\n" "$@"\n',
      { mode: 0o700 },
    );
    return spawnSync(
      "/bin/bash",
      [
        "-c",
        'ulimit -Sn "$1" || exit 90; ulimit -Hn "$2" || exit 91; /bin/bash "$3"; result=$?; printf "parent_limit=%s\\n" "$(ulimit -Sn)"; exit "$result"',
        "test-harness",
        String(soft),
        String(hard),
        launcher,
      ],
      {
        env: { ...process.env, PATH: `${fixture}:${process.env.PATH ?? ""}` },
        encoding: "utf8",
        timeout: 10_000,
      },
    );
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
}

test("CLI harness raises only its child soft limit and runs all CLI tests in parallel", { skip: process.platform === "win32" }, () => {
  const result = runHarness(256, 4096);
  assert.equal(result.error, undefined);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /child_limit=1024\n/);
  assert.match(result.stdout, /parent_limit=256\n/);
  assert.match(result.stdout, /arg=\+1\.97\.0\narg=test\narg=--locked\narg=-p\narg=agent-observability-cli\narg=--no-fail-fast\narg=--\narg=--test-threads=32\n/);
});

test("CLI harness preserves an already sufficient child limit", { skip: process.platform === "win32" }, () => {
  const result = runHarness(2048, 4096);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /child_limit=2048\n/);
  assert.match(result.stdout, /parent_limit=2048\n/);
});

test("CLI harness rejects insufficient hard limits before running cargo", { skip: process.platform === "win32" }, () => {
  const result = runHarness(256, 512);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /requires a hard file-descriptor limit of at least 1024/);
  assert.doesNotMatch(result.stdout, /child_limit=|arg=/);
  assert.match(result.stdout, /parent_limit=256\n/);
});
