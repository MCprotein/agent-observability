/* Generated from contracts/codex-integration-status-v1.schema.json. Do not edit. */

export type CodexIntegrationStatusV1 = {
  schema_version: "codex_integration_status.v1";
  config: CodexConnectionStatusV1;
  notify: CodexNotifyStatusV1 | null;
  collector: CollectorStatusV1;
  endpoint: string | null;
  service: string | null;
  data_retained: boolean;
  /**
   * @maxItems 3
   */
  collector_degradation_reasons:
    | []
    | [CollectorDegradationReasonV1]
    | [CollectorDegradationReasonV1, CollectorDegradationReasonV1]
    | [CollectorDegradationReasonV1, CollectorDegradationReasonV1, CollectorDegradationReasonV1];
};
export type CodexConnectionStatusV1 = "connected" | "disconnected" | "conflict";
export type CodexNotifyStatusV1 = "agentobs_owned" | "external_preserved";
export type CollectorStatusV1 = "ready" | "degraded" | "unavailable";
export type CollectorDegradationReasonV1 = "lifecycle_failure" | "storage_pressure" | "expired_trace";
