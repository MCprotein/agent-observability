import validateSchema from "./generated/validate-local-runtime-config-v5.js";
import type { LocalRuntimeConfigV5 } from "./generated/local-runtime-config-v5.js";

export type ConfigValidationResult =
  | { valid: true; errors: [] }
  | { valid: false; errors: Array<{ path: string; message: string }> };

export function validateLocalRuntimeConfig(value: unknown): ConfigValidationResult {
  if (!validateSchema(value)) {
    return {
      valid: false,
      errors: (validateSchema.errors ?? []).map((error) => ({
        path: error.instancePath?.replace(/^\//, "").replaceAll("/", ".") ?? "",
        message: error.message ?? "허용 범위를 확인하세요.",
      })),
    };
  }

  const config = value as LocalRuntimeConfigV5;
  const { hot_days: hot, warm_days: warm, delete_after_days: expiry } = config.lifecycle;
  if (hot > warm) {
    return {
      valid: false,
      errors: [{ path: "lifecycle.warm_days", message: "Hot 기준일 이상이어야 합니다." }],
    };
  }
  if (warm >= expiry) {
    return {
      valid: false,
      errors: [{ path: "lifecycle.delete_after_days", message: "Warm 기준일보다 커야 합니다." }],
    };
  }
  return { valid: true, errors: [] };
}
