//! Pure application use cases for local pricing and cost aggregation.

use agent_observability_contracts::{
    AttributesV1, AvailabilityStateV2, CostComponentV1, CostDetailV1, CostEstimateV1,
    DurableRecordV1, FieldAvailabilityV2, MetricsV1, REPORT_DTO_VERSION, RateTableRefV1,
    ReportAgentV1, ReportAttributesV1, ReportAvailabilityV2, ReportDtoV2, ReportFiltersV1,
    ReportMetricsV1, ReportSpanV2, ReportSummaryV1, ScalarValueV1, TraceSummaryV1,
    hash_opaque_identifier, redact_sensitive_text, sanitize_durable_record,
    sanitize_owned_durable_record,
};
use agent_observability_domain::SpanKind;
use agent_observability_domain::StatusCode;
use agent_observability_domain::{
    DomainError, DomainSpanState, LifecycleReducer, validate_topology,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

const TOKEN_KEYS: [&str; 5] = [
    "input_tokens",
    "output_tokens",
    "cached_input_tokens",
    "cache_creation_input_tokens",
    "reasoning_output_tokens",
];
const OVERLAP_KEYS: [&str; 3] = [
    "cached_input_tokens",
    "cache_creation_input_tokens",
    "reasoning_output_tokens",
];
const DEFAULT_ASSUMPTION: &str =
    "Estimated from a local static rate table; not a billing statement.";
const NO_TABLE_ASSUMPTION: &str = "No local rate table was supplied.";
pub const RATE_TABLE_VERSION: &str = "agent_observability.rate_table.v1";

/// Reduces one canonical observation into current application state and validates topology.
///
/// # Errors
///
/// Returns [`DomainError`] when lifecycle, identity, metrics, or topology cannot converge.
pub fn reduce_observation_state(
    states: &mut Vec<DomainSpanState>,
    incoming: DomainSpanState,
) -> Result<DomainSpanState, DomainError> {
    let existing_index = states
        .iter()
        .position(|existing| existing.span_id == incoming.span_id);
    let reduced = reduce_span_state(existing_index.map(|index| &states[index]), incoming)?;
    if let Some(index) = existing_index {
        states[index] = reduced.clone();
    } else {
        states.push(reduced.clone());
    }
    validate_topology(states)?;
    Ok(reduced)
}

/// Reduces one span without loading unrelated topology state.
///
/// Callers that persist topology separately must validate the affected parent chain before commit.
///
/// # Errors
///
/// Returns [`DomainError`] when lifecycle, identity, or metrics cannot converge.
pub fn reduce_span_state(
    existing: Option<&DomainSpanState>,
    incoming: DomainSpanState,
) -> Result<DomainSpanState, DomainError> {
    let Some(existing) = existing else {
        return Ok(incoming);
    };
    let mut reduced = LifecycleReducer::reduce([existing.clone(), incoming])?;
    reduced.pop().ok_or_else(|| DomainError::LifecycleConflict {
        span_id: existing.span_id.clone(),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenSemantics {
    Exclusive,
    IncludedInTotal,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelRates {
    pub input_tokens: Option<f64>,
    pub output_tokens: Option<f64>,
    pub cached_input_tokens: Option<f64>,
    pub cache_creation_input_tokens: Option<f64>,
    pub reasoning_output_tokens: Option<f64>,
    pub token_semantics: BTreeMap<String, TokenSemantics>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RateTable {
    version: String,
    currency: String,
    unit: String,
    assumption: String,
    models: BTreeMap<String, ModelRates>,
}

impl Default for RateTable {
    fn default() -> Self {
        Self {
            version: "unversioned".into(),
            currency: "USD".into(),
            unit: "per_1m_tokens".into(),
            assumption: DEFAULT_ASSUMPTION.into(),
            models: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RateTableInput {
    pub version: Option<String>,
    pub currency: Option<String>,
    pub unit: Option<String>,
    pub assumption: Option<String>,
    pub models: BTreeMap<String, ModelRatesInput>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ModelRatesInput {
    pub input_tokens: Option<f64>,
    pub output_tokens: Option<f64>,
    pub cached_input_tokens: Option<f64>,
    pub cache_creation_input_tokens: Option<f64>,
    pub reasoning_output_tokens: Option<f64>,
    pub token_semantics: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RateTableDocumentV1 {
    schema_version: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    assumption: Option<String>,
    models: BTreeMap<String, ModelRatesInput>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReportProjectionError {
    InvalidRecord {
        index: usize,
        source: agent_observability_contracts::ContractError,
    },
    InvalidReport(agent_observability_contracts::ContractError),
    InvalidSummaryMetric {
        field: &'static str,
        value: f64,
    },
}

impl Display for ReportProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRecord { index, source } => {
                write!(formatter, "record {index} is invalid: {source}")
            }
            Self::InvalidReport(source) => {
                write!(formatter, "projected report is invalid: {source}")
            }
            Self::InvalidSummaryMetric { field, value } => {
                write!(
                    formatter,
                    "summary metric {field} must be an unsigned integer: {value}"
                )
            }
        }
    }
}

impl Error for ReportProjectionError {}

/// Incrementally projects durable records without retaining the source records.
#[derive(Debug)]
pub struct ReportProjector<'a> {
    spans: Vec<ReportSpanV2>,
    table: Option<&'a RateTable>,
}

impl<'a> ReportProjector<'a> {
    #[must_use]
    pub fn new(capacity: usize, table: Option<&'a RateTable>) -> Self {
        Self {
            spans: Vec::with_capacity(capacity),
            table,
        }
    }

    /// Validates and projects one record, retaining only its privacy-safe report span.
    ///
    /// # Errors
    ///
    /// Returns [`ReportProjectionError`] when the record is invalid.
    pub fn push(
        &mut self,
        index: usize,
        record: &DurableRecordV1,
    ) -> Result<(), ReportProjectionError> {
        let record = sanitize_durable_record(record)
            .map_err(|source| ReportProjectionError::InvalidRecord { index, source })?;
        self.spans.push(report_span(&record, self.table));
        Ok(())
    }

    /// Validates and projects one owned record without cloning its durable representation.
    ///
    /// # Errors
    ///
    /// Returns [`ReportProjectionError`] when the record is invalid.
    pub fn push_owned(
        &mut self,
        index: usize,
        record: DurableRecordV1,
    ) -> Result<(), ReportProjectionError> {
        let record = sanitize_owned_durable_record(record)
            .map_err(|source| ReportProjectionError::InvalidRecord { index, source })?;
        self.spans.push(report_span(&record, self.table));
        Ok(())
    }

    /// Finalizes aggregate, filter, trace, and ordering projections.
    ///
    /// # Errors
    ///
    /// Returns [`ReportProjectionError`] when a derived report value is invalid.
    pub fn finish(
        mut self,
        generated_at: impl Into<String>,
        title: impl Into<String>,
    ) -> Result<ReportDtoV2, ReportProjectionError> {
        self.spans.sort_by(|left, right| {
            left.start_time_unix_ms
                .total_cmp(&right.start_time_unix_ms)
                .then_with(|| left.trace_id.cmp(&right.trace_id))
                .then_with(|| left.span_id.cmp(&right.span_id))
        });
        propagate_trace_repositories(&mut self.spans);
        let summary = summarize_report(&self.spans)?;
        let cost = estimate_cost_for_spans(&self.spans, self.table);
        let report = ReportDtoV2 {
            schema_version: REPORT_DTO_VERSION.into(),
            generated_at: generated_at.into(),
            title: redact_sensitive_text(&title.into(), "title"),
            summary,
            cost,
            filters: report_filters(&self.spans),
            traces: trace_summaries(&self.spans),
            spans: self.spans,
        };
        report
            .validate()
            .map_err(ReportProjectionError::InvalidReport)?;
        Ok(report)
    }
}

/// Projects validated durable spans into the privacy-safe report DTO.
///
/// The projector is pure: it does not inspect or copy durable content, and it rejects the
/// complete input when any record or derived report value cannot satisfy the wire contract.
///
/// # Errors
///
/// Returns [`ReportProjectionError`] when an input record or derived report value is invalid.
pub fn project_report(
    records: &[DurableRecordV1],
    generated_at: impl Into<String>,
    title: impl Into<String>,
    table: Option<&RateTable>,
) -> Result<ReportDtoV2, ReportProjectionError> {
    let mut projector = ReportProjector::new(records.len(), table);
    for (index, record) in records.iter().enumerate() {
        projector.push(index, record)?;
    }
    projector.finish(generated_at, title)
}

fn report_span(record: &DurableRecordV1, table: Option<&RateTable>) -> ReportSpanV2 {
    let attributes = report_attributes(&record.attributes);
    let metrics = report_metrics(&record.metrics);
    let cost = estimate_span_cost(record, table);
    let repo = repo_name(record);
    let session_id = scalar_string(attributes.session_id.as_ref());
    let turn_id = scalar_string(attributes.turn_id.as_ref());
    let model = record.agent.model.clone();
    let tokens_present = report_token_metrics_present(&metrics);
    let latency_present = metrics.latency_ms.is_some() || metrics.duration_ms.is_some();
    let private_detail = private_lookup_availability(
        record.agent.name.as_deref(),
        &attributes,
        turn_id.as_deref(),
        record.span_kind,
    );
    ReportSpanV2 {
        schema_version: record.schema_version.clone(),
        trace_id: hash_opaque_identifier(&record.trace_id),
        span_id: hash_opaque_identifier(&record.span_id),
        parent_span_id: record.parent_span_id.as_deref().map(hash_opaque_identifier),
        kind: record.span_kind,
        name: span_name(record, &attributes),
        status: record.status.code,
        start_time_unix_ms: record.start_time_unix_ms,
        end_time_unix_ms: record.end_time_unix_ms,
        repo: repo.clone(),
        agent: ReportAgentV1 {
            name: record.agent.name.clone(),
            model: model.clone(),
            version: record.agent.version.clone(),
        },
        availability: report_availability(
            &repo,
            turn_id.as_deref(),
            model.as_deref(),
            tokens_present,
            latency_present,
            record.span_kind,
            private_detail,
        ),
        session_id,
        turn_id,
        tool_name: scalar_string(attributes.tool_name.as_ref()),
        attributes,
        metrics,
        estimated_cost: cost.estimated_cost,
        cost,
    }
}

fn field_availability(state: AvailabilityStateV2, reason: &str) -> FieldAvailabilityV2 {
    FieldAvailabilityV2 {
        state,
        reason: reason.into(),
    }
}

fn report_availability(
    repo: &str,
    turn_id: Option<&str>,
    model: Option<&str>,
    tokens_present: bool,
    latency_present: bool,
    kind: SpanKind,
    private_detail: FieldAvailabilityV2,
) -> ReportAvailabilityV2 {
    let repository = if repo == "unknown" {
        field_availability(
            AvailabilityStateV2::SourceUnavailable,
            "source_not_provided",
        )
    } else {
        field_availability(AvailabilityStateV2::Available, "reported_by_adapter")
    };
    let turn = if turn_id.is_some() {
        field_availability(AvailabilityStateV2::Available, "reported_by_adapter")
    } else {
        field_availability(
            AvailabilityStateV2::SourceUnavailable,
            "source_not_provided",
        )
    };
    let model = if model.is_some() {
        field_availability(AvailabilityStateV2::Available, "reported_by_adapter")
    } else if matches!(kind, SpanKind::LlmRequest | SpanKind::AgentSession) {
        field_availability(
            AvailabilityStateV2::SourceUnavailable,
            "source_not_provided",
        )
    } else {
        field_availability(
            AvailabilityStateV2::NotApplicable,
            "span_kind_not_model_backed",
        )
    };
    let latency = if latency_present {
        field_availability(AvailabilityStateV2::Available, "reported_by_adapter")
    } else if matches!(kind, SpanKind::LlmRequest | SpanKind::ToolExecution) {
        field_availability(
            AvailabilityStateV2::SourceUnavailable,
            "source_not_provided",
        )
    } else {
        field_availability(
            AvailabilityStateV2::NotApplicable,
            "span_kind_has_no_latency",
        )
    };
    let tokens = if tokens_present {
        field_availability(AvailabilityStateV2::Available, "reported_by_adapter")
    } else if matches!(kind, SpanKind::LlmRequest) {
        field_availability(
            AvailabilityStateV2::SourceUnavailable,
            "source_not_provided",
        )
    } else {
        field_availability(
            AvailabilityStateV2::NotApplicable,
            "span_kind_has_no_token_usage",
        )
    };
    ReportAvailabilityV2 {
        repository,
        turn,
        model,
        tokens,
        latency,
        source_location: private_detail.clone(),
        request_content: private_detail.clone(),
        response_content: private_detail,
    }
}

fn private_lookup_availability(
    agent_name: Option<&str>,
    attributes: &ReportAttributesV1,
    turn_id: Option<&str>,
    kind: SpanKind,
) -> FieldAvailabilityV2 {
    let source = scalar_string(attributes.source.as_ref());
    let event_type = scalar_string(attributes.event_type.as_ref());
    if source.as_deref() == Some("codex.notify_or_session_jsonl") {
        return field_availability(
            AvailabilityStateV2::SourceUnavailable,
            "historical_codex_source_not_lookup_eligible",
        );
    }
    match agent_name {
        Some("claude-code") => field_availability(
            AvailabilityStateV2::NotApplicable,
            "claude_private_lookup_not_supported",
        ),
        Some("cursor") => field_availability(
            AvailabilityStateV2::NotApplicable,
            "cursor_private_lookup_not_supported",
        ),
        Some("codex")
            if source.as_deref() == Some("codex")
                && event_type.as_deref() == Some("turn")
                && kind == SpanKind::Turn =>
        {
            if turn_id.is_some() {
                field_availability(
                    AvailabilityStateV2::PrivateLookup,
                    "local_opt_in_lookup_required",
                )
            } else {
                field_availability(
                    AvailabilityStateV2::SourceUnavailable,
                    "codex_notify_turn_correlation_unavailable",
                )
            }
        }
        Some("codex") => field_availability(
            AvailabilityStateV2::NotApplicable,
            "codex_span_not_notify_derived",
        ),
        _ => field_availability(
            AvailabilityStateV2::NotApplicable,
            "agent_private_lookup_not_supported",
        ),
    }
}

fn report_token_metrics_present(metrics: &ReportMetricsV1) -> bool {
    [
        metrics.input_tokens,
        metrics.output_tokens,
        metrics.total_tokens,
        metrics.total_input_tokens,
        metrics.total_output_tokens,
        metrics.total_accumulated_tokens,
    ]
    .into_iter()
    .any(|value| value.is_some())
}

fn propagate_trace_repositories(spans: &mut [ReportSpanV2]) {
    let mut known = BTreeMap::<_, BTreeSet<_>>::new();
    for span in spans.iter().filter(|span| span.repo != "unknown") {
        known
            .entry(span.trace_id.clone())
            .or_default()
            .insert(span.repo.clone());
    }
    for span in spans {
        if span.repo != "unknown" {
            continue;
        }
        match known.get(&span.trace_id) {
            Some(repos) if repos.len() == 1 => {
                span.repo
                    .clone_from(repos.first().expect("single repository"));
                span.availability.repository = field_availability(
                    AvailabilityStateV2::Available,
                    "derived_from_trace_context",
                );
            }
            Some(_) => {
                span.availability.repository = field_availability(
                    AvailabilityStateV2::SourceUnavailable,
                    "ambiguous_trace_repository",
                );
            }
            None => {}
        }
    }
}

fn report_attributes(attributes: &AttributesV1) -> ReportAttributesV1 {
    ReportAttributesV1 {
        source: attributes.source.clone(),
        event_type: attributes.event_type.clone(),
        envelope_type: attributes.envelope_type.clone(),
        session_id: hash_scalar_identifier(attributes.session_id.as_ref()),
        turn_id: hash_scalar_identifier(attributes.turn_id.as_ref()),
        request_id: hash_scalar_identifier(attributes.request_id.as_ref()),
        call_id: hash_scalar_identifier(attributes.call_id.as_ref()),
        tool_name: attributes.tool_name.clone(),
        phase: attributes.phase.clone(),
        exit_code: attributes.exit_code.clone(),
        sandbox: attributes.sandbox.clone(),
        approval: attributes.approval.clone(),
    }
}

fn hash_scalar_identifier(value: Option<&ScalarValueV1>) -> Option<ScalarValueV1> {
    match value {
        Some(ScalarValueV1::String(value)) => {
            Some(ScalarValueV1::String(hash_opaque_identifier(value)))
        }
        Some(value) => Some(value.clone()),
        None => None,
    }
}

fn report_metrics(metrics: &MetricsV1) -> ReportMetricsV1 {
    ReportMetricsV1 {
        input_tokens: metrics.input_tokens,
        output_tokens: metrics.output_tokens,
        cached_input_tokens: metrics.cached_input_tokens,
        cache_creation_input_tokens: metrics.cache_creation_input_tokens,
        reasoning_output_tokens: metrics.reasoning_output_tokens,
        total_tokens: metrics.total_tokens,
        latency_ms: metrics.latency_ms,
        duration_ms: metrics.duration_ms,
        total_input_tokens: metrics.total_input_tokens,
        total_output_tokens: metrics.total_output_tokens,
        total_cached_input_tokens: metrics.total_cached_input_tokens,
        total_reasoning_output_tokens: metrics.total_reasoning_output_tokens,
        total_accumulated_tokens: metrics.total_accumulated_tokens,
        context_window_tokens: metrics.context_window_tokens,
    }
}

fn span_name(record: &DurableRecordV1, attributes: &ReportAttributesV1) -> String {
    match record.span_kind {
        SpanKind::AgentSession => format!(
            "{} session",
            record.agent.name.as_deref().unwrap_or("Agent")
        ),
        SpanKind::Turn => "Turn".into(),
        SpanKind::LlmRequest => record
            .agent
            .model
            .as_deref()
            .map_or_else(|| "LLM request".into(), |model| format!("LLM {model}")),
        SpanKind::ToolExecution => {
            scalar_string(attributes.tool_name.as_ref()).unwrap_or_else(|| "Tool execution".into())
        }
        SpanKind::Permission => "Permission".into(),
        SpanKind::Compaction => "Compaction".into(),
        SpanKind::Workstream => "Workstream".into(),
    }
}

fn repo_name(record: &DurableRecordV1) -> String {
    if let Some(name) = record
        .project
        .name
        .as_deref()
        .filter(|name| !name.is_empty())
    {
        return name.into();
    }
    "unknown".into()
}

fn scalar_string(value: Option<&ScalarValueV1>) -> Option<String> {
    match value {
        Some(ScalarValueV1::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => None,
    }
}

fn summarize_report(spans: &[ReportSpanV2]) -> Result<ReportSummaryV1, ReportProjectionError> {
    Ok(ReportSummaryV1 {
        generated_spans: spans.len() as u64,
        sessions: count_kind(spans, SpanKind::AgentSession),
        turns: count_kind(spans, SpanKind::Turn),
        llm_requests: count_kind(spans, SpanKind::LlmRequest),
        tool_executions: count_kind(spans, SpanKind::ToolExecution),
        errors: spans
            .iter()
            .filter(|span| span.status == StatusCode::Error)
            .count() as u64,
        input_tokens: sum_integer_metric(spans, |metrics| metrics.input_tokens, "inputTokens")?,
        output_tokens: sum_integer_metric(spans, |metrics| metrics.output_tokens, "outputTokens")?,
        cached_input_tokens: sum_integer_metric(
            spans,
            |metrics| metrics.cached_input_tokens,
            "cachedInputTokens",
        )?,
        cache_creation_input_tokens: sum_integer_metric(
            spans,
            |metrics| metrics.cache_creation_input_tokens,
            "cacheCreationInputTokens",
        )?,
        reasoning_output_tokens: sum_integer_metric(
            spans,
            |metrics| metrics.reasoning_output_tokens,
            "reasoningOutputTokens",
        )?,
        latency_ms: sum_integer_metric(spans, |metrics| metrics.latency_ms, "latencyMs")?,
        duration_ms: sum_integer_metric(spans, |metrics| metrics.duration_ms, "durationMs")?,
        estimated_cost: spans
            .iter()
            .map(|span| span.estimated_cost.unwrap_or(0.0))
            .sum::<f64>(),
    })
}

fn count_kind(spans: &[ReportSpanV2], kind: SpanKind) -> u64 {
    spans.iter().filter(|span| span.kind == kind).count() as u64
}

fn sum_integer_metric(
    spans: &[ReportSpanV2],
    get: impl Fn(&ReportMetricsV1) -> Option<f64>,
    field: &'static str,
) -> Result<u64, ReportProjectionError> {
    let mut total = 0_u64;
    for value in spans.iter().filter_map(|span| get(&span.metrics)) {
        if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > MAX_EXACT_U64 {
            return Err(ReportProjectionError::InvalidSummaryMetric { field, value });
        }
        total = total.checked_add(exact_u64(value)).ok_or(
            ReportProjectionError::InvalidSummaryMetric {
                field,
                value: f64::INFINITY,
            },
        )?;
    }
    Ok(total)
}

fn report_filters(spans: &[ReportSpanV2]) -> ReportFiltersV1 {
    ReportFiltersV1 {
        repos: unique_sorted(spans.iter().map(|span| span.repo.clone())),
        sessions: unique_sorted(spans.iter().filter_map(|span| span.session_id.clone())),
        turns: unique_sorted(spans.iter().filter_map(|span| span.turn_id.clone())),
        agents: unique_sorted(
            spans
                .iter()
                .map(|span| span.agent.name.clone().unwrap_or_else(|| "unknown".into())),
        ),
        models: unique_sorted(
            spans
                .iter()
                .map(|span| span.agent.model.clone().unwrap_or_else(|| "unknown".into())),
        ),
    }
}

fn unique_sorted(values: impl Iterator<Item = String>) -> Vec<String> {
    values
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn trace_summaries(spans: &[ReportSpanV2]) -> Vec<TraceSummaryV1> {
    let mut groups = BTreeMap::<String, TraceSummaryV1>::new();
    let mut repositories = BTreeMap::<String, BTreeSet<String>>::new();
    for span in spans {
        if span.repo != "unknown" {
            repositories
                .entry(span.trace_id.clone())
                .or_default()
                .insert(span.repo.clone());
        }
        let group = groups
            .entry(span.trace_id.clone())
            .or_insert_with(|| TraceSummaryV1 {
                trace_id: span.trace_id.clone(),
                repo: "unknown".into(),
                start_time_unix_ms: span.start_time_unix_ms,
                end_time_unix_ms: span.end_time_unix_ms,
                ..TraceSummaryV1::default()
            });
        group.spans += 1;
        group.errors += u64::from(span.status == StatusCode::Error);
        group.input_tokens += exact_u64(span.metrics.input_tokens.unwrap_or(0.0));
        group.output_tokens += exact_u64(span.metrics.output_tokens.unwrap_or(0.0));
        group.estimated_cost += span.estimated_cost.unwrap_or(0.0);
        group.start_time_unix_ms = group.start_time_unix_ms.min(span.start_time_unix_ms);
        group.end_time_unix_ms = max_nullable(group.end_time_unix_ms, span.end_time_unix_ms);
        if let Some(value) = &span.session_id {
            group.sessions.push(value.clone());
        }
        if let Some(value) = &span.turn_id {
            group.turns.push(value.clone());
        }
    }
    let mut traces = groups.into_values().collect::<Vec<_>>();
    for trace in &mut traces {
        if let Some(repos) = repositories.get(&trace.trace_id)
            && repos.len() == 1
        {
            trace
                .repo
                .clone_from(repos.first().expect("single repository"));
        }
        trace.sessions.sort();
        trace.sessions.dedup();
        trace.turns.sort();
        trace.turns.dedup();
    }
    traces.sort_by(|left, right| {
        left.start_time_unix_ms
            .total_cmp(&right.start_time_unix_ms)
            .then_with(|| left.trace_id.cmp(&right.trace_id))
    });
    traces
}

const MAX_EXACT_U64: f64 = 9_007_199_254_740_991.0;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn exact_u64(value: f64) -> u64 {
    value as u64
}

fn max_nullable(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (None, value) | (value, None) => value,
        (Some(left), Some(right)) => Some(left.max(right)),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PricingError {
    InvalidDocument,
    UnsupportedVersion,
    UnsupportedUnit,
    InvalidRate(String),
    UnsupportedSemantic(String),
}

impl Display for PricingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDocument => formatter.write_str("invalid rate table document"),
            Self::UnsupportedVersion => formatter.write_str("unsupported rate table version"),
            Self::UnsupportedUnit => formatter.write_str("rate table unit must be per_1m_tokens"),
            Self::InvalidRate(label) => write!(
                formatter,
                "rate table {label} must be a non-negative finite number"
            ),
            Self::UnsupportedSemantic(key) => write!(
                formatter,
                "rate table token_semantics.{key} is not supported"
            ),
        }
    }
}

impl Error for PricingError {}

/// Parses the closed local rate-table document and normalizes its pricing rules.
///
/// # Errors
///
/// Returns [`PricingError`] for malformed JSON, unknown fields, unsupported versions, or invalid
/// pricing values.
pub fn parse_rate_table_json(input: &str) -> Result<RateTable, PricingError> {
    let document: RateTableDocumentV1 =
        serde_json::from_str(input).map_err(|_| PricingError::InvalidDocument)?;
    if document.schema_version != RATE_TABLE_VERSION {
        return Err(PricingError::UnsupportedVersion);
    }
    normalize_rate_table(RateTableInput {
        version: document.version,
        currency: document.currency,
        unit: document.unit,
        assumption: document.assumption,
        models: document.models,
    })
}

/// Validates and normalizes a local rate table. Rates are always per million tokens.
///
/// # Errors
///
/// Returns [`PricingError`] when the unit, rate, or token semantic is invalid.
pub fn normalize_rate_table(input: RateTableInput) -> Result<RateTable, PricingError> {
    if input
        .unit
        .as_deref()
        .is_some_and(|unit| unit != "per_1m_tokens")
    {
        return Err(PricingError::UnsupportedUnit);
    }
    let mut models = BTreeMap::new();
    for (model, rates) in input.models {
        let normalized = ModelRates {
            input_tokens: normalize_rate(rates.input_tokens, &format!("{model}.input_tokens"))?,
            output_tokens: normalize_rate(rates.output_tokens, &format!("{model}.output_tokens"))?,
            cached_input_tokens: normalize_rate(
                rates.cached_input_tokens,
                &format!("{model}.cached_input_tokens"),
            )?,
            cache_creation_input_tokens: normalize_rate(
                rates.cache_creation_input_tokens,
                &format!("{model}.cache_creation_input_tokens"),
            )?,
            reasoning_output_tokens: normalize_rate(
                rates.reasoning_output_tokens,
                &format!("{model}.reasoning_output_tokens"),
            )?,
            token_semantics: normalize_semantics(rates.token_semantics)?,
        };
        models.insert(model, normalized);
    }
    Ok(RateTable {
        version: nonempty_or(input.version, "unversioned"),
        currency: nonempty_or(input.currency, "USD"),
        unit: "per_1m_tokens".into(),
        assumption: nonempty_or(input.assumption, DEFAULT_ASSUMPTION),
        models,
    })
}

#[must_use]
pub fn estimate_span_cost(record: &DurableRecordV1, table: Option<&RateTable>) -> CostEstimateV1 {
    let Some(table) = table else {
        return unknown_cost("missing_rate_table");
    };
    let model = record.agent.model.clone();
    let Some(model_name) = model.as_deref() else {
        return cost_result(
            table,
            "unknown",
            Some("missing_model"),
            model,
            None,
            BTreeMap::new(),
            vec![],
            vec![],
        );
    };
    let Some(rates) = table.models.get(model_name) else {
        return cost_result(
            table,
            "unknown",
            Some("missing_model_rate"),
            model,
            None,
            BTreeMap::new(),
            vec![],
            vec![],
        );
    };
    let (mut tokens, semantic_errors) = billable_metrics(&record.metrics, rates);
    let mut missing = Vec::new();
    let mut components = BTreeMap::new();
    let mut amount = 0.0;
    for key in TOKEN_KEYS {
        let Some(token_count) = tokens.remove(key) else {
            continue;
        };
        if token_count == 0.0 {
            continue;
        }
        let rate = rate_for(rates, key);
        let Some(rate) = rate else {
            missing.push(key.into());
            continue;
        };
        let component_cost = token_count / 1_000_000.0 * rate;
        components.insert(
            key.into(),
            CostComponentV1 {
                tokens: token_count,
                rate_per_1m: rate,
                estimated_cost: round_currency(component_cost),
            },
        );
        amount += component_cost;
    }
    if components.is_empty() && missing.is_empty() && semantic_errors.is_empty() {
        return cost_result(
            table,
            "unknown",
            Some("missing_token_metrics"),
            model,
            None,
            components,
            missing,
            semantic_errors,
        );
    }
    let status = if missing.is_empty() && semantic_errors.is_empty() {
        "estimated"
    } else {
        "incomplete"
    };
    let reason = if !semantic_errors.is_empty() {
        Some("ambiguous_token_semantics")
    } else if !missing.is_empty() {
        Some("missing_token_rates")
    } else {
        None
    };
    cost_result(
        table,
        status,
        reason,
        model,
        Some(round_currency(amount)),
        components,
        missing,
        semantic_errors,
    )
}

#[must_use]
pub fn estimate_cost_for_records(
    records: &[DurableRecordV1],
    table: Option<&RateTable>,
) -> CostEstimateV1 {
    let Some(table) = table else {
        return unknown_cost("missing_rate_table");
    };
    let billable: Vec<_> = records
        .iter()
        .filter(|record| has_token_metrics(&record.metrics))
        .collect();
    if billable.is_empty() {
        return cost_result(
            table,
            "unknown",
            Some("missing_token_metrics"),
            None,
            None,
            BTreeMap::new(),
            vec![],
            vec![],
        );
    }
    let costs: Vec<_> = billable
        .into_iter()
        .map(|record| estimate_span_cost(record, Some(table)))
        .collect();
    let aggregate = aggregate_costs(&costs);
    CostEstimateV1 {
        status: aggregate.status.into(),
        reason: None,
        estimated_cost: Some(round_currency(aggregate.amount)),
        currency: Some(table.currency.clone()),
        model: None,
        rate_table: table_ref(table),
        cost: CostDetailV1 {
            assumption: table.assumption.clone(),
            incomplete_count: Some(aggregate.incomplete),
            unknown_count: Some(aggregate.unknown),
            ..CostDetailV1::default()
        },
    }
}

fn estimate_cost_for_spans(spans: &[ReportSpanV2], table: Option<&RateTable>) -> CostEstimateV1 {
    let Some(table) = table else {
        return unknown_cost("missing_rate_table");
    };
    let costs = spans
        .iter()
        .filter(|span| has_report_token_metrics(&span.metrics))
        .map(|span| &span.cost);
    let aggregate = aggregate_cost_refs(costs);
    if aggregate.count == 0 {
        return cost_result(
            table,
            "unknown",
            Some("missing_token_metrics"),
            None,
            None,
            BTreeMap::new(),
            vec![],
            vec![],
        );
    }
    aggregate_report_costs(table, &aggregate)
}

fn has_report_token_metrics(metrics: &ReportMetricsV1) -> bool {
    [
        metrics.input_tokens,
        metrics.output_tokens,
        metrics.cached_input_tokens,
        metrics.cache_creation_input_tokens,
        metrics.reasoning_output_tokens,
    ]
    .into_iter()
    .any(|value| metric(value).is_some())
}

fn aggregate_report_costs(table: &RateTable, aggregate: &AggregateCost) -> CostEstimateV1 {
    CostEstimateV1 {
        status: aggregate.status.into(),
        reason: None,
        estimated_cost: Some(round_currency(aggregate.amount)),
        currency: Some(table.currency.clone()),
        model: None,
        rate_table: table_ref(table),
        cost: CostDetailV1 {
            assumption: table.assumption.clone(),
            incomplete_count: Some(aggregate.incomplete),
            unknown_count: Some(aggregate.unknown),
            ..CostDetailV1::default()
        },
    }
}

struct AggregateCost {
    count: u64,
    status: &'static str,
    amount: f64,
    incomplete: u64,
    unknown: u64,
}

fn aggregate_costs(costs: &[CostEstimateV1]) -> AggregateCost {
    aggregate_cost_refs(costs.iter())
}

fn aggregate_cost_refs<'a>(costs: impl Iterator<Item = &'a CostEstimateV1>) -> AggregateCost {
    let mut count = 0_u64;
    let mut incomplete = 0_u64;
    let mut unknown = 0_u64;
    let mut estimated = 0_u64;
    let mut amount = 0.0;
    for cost in costs {
        count = count.saturating_add(1);
        incomplete = incomplete.saturating_add(u64::from(cost.status == "incomplete"));
        unknown = unknown.saturating_add(u64::from(cost.status == "unknown"));
        estimated = estimated.saturating_add(u64::from(cost.status == "estimated"));
        amount += cost.estimated_cost.unwrap_or(0.0);
    }
    let status = if estimated == 0 && incomplete == 0 {
        "unknown"
    } else if incomplete > 0 || unknown > 0 {
        "incomplete"
    } else {
        "estimated"
    };
    AggregateCost {
        count,
        status,
        amount,
        incomplete,
        unknown,
    }
}

fn normalize_rate(rate: Option<f64>, label: &str) -> Result<Option<f64>, PricingError> {
    match rate {
        None => Ok(None),
        Some(value) if value.is_finite() && value >= 0.0 => Ok(Some(value)),
        Some(_) => Err(PricingError::InvalidRate(label.into())),
    }
}
fn normalize_semantics(
    input: BTreeMap<String, String>,
) -> Result<BTreeMap<String, TokenSemantics>, PricingError> {
    input
        .into_iter()
        .map(|(key, value)| {
            if !OVERLAP_KEYS.contains(&key.as_str()) {
                return Err(PricingError::UnsupportedSemantic(key));
            }
            let semantic = match value.as_str() {
                "exclusive" => TokenSemantics::Exclusive,
                "included_in_total" => TokenSemantics::IncludedInTotal,
                _ => return Err(PricingError::UnsupportedSemantic(key)),
            };
            Ok((key, semantic))
        })
        .collect()
}
fn nonempty_or(value: Option<String>, default: &str) -> String {
    value
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.into())
}
fn rate_for(rates: &ModelRates, key: &str) -> Option<f64> {
    match key {
        "input_tokens" => rates.input_tokens,
        "output_tokens" => rates.output_tokens,
        "cached_input_tokens" => rates.cached_input_tokens,
        "cache_creation_input_tokens" => rates.cache_creation_input_tokens,
        "reasoning_output_tokens" => rates.reasoning_output_tokens,
        _ => None,
    }
}
fn metric(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}
fn billable_metrics(
    metrics: &MetricsV1,
    rates: &ModelRates,
) -> (BTreeMap<&'static str, f64>, Vec<String>) {
    let mut values = BTreeMap::from([
        ("input_tokens", metric(metrics.input_tokens)),
        ("output_tokens", metric(metrics.output_tokens)),
        ("cached_input_tokens", metric(metrics.cached_input_tokens)),
        (
            "cache_creation_input_tokens",
            metric(metrics.cache_creation_input_tokens),
        ),
        (
            "reasoning_output_tokens",
            metric(metrics.reasoning_output_tokens),
        ),
    ])
    .into_iter()
    .filter_map(|(key, value)| value.map(|value| (key, value)))
    .collect::<BTreeMap<_, _>>();
    let mut errors = Vec::new();
    for (total, breakdowns) in [
        (
            "input_tokens",
            ["cached_input_tokens", "cache_creation_input_tokens"].as_slice(),
        ),
        ("output_tokens", ["reasoning_output_tokens"].as_slice()),
    ] {
        let mut included = Vec::new();
        let present: Vec<_> = breakdowns
            .iter()
            .copied()
            .filter(|key| values.contains_key(*key))
            .collect();
        for key in present {
            match rates.token_semantics.get(key) {
                Some(TokenSemantics::IncludedInTotal) => included.push(key),
                Some(TokenSemantics::Exclusive) => {}
                None => {
                    errors.push(format!("{key}:missing_semantics"));
                    values.remove(key);
                }
            }
        }
        if included.is_empty() {
            continue;
        }
        let included_total: f64 = included.iter().map(|key| values[key]).sum();
        let Some(total_value) = values.get_mut(total) else {
            errors.push(format!("{total}:invalid_included_breakdown"));
            for key in included {
                values.remove(key);
            }
            continue;
        };
        if included_total > *total_value {
            errors.push(format!("{total}:invalid_included_breakdown"));
            for key in included {
                values.remove(key);
            }
        } else {
            *total_value -= included_total;
        }
    }
    (values, errors)
}
fn table_ref(table: &RateTable) -> RateTableRefV1 {
    RateTableRefV1 {
        version: Some(table.version.clone()),
        unit: Some("per_1m_tokens".into()),
    }
}
fn unknown_cost(reason: &str) -> CostEstimateV1 {
    CostEstimateV1 {
        status: "unknown".into(),
        reason: Some(reason.into()),
        estimated_cost: None,
        currency: None,
        model: None,
        rate_table: RateTableRefV1::default(),
        cost: CostDetailV1 {
            assumption: NO_TABLE_ASSUMPTION.into(),
            ..CostDetailV1::default()
        },
    }
}
#[allow(clippy::too_many_arguments)]
fn cost_result(
    table: &RateTable,
    status: &str,
    reason: Option<&str>,
    model: Option<String>,
    amount: Option<f64>,
    components: BTreeMap<String, CostComponentV1>,
    missing: Vec<String>,
    semantic_errors: Vec<String>,
) -> CostEstimateV1 {
    CostEstimateV1 {
        status: status.into(),
        reason: reason.map(str::to_owned),
        estimated_cost: amount,
        currency: Some(table.currency.clone()),
        model,
        rate_table: table_ref(table),
        cost: CostDetailV1 {
            assumption: table.assumption.clone(),
            missing,
            semantic_errors,
            components,
            ..CostDetailV1::default()
        },
    }
}
fn has_token_metrics(metrics: &MetricsV1) -> bool {
    [
        metrics.input_tokens,
        metrics.output_tokens,
        metrics.cached_input_tokens,
        metrics.cache_creation_input_tokens,
        metrics.reasoning_output_tokens,
    ]
    .into_iter()
    .any(|value| metric(value).is_some())
}
fn round_currency(value: f64) -> f64 {
    (value * 1_000_000_000.0).round() / 1_000_000_000.0
}

#[cfg(test)]
mod tests;
