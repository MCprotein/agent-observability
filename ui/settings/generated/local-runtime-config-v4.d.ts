/* Generated from contracts/local-runtime-config-v4.schema.json. Do not edit. */

export interface LocalRuntimeConfigV4 {
  schema_version: "local_runtime.v4";
  enabled: boolean;
  capture_private_codex_turn_details: boolean;
  collection: Collection;
  retention: Retention;
  lifecycle: Lifecycle;
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
export interface Lifecycle {
  enabled: boolean;
  hot_days: number;
  warm_days: number;
  delete_after_days: number;
  private_raw_days: number;
  maintenance_interval_seconds: number;
  max_traces_per_pass: number;
}
