use agent_observability_domain::{
    CorrelationIds, LifecycleState, ObservationId, SourceCursor, SourceGeneration, SpanId,
    SpanKind, StatusCode, Timing, TokenUsage, TraceId,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

pub const CONTRACT_MANIFEST: &str = include_str!("../../../contracts/contract-manifest.v1");
pub const DURABLE_RECORD_SCHEMA: &str =
    include_str!("../../../contracts/durable-record-v1.schema.json");
pub const REPORT_DTO_SCHEMA: &str = include_str!("../../../contracts/report-dto-v1.schema.json");
pub const DURABLE_RECORD_VERSION: &str = "agent_observability.v1";
pub const REPORT_DTO_VERSION: &str = "agent_observability.report.v1";

pub const DURABLE_RECORD_FIELDS: &[&str] = &[
    "schema_version",
    "record_type",
    "trace_id",
    "span_id",
    "parent_span_id",
    "span_kind",
    "name",
    "start_time_unix_ms",
    "end_time_unix_ms",
    "status",
    "agent",
    "project",
    "attributes",
    "metrics",
    "content",
    "redaction",
];
pub const REPORT_DTO_FIELDS: &[&str] = &[
    "schemaVersion",
    "generatedAt",
    "title",
    "summary",
    "cost",
    "filters",
    "traces",
    "spans",
];
pub const REDACTION_RECORD_FIELDS: [&str; 3] = ["applied", "count", "fields"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentSource {
    Codex,
    ClaudeCode,
    Cursor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObservationEvent {
    Session {
        model: Option<String>,
        project: Option<String>,
    },
    Turn,
    ModelRequest {
        model: Option<String>,
    },
    ToolOperation {
        tool_name: Option<String>,
        phase: Option<String>,
    },
    Permission {
        decision: Option<String>,
    },
    Compaction {
        trigger: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceObservation {
    pub source: AgentSource,
    pub source_generation: SourceGeneration,
    pub source_cursor: SourceCursor,
    pub observation_id: ObservationId,
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub correlation: CorrelationIds,
    pub event: ObservationEvent,
    pub lifecycle: LifecycleState,
    pub timing: Timing,
    pub token_usage: TokenUsage,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScalarValueV1 {
    Boolean(bool),
    Number(f64),
    String(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusV1 {
    pub code: StatusCode,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AgentV1 {
    pub name: Option<String>,
    pub version: Option<String>,
    pub model: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectV1 {
    pub name: Option<String>,
    pub repo_path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AttributesV1 {
    pub source: Option<ScalarValueV1>,
    pub event_type: Option<ScalarValueV1>,
    pub envelope_type: Option<ScalarValueV1>,
    pub session_id: Option<ScalarValueV1>,
    pub turn_id: Option<ScalarValueV1>,
    pub request_id: Option<ScalarValueV1>,
    pub call_id: Option<ScalarValueV1>,
    pub tool_name: Option<ScalarValueV1>,
    pub phase: Option<ScalarValueV1>,
    pub exit_code: Option<ScalarValueV1>,
    pub sandbox: Option<ScalarValueV1>,
    pub approval: Option<ScalarValueV1>,
    pub permission_id: Option<ScalarValueV1>,
    pub decision: Option<ScalarValueV1>,
    pub command_kind: Option<ScalarValueV1>,
    pub compaction_id: Option<ScalarValueV1>,
    pub trigger: Option<ScalarValueV1>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetricsV1 {
    pub input_tokens: Option<f64>,
    pub output_tokens: Option<f64>,
    pub cached_input_tokens: Option<f64>,
    pub cache_creation_input_tokens: Option<f64>,
    pub reasoning_output_tokens: Option<f64>,
    pub total_tokens: Option<f64>,
    pub total_input_tokens: Option<f64>,
    pub total_output_tokens: Option<f64>,
    pub total_cached_input_tokens: Option<f64>,
    pub total_reasoning_output_tokens: Option<f64>,
    pub total_accumulated_tokens: Option<f64>,
    pub context_window_tokens: Option<f64>,
    pub input_tokens_before: Option<f64>,
    pub input_tokens_after: Option<f64>,
    pub latency_ms: Option<f64>,
    pub duration_ms: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContentV1 {
    pub prompt: Option<JsonValue>,
    pub output: Option<JsonValue>,
    pub tool_input: Option<JsonValue>,
    pub tool_output: Option<JsonValue>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RedactionV1 {
    pub applied: bool,
    pub count: u64,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DurableRecordV1 {
    pub schema_version: &'static str,
    pub record_type: &'static str,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub span_kind: SpanKind,
    pub name: String,
    pub start_time_unix_ms: f64,
    pub end_time_unix_ms: Option<f64>,
    pub status: StatusV1,
    pub agent: AgentV1,
    pub project: ProjectV1,
    pub attributes: AttributesV1,
    pub metrics: MetricsV1,
    pub content: ContentV1,
    pub redaction: RedactionV1,
}

impl DurableRecordV1 {
    /// Validates the closed durable wire contract.
    ///
    /// # Errors
    ///
    /// Returns [`ContractError`] for an invalid header, identity, or time range.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.schema_version != DURABLE_RECORD_VERSION || self.record_type != "span" {
            return Err(ContractError::InvalidDurableHeader);
        }
        if self.trace_id.is_empty() || self.span_id.is_empty() || self.name.is_empty() {
            return Err(ContractError::MissingDurableIdentity);
        }
        validate_finite(self.start_time_unix_ms)?;
        if let Some(end) = self.end_time_unix_ms {
            validate_finite(end)?;
        }
        if self
            .end_time_unix_ms
            .is_some_and(|end| end < self.start_time_unix_ms)
        {
            return Err(ContractError::EndBeforeStart);
        }
        validate_optional_nonempty(self.parent_span_id.as_deref())?;
        for value in [
            self.agent.name.as_deref(),
            self.agent.version.as_deref(),
            self.agent.model.as_deref(),
            self.project.name.as_deref(),
            self.project.repo_path.as_deref(),
        ] {
            validate_optional_nonempty(value)?;
        }
        validate_attributes(&self.attributes)?;
        validate_metrics(&self.metrics)?;
        for value in [
            self.content.prompt.as_ref(),
            self.content.output.as_ref(),
            self.content.tool_input.as_ref(),
            self.content.tool_output.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            validate_json_value(value)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReportSummaryV1 {
    pub generated_spans: u64,
    pub sessions: u64,
    pub turns: u64,
    pub llm_requests: u64,
    pub tool_executions: u64,
    pub errors: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub latency_ms: u64,
    pub duration_ms: u64,
    pub estimated_cost: f64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReportFiltersV1 {
    pub repos: Vec<String>,
    pub sessions: Vec<String>,
    pub turns: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CostComponentV1 {
    pub tokens: f64,
    pub rate_per_1m: f64,
    pub estimated_cost: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CostDetailV1 {
    pub assumption: String,
    pub incomplete_count: Option<u64>,
    pub unknown_count: Option<u64>,
    pub missing: Vec<String>,
    pub semantic_errors: Vec<String>,
    pub components: BTreeMap<String, CostComponentV1>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CostEstimateV1 {
    pub status: String,
    pub reason: Option<String>,
    pub estimated_cost: Option<f64>,
    pub currency: Option<String>,
    pub model: Option<String>,
    pub rate_table: RateTableRefV1,
    pub cost: CostDetailV1,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RateTableRefV1 {
    pub version: Option<String>,
    pub unit: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReportMetricsV1 {
    pub input_tokens: Option<f64>,
    pub output_tokens: Option<f64>,
    pub cached_input_tokens: Option<f64>,
    pub cache_creation_input_tokens: Option<f64>,
    pub reasoning_output_tokens: Option<f64>,
    pub total_tokens: Option<f64>,
    pub latency_ms: Option<f64>,
    pub duration_ms: Option<f64>,
    pub total_input_tokens: Option<f64>,
    pub total_output_tokens: Option<f64>,
    pub total_cached_input_tokens: Option<f64>,
    pub total_reasoning_output_tokens: Option<f64>,
    pub total_accumulated_tokens: Option<f64>,
    pub context_window_tokens: Option<f64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReportAgentV1 {
    pub name: Option<String>,
    pub model: Option<String>,
    pub version: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReportAttributesV1 {
    pub source: Option<ScalarValueV1>,
    pub event_type: Option<ScalarValueV1>,
    pub envelope_type: Option<ScalarValueV1>,
    pub session_id: Option<ScalarValueV1>,
    pub turn_id: Option<ScalarValueV1>,
    pub request_id: Option<ScalarValueV1>,
    pub call_id: Option<ScalarValueV1>,
    pub tool_name: Option<ScalarValueV1>,
    pub phase: Option<ScalarValueV1>,
    pub exit_code: Option<ScalarValueV1>,
    pub sandbox: Option<ScalarValueV1>,
    pub approval: Option<ScalarValueV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReportSpanV1 {
    pub schema_version: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub kind: SpanKind,
    pub name: String,
    pub status: StatusCode,
    pub start_time_unix_ms: f64,
    pub end_time_unix_ms: Option<f64>,
    pub repo: String,
    pub agent: ReportAgentV1,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub tool_name: Option<String>,
    pub attributes: ReportAttributesV1,
    pub metrics: ReportMetricsV1,
    pub estimated_cost: Option<f64>,
    pub cost: CostEstimateV1,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TraceSummaryV1 {
    pub trace_id: String,
    pub repo: String,
    pub spans: u64,
    pub errors: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated_cost: f64,
    pub start_time_unix_ms: f64,
    pub end_time_unix_ms: Option<f64>,
    pub sessions: Vec<String>,
    pub turns: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReportDtoV1 {
    pub schema_version: &'static str,
    pub generated_at: String,
    pub title: String,
    pub summary: ReportSummaryV1,
    pub cost: CostEstimateV1,
    pub filters: ReportFiltersV1,
    pub traces: Vec<TraceSummaryV1>,
    pub spans: Vec<ReportSpanV1>,
}

impl ReportDtoV1 {
    /// Validates the closed report wire contract.
    ///
    /// # Errors
    ///
    /// Returns [`ContractError`] for a wrong version or invalid nested number/status.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.schema_version != REPORT_DTO_VERSION {
            return Err(ContractError::InvalidReportVersion);
        }
        validate_finite(self.summary.estimated_cost)?;
        validate_cost(&self.cost)?;
        for trace in &self.traces {
            validate_finite(trace.estimated_cost)?;
            validate_finite(trace.start_time_unix_ms)?;
            if let Some(end) = trace.end_time_unix_ms {
                validate_finite(end)?;
            }
        }
        for span in &self.spans {
            validate_finite(span.start_time_unix_ms)?;
            if let Some(end) = span.end_time_unix_ms {
                validate_finite(end)?;
            }
            validate_report_attributes(&span.attributes)?;
            validate_report_metrics(&span.metrics)?;
            if let Some(amount) = span.estimated_cost {
                validate_finite(amount)?;
            }
            validate_cost(&span.cost)?;
        }
        Ok(())
    }
}

fn validate_optional_nonempty(value: Option<&str>) -> Result<(), ContractError> {
    if value.is_some_and(str::is_empty) {
        return Err(ContractError::EmptyOptionalString);
    }
    Ok(())
}

fn validate_attributes(attributes: &AttributesV1) -> Result<(), ContractError> {
    for value in [
        attributes.source.as_ref(),
        attributes.event_type.as_ref(),
        attributes.envelope_type.as_ref(),
        attributes.session_id.as_ref(),
        attributes.turn_id.as_ref(),
        attributes.request_id.as_ref(),
        attributes.call_id.as_ref(),
        attributes.tool_name.as_ref(),
        attributes.phase.as_ref(),
        attributes.exit_code.as_ref(),
        attributes.sandbox.as_ref(),
        attributes.approval.as_ref(),
        attributes.permission_id.as_ref(),
        attributes.decision.as_ref(),
        attributes.command_kind.as_ref(),
        attributes.compaction_id.as_ref(),
        attributes.trigger.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        validate_scalar(value)?;
    }
    Ok(())
}

fn validate_metrics(metrics: &MetricsV1) -> Result<(), ContractError> {
    for value in [
        metrics.input_tokens,
        metrics.output_tokens,
        metrics.cached_input_tokens,
        metrics.cache_creation_input_tokens,
        metrics.reasoning_output_tokens,
        metrics.total_tokens,
        metrics.total_input_tokens,
        metrics.total_output_tokens,
        metrics.total_cached_input_tokens,
        metrics.total_reasoning_output_tokens,
        metrics.total_accumulated_tokens,
        metrics.context_window_tokens,
        metrics.input_tokens_before,
        metrics.input_tokens_after,
        metrics.latency_ms,
        metrics.duration_ms,
    ]
    .into_iter()
    .flatten()
    {
        validate_nonnegative_finite(value)?;
    }
    Ok(())
}

fn validate_report_attributes(attributes: &ReportAttributesV1) -> Result<(), ContractError> {
    for value in [
        attributes.source.as_ref(),
        attributes.event_type.as_ref(),
        attributes.envelope_type.as_ref(),
        attributes.session_id.as_ref(),
        attributes.turn_id.as_ref(),
        attributes.request_id.as_ref(),
        attributes.call_id.as_ref(),
        attributes.tool_name.as_ref(),
        attributes.phase.as_ref(),
        attributes.exit_code.as_ref(),
        attributes.sandbox.as_ref(),
        attributes.approval.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        validate_scalar(value)?;
    }
    Ok(())
}

fn validate_report_metrics(metrics: &ReportMetricsV1) -> Result<(), ContractError> {
    for value in [
        metrics.input_tokens,
        metrics.output_tokens,
        metrics.cached_input_tokens,
        metrics.cache_creation_input_tokens,
        metrics.reasoning_output_tokens,
        metrics.total_tokens,
        metrics.latency_ms,
        metrics.duration_ms,
        metrics.total_input_tokens,
        metrics.total_output_tokens,
        metrics.total_cached_input_tokens,
        metrics.total_reasoning_output_tokens,
        metrics.total_accumulated_tokens,
        metrics.context_window_tokens,
    ]
    .into_iter()
    .flatten()
    {
        validate_finite(value)?;
    }
    Ok(())
}

fn validate_scalar(value: &ScalarValueV1) -> Result<(), ContractError> {
    if let ScalarValueV1::Number(number) = value {
        validate_finite(*number)?;
    }
    Ok(())
}

fn validate_json_value(value: &JsonValue) -> Result<(), ContractError> {
    match value {
        JsonValue::Number(number) => validate_finite(*number),
        JsonValue::Array(values) => values.iter().try_for_each(validate_json_value),
        JsonValue::Object(values) => values.values().try_for_each(validate_json_value),
        JsonValue::Null | JsonValue::Boolean(_) | JsonValue::String(_) => Ok(()),
    }
}

fn validate_cost(cost: &CostEstimateV1) -> Result<(), ContractError> {
    if !matches!(cost.status.as_str(), "estimated" | "incomplete" | "unknown") {
        return Err(ContractError::InvalidCostStatus);
    }
    if let Some(amount) = cost.estimated_cost {
        validate_finite(amount)?;
    }
    for component in cost.cost.components.values() {
        validate_finite(component.tokens)?;
        validate_finite(component.rate_per_1m)?;
        validate_finite(component.estimated_cost)?;
    }
    Ok(())
}

fn validate_nonnegative_finite(value: f64) -> Result<(), ContractError> {
    validate_finite(value)?;
    if value < 0.0 {
        return Err(ContractError::NegativeMetric);
    }
    Ok(())
}

fn validate_finite(value: f64) -> Result<(), ContractError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(ContractError::NonFiniteNumber)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractManifest {
    entries: BTreeMap<String, String>,
}

impl ContractManifest {
    /// Parses the shared line-oriented contract manifest.
    ///
    /// # Errors
    ///
    /// Returns [`ContractError::InvalidManifestLine`] for malformed or duplicate entries.
    pub fn parse(input: &str) -> Result<Self, ContractError> {
        let mut entries = BTreeMap::new();
        for (index, line) in input.lines().enumerate() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or(ContractError::InvalidManifestLine(index + 1))?;
            if key.is_empty()
                || value.is_empty()
                || entries.insert(key.into(), value.into()).is_some()
            {
                return Err(ContractError::InvalidManifestLine(index + 1));
            }
        }
        Ok(Self { entries })
    }

    #[must_use]
    pub fn value(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    /// Confirms stable versions, schema paths, and disabled future boundaries.
    ///
    /// # Errors
    ///
    /// Returns [`ContractError`] when a required entry is missing or differs.
    pub fn validate_release_boundary(&self) -> Result<(), ContractError> {
        self.expect("durable_record", DURABLE_RECORD_VERSION)?;
        self.expect("report_dto", REPORT_DTO_VERSION)?;
        self.expect("durable_schema", "contracts/durable-record-v1.schema.json")?;
        self.expect("report_schema", "contracts/report-dto-v1.schema.json")?;
        self.expect("team_ingest", "disabled")?;
        Ok(())
    }

    fn expect(&self, key: &'static str, expected: &'static str) -> Result<(), ContractError> {
        match self.value(key) {
            Some(actual) if actual == expected => Ok(()),
            Some(actual) => Err(ContractError::ManifestMismatch {
                key,
                expected,
                actual: actual.into(),
            }),
            None => Err(ContractError::MissingManifestKey(key)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractError {
    EndBeforeStart,
    InvalidDurableHeader,
    MissingDurableIdentity,
    InvalidReportVersion,
    EmptyOptionalString,
    InvalidCostStatus,
    NonFiniteNumber,
    NegativeMetric,
    InvalidManifestLine(usize),
    MissingManifestKey(&'static str),
    ManifestMismatch {
        key: &'static str,
        expected: &'static str,
        actual: String,
    },
}

impl Display for ContractError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EndBeforeStart => formatter.write_str("end time must not precede start time"),
            Self::InvalidDurableHeader => formatter.write_str("invalid durable record header"),
            Self::MissingDurableIdentity => {
                formatter.write_str("durable identity fields are required")
            }
            Self::InvalidReportVersion => formatter.write_str("invalid report DTO version"),
            Self::EmptyOptionalString => {
                formatter.write_str("optional contract strings must not be empty")
            }
            Self::InvalidCostStatus => formatter.write_str("invalid cost status"),
            Self::NonFiniteNumber => formatter.write_str("contract numbers must be finite"),
            Self::NegativeMetric => formatter.write_str("durable metrics must not be negative"),
            Self::InvalidManifestLine(line) => {
                write!(formatter, "invalid contract manifest line {line}")
            }
            Self::MissingManifestKey(key) => {
                write!(formatter, "missing contract manifest key {key}")
            }
            Self::ManifestMismatch {
                key,
                expected,
                actual,
            } => {
                write!(
                    formatter,
                    "contract manifest {key} expected {expected}, received {actual}"
                )
            }
        }
    }
}

impl Error for ContractError {}

#[cfg(test)]
mod tests {
    use super::{CONTRACT_MANIFEST, ContractManifest, DURABLE_RECORD_SCHEMA, REPORT_DTO_SCHEMA};

    #[test]
    fn shared_manifest_and_closed_schemas_match_release_boundary() {
        let manifest = ContractManifest::parse(CONTRACT_MANIFEST).expect("manifest parses");
        manifest
            .validate_release_boundary()
            .expect("release boundary matches");
        for schema in [DURABLE_RECORD_SCHEMA, REPORT_DTO_SCHEMA] {
            assert!(schema.contains("\"additionalProperties\": false"));
        }
    }
}
