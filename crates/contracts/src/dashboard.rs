use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{AvailabilityStateV2, FieldAvailabilityV2, ReportSpanV2, valid_availability_reason};

pub const DASHBOARD_QUERY_VERSION: &str = "agent_observability.dashboard_query.v1";
pub const DASHBOARD_QUERY_SCHEMA: &str =
    include_str!("../../../contracts/dashboard-query-v1.schema.json");
pub const DASHBOARD_QUERY_FIXTURE: &str =
    include_str!("../../../contracts/dashboard-query-v1.fixture.json");
pub const DASHBOARD_QUERY_PARITY: &str =
    include_str!("../../../contracts/dashboard-query-v1.parity.json");
pub const DASHBOARD_REQUEST_MAX_SERIALIZED_UTF8_BYTES: usize = 8 * 1024;
pub const DASHBOARD_RESPONSE_MAX_SERIALIZED_UTF8_BYTES: usize = 1024 * 1024;
pub const DASHBOARD_TRACE_PAGE_MAX_ROWS: usize = 100;
pub const DASHBOARD_SPAN_PAGE_MAX_ROWS: usize = 200;
pub const DASHBOARD_TIMELINE_MAX_ROWS: usize = 120;
pub const DASHBOARD_FACET_MAX_VALUES: usize = 500;
pub const DASHBOARD_FILTER_MAX_VALUES: usize = 16;
pub const DASHBOARD_FILTER_VALUE_MAX_CHARS: usize = 256;
pub const DASHBOARD_TEXT_FILTER_MAX_CHARS: usize = 512;
pub const DASHBOARD_CURSOR_MAX_CHARS: usize = 2048;
pub const DASHBOARD_GENERATED_AT_MAX_CHARS: usize = 64;
pub const DASHBOARD_SAFE_INTEGER_MAX: u64 = 9_007_199_254_740_991;
pub const DASHBOARD_ESTIMATED_COST_MAX: f64 = 1_000_000_000_000.0;

pub const DASHBOARD_REQUEST_KINDS: &[&str] =
    &["bootstrap", "traces", "spans", "summary", "span", "facets"];
