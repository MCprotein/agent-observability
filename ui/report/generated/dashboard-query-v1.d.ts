/* Generated from contracts/dashboard-query-v1.schema.json. Do not edit. */

/**
 * Closed request and response wire contract for the bounded standalone paged dashboard. Cursors are opaque server leases and never grant query authority.
 */
export type DashboardQueryWireV1 = DashboardQueryRequestV1 | DashboardQueryResponseV1;
export type DashboardQueryRequestV1 =
  | DashboardBootstrapRequestV1
  | DashboardTracesRequestV1
  | DashboardSpansRequestV1
  | DashboardSummaryRequestV1
  | DashboardSpanRequestV1
  | DashboardFacetsRequestV1;
export type DashboardBootstrapRequestV1 = RequestBase & {
  kind: "bootstrap";
};
/**
 * @maxItems 16
 */
export type FilterValues =
  | []
  | [string]
  | [string, string]
  | [string, string, string]
  | [string, string, string, string]
  | [string, string, string, string, string]
  | [string, string, string, string, string, string]
  | [string, string, string, string, string, string, string]
  | [string, string, string, string, string, string, string, string]
  | [string, string, string, string, string, string, string, string, string]
  | [string, string, string, string, string, string, string, string, string, string]
  | [string, string, string, string, string, string, string, string, string, string, string]
  | [string, string, string, string, string, string, string, string, string, string, string, string]
  | [string, string, string, string, string, string, string, string, string, string, string, string, string]
  | [string, string, string, string, string, string, string, string, string, string, string, string, string, string]
  | [
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string
    ]
  | [
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string,
      string
    ];
export type DashboardTracesRequestV1 = RequestBase & {
  kind: "traces";
  snapshotId: SnapshotId;
  cursor?: NullableCursor;
};
export type SnapshotId = string;
export type NullableCursor = Cursor | null;
/**
 * Opaque server-issued lease token bound to the normalized query, snapshot generation, visibility epoch, query kind and scan key. Clients must not parse or construct it.
 */
export type Cursor = string;
export type DashboardSpansRequestV1 = RequestBase & {
  kind: "spans";
  snapshotId: SnapshotId;
  traceId: ProjectedId;
  cursor?: NullableCursor;
};
export type ProjectedId = string;
export type DashboardSummaryRequestV1 = RequestBase & {
  kind: "summary";
  snapshotId: SnapshotId;
  cursor?: NullableCursor;
};
export type DashboardSpanRequestV1 = RequestBase & {
  kind: "span";
  snapshotId: SnapshotId;
  traceId: ProjectedId;
  spanId: ProjectedId;
};
export type DashboardFacetsRequestV1 = RequestBase & {
  kind: "facets";
  snapshotId: SnapshotId;
  cursor?: NullableCursor;
};
export type DashboardQueryResponseV1 =
  | DashboardBootstrapResponseV1
  | DashboardTracesResponseV1
  | DashboardSpansResponseV1
  | DashboardSummaryResponseV1
  | DashboardSpanResponseV1
  | DashboardFacetsResponseV1
  | DashboardStatusResponseV1;
export type DashboardBootstrapResponseV1 = ResponseBase & {
  kind: "bootstrap";
  limits: DashboardQueryLimitsV1;
};
/**
 * Unsigned 64-bit counter serialized as a decimal string to avoid JavaScript precision loss. Semantic validators additionally reject values above u64::MAX.
 */
export type DecimalU64 = string;
export type DashboardSnapshotStateV1 = "current" | "stale";
/**
 * KPI values have no independent generation field and therefore inherit the enclosing snapshot generation by construction.
 */
export type DashboardWorkV1 = DashboardPendingWorkV1 | DashboardCompleteWorkV1;
export type DashboardKpisV1 = (
  | {
      tokenStatus?: "complete";
      totalTokens?: SafeCount;
    }
  | {
      tokenStatus?: "incomplete";
      totalTokens?: null;
    }
  | {
      tokenStatus?: "unavailable";
      inputTokens?: null;
      outputTokens?: null;
      totalTokens?: null;
    }
) &
  (
    | {
        costStatus?: "estimated";
        estimatedCost?: number;
        currency?: string;
      }
    | {
        costStatus?: "incomplete";
      }
    | {
        costStatus?: "unknown";
        estimatedCost?: null;
        currency?: null;
      }
  ) &
  Kpis;
