/* Generated from contracts/report-dto-v2.schema.json. Do not edit. */

export type Strings = string[];
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
export type Scalar = string | number | boolean;

export interface AgentObservabilityReportV2 {
  schemaVersion: "agent_observability.report.v2";
  generatedAt: string;
  title: string;
  summary: Summary;
  cost: Cost;
  filters: Filters;
  traces: Trace[];
  spans: Span[];
}
export interface Summary {
  generatedSpans: number;
  sessions: number;
  turns: number;
  llmRequests: number;
  toolExecutions: number;
  errors: number;
  inputTokens: number;
  outputTokens: number;
  cachedInputTokens: number;
  cacheCreationInputTokens: number;
  reasoningOutputTokens: number;
  latencyMs: number;
  durationMs: number;
  estimatedCost: number;
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
export interface Filters {
  repos: Strings;
  sessions: Strings;
  turns: Strings;
  agents?: Strings;
  models?: Strings;
}
export interface Trace {
  traceId: string;
  repo: string;
  spans: number;
  errors: number;
  inputTokens: number;
  outputTokens: number;
  estimatedCost: number;
  startTimeUnixMs: number;
  endTimeUnixMs: number | null;
  sessions: Strings;
  turns: Strings;
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
export interface FieldAvailability {
  state: "available" | "source_unavailable" | "withheld" | "not_applicable" | "private_lookup";
  reason: string;
}
export interface Attributes {
  source?: Scalar;
  event_type?: Scalar;
  envelope_type?: Scalar;
  session_id?: Scalar;
  turn_id?: Scalar;
  request_id?: Scalar;
  call_id?: Scalar;
  tool_name?: Scalar;
  phase?: Scalar;
  exit_code?: Scalar;
  sandbox?: Scalar;
  approval?: Scalar;
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
