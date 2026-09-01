import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  classifyPackageView,
  classifyReleaseView,
  ensureDraft,
  finalizeRelease,
  publishPackage,
} from "../scripts/publish-release.mjs";

const rootPackage = JSON.parse(readFileSync("package.json", "utf8"));
const releasePackage = JSON.parse(
  readFileSync("distribution/npm/package.json", "utf8"),
);
const releaseWorkflow = readFileSync(".github/workflows/release.yml", "utf8");
const cargoMetadata = JSON.parse(
  execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], {
    encoding: "utf8",
  }),
);
const workspaceVersion = cargoMetadata.packages.find(
  ({ name }) => name === "agent-observability-cli",
).version;

test("release metadata has one synchronized Apache-2.0 version", () => {
  assert.equal(rootPackage.version, workspaceVersion);
  assert.equal(releasePackage.version, workspaceVersion);
  assert.equal(releasePackage.license, "Apache-2.0");
  assert.equal(releasePackage.repository.url, "git+https://github.com/MCprotein/agent-observability.git");
});

test("GitHub package exposes only the universal native macOS CLI", () => {
  assert.deepEqual(releasePackage.os, ["darwin"]);
  assert.deepEqual(releasePackage.cpu, ["arm64", "x64"]);
  assert.deepEqual(releasePackage.bin, {
    "agent-observability": "bin/agent-observability",
  });
  assert.equal(releasePackage.publishConfig.registry, "https://npm.pkg.github.com");
  assert.equal(releasePackage.scripts, undefined);
  assert.equal(releasePackage.dependencies, undefined);
  assert.equal(releasePackage.devDependencies, undefined);
  assert.ok(releasePackage.files.includes("examples/codex-handoff.v1.jsonl"));
});

test("release retry state distinguishes absence from lookup failure", () => {
  assert.equal(classifyReleaseView({ status: 0, stdout: "true\n", stderr: "" }), "draft");
  assert.equal(classifyReleaseView({ status: 0, stdout: "false\n", stderr: "" }), "published");
  assert.equal(classifyReleaseView({ status: 1, stdout: "", stderr: "release not found" }), "missing");
  assert.equal(classifyReleaseView({ status: 1, stdout: "", stderr: "HTTP 401" }), "error");
});

test("package retry state publishes only after an explicit not-found", () => {
  assert.equal(
    classifyPackageView({ status: 0, stdout: '"1.5.0"\n', stderr: "" }, "1.5.0"),
    "published",
  );
  assert.equal(
    classifyPackageView({ status: 1, stdout: "", stderr: "npm error code E404" }, "1.5.0"),
    "missing",
  );
  assert.equal(
    classifyPackageView({ status: 1, stdout: "", stderr: "npm error code E401" }, "1.5.0"),
    "error",
  );
  assert.equal(
    classifyPackageView({ status: 1, stdout: "", stderr: "network timeout" }, "1.5.0"),
    "error",
  );
});

test("release workflow pins actions and uses the tested publication state machine", () => {
  assert.doesNotMatch(releaseWorkflow, /uses:\s+actions\/[^@\s]+@v\d/);
  assert.match(
    releaseWorkflow,
    /lipo stage\/agent-observability -verify_arch arm64 x86_64/,
  );
  assert.match(releaseWorkflow, /publish-release\.mjs draft/);
  assert.match(releaseWorkflow, /publish-release\.mjs package/);
  assert.match(releaseWorkflow, /publish-release\.mjs finalize/);
  assert.match(releaseWorkflow, /install -m 0755 scripts\/install\.sh dist\/install\.sh/);
  assert.match(releaseWorkflow, /\*\.tar\.gz \*\.tgz install\.sh > SHA256SUMS/);
  assert.match(releaseWorkflow, /dist\/install\.sh/);
});

function scriptedExecutor(results) {
  const calls = [];
  return {
    calls,
    execute(command, args) {
      calls.push([command, args]);
      const result = results.shift();
      assert.ok(result, `unexpected command: ${command} ${args.join(" ")}`);
      return result;
    },
  };
}

const success = (stdout = "") => ({ status: 0, stdout, stderr: "" });
const failure = (stderr) => ({ status: 1, stdout: "", stderr });

test("draft transition creates, refreshes, and skips the expected release states", () => {
  const missing = scriptedExecutor([failure("release not found"), success()]);
  ensureDraft("v1.5.0", {
    execute: missing.execute,
    files: ["dist/a.tgz"],
    write() {},
  });
  assert.deepEqual(missing.calls[1], [
    "gh",
    [
      "release",
      "create",
      "v1.5.0",
      "dist/a.tgz",
      "--draft",
      "--verify-tag",
      "--generate-notes",
      "--title",
      "v1.5.0",
    ],
  ]);

  const draft = scriptedExecutor([success("true\n"), success()]);
  ensureDraft("v1.5.0", {
    execute: draft.execute,
    files: ["dist/a.tgz"],
    write() {},
  });
  assert.deepEqual(draft.calls[1], [
    "gh",
    ["release", "upload", "v1.5.0", "dist/a.tgz", "--clobber"],
  ]);

  const published = scriptedExecutor([success("false\n")]);
  ensureDraft("v1.5.0", {
    execute: published.execute,
    files: ["dist/a.tgz"],
    write() {},
  });
  assert.equal(published.calls.length, 1);
});

test("package transition publishes only a missing version and skips an existing one", () => {
  const missing = scriptedExecutor([failure("npm error code E404"), success()]);
  publishPackage("1.5.0", { execute: missing.execute, write() {} });
  assert.deepEqual(missing.calls[1], [
    "npm",
    ["publish", "./dist/mcprotein-agent-observability-1.5.0.tgz"],
  ]);

  const published = scriptedExecutor([success('"1.5.0"\n')]);
  publishPackage("1.5.0", { execute: published.execute, write() {} });
  assert.equal(published.calls.length, 1);

  const unauthorized = scriptedExecutor([failure("npm error code E401")]);
  assert.throws(
    () => publishPackage("1.5.0", { execute: unauthorized.execute, write() {} }),
    /package lookup failed/,
  );
});

test("finalize transition publishes a draft and treats publication as idempotent", () => {
  const draft = scriptedExecutor([success("true\n"), success()]);
  finalizeRelease("v1.5.0", { execute: draft.execute, write() {} });
  assert.deepEqual(draft.calls[1], [
    "gh",
    ["release", "edit", "v1.5.0", "--draft=false", "--latest"],
  ]);

  const published = scriptedExecutor([success("false\n")]);
  finalizeRelease("v1.5.0", { execute: published.execute, write() {} });
  assert.equal(published.calls.length, 1);

  const missing = scriptedExecutor([failure("release not found")]);
  assert.throws(
    () => finalizeRelease("v1.5.0", { execute: missing.execute, write() {} }),
    /release finalization lookup failed/,
  );
});
