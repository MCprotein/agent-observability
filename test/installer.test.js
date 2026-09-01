import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import {
  chmodSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  symlinkSync,
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

test("custom install and profile paths remain valid shell syntax", (t) => {
  const fixture = makeFixture();
  t.after(() => rmSync(fixture.root, { recursive: true, force: true }));

  const installDir = join(fixture.home, "tools' bin");
  const profile = join(fixture.home, "shell profiles", "agent profile");
  execFileSync("sh", [installer], {
    env: {
      ...fixture.env,
      AGENT_OBSERVABILITY_INSTALL_DIR: installDir,
      AGENT_OBSERVABILITY_SHELL_PROFILE: profile,
    },
  });

  const quotedProfile = profile.replaceAll("'", "'\\''");
  const output = execFileSync(
    "sh",
    ["-c", `. '${quotedProfile}'; agent-observability --version`],
    { encoding: "utf8", env: fixture.env },
  );
  assert.equal(output.trim(), "1.6.0");
});

test("reinstalling to a new directory replaces the managed PATH block", (t) => {
  const fixture = makeFixture();
  t.after(() => rmSync(fixture.root, { recursive: true, force: true }));

  const firstDir = join(fixture.home, "first-bin");
  const secondDir = join(fixture.home, "second-bin");
  execFileSync("sh", [installer], {
    env: { ...fixture.env, AGENT_OBSERVABILITY_INSTALL_DIR: firstDir },
  });
  execFileSync("sh", [installer], {
    env: { ...fixture.env, AGENT_OBSERVABILITY_INSTALL_DIR: secondDir },
  });

  const profile = readFileSync(join(fixture.home, ".zshrc"), "utf8");
  assert.doesNotMatch(profile, /first-bin/);
  assert.match(profile, /second-bin/);
  assert.equal(profile.match(/>>> agent-observability PATH >>>/g)?.length, 1);
});

test("installer preserves permissions on existing private directories", (t) => {
  const fixture = makeFixture();
  t.after(() => rmSync(fixture.root, { recursive: true, force: true }));

  const installDir = join(fixture.home, "private-bin");
  const profileDir = join(fixture.home, "private-shell");
  mkdirSync(installDir, { mode: 0o700 });
  mkdirSync(profileDir, { mode: 0o700 });
  execFileSync("sh", [installer], {
    env: {
      ...fixture.env,
      AGENT_OBSERVABILITY_INSTALL_DIR: installDir,
      AGENT_OBSERVABILITY_SHELL_PROFILE: join(profileDir, "profile"),
    },
  });

  assert.equal(statSync(installDir).mode & 0o777, 0o700);
  assert.equal(statSync(profileDir).mode & 0o777, 0o700);
});

test("atomic profile updates preserve a profile symlink and target mode", (t) => {
  const fixture = makeFixture();
  t.after(() => rmSync(fixture.root, { recursive: true, force: true }));

  const targetDir = join(fixture.home, "shell");
  const target = join(targetDir, "profile");
  mkdirSync(targetDir);
  writeFileSync(target, "existing profile\n", { mode: 0o600 });
  symlinkSync("shell/profile", join(fixture.home, ".zshrc"));

  execFileSync("sh", [installer], { env: fixture.env });

  assert.equal(lstatSync(join(fixture.home, ".zshrc")).isSymbolicLink(), true);
  assert.match(readFileSync(target, "utf8"), /agent-observability PATH/);
  assert.equal(statSync(target).mode & 0o777, 0o600);
});

test("reversed managed markers fail without changing the profile", (t) => {
  const fixture = makeFixture();
  t.after(() => rmSync(fixture.root, { recursive: true, force: true }));

  const profile = join(fixture.home, ".zshrc");
  const original = [
    "existing profile",
    "# <<< agent-observability PATH <<<",
    "# >>> agent-observability PATH >>>",
    "",
  ].join("\n");
  writeFileSync(profile, original);

  const result = spawnSync("sh", [installer], {
    encoding: "utf8",
    env: fixture.env,
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /invalid agent-observability PATH block/);
  assert.equal(readFileSync(profile, "utf8"), original);
});

test("latest release URL selects the redirected semantic version", (t) => {
  const fixture = makeFixture();
  t.after(() => rmSync(fixture.root, { recursive: true, force: true }));

  const stubDir = join(fixture.root, "stub-bin");
  const curlStub = join(stubDir, "curl");
  mkdirSync(stubDir);
  writeFileSync(
    curlStub,
    `#!/usr/bin/env node
import { copyFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
const args = process.argv.slice(2);
if (args.includes("-w")) {
  process.stdout.write("https://github.com/MCprotein/agent-observability/releases/tag/v1.6.0");
  process.exit(0);
}
const outputIndex = args.indexOf("-o");
copyFileSync(fileURLToPath(args[outputIndex - 1]), args[outputIndex + 1]);
`,
  );
  chmodSync(curlStub, 0o755);

  const env = {
    ...fixture.env,
    PATH: `${stubDir}:${fixture.env.PATH}`,
  };
  delete env.AGENT_OBSERVABILITY_VERSION;
  const output = execFileSync("sh", [installer], { encoding: "utf8", env });
  assert.match(output, /Installed agent-observability 1\.6\.0/);
});