export type SafeCount = number;
export type NullableSafeCount = SafeCount | null;
export type DashboardTokenStatusV1 = "complete" | "incomplete" | "unavailable";
export type DashboardCostStatusV1 = "estimated" | "incomplete" | "unknown";
export type DashboardTracesResponseV1 = ResponseBase & {
  kind: "traces";
  /**
   * @maxItems 100
   */
  rows: DashboardTraceRowV1[];
  pagination: DashboardPaginationV1;
};
/**
 * @maxItems 9
 */
export type AvailabilityReasons = AvailabilityReasons1;
export type AvailabilityReasons1 =
  | []
  | [DashboardAvailabilityReasonV1]
  | [DashboardAvailabilityReasonV1, DashboardAvailabilityReasonV1]
  | [DashboardAvailabilityReasonV1, DashboardAvailabilityReasonV1, DashboardAvailabilityReasonV1]
  | [
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1
    ]
  | [
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1
    ]
  | [
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1
    ]
  | [
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1
    ]
  | [
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1
    ]
  | [
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1,
      DashboardAvailabilityReasonV1
    ];
export type DashboardAvailabilityReasonV1 = AvailabilityReason &
  AvailabilityReason1 &
  AvailabilityReason &
  AvailabilityReason1 &
  AvailabilityReason &
  AvailabilityReason1 &
  AvailabilityReason &
  AvailabilityReason1 &
  AvailabilityReason &
  AvailabilityReason1 &
  AvailabilityReason &
  AvailabilityReason1 &
  AvailabilityReason &
  AvailabilityReason1 &
  AvailabilityReason &
  AvailabilityReason1 &
  AvailabilityReason &
  AvailabilityReason1;
export type AvailabilityReason =
  | {
      state: "source_unavailable";
      reason:
        | "source_not_provided"
        | "not_evaluated"
        | "partial_token_metrics"
        | "historical_codex_source_not_lookup_eligible"
        | "codex_notify_turn_correlation_unavailable"
        | "ambiguous_trace_repository"
        | "legacy_v1_report";
    }
  | {
      state: "not_applicable";
      reason:
        | "span_kind_not_model_backed"
        | "span_kind_has_no_latency"
        | "span_kind_has_no_token_usage"
        | "claude_private_lookup_not_supported"
        | "cursor_private_lookup_not_supported"
        | "codex_span_not_notify_derived"
        | "agent_private_lookup_not_supported";
    }
  | {
      state: "private_lookup";
      reason: "local_opt_in_lookup_required";
    }
  | {
      state: "withheld";
      reason: "withheld_by_privacy_policy";
    };
export type DashboardAvailabilityFieldV1 =
  | "repository"
  | "session"
  | "turn"
  | "model"
  | "tokens"
  | "latency"
  | "source_location"
  | "request_content"
  | "response_content";
export type DashboardSpansResponseV1 = ResponseBase & {
  kind: "spans";
  /**
   * @maxItems 200
   */
  rows: DashboardSpanRowV1[];
  pagination: DashboardPaginationV1;
};
export type DashboardSummaryResponseV1 = ResponseBase & {
  kind: "summary";
  pagination: DashboardPaginationV1;
} & SummaryResponse;
export type SummaryResponse =
  | {
      work?: {
        state?: "pending";
      };
      pagination?: {
        nextCursor?: Cursor;
      };
    }
  | {
      work?: {
        state?: "complete";
      };
      pagination?: {
        nextCursor?: null;
      };
    };
