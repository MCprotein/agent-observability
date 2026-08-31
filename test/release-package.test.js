import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  classifyPackageView,
  classifyReleaseView,
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
    classifyPackageView({ status: 0, stdout: '"1.3.0"\n', stderr: "" }, "1.3.0"),
    "published",
  );
  assert.equal(
    classifyPackageView({ status: 1, stdout: "", stderr: "npm error code E404" }, "1.3.0"),
    "missing",
  );
  assert.equal(
    classifyPackageView({ status: 1, stdout: "", stderr: "npm error code E401" }, "1.3.0"),
    "error",
  );
  assert.equal(
    classifyPackageView({ status: 1, stdout: "", stderr: "network timeout" }, "1.3.0"),
    "error",
  );
});

test("release workflow pins actions and uses the tested publication state machine", () => {
  assert.doesNotMatch(releaseWorkflow, /uses:\s+actions\/[^@\s]+@v\d/);
  assert.match(releaseWorkflow, /publish-release\.mjs draft/);
  assert.match(releaseWorkflow, /publish-release\.mjs package/);
  assert.match(releaseWorkflow, /publish-release\.mjs finalize/);
});
