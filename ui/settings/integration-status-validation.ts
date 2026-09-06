import validateSchema from "./generated/validate-codex-integration-status-v1.js";
import type { CodexIntegrationStatusV1 } from "./generated/codex-integration-status-v1.js";

export function validateCodexIntegrationStatus(
  value: unknown,
): value is CodexIntegrationStatusV1 {
  return validateSchema(value);
}
