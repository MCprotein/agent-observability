import { createHash } from "node:crypto";

const HASHED_ID_PREFIX = "id:sha256:";

export function composeSpanId(prefix, ...parts) {
  return [prefix, ...parts.map((part) => encodeURIComponent(String(part)))].join(":");
}

export function hashOpaqueIdentifier(value) {
  if (value === null || value === undefined || String(value).startsWith(HASHED_ID_PREFIX)) {
    return value;
  }
  return `${HASHED_ID_PREFIX}${createHash("sha256").update(String(value)).digest("hex")}`;
}

export function stableSourceIdentity(event, label) {
  const sourceKey = event.source_offset ?? event.source_index ?? event.cursor;
  if (sourceKey === undefined || sourceKey === null || sourceKey === "") {
    throw new Error(`${label} requires a stable source identity`);
  }
  return `${label}:${sourceKey}`;
}
