import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";

const installer = join(process.cwd(), "scripts", "install.sh");

function makeFixture(version = "1.6.0") {
  const root = mkdtempSync(join(tmpdir(), "agent-observability-installer-"));
  const home = join(root, "home");
  const releaseRoot = join(root, "releases");
  const downloadDir = join(releaseRoot, "download", `v${version}`);
  const archiveRoot = `agent-observability-${version}-darwin-universal2`;
  const stage = join(root, "stage", archiveRoot);
  const archiveName = `${archiveRoot}.tar.gz`;
  const archive = join(downloadDir, archiveName);

  mkdirSync(home, { recursive: true });
  mkdirSync(downloadDir, { recursive: true });
  mkdirSync(stage, { recursive: true });
  writeFileSync(
    join(stage, "agent-observability"),
    `#!/bin/sh\nprintf '%s\\n' '${version}'\n`,
  );
  chmodSync(join(stage, "agent-observability"), 0o755);
  execFileSync("tar", ["-C", join(root, "stage"), "-czf", archive, archiveRoot]);
  const checksum = execFileSync("shasum", ["-a", "256", archive], {
    encoding: "utf8",
  }).split(/\s+/)[0];
  writeFileSync(join(downloadDir, "SHA256SUMS"), `${checksum}  ${archiveName}\n`);

  const env = {
    ...process.env,
    AGENT_OBSERVABILITY_PLATFORM: "Darwin",
    AGENT_OBSERVABILITY_RELEASE_BASE_URL: pathToFileURL(releaseRoot).href,
    AGENT_OBSERVABILITY_VERSION: version,
    HOME: home,
    SHELL: "/bin/zsh",
  };
  return { env, home, root };
}

test("installer verifies, installs, and registers PATH idempotently", (t) => {
  const fixture = makeFixture();
  t.after(() => rmSync(fixture.root, { recursive: true, force: true }));
  writeFileSync(join(fixture.home, ".zshrc"), "existing profile\n");

  const first = execFileSync("sh", [installer], {
    encoding: "utf8",
    env: fixture.env,
  });
  const second = execFileSync("sh", [installer], {
    encoding: "utf8",
    env: fixture.env,
  });

  const binary = join(fixture.home, ".local", "bin", "agent-observability");
  assert.equal(execFileSync(binary, { encoding: "utf8" }).trim(), "1.6.0");
  assert.match(first, /Activate it in this terminal: \. '.*\.zshrc'/);
  assert.match(second, /Installed agent-observability 1\.6\.0/);

  const profile = readFileSync(join(fixture.home, ".zshrc"), "utf8");
  assert.match(profile, /^existing profile\n/);
  assert.equal(profile.match(/>>> agent-observability PATH >>>/g)?.length, 1);
  assert.match(profile, /export PATH='.*\.local\/bin':"\$PATH"/);
});

test("checksum failure preserves an existing installation and profile", (t) => {
  const fixture = makeFixture();
  t.after(() => rmSync(fixture.root, { recursive: true, force: true }));

  const binDir = join(fixture.home, ".local", "bin");
  const binary = join(binDir, "agent-observability");
  mkdirSync(binDir, { recursive: true });
  writeFileSync(binary, "existing installation\n");
  writeFileSync(join(fixture.home, ".zshrc"), "existing profile\n");

  const sums = join(
    fixture.root,
    "releases",
    "download",
    "v1.6.0",
    "SHA256SUMS",
  );
  writeFileSync(sums, readFileSync(sums, "utf8").replace(/^[0-9a-f]+/, "0".repeat(40)));

  const result = spawnSync("sh", [installer], {
    encoding: "utf8",
    env: fixture.env,
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /checksum verification failed/);
  assert.equal(readFileSync(binary, "utf8"), "existing installation\n");
  assert.equal(readFileSync(join(fixture.home, ".zshrc"), "utf8"), "existing profile\n");
});
