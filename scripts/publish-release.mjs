import { readdirSync, statSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

function commandResult(command, args) {
  return spawnSync(command, args, {
    encoding: "utf8",
    env: process.env,
  });
}

function failure(result, context) {
  const detail = [result.stdout, result.stderr].filter(Boolean).join("\n").trim();
  return new Error(`${context} failed with exit ${result.status}: ${detail}`);
}

function run(execute, command, args, context = command) {
  const result = execute(command, args);
  if (result.status !== 0) {
    throw failure(result, context);
  }
  return result.stdout.trim();
}

export function classifyReleaseView(result) {
  if (result.status === 0) {
    const draft = result.stdout.trim();
    if (draft === "true") return "draft";
    if (draft === "false") return "published";
    return "invalid";
  }
  const detail = `${result.stdout}\n${result.stderr}`;
  return /release not found|HTTP 404|Not Found/i.test(detail) ? "missing" : "error";
}

export function classifyPackageView(result, version) {
  if (result.status === 0) {
    let found;
    try {
      found = JSON.parse(result.stdout);
    } catch {
      return "invalid";
    }
    return found === version ? "published" : "invalid";
  }
  const detail = `${result.stdout}\n${result.stderr}`;
  return /\bE404\b|404 Not Found/i.test(detail) ? "missing" : "error";
}

function releaseView(execute, tag) {
  return execute("gh", [
    "release",
    "view",
    tag,
    "--json",
    "isDraft",
    "--jq",
    ".isDraft",
  ]);
}

function distributionFiles() {
  return readdirSync("dist")
    .map((name) => `dist/${name}`)
    .filter((path) => statSync(path).isFile())
    .sort();
}

export function ensureDraft(
  tag,
  {
    execute = commandResult,
    files = distributionFiles(),
    write = (message) => process.stdout.write(message),
  } = {},
) {
  const view = releaseView(execute, tag);
  const state = classifyReleaseView(view);
  if (state === "error" || state === "invalid") {
    throw failure(view, "release lookup");
  }
  if (state === "published") {
    write("release=already-published\n");
    return;
  }

  if (state === "draft") {
    run(execute, "gh", ["release", "upload", tag, ...files, "--clobber"], "release upload");
  } else {
    run(
      execute,
      "gh",
      [
        "release",
        "create",
        tag,
        ...files,
        "--draft",
        "--verify-tag",
        "--generate-notes",
        "--title",
        tag,
      ],
      "release creation",
    );
  }
  write("release=draft\n");
}

export function publishPackage(
  version,
  {
    execute = commandResult,
    write = (message) => process.stdout.write(message),
  } = {},
) {
  const packageName = "@mcprotein/agent-observability";
  const registry = "https://npm.pkg.github.com";
  const view = execute("npm", [
    "view",
    `${packageName}@${version}`,
    "version",
    "--registry",
    registry,
    "--json",
  ]);
  const state = classifyPackageView(view, version);
  if (state === "error" || state === "invalid") {
    throw failure(view, "package lookup");
  }
  if (state === "published") {
    write("package=already-published\n");
    return;
  }

  run(
    execute,
    "npm",
    ["publish", `./dist/mcprotein-agent-observability-${version}.tgz`],
    "package publication",
  );
  write("package=published\n");
}

export function finalizeRelease(
  tag,
  {
    execute = commandResult,
    write = (message) => process.stdout.write(message),
  } = {},
) {
  const view = releaseView(execute, tag);
  const state = classifyReleaseView(view);
  if (state === "published") {
    write("release=already-published\n");
    return;
  }
  if (state !== "draft") {
    throw failure(view, "release finalization lookup");
  }
  run(
    execute,
    "gh",
    ["release", "edit", tag, "--draft=false", "--latest"],
    "release finalization",
  );
  write("release=published\n");
}

function main() {
  const [command, value] = process.argv.slice(2);
  if (command === "draft" && value) return ensureDraft(value);
  if (command === "package" && value) return publishPackage(value);
  if (command === "finalize" && value) return finalizeRelease(value);
  throw new Error("usage: publish-release.mjs <draft TAG|package VERSION|finalize TAG>");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
