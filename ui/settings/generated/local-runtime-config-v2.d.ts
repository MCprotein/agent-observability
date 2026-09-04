/* Generated from contracts/local-runtime-config-v2.schema.json. Do not edit. */

export interface LocalRuntimeConfigV2 {
  schema_version: "local_runtime.v2";
  enabled: boolean;
  capture_private_codex_turn_details?: boolean;
  collection: Collection;
  retention: Retention;
}
export interface Collection {
  file_reconcile_interval_ms: number;
  flush_interval_ms: number;
  max_batch_records: number;
  max_batch_bytes: number;
  active_heartbeat_interval_ms: number;
  idle_heartbeat_interval_ms: number;
  local_storage_budget_bytes: number;
}
export interface Retention {
  max_record_age_days: number;
  max_archive_records: number;
  max_archive_bytes: number;
}