export type DashboardSpanResponseV1 = ResponseBase & {
  kind: "span";
  detail: Span & {
    traceId?: ProjectedId;
    spanId?: ProjectedId;
    parentSpanId?: ProjectedId | null;
  };
};
export type Span = {
  schemaVersion: string;
  traceId: string;
  spanId: string;
  parentSpanId: string | null;
  kind: string;
  name: string;
  status: string;
  startTimeUnixMs: number;
  endTimeUnixMs: number | null;
  repo: string;
  agent: Agent;
  availability: Availability;
  sessionId?: string;
  turnId?: string;
  toolName?: string;
  attributes: Attributes;
  metrics: Metrics;
  estimatedCost?: number;
  cost: Cost;
};
export type FieldAvailability = {
  state: "available" | "source_unavailable" | "withheld" | "not_applicable" | "private_lookup";
  reason:
    | "reported_by_adapter"
    | "derived_from_trace_context"
    | "legacy_v1_report"
    | "source_not_provided"
    | "not_evaluated"
    | "partial_token_metrics"
    | "historical_codex_source_not_lookup_eligible"
    | "codex_notify_turn_correlation_unavailable"
    | "ambiguous_trace_repository"
    | "span_kind_not_model_backed"
    | "span_kind_has_no_latency"
    | "span_kind_has_no_token_usage"
    | "claude_private_lookup_not_supported"
    | "cursor_private_lookup_not_supported"
    | "codex_span_not_notify_derived"
    | "agent_private_lookup_not_supported"
    | "local_opt_in_lookup_required"
    | "withheld_by_privacy_policy";
} & (
  | {
      state?: "available";
      reason?: "reported_by_adapter" | "derived_from_trace_context" | "legacy_v1_report";
    }
  | {
      state?: "source_unavailable";
      reason?:
        | "source_not_provided"
        | "not_evaluated"
        | "partial_token_metrics"
        | "historical_codex_source_not_lookup_eligible"
        | "codex_notify_turn_correlation_unavailable"
        | "ambiguous_trace_repository"
        | "legacy_v1_report";
    }
  | {
      state?: "not_applicable";
      reason?:
        | "span_kind_not_model_backed"
        | "span_kind_has_no_latency"
        | "span_kind_has_no_token_usage"
        | "claude_private_lookup_not_supported"
        | "cursor_private_lookup_not_supported"
        | "codex_span_not_notify_derived"
        | "agent_private_lookup_not_supported";
    }
  | {
      state?: "private_lookup";
      reason?: "local_opt_in_lookup_required";
    }
  | {
      state?: "withheld";
      reason?: "withheld_by_privacy_policy";
    }
);
export type Strings = string[];
export type DashboardFacetsResponseV1 = ResponseBase & {
  kind: "facets";
  /**
   * @maxItems 500
   */
  rows: DashboardFacetRowV1[];
  pagination: DashboardPaginationV1;
};
export type DashboardFacetDimensionV1 = "repo" | "session" | "agent" | "model";
export type DashboardRequestKindV1 = "bootstrap" | "traces" | "spans" | "summary" | "span" | "facets";
export type DashboardStatusReasonV1 =
  "building" | "refresh_pending" | "snapshot_expired" | "busy" | "capacity" | "invalid_query";

export interface RequestBase {
  schemaVersion: "agent_observability.dashboard_query.v1";
  filters?: DashboardFiltersV1;
}
export interface DashboardFiltersV1 {
  repo?: FilterValues;
  session?: FilterValues;
  agent?: FilterValues;
  model?: FilterValues;
  text?: string;
}
export interface ResponseBase {
  schemaVersion: "agent_observability.dashboard_query.v1";
  snapshot: DashboardSnapshotV1;
  scope: DashboardQueryScopeV1;
  work: DashboardWorkV1;
}
export interface DashboardSnapshotV1 {
  id: SnapshotId;
  generation: DecimalU64;
  visibilityEpoch: DecimalU64;
  generatedAt: string;
  state: DashboardSnapshotStateV1;
}
export interface DashboardQueryScopeV1 {
  filters: DashboardFiltersV1;
  selectedTraceId: ProjectedId | null;
  coldExcluded: true;
}
export interface DashboardPendingWorkV1 {
  state: "pending";
}
export interface DashboardCompleteWorkV1 {
  state: "complete";
  kpis: DashboardKpisV1;
}
export interface Kpis {
  sessions: SafeCount;
  turns: SafeCount;
  llm: SafeCount;
  tools: SafeCount;
  errors: SafeCount;
  inputTokens: NullableSafeCount;
  outputTokens: NullableSafeCount;
  totalTokens: NullableSafeCount;
  tokenStatus: DashboardTokenStatusV1;
  estimatedCost: number | null;
  costStatus: DashboardCostStatusV1;
  currency: string | null;
}
export interface DashboardQueryLimitsV1 {
  traceRows: 100;
  spanRows: 200;
  timelineRows: 120;
  facetValues: 500;
  requestBytes: 8192;
  responseBytes: 1048576;
}
export interface DashboardTraceRowV1 {
  traceId: ProjectedId;
  repo: string;
  spanCount: SafeCount;
  errorCount: SafeCount;
  startTimeUnixMs: number;
  endTimeUnixMs: number | null;
  availabilityReasons: AvailabilityReasons;
}
export interface AvailabilityReason1 {
  field: DashboardAvailabilityFieldV1;
  state: "available" | "source_unavailable" | "withheld" | "not_applicable" | "private_lookup";
  reason:
    | "reported_by_adapter"
    | "derived_from_trace_context"
    | "legacy_v1_report"
    | "source_not_provided"
    | "not_evaluated"
    | "partial_token_metrics"
    | "historical_codex_source_not_lookup_eligible"
    | "codex_notify_turn_correlation_unavailable"
    | "ambiguous_trace_repository"
    | "span_kind_not_model_backed"
    | "span_kind_has_no_latency"
    | "span_kind_has_no_token_usage"
    | "claude_private_lookup_not_supported"
    | "cursor_private_lookup_not_supported"
    | "codex_span_not_notify_derived"
    | "agent_private_lookup_not_supported"
    | "local_opt_in_lookup_required"
    | "withheld_by_privacy_policy";
}
export interface DashboardPaginationV1 {
  nextCursor: NullableCursor;
  total?: SafeCount;
}
/**
 * Compact projected list row. It intentionally omits attributes, metrics, source content, response content and private raw detail.
 */
