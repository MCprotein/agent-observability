import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";

const rootPackage = JSON.parse(readFileSync("package.json", "utf8"));
const releasePackage = JSON.parse(
  readFileSync("distribution/npm/package.json", "utf8"),
);
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
});