pub const DASHBOARD_STATUS_REASONS: &[&str] = &[
    "building",
    "refresh_pending",
    "snapshot_expired",
    "busy",
    "capacity",
    "invalid_query",
];
pub const DASHBOARD_SNAPSHOT_STATES: &[&str] = &["current", "stale"];
pub const DASHBOARD_TOKEN_STATUSES: &[&str] = &["complete", "incomplete", "unavailable"];
pub const DASHBOARD_COST_STATUSES: &[&str] = &["estimated", "incomplete", "unknown"];
pub const DASHBOARD_FACET_DIMENSIONS: &[&str] = &["repo", "session", "agent", "model"];
pub const DASHBOARD_AVAILABILITY_FIELDS: &[&str] = &[
    "repository",
    "session",
    "turn",
    "model",
    "tokens",
    "latency",
    "source_location",
    "request_content",
    "response_content",
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DashboardRequestKindV1 {
    Bootstrap,
    Traces,
    Spans,
    Summary,
    Span,
    Facets,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DashboardStatusReasonV1 {
    Building,
    RefreshPending,
    SnapshotExpired,
    Busy,
    Capacity,
    InvalidQuery,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DashboardSnapshotStateV1 {
    Current,
    Stale,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DashboardTokenStatusV1 {
    Complete,
    Incomplete,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DashboardCostStatusV1 {
    Estimated,
    Incomplete,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DashboardFacetDimensionV1 {
    Repo,
    Session,
    Agent,
    Model,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DashboardAvailabilityFieldV1 {
    Repository,
    Session,
    Turn,
    Model,
    Tokens,
    Latency,
    SourceLocation,
    RequestContent,
    ResponseContent,
}

macro_rules! dashboard_kind_marker {
    ($name:ident, $variant:ident, $wire:literal) => {
        #[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
        pub enum $name {
            #[serde(rename = $wire)]
            $variant,
        }
    };
}

dashboard_kind_marker!(DashboardBootstrapKindV1, Bootstrap, "bootstrap");
dashboard_kind_marker!(DashboardTracesKindV1, Traces, "traces");
dashboard_kind_marker!(DashboardSpansKindV1, Spans, "spans");
dashboard_kind_marker!(DashboardSummaryKindV1, Summary, "summary");
dashboard_kind_marker!(DashboardSpanKindV1, Span, "span");
dashboard_kind_marker!(DashboardFacetsKindV1, Facets, "facets");
dashboard_kind_marker!(DashboardStatusKindV1, Status, "status");
dashboard_kind_marker!(DashboardPendingStateV1, Pending, "pending");
dashboard_kind_marker!(DashboardCompleteStateV1, Complete, "complete");

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardFiltersV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repo: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub session: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model: Vec<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub text: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardBootstrapRequestV1 {
    pub schema_version: String,
    pub kind: DashboardBootstrapKindV1,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub filters: Option<DashboardFiltersV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardTracesRequestV1 {
    pub schema_version: String,
    pub kind: DashboardTracesKindV1,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub filters: Option<DashboardFiltersV1>,
    pub snapshot_id: String,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardSpansRequestV1 {
    pub schema_version: String,
    pub kind: DashboardSpansKindV1,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub filters: Option<DashboardFiltersV1>,
    pub snapshot_id: String,
    pub trace_id: String,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardSummaryRequestV1 {
    pub schema_version: String,
    pub kind: DashboardSummaryKindV1,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub filters: Option<DashboardFiltersV1>,
    pub snapshot_id: String,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardSpanRequestV1 {
    pub schema_version: String,
    pub kind: DashboardSpanKindV1,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub filters: Option<DashboardFiltersV1>,
    pub snapshot_id: String,
    pub trace_id: String,
    pub span_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardFacetsRequestV1 {
    pub schema_version: String,
    pub kind: DashboardFacetsKindV1,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub filters: Option<DashboardFiltersV1>,
    pub snapshot_id: String,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum DashboardQueryRequestV1 {
    Bootstrap(DashboardBootstrapRequestV1),
    Traces(DashboardTracesRequestV1),
    Spans(DashboardSpansRequestV1),
    Summary(DashboardSummaryRequestV1),
    Span(DashboardSpanRequestV1),
    Facets(DashboardFacetsRequestV1),
}

impl DashboardQueryRequestV1 {
    /// Validates semantic limits in addition to the closed serde shape.
    ///
    /// Cursors remain opaque: validation checks only their wire bound, never their contents.
    /// Server-side lease validation must bind them to the normalized query and snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`DashboardContractError`] when the version, identifiers, filters, cursor or
    /// serialized UTF-8 request size violates the versioned contract.
    pub fn validate(&self) -> Result<(), DashboardContractError> {
        match self {
            Self::Bootstrap(request) => {
                validate_request_common(&request.schema_version, request.filters.as_ref())?;
            }
            Self::Traces(request) => {
                validate_request_common(&request.schema_version, request.filters.as_ref())?;
                validate_snapshot_id(&request.snapshot_id)?;
                validate_optional_cursor(request.cursor.as_deref())?;
            }
            Self::Spans(request) => {
                validate_request_common(&request.schema_version, request.filters.as_ref())?;
                validate_snapshot_id(&request.snapshot_id)?;
                validate_projected_id(&request.trace_id)?;
                validate_optional_cursor(request.cursor.as_deref())?;
            }
            Self::Summary(request) => {
                validate_request_common(&request.schema_version, request.filters.as_ref())?;
                validate_snapshot_id(&request.snapshot_id)?;
                validate_optional_cursor(request.cursor.as_deref())?;
            }
            Self::Span(request) => {
                validate_request_common(&request.schema_version, request.filters.as_ref())?;
                validate_snapshot_id(&request.snapshot_id)?;
                validate_projected_id(&request.trace_id)?;
                validate_projected_id(&request.span_id)?;
            }
            Self::Facets(request) => {
                validate_request_common(&request.schema_version, request.filters.as_ref())?;
                validate_snapshot_id(&request.snapshot_id)?;
                validate_optional_cursor(request.cursor.as_deref())?;
            }
        }
        validate_wire_bytes(self, DASHBOARD_REQUEST_MAX_SERIALIZED_UTF8_BYTES, true)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardSnapshotV1 {
    pub id: String,
    pub generation: String,
    pub visibility_epoch: String,
    pub generated_at: String,
    pub state: DashboardSnapshotStateV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardQueryScopeV1 {
    pub filters: DashboardFiltersV1,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub selected_trace_id: Option<String>,
    pub cold_excluded: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardPendingWorkV1 {
    pub state: DashboardPendingStateV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardCompleteWorkV1 {
    pub state: DashboardCompleteStateV1,
    pub kpis: DashboardKpisV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum DashboardWorkV1 {
    Pending(DashboardPendingWorkV1),
    Complete(DashboardCompleteWorkV1),
}

impl DashboardWorkV1 {
    #[must_use]
    pub const fn pending() -> Self {
        Self::Pending(DashboardPendingWorkV1 {
            state: DashboardPendingStateV1::Pending,
        })
    }

    #[must_use]
    pub const fn complete(kpis: DashboardKpisV1) -> Self {
        Self::Complete(DashboardCompleteWorkV1 {
            state: DashboardCompleteStateV1::Complete,
            kpis,
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardKpisV1 {
    pub sessions: u64,
    pub turns: u64,
    pub llm: u64,
    pub tools: u64,
    pub errors: u64,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub input_tokens: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub output_tokens: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub total_tokens: Option<u64>,
    pub token_status: DashboardTokenStatusV1,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub estimated_cost: Option<f64>,
    pub cost_status: DashboardCostStatusV1,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub currency: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardPaginationV1 {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub next_cursor: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub total: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardAvailabilityReasonV1 {
    pub field: DashboardAvailabilityFieldV1,
    #[serde(flatten)]
    pub availability: FieldAvailabilityV2,
}

impl<'de> Deserialize<'de> for DashboardAvailabilityReasonV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            field: DashboardAvailabilityFieldV1,
            state: AvailabilityStateV2,
            reason: String,
        }

        let wire = Wire::deserialize(deserializer)?;
        Ok(Self {
            field: wire.field,
            availability: FieldAvailabilityV2 {
                state: wire.state,
                reason: wire.reason,
            },
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardTraceRowV1 {
    pub trace_id: String,
    pub repo: String,
    pub span_count: u64,
    pub error_count: u64,
    pub start_time_unix_ms: f64,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub end_time_unix_ms: Option<f64>,
    pub availability_reasons: Vec<DashboardAvailabilityReasonV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardSpanRowV1 {
    pub trace_id: String,
    pub span_id: String,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub parent_span_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub status: String,
    pub start_time_unix_ms: f64,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub end_time_unix_ms: Option<f64>,
    pub repo: String,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub agent: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub model: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub session_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub turn_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_name: Option<String>,
    pub availability_reasons: Vec<DashboardAvailabilityReasonV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardFacetRowV1 {
    pub dimension: DashboardFacetDimensionV1,
    pub value: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardQueryLimitsV1 {
    pub trace_rows: u16,
    pub span_rows: u16,
    pub timeline_rows: u16,
    pub facet_values: u16,
    pub request_bytes: u32,
    pub response_bytes: u32,
}

impl Default for DashboardQueryLimitsV1 {
    fn default() -> Self {
        Self {
            trace_rows: u16::try_from(DASHBOARD_TRACE_PAGE_MAX_ROWS).expect("trace limit fits u16"),
            span_rows: u16::try_from(DASHBOARD_SPAN_PAGE_MAX_ROWS).expect("span limit fits u16"),
            timeline_rows: u16::try_from(DASHBOARD_TIMELINE_MAX_ROWS)
                .expect("timeline limit fits u16"),
            facet_values: u16::try_from(DASHBOARD_FACET_MAX_VALUES).expect("facet limit fits u16"),
            request_bytes: u32::try_from(DASHBOARD_REQUEST_MAX_SERIALIZED_UTF8_BYTES)
                .expect("request byte limit fits u32"),
            response_bytes: u32::try_from(DASHBOARD_RESPONSE_MAX_SERIALIZED_UTF8_BYTES)
                .expect("response byte limit fits u32"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardBootstrapResponseV1 {
    pub schema_version: String,
    pub kind: DashboardBootstrapKindV1,
    pub snapshot: DashboardSnapshotV1,
    pub scope: DashboardQueryScopeV1,
    pub work: DashboardWorkV1,
    pub limits: DashboardQueryLimitsV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardTracesResponseV1 {
    pub schema_version: String,
    pub kind: DashboardTracesKindV1,
    pub snapshot: DashboardSnapshotV1,
    pub scope: DashboardQueryScopeV1,
    pub work: DashboardWorkV1,
    pub rows: Vec<DashboardTraceRowV1>,
    pub pagination: DashboardPaginationV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardSpansResponseV1 {
    pub schema_version: String,
    pub kind: DashboardSpansKindV1,
    pub snapshot: DashboardSnapshotV1,
    pub scope: DashboardQueryScopeV1,
    pub work: DashboardWorkV1,
    pub rows: Vec<DashboardSpanRowV1>,
    pub pagination: DashboardPaginationV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardSummaryResponseV1 {
    pub schema_version: String,
    pub kind: DashboardSummaryKindV1,
    pub snapshot: DashboardSnapshotV1,
    pub scope: DashboardQueryScopeV1,
    pub work: DashboardWorkV1,
    pub pagination: DashboardPaginationV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardSpanResponseV1 {
    pub schema_version: String,
    pub kind: DashboardSpanKindV1,
    pub snapshot: DashboardSnapshotV1,
    pub scope: DashboardQueryScopeV1,
    pub work: DashboardWorkV1,
    pub detail: Box<ReportSpanV2>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardFacetsResponseV1 {
    pub schema_version: String,
    pub kind: DashboardFacetsKindV1,
    pub snapshot: DashboardSnapshotV1,
    pub scope: DashboardQueryScopeV1,
    pub work: DashboardWorkV1,
    pub rows: Vec<DashboardFacetRowV1>,
    pub pagination: DashboardPaginationV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardStatusResponseV1 {
    pub schema_version: String,
    pub kind: DashboardStatusKindV1,
    pub request_kind: DashboardRequestKindV1,
    pub reason: DashboardStatusReasonV1,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub snapshot: Option<DashboardSnapshotV1>,
}

fn deserialize_required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

fn deserialize_optional_nonnull<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum DashboardQueryResponseV1 {
    Bootstrap(DashboardBootstrapResponseV1),
    Traces(DashboardTracesResponseV1),
    Spans(DashboardSpansResponseV1),
    Summary(DashboardSummaryResponseV1),
    Span(DashboardSpanResponseV1),
    Facets(DashboardFacetsResponseV1),
    Status(DashboardStatusResponseV1),
}

impl DashboardQueryResponseV1 {
    /// Validates the closed response, semantic limits and independent 1 MiB wire cap.
    ///
    /// # Errors
    ///
    /// Returns [`DashboardContractError`] when snapshot, scope, work, pagination, row, detail or
    /// serialized UTF-8 response constraints are violated.
    pub fn validate(&self) -> Result<(), DashboardContractError> {
        match self {
            Self::Bootstrap(response) => {
                validate_response_common(
                    &response.schema_version,
                    &response.snapshot,
                    &response.scope,
                    &response.work,
                )?;
                if response.limits != DashboardQueryLimitsV1::default() {
                    return Err(DashboardContractError::InvalidLimits);
                }
            }
            Self::Traces(response) => {
                validate_response_common(
                    &response.schema_version,
                    &response.snapshot,
                    &response.scope,
                    &response.work,
                )?;
                validate_pagination(&response.pagination)?;
                if response.rows.len() > DASHBOARD_TRACE_PAGE_MAX_ROWS {
                    return Err(DashboardContractError::TooManyRows);
                }
                for row in &response.rows {
                    validate_trace_row(row)?;
                }
            }
            Self::Spans(response) => {
                validate_response_common(
                    &response.schema_version,
                    &response.snapshot,
                    &response.scope,
                    &response.work,
                )?;
                validate_pagination(&response.pagination)?;
                if response.rows.len() > DASHBOARD_SPAN_PAGE_MAX_ROWS {
                    return Err(DashboardContractError::TooManyRows);
                }
                for row in &response.rows {
                    validate_span_row(row)?;
                }
            }
            Self::Summary(response) => {
                validate_response_common(
                    &response.schema_version,
                    &response.snapshot,
                    &response.scope,
                    &response.work,
                )?;
                validate_pagination(&response.pagination)?;
                match (&response.work, &response.pagination.next_cursor) {
                    (DashboardWorkV1::Pending(_), Some(_))
                    | (DashboardWorkV1::Complete(_), None) => {}
                    _ => return Err(DashboardContractError::InvalidPagination),
                }
            }
            Self::Span(response) => {
                validate_response_common(
                    &response.schema_version,
                    &response.snapshot,
                    &response.scope,
                    &response.work,
                )?;
                validate_projected_id(&response.detail.trace_id)?;
                validate_projected_id(&response.detail.span_id)?;
                if let Some(parent_span_id) = &response.detail.parent_span_id {
                    validate_projected_id(parent_span_id)?;
                }
                response
                    .detail
                    .validate()
                    .map_err(|_| DashboardContractError::InvalidSpanDetail)?;
            }
            Self::Facets(response) => {
                validate_response_common(
                    &response.schema_version,
                    &response.snapshot,
                    &response.scope,
                    &response.work,
                )?;
                validate_pagination(&response.pagination)?;
                if response.rows.len() > DASHBOARD_FACET_MAX_VALUES {
                    return Err(DashboardContractError::TooManyRows);
                }
                for row in &response.rows {
                    validate_nonempty_bounded(&row.value, DASHBOARD_FILTER_VALUE_MAX_CHARS)?;
                }
            }
            Self::Status(response) => {
                validate_version(&response.schema_version)?;
                if let Some(snapshot) = &response.snapshot {
                    validate_snapshot(snapshot)?;
                }
            }
        }
        validate_wire_bytes(self, DASHBOARD_RESPONSE_MAX_SERIALIZED_UTF8_BYTES, false)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DashboardContractError {
    InvalidVersion,
    RequestTooLarge,
    ResponseTooLarge,
    TooManyFilterValues,
    DuplicateFilterValue,
    EmptyString,
    StringTooLong,
    InvalidProjectedId,
    InvalidCursor,
    InvalidDecimalU64,
    InvalidSnapshotId,
    InvalidNumber,
    InvalidScope,
    InvalidKpis,
    InvalidPagination,
    InvalidLimits,
    InvalidSpanDetail,
    InvalidAvailabilityReason,
    DuplicateAvailabilityField,
    TooManyRows,
}

impl fmt::Display for DashboardContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidVersion => "invalid dashboard query schema version",
            Self::RequestTooLarge => "dashboard query request exceeds the UTF-8 wire limit",
            Self::ResponseTooLarge => "dashboard query response exceeds the UTF-8 wire limit",
            Self::TooManyFilterValues => "dashboard filter contains too many values",
            Self::DuplicateFilterValue => "dashboard filter contains duplicate values",
            Self::EmptyString => "dashboard contract string is empty",
            Self::StringTooLong => "dashboard contract string exceeds its bound",
            Self::InvalidProjectedId => "dashboard projected ID is invalid",
            Self::InvalidCursor => "dashboard cursor is invalid",
            Self::InvalidDecimalU64 => "dashboard decimal counter is invalid",
            Self::InvalidSnapshotId => "dashboard snapshot ID is invalid",
            Self::InvalidNumber => "dashboard numeric value is invalid",
            Self::InvalidScope => "dashboard query scope is invalid",
            Self::InvalidKpis => "dashboard KPI state is inconsistent",
            Self::InvalidPagination => "dashboard pagination state is invalid",
            Self::InvalidLimits => "dashboard advertised limits drifted",
            Self::InvalidSpanDetail => "dashboard span detail is invalid",
            Self::InvalidAvailabilityReason => {
                "dashboard availability reason is not a canonical unavailable state"
            }
            Self::DuplicateAvailabilityField => {
                "dashboard availability reasons contain a duplicate field"
            }
            Self::TooManyRows => "dashboard response contains too many rows",
        })
    }
}

impl std::error::Error for DashboardContractError {}

fn validate_request_common(
    schema_version: &str,
    filters: Option<&DashboardFiltersV1>,
) -> Result<(), DashboardContractError> {
    validate_version(schema_version)?;
    if let Some(filters) = filters {
        validate_filters(filters)?;
    }
    Ok(())
}

fn validate_version(schema_version: &str) -> Result<(), DashboardContractError> {
    if schema_version == DASHBOARD_QUERY_VERSION {
        Ok(())
    } else {
        Err(DashboardContractError::InvalidVersion)
    }
}

fn validate_filters(filters: &DashboardFiltersV1) -> Result<(), DashboardContractError> {
    for values in [
        &filters.repo,
        &filters.session,
        &filters.agent,
        &filters.model,
    ] {
        if values.len() > DASHBOARD_FILTER_MAX_VALUES {
            return Err(DashboardContractError::TooManyFilterValues);
        }
        for (index, value) in values.iter().enumerate() {
            validate_nonempty_bounded(value, DASHBOARD_FILTER_VALUE_MAX_CHARS)?;
            if values[..index].contains(value) {
                return Err(DashboardContractError::DuplicateFilterValue);
            }
        }
    }
    if let Some(text) = &filters.text {
        validate_nonempty_bounded(text, DASHBOARD_TEXT_FILTER_MAX_CHARS)?;
    }
    Ok(())
}

fn validate_optional_cursor(cursor: Option<&str>) -> Result<(), DashboardContractError> {
    let Some(cursor) = cursor else {
        return Ok(());
    };
    let chars = cursor.chars().count();
    if chars == 0 || chars > DASHBOARD_CURSOR_MAX_CHARS {
        return Err(DashboardContractError::InvalidCursor);
    }
    Ok(())
}

fn validate_response_common(
    schema_version: &str,
    snapshot: &DashboardSnapshotV1,
    scope: &DashboardQueryScopeV1,
    work: &DashboardWorkV1,
) -> Result<(), DashboardContractError> {
    validate_version(schema_version)?;
    validate_snapshot(snapshot)?;
    validate_scope(scope)?;
    validate_work(work)
}

fn validate_snapshot(snapshot: &DashboardSnapshotV1) -> Result<(), DashboardContractError> {
    validate_snapshot_id(&snapshot.id)?;
    validate_decimal_u64(&snapshot.generation)?;
    validate_decimal_u64(&snapshot.visibility_epoch)?;
    validate_nonempty_bounded(&snapshot.generated_at, DASHBOARD_GENERATED_AT_MAX_CHARS)
}

fn validate_snapshot_id(value: &str) -> Result<(), DashboardContractError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(DashboardContractError::InvalidSnapshotId)
    }
}

fn validate_decimal_u64(value: &str) -> Result<u64, DashboardContractError> {
    if value == "0" {
        return Ok(0);
    }
    if value.starts_with('0')
        || value.len() > 20
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(DashboardContractError::InvalidDecimalU64);
    }
    value
        .parse::<u64>()
        .map_err(|_| DashboardContractError::InvalidDecimalU64)
}

fn validate_scope(scope: &DashboardQueryScopeV1) -> Result<(), DashboardContractError> {
    validate_filters(&scope.filters)?;
    if let Some(trace_id) = &scope.selected_trace_id {
        validate_projected_id(trace_id)?;
    }
    if !scope.cold_excluded {
        return Err(DashboardContractError::InvalidScope);
    }
    Ok(())
}

fn validate_work(work: &DashboardWorkV1) -> Result<(), DashboardContractError> {
    if let DashboardWorkV1::Complete(complete) = work {
        validate_kpis(&complete.kpis)?;
    }
    Ok(())
}

fn validate_kpis(kpis: &DashboardKpisV1) -> Result<(), DashboardContractError> {
    for value in [kpis.sessions, kpis.turns, kpis.llm, kpis.tools, kpis.errors] {
        validate_safe_count(value)?;
    }
    for value in [kpis.input_tokens, kpis.output_tokens, kpis.total_tokens]
        .into_iter()
        .flatten()
    {
        validate_safe_count(value)?;
    }
    match kpis.token_status {
        DashboardTokenStatusV1::Complete => {
            if kpis.total_tokens.is_none() {
                return Err(DashboardContractError::InvalidKpis);
            }
        }
        DashboardTokenStatusV1::Incomplete => {
            if kpis.total_tokens.is_some() {
                return Err(DashboardContractError::InvalidKpis);
            }
        }
        DashboardTokenStatusV1::Unavailable => {
            if kpis.input_tokens.is_some()
                || kpis.output_tokens.is_some()
                || kpis.total_tokens.is_some()
            {
                return Err(DashboardContractError::InvalidKpis);
            }
        }
    }
    match kpis.cost_status {
        DashboardCostStatusV1::Estimated => {
            validate_estimated_cost(kpis.estimated_cost)?;
            validate_optional_required_string(kpis.currency.as_deref(), 16)?;
        }
        DashboardCostStatusV1::Incomplete => {
            if kpis.estimated_cost.is_some() {
                validate_estimated_cost(kpis.estimated_cost)?;
            }
            if let Some(currency) = &kpis.currency {
                validate_nonempty_bounded(currency, 16)?;
            }
        }
        DashboardCostStatusV1::Unknown => {
            if kpis.estimated_cost.is_some() || kpis.currency.is_some() {
                return Err(DashboardContractError::InvalidKpis);
            }
        }
    }
    Ok(())
}

fn validate_estimated_cost(value: Option<f64>) -> Result<(), DashboardContractError> {
    let Some(value) = value else {
        return Err(DashboardContractError::InvalidKpis);
    };
    if !value.is_finite() || !(0.0..=DASHBOARD_ESTIMATED_COST_MAX).contains(&value) {
        return Err(DashboardContractError::InvalidNumber);
    }
    Ok(())
}

fn validate_optional_required_string(
    value: Option<&str>,
    max_chars: usize,
) -> Result<(), DashboardContractError> {
    let Some(value) = value else {
        return Err(DashboardContractError::InvalidKpis);
    };
    validate_nonempty_bounded(value, max_chars)
}

fn validate_safe_count(value: u64) -> Result<(), DashboardContractError> {
    if value <= DASHBOARD_SAFE_INTEGER_MAX {
        Ok(())
    } else {
        Err(DashboardContractError::InvalidNumber)
    }
}

fn validate_pagination(pagination: &DashboardPaginationV1) -> Result<(), DashboardContractError> {
    validate_optional_cursor(pagination.next_cursor.as_deref())?;
    if let Some(total) = pagination.total {
        validate_safe_count(total).map_err(|_| DashboardContractError::InvalidPagination)?;
    }
    Ok(())
}

fn validate_trace_row(row: &DashboardTraceRowV1) -> Result<(), DashboardContractError> {
    validate_projected_id(&row.trace_id)?;
    validate_nonempty_bounded(&row.repo, DASHBOARD_FILTER_VALUE_MAX_CHARS)?;
    validate_safe_count(row.span_count)?;
    validate_safe_count(row.error_count)?;
    validate_time(row.start_time_unix_ms)?;
    if let Some(end) = row.end_time_unix_ms {
        validate_time(end)?;
    }
    validate_availability_reasons(&row.availability_reasons)
}

fn validate_span_row(row: &DashboardSpanRowV1) -> Result<(), DashboardContractError> {
    validate_projected_id(&row.trace_id)?;
    validate_projected_id(&row.span_id)?;
    if let Some(parent_span_id) = &row.parent_span_id {
        validate_projected_id(parent_span_id)?;
    }
    validate_nonempty_bounded(&row.kind, 64)?;
    validate_nonempty_bounded(&row.name, DASHBOARD_FILTER_VALUE_MAX_CHARS)?;
    validate_nonempty_bounded(&row.status, 64)?;
    validate_time(row.start_time_unix_ms)?;
    if let Some(end) = row.end_time_unix_ms {
        validate_time(end)?;
    }
    validate_nonempty_bounded(&row.repo, DASHBOARD_FILTER_VALUE_MAX_CHARS)?;
    for (value, max) in [
        (row.agent.as_deref(), 128),
        (row.model.as_deref(), DASHBOARD_FILTER_VALUE_MAX_CHARS),
        (row.tool_name.as_deref(), 128),
    ] {
        if let Some(value) = value {
            validate_nonempty_bounded(value, max)?;
        }
    }
    for id in [row.session_id.as_deref(), row.turn_id.as_deref()]
        .into_iter()
        .flatten()
    {
        validate_projected_id(id)?;
    }
    validate_availability_reasons(&row.availability_reasons)
}

fn validate_availability_reasons(
    reasons: &[DashboardAvailabilityReasonV1],
) -> Result<(), DashboardContractError> {
    if reasons.len() > DASHBOARD_AVAILABILITY_FIELDS.len() {
        return Err(DashboardContractError::TooManyRows);
    }
    for (index, reason) in reasons.iter().enumerate() {
        if reason.availability.state == AvailabilityStateV2::Available
            || !valid_availability_reason(&reason.availability)
        {
            return Err(DashboardContractError::InvalidAvailabilityReason);
        }
        if reasons[..index]
            .iter()
            .any(|prior| prior.field == reason.field)
        {
            return Err(DashboardContractError::DuplicateAvailabilityField);
        }
    }
    Ok(())
}

fn validate_projected_id(value: &str) -> Result<(), DashboardContractError> {
    let Some(digest) = value.strip_prefix("id:sha256:") else {
        return Err(DashboardContractError::InvalidProjectedId);
    };
    if digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(DashboardContractError::InvalidProjectedId)
    }
}

fn validate_nonempty_bounded(value: &str, max_chars: usize) -> Result<(), DashboardContractError> {
    let chars = value.chars().count();
    if chars == 0 {
        return Err(DashboardContractError::EmptyString);
    }
    if chars > max_chars {
        return Err(DashboardContractError::StringTooLong);
    }
    Ok(())
}

fn validate_time(value: f64) -> Result<(), DashboardContractError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(DashboardContractError::InvalidNumber)
    }
}

fn validate_wire_bytes<T: Serialize>(
    value: &T,
    limit: usize,
    request: bool,
) -> Result<(), DashboardContractError> {
    let bytes = serde_json::to_vec(value).map_err(|_| DashboardContractError::InvalidNumber)?;
    if bytes.len() <= limit {
        Ok(())
    } else if request {
        Err(DashboardContractError::RequestTooLarge)
    } else {
        Err(DashboardContractError::ResponseTooLarge)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn projected_id(character: char) -> String {
        format!("id:sha256:{}", character.to_string().repeat(64))
    }

    fn snapshot() -> DashboardSnapshotV1 {
        DashboardSnapshotV1 {
            id: "b".repeat(64),
            generation: "42".into(),
            visibility_epoch: "7".into(),
            generated_at: "2026-09-07T00:00:00.000Z".into(),
            state: DashboardSnapshotStateV1::Current,
        }
    }

    fn scope() -> DashboardQueryScopeV1 {
        DashboardQueryScopeV1 {
            filters: DashboardFiltersV1::default(),
            selected_trace_id: None,
            cold_excluded: true,
        }
    }

    fn complete_work() -> DashboardWorkV1 {
        DashboardWorkV1::complete(DashboardKpisV1 {
            sessions: 2,
            turns: 3,
            llm: 4,
            tools: 5,
            errors: 1,
            input_tokens: Some(100),
            output_tokens: Some(20),
            total_tokens: Some(120),
            token_status: DashboardTokenStatusV1::Complete,
            estimated_cost: Some(0.01),
            cost_status: DashboardCostStatusV1::Estimated,
            currency: Some("USD".into()),
        })
    }

    #[test]
    fn request_and_response_round_trip_as_closed_tagged_contracts() {
        let request = DashboardQueryRequestV1::Spans(DashboardSpansRequestV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardSpansKindV1::Spans,
            filters: Some(DashboardFiltersV1 {
                repo: vec!["workspace".into()],
                text: Some("failed tool".into()),
                ..DashboardFiltersV1::default()
            }),
            snapshot_id: "b".repeat(64),
            trace_id: projected_id('a'),
            cursor: Some("opaque-server-lease".into()),
        });
        request.validate().unwrap();
        let request_json = serde_json::to_value(&request).unwrap();
        assert_eq!(request_json["kind"], "spans");
        assert_eq!(request_json["schemaVersion"], DASHBOARD_QUERY_VERSION);
        assert!(serde_json::from_value::<DashboardQueryRequestV1>(request_json).is_ok());

        let response = DashboardQueryResponseV1::Summary(DashboardSummaryResponseV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardSummaryKindV1::Summary,
            snapshot: snapshot(),
            scope: scope(),
            work: complete_work(),
            pagination: DashboardPaginationV1 {
                next_cursor: None,
                total: None,
            },
        });
        response.validate().unwrap();
        let response_json = serde_json::to_value(&response).unwrap();
        assert_eq!(response_json["snapshot"]["generation"], "42");
        assert!(response_json["work"].get("kpis").is_some());
        assert!(serde_json::from_value::<DashboardQueryResponseV1>(response_json).is_ok());
    }

    #[test]
    fn canonical_fixture_is_a_valid_dashboard_response() {
        let response: DashboardQueryResponseV1 =
            serde_json::from_str(DASHBOARD_QUERY_FIXTURE).expect("dashboard fixture deserializes");
        response.validate().expect("dashboard fixture validates");
    }

    #[test]
    fn dashboard_wire_matches_shared_rust_typescript_parity_corpus() {
        let corpus: Value = serde_json::from_str(DASHBOARD_QUERY_PARITY).unwrap();
        let bases = corpus["bases"].as_object().unwrap();
        let mut categories = std::collections::BTreeSet::new();
        for parity_case in corpus["cases"].as_array().unwrap() {
            let name = parity_case["name"].as_str().unwrap();
            categories.insert(parity_case["category"].as_str().unwrap());
            let base = parity_case["base"].as_str().unwrap();
            let mut document = bases.get(base).unwrap().clone();
            apply_dashboard_parity_case(&mut document, parity_case);
            assert_eq!(
                dashboard_document_is_valid(document),
                parity_case["valid"].as_bool().unwrap(),
                "{name}"
            );
        }
        assert_eq!(categories, ["negative", "positive", "privacy"].into());
    }

    fn dashboard_document_is_valid(document: Value) -> bool {
        serde_json::from_value::<DashboardQueryRequestV1>(document.clone())
            .is_ok_and(|request| request.validate().is_ok())
            || serde_json::from_value::<DashboardQueryResponseV1>(document)
                .is_ok_and(|response| response.validate().is_ok())
    }

    fn apply_dashboard_parity_case(document: &mut Value, parity_case: &Value) {
        let operation = parity_case["operation"].as_str().unwrap();
        if operation == "none" {
            return;
        }
        let path = parity_case["path"].as_array().unwrap();
        let (field, parents) = path.split_last().unwrap();
        let field = field.as_str().unwrap();
        let mut parent = document;
        for segment in parents {
            parent = if let Some(field) = segment.as_str() {
                parent.as_object_mut().unwrap().get_mut(field).unwrap()
            } else {
                let index = usize::try_from(segment.as_u64().unwrap()).unwrap();
                parent.as_array_mut().unwrap().get_mut(index).unwrap()
            };
        }
        let object = parent.as_object_mut().unwrap();
        match operation {
            "set" => {
                object.insert(field.into(), parity_case["value"].clone());
            }
            "remove" => {
                object.remove(field);
            }
            _ => panic!("unsupported dashboard parity operation: {operation}"),
        }
    }

    #[test]
    fn pending_work_cannot_claim_kpis_or_totals() {
        let pending_with_kpis = json!({
            "state": "pending",
            "kpis": {
                "sessions": 1
            }
        });
        assert!(serde_json::from_value::<DashboardWorkV1>(pending_with_kpis).is_err());

        let response = DashboardQueryResponseV1::Summary(DashboardSummaryResponseV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardSummaryKindV1::Summary,
            snapshot: snapshot(),
            scope: scope(),
            work: DashboardWorkV1::pending(),
            pagination: DashboardPaginationV1 {
                next_cursor: Some("opaque-summary-progress".into()),
                total: None,
            },
        });
        let value = serde_json::to_value(response).unwrap();
        assert!(value["work"].get("kpis").is_none());
    }

    #[test]
    fn request_and_response_variants_reject_unknown_fields() {
        let request = json!({
            "schemaVersion": DASHBOARD_QUERY_VERSION,
            "kind": "bootstrap",
            "sql": "select 1"
        });
        assert!(serde_json::from_value::<DashboardQueryRequestV1>(request).is_err());

        let response = json!({
            "schemaVersion": DASHBOARD_QUERY_VERSION,
            "kind": "status",
            "requestKind": "traces",
            "reason": "busy",
            "path": "/tmp/private.db"
        });
        assert!(serde_json::from_value::<DashboardQueryResponseV1>(response).is_err());
    }

    #[test]
    fn compact_rows_reject_unknown_or_raw_detail_fields() {
        let row = json!({
            "traceId": projected_id('a'),
            "spanId": projected_id('b'),
            "parentSpanId": null,
            "kind": "tool",
            "name": "shell",
            "status": "ok",
            "startTimeUnixMs": 1,
            "endTimeUnixMs": 2,
            "repo": "workspace",
            "availabilityReasons": [],
            "rawContent": "forbidden"
        });
        assert!(serde_json::from_value::<DashboardSpanRowV1>(row).is_err());

        let clean = DashboardSpanRowV1 {
            trace_id: projected_id('a'),
            span_id: projected_id('b'),
            parent_span_id: None,
            kind: "tool".into(),
            name: "shell".into(),
            status: "ok".into(),
            start_time_unix_ms: 1.0,
            end_time_unix_ms: Some(2.0),
            repo: "workspace".into(),
            agent: None,
            model: None,
            session_id: None,
            turn_id: None,
            tool_name: None,
            availability_reasons: Vec::new(),
        };
        let serialized = serde_json::to_string(&clean).unwrap();
        for forbidden in [
            "raw",
            "attributes",
            "metrics",
            "requestContent",
            "responseContent",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn availability_reason_flattens_canonical_state_and_remains_closed() {
        let reason = DashboardAvailabilityReasonV1 {
            field: DashboardAvailabilityFieldV1::Tokens,
            availability: FieldAvailabilityV2 {
                state: AvailabilityStateV2::NotApplicable,
                reason: "span_kind_has_no_token_usage".into(),
            },
        };
        let value = serde_json::to_value(&reason).unwrap();
        assert_eq!(
            value,
            json!({
                "field": "tokens",
                "state": "not_applicable",
                "reason": "span_kind_has_no_token_usage"
            })
        );
        let round_trip: DashboardAvailabilityReasonV1 =
            serde_json::from_value(value.clone()).unwrap();
        assert_eq!(round_trip, reason);
        validate_availability_reasons(&[round_trip]).unwrap();

        let mut unknown = value;
        unknown["detail"] = json!("must remain closed");
        assert!(serde_json::from_value::<DashboardAvailabilityReasonV1>(unknown).is_err());
    }

    #[test]
    fn decimal_counters_are_strings_and_reject_values_above_u64() {
        let numeric_generation = json!({
            "id": "b".repeat(64),
            "generation": 42,
            "visibilityEpoch": "7",
            "generatedAt": "2026-09-07T00:00:00.000Z",
            "state": "current"
        });
        assert!(serde_json::from_value::<DashboardSnapshotV1>(numeric_generation).is_err());

        let mut invalid = snapshot();
        invalid.generation = "18446744073709551616".into();
        assert_eq!(
            validate_snapshot(&invalid),
            Err(DashboardContractError::InvalidDecimalU64)
        );
    }

    #[test]
    fn opaque_cursor_is_only_shape_validated() {
        let request = DashboardQueryRequestV1::Traces(DashboardTracesRequestV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardTracesKindV1::Traces,
            filters: None,
            snapshot_id: "b".repeat(64),
            cursor: Some("not-client-parseable-but-server-owned".into()),
        });
        request.validate().unwrap();

        let oversized = DashboardQueryRequestV1::Traces(DashboardTracesRequestV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardTracesKindV1::Traces,
            filters: None,
            snapshot_id: "b".repeat(64),
            cursor: Some("x".repeat(DASHBOARD_CURSOR_MAX_CHARS + 1)),
        });
        assert_eq!(
            oversized.validate(),
            Err(DashboardContractError::InvalidCursor)
        );
    }

    #[test]
    fn bootstrap_snapshot_id_is_required_by_every_followup_request() {
        let bootstrap = DashboardQueryResponseV1::Bootstrap(DashboardBootstrapResponseV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardBootstrapKindV1::Bootstrap,
            snapshot: snapshot(),
            scope: scope(),
            work: DashboardWorkV1::pending(),
            limits: DashboardQueryLimitsV1::default(),
        });
        bootstrap.validate().unwrap();
        let snapshot_id = match &bootstrap {
            DashboardQueryResponseV1::Bootstrap(response) => response.snapshot.id.clone(),
            _ => unreachable!(),
        };
        let followup = DashboardQueryRequestV1::Summary(DashboardSummaryRequestV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardSummaryKindV1::Summary,
            filters: None,
            snapshot_id,
            cursor: None,
        });
        followup.validate().unwrap();

        let missing_snapshot = json!({
            "schemaVersion": DASHBOARD_QUERY_VERSION,
            "kind": "summary"
        });
        assert!(serde_json::from_value::<DashboardQueryRequestV1>(missing_snapshot).is_err());
    }

    #[test]
    fn summary_progress_cursor_matches_work_state() {
        let pending_without_cursor =
            DashboardQueryResponseV1::Summary(DashboardSummaryResponseV1 {
                schema_version: DASHBOARD_QUERY_VERSION.into(),
                kind: DashboardSummaryKindV1::Summary,
                snapshot: snapshot(),
                scope: scope(),
                work: DashboardWorkV1::pending(),
                pagination: DashboardPaginationV1 {
                    next_cursor: None,
                    total: None,
                },
            });
        assert_eq!(
            pending_without_cursor.validate(),
            Err(DashboardContractError::InvalidPagination)
        );

        let complete_with_cursor = DashboardQueryResponseV1::Summary(DashboardSummaryResponseV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardSummaryKindV1::Summary,
            snapshot: snapshot(),
            scope: scope(),
            work: complete_work(),
            pagination: DashboardPaginationV1 {
                next_cursor: Some("must-end-when-complete".into()),
                total: None,
            },
        });
        assert_eq!(
            complete_with_cursor.validate(),
            Err(DashboardContractError::InvalidPagination)
        );
    }

    #[test]
    fn response_wire_byte_cap_is_independent_of_row_limits() {
        let mut report: crate::ReportDtoV2 = serde_json::from_str(include_str!(
            "../../../contracts/report-dto-v2.fixture.json"
        ))
        .unwrap();
        let mut detail = report.spans.remove(0);
        detail.trace_id = projected_id('a');
        detail.span_id = projected_id('b');
        detail.parent_span_id = None;
        detail.name = "x".repeat(DASHBOARD_RESPONSE_MAX_SERIALIZED_UTF8_BYTES);
        let response = DashboardQueryResponseV1::Span(DashboardSpanResponseV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardSpanKindV1::Span,
            snapshot: snapshot(),
            scope: scope(),
            work: DashboardWorkV1::pending(),
            detail: Box::new(detail),
        });
        assert_eq!(
            response.validate(),
            Err(DashboardContractError::ResponseTooLarge)
        );
    }

    #[test]
    fn schema_limits_and_enums_match_rust_sources() {
        let schema: Value = serde_json::from_str(DASHBOARD_QUERY_SCHEMA).unwrap();
        assert_eq!(
            schema["x-agent-observability-max-request-serialized-utf8-bytes"],
            DASHBOARD_REQUEST_MAX_SERIALIZED_UTF8_BYTES
        );
        assert_eq!(
            schema["x-agent-observability-max-response-serialized-utf8-bytes"],
            DASHBOARD_RESPONSE_MAX_SERIALIZED_UTF8_BYTES
        );
        assert_eq!(
            schema["$defs"]["limits"]["properties"]["traceRows"]["const"],
            DASHBOARD_TRACE_PAGE_MAX_ROWS
        );
        assert_eq!(
            schema["$defs"]["limits"]["properties"]["spanRows"]["const"],
            DASHBOARD_SPAN_PAGE_MAX_ROWS
        );
        assert_eq!(
            schema["$defs"]["limits"]["properties"]["timelineRows"]["const"],
            DASHBOARD_TIMELINE_MAX_ROWS
        );
        assert_eq!(
            schema["$defs"]["limits"]["properties"]["facetValues"]["const"],
            DASHBOARD_FACET_MAX_VALUES
        );
        assert_schema_enum(&schema, "request_kind", DASHBOARD_REQUEST_KINDS);
        assert_schema_enum(&schema, "status_reason", DASHBOARD_STATUS_REASONS);
        assert_schema_enum(&schema, "snapshot_state", DASHBOARD_SNAPSHOT_STATES);
        assert_schema_enum(&schema, "token_status", DASHBOARD_TOKEN_STATUSES);
        assert_schema_enum(&schema, "cost_status", DASHBOARD_COST_STATUSES);
        assert_schema_enum(&schema, "facet_dimension", DASHBOARD_FACET_DIMENSIONS);
        assert_schema_enum(&schema, "availability_field", DASHBOARD_AVAILABILITY_FIELDS);
    }

    fn assert_schema_enum(schema: &Value, definition: &str, expected: &[&str]) {
        let actual = schema["$defs"][definition]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(Value::as_str)
            .collect::<Option<Vec<_>>>()
            .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn schema_is_closed_and_references_existing_report_span_detail() {
        assert!(DASHBOARD_QUERY_SCHEMA.contains("\"unevaluatedProperties\": false"));
        assert!(DASHBOARD_QUERY_SCHEMA.contains("\"additionalProperties\": false"));
        assert!(
            DASHBOARD_QUERY_SCHEMA.contains("\"$ref\": \"report-dto-v2.schema.json#/$defs/span\"")
        );
        for forbidden in ["sql", "path", "rawContent", "privateRawDetail"] {
            assert!(!DASHBOARD_QUERY_SCHEMA.contains(&format!("\"{forbidden}\"")));
        }
    }
}