export interface DashboardSpanRowV1 {
  traceId: ProjectedId;
  spanId: ProjectedId;
  parentSpanId: ProjectedId | null;
  kind: string;
  name: string;
  status: string;
  startTimeUnixMs: number;
  endTimeUnixMs: number | null;
  repo: string;
  agent?: string;
  model?: string;
  sessionId?: ProjectedId;
  turnId?: ProjectedId;
  toolName?: string;
  availabilityReasons: AvailabilityReasons;
}
export interface Agent {
  name?: string;
  model?: string;
  version?: string;
}
export interface Availability {
  repository: FieldAvailability;
  turn: FieldAvailability;
  model: FieldAvailability;
  tokens: FieldAvailability;
  latency: FieldAvailability;
  sourceLocation: FieldAvailability;
  requestContent: FieldAvailability;
  responseContent: FieldAvailability;
}
export interface Attributes {
  source?: string | number | boolean;
  event_type?: string | number | boolean;
  envelope_type?: string | number | boolean;
  session_id?: string | number | boolean;
  turn_id?: string | number | boolean;
  request_id?: string | number | boolean;
  call_id?: string | number | boolean;
  tool_name?: string | number | boolean;
  phase?: string | number | boolean;
  exit_code?: string | number | boolean;
  sandbox?: string | number | boolean;
  approval?: string | number | boolean;
}
export interface Metrics {
  inputTokens?: number;
  outputTokens?: number;
  cachedInputTokens?: number;
  cacheCreationInputTokens?: number;
  reasoningOutputTokens?: number;
  totalTokens?: number;
  latencyMs?: number;
  durationMs?: number;
  totalInputTokens?: number;
  totalOutputTokens?: number;
  totalCachedInputTokens?: number;
  totalReasoningOutputTokens?: number;
  totalAccumulatedTokens?: number;
  contextWindowTokens?: number;
}
export interface Cost {
  status: "estimated" | "incomplete" | "unknown";
  reason?: string;
  estimated_cost?: number;
  currency?: string;
  model?: string;
  rate_table: {
    version?: string;
    unit?: string;
  };
  cost: CostDetail;
}
export interface CostDetail {
  assumption: string;
  incomplete_count?: number;
  unknown_count?: number;
  missing?: Strings;
  semantic_errors?: Strings;
  components?: {
    [k: string]: CostComponent;
  };
}
export interface CostComponent {
  tokens: number;
  rate_per_1m: number;
  estimated_cost: number;
}
export interface DashboardFacetRowV1 {
  dimension: DashboardFacetDimensionV1;
  value: string;
}
export interface DashboardStatusResponseV1 {
  schemaVersion: "agent_observability.dashboard_query.v1";
  kind: "status";
  requestKind: DashboardRequestKindV1;
  reason: DashboardStatusReasonV1;
  snapshot?: DashboardSnapshotV1;
}
