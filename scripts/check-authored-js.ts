import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";

const generatedJavaScript = new Set([
  "crates/local-ui/src/generated/settings-ui.js",
  "src/report/generated/report-ui.js",
  "ui/report/generated/validate-report-dto-v2.js",
  "ui/report/generated/view-state.js",
  "ui/report/generated/view-summary.js",
  "ui/settings/generated/validate-local-runtime-config-v3.js",
]);
const managedToolingPrefixes = [".anamnesis/", ".claude/", ".codex/", ".cursor/"];

const trackedJavaScript = execFileSync(
  "git",
  ["ls-files", "--cached", "--others", "--exclude-standard", "*.js", "*.mjs", "*.cjs"],
  { encoding: "utf8" },
)
  .split("\n")
  .filter((path) => path.length > 0 && existsSync(path))
  .filter((path) => !managedToolingPrefixes.some((prefix) => path.startsWith(prefix)));

const authoredJavaScript = trackedJavaScript.filter((path) => !generatedJavaScript.has(path));
if (authoredJavaScript.length > 0) {
  throw new Error(`Authored JavaScript is not allowed:\n${authoredJavaScript.join("\n")}`);
}

const missingArtifacts = [...generatedJavaScript].filter((path) => !trackedJavaScript.includes(path));
if (missingArtifacts.length > 0) {
  throw new Error(`Generated JavaScript allowlist is stale:\n${missingArtifacts.join("\n")}`);
}
