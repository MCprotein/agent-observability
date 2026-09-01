use agent_observability_domain::{
    CorrelationIds, DomainSpanState, LifecycleState, ObservationId, SourceCursor, SourceGeneration,
    SpanId, SpanKind, StatusCode, Timing, TokenUsage, TraceId,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

pub const CONTRACT_MANIFEST: &str = include_str!("../../../contracts/contract-manifest.v1");
pub const DURABLE_RECORD_SCHEMA: &str =
    include_str!("../../../contracts/durable-record-v1.schema.json");
pub const REPORT_DTO_SCHEMA: &str = include_str!("../../../contracts/report-dto-v1.schema.json");
pub const RATE_TABLE_SCHEMA: &str = include_str!("../../../contracts/rate-table-v1.schema.json");
pub const RETENTION_ARCHIVE_SCHEMA: &str =
    include_str!("../../../contracts/retention-archive-entry-v1.schema.json");
pub const LOCAL_RUNTIME_CONFIG_SCHEMA: &str =
    include_str!("../../../contracts/local-runtime-config-v2.schema.json");
pub const ADAPTER_CAPABILITY_V1: &str = include_str!("../capabilities/adapter-capability-v1.yaml");
pub const DURABLE_RECORD_VERSION: &str = "agent_observability.v1";
pub const REPORT_DTO_VERSION: &str = "agent_observability.report.v1";
pub const RETENTION_ARCHIVE_VERSION: &str = "agent_observability.retention_archive.v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AdapterCapabilityManifestV1 {
    pub schema_version: String,
    pub entries: Vec<AdapterCapabilityEntryV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AdapterCapabilityEntryV1 {
    pub adapter_family: String,
    pub support_status: String,
    pub platforms: Vec<String>,
    pub profiles: Vec<String>,
    pub ingest_boundary: String,
    pub product_versions: ProductVersionRangeV1,
    pub verified_at: String,
    pub official_references: Vec<String>,
    pub surfaces: Vec<AdapterSurfaceV1>,
    pub correlation_keys: Vec<String>,
    pub privacy: AdapterPrivacyV1,
    pub known_gaps: Vec<String>,
    pub fixture_ids: Vec<String>,
    pub fixture_hashes: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProductVersionRangeV1 {
    pub oldest: String,
    pub newest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AdapterSurfaceV1 {
    pub id: String,
    pub role: String,
    pub events: Vec<String>,
    pub owned_fields: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AdapterPrivacyV1 {
    pub content_fields_accepted: bool,
    pub raw_identifiers_durable: bool,
}

impl AdapterCapabilityManifestV1 {
    /// Parses the JSON-compatible YAML document and validates its closed invariants.
    ///
    /// # Errors
    ///
    /// Returns [`ContractError::InvalidCapabilityManifest`] for malformed content, duplicate
    /// ownership, unsupported roles, missing evidence, or privacy-open settings.
    pub fn parse_and_validate(input: &str) -> Result<Self, ContractError> {
        let manifest: Self =
            serde_json::from_str(input).map_err(|_| ContractError::InvalidCapabilityManifest)?;
        if manifest.schema_version != "adapter_capability.v1" || manifest.entries.is_empty() {
            return Err(ContractError::InvalidCapabilityManifest);
        }
        for entry in &manifest.entries {
            validate_capability_entry(entry)?;
        }
        Ok(manifest)
    }
}

fn validate_capability_entry(entry: &AdapterCapabilityEntryV1) -> Result<(), ContractError> {
    if entry.adapter_family.is_empty()
        || !matches!(entry.support_status.as_str(), "experimental" | "supported")
        || entry.platforms != ["macos"]
        || entry.profiles != ["standalone"]
        || entry.ingest_boundary != "private_canonical_handoff_v1"
        || entry.product_versions.oldest.is_empty()
        || entry.product_versions.newest.is_empty()
        || entry.verified_at.is_empty()
        || entry.official_references.is_empty()
        || entry.surfaces.is_empty()
        || entry.correlation_keys.is_empty()
        || entry.fixture_ids.is_empty()
        || entry.fixture_hashes.len() < 2
        || entry.privacy.content_fields_accepted
        || entry.privacy.raw_identifiers_durable
    {
        return Err(ContractError::InvalidCapabilityManifest);
    }
    if entry
        .fixture_hashes
        .values()
        .any(|digest| digest.len() != 71 || !digest.starts_with("sha256:"))
    {
        return Err(ContractError::InvalidCapabilityManifest);
    }
    let mut ownership = BTreeSet::new();
    let mut primary_count = 0_u8;
    for surface in &entry.surfaces {
        if surface.id.is_empty() || surface.events.is_empty() || surface.owned_fields.is_empty() {
            return Err(ContractError::InvalidCapabilityManifest);
        }
        match surface.role.as_str() {
            "primary" => primary_count = primary_count.saturating_add(1),
            "supplement" => {}
            _ => return Err(ContractError::InvalidCapabilityManifest),
        }
        for field in &surface.owned_fields {
            if field.is_empty() || !ownership.insert(field) {
                return Err(ContractError::InvalidCapabilityManifest);
            }
        }
    }
    if primary_count != 1 {
        return Err(ContractError::InvalidCapabilityManifest);
    }
    Ok(())
}

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

mod span_kind_serde {
    use super::SpanKind;
    use serde::{Deserialize, Deserializer, Serializer};

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn serialize<S>(value: &SpanKind, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match value {
            SpanKind::Workstream => "workstream",
            SpanKind::AgentSession => "agent.session",
            SpanKind::Turn => "turn",
            SpanKind::LlmRequest => "llm.request",
            SpanKind::ToolExecution => "tool.execution",
            SpanKind::Permission => "permission",
            SpanKind::Compaction => "compaction",
        })
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SpanKind, D::Error>
    where
        D: Deserializer<'de>,
    {
        match String::deserialize(deserializer)?.as_str() {
            "workstream" => Ok(SpanKind::Workstream),
            "agent.session" => Ok(SpanKind::AgentSession),
            "turn" => Ok(SpanKind::Turn),
            "llm.request" => Ok(SpanKind::LlmRequest),
            "tool.execution" => Ok(SpanKind::ToolExecution),
            "permission" => Ok(SpanKind::Permission),
            "compaction" => Ok(SpanKind::Compaction),
            value => Err(serde::de::Error::unknown_variant(
                value,
                &[
                    "workstream",
                    "agent.session",
                    "turn",
                    "llm.request",
                    "tool.execution",
                    "permission",
                    "compaction",
                ],
            )),
        }
    }
}

mod status_code_serde {
    use super::StatusCode;
    use serde::{Deserialize, Deserializer, Serializer};

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn serialize<S>(value: &StatusCode, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match value {
            StatusCode::Unset => "unset",
            StatusCode::Ok => "ok",
            StatusCode::Error => "error",
        })
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<StatusCode, D::Error>
    where
        D: Deserializer<'de>,
    {
        match String::deserialize(deserializer)?.as_str() {
            "unset" => Ok(StatusCode::Unset),
            "ok" => Ok(StatusCode::Ok),
            "error" => Ok(StatusCode::Error),
            value => Err(serde::de::Error::unknown_variant(
                value,
                &["unset", "ok", "error"],
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentSource {
    Codex,
    ClaudeCode,
    Cursor,
}

impl AgentSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
            Self::Cursor => "cursor",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterDispositionKind {
    Diagnostic,
    Suppressed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterDispositionCode {
    UnsupportedEvent,
    UnsupportedEventVariant,
    MissingCorrelation,
    InvalidFieldType,
    ContentEventIgnored,
    PrimarySuperseded,
    DuplicateObservation,
}

impl AdapterDispositionCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedEvent => "unsupported_event",
            Self::UnsupportedEventVariant => "unsupported_event_variant",
            Self::MissingCorrelation => "missing_correlation",
            Self::InvalidFieldType => "invalid_field_type",
            Self::ContentEventIgnored => "content_event_ignored",
            Self::PrimarySuperseded => "primary_superseded",
            Self::DuplicateObservation => "duplicate_observation",
        }
    }
}

impl AdapterDispositionKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Diagnostic => "diagnostic",
            Self::Suppressed => "suppressed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceCheckpoint {
    pub source: AgentSource,
    pub source_generation: SourceGeneration,
    pub previous_source_cursor: Option<SourceCursor>,
    pub source_cursor: SourceCursor,
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
    pub previous_source_cursor: Option<SourceCursor>,
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

/// Projects transient source data and domain state into the durable, privacy-safe contract.
///
/// Only identifiers, lifecycle metadata, bounded classifications, and token counts cross this
/// boundary. Source payloads are intentionally not represented by [`SourceObservation`].
///
/// # Errors
///
/// Returns [`ContractError`] when identity fields disagree or a numeric value cannot be projected
/// exactly into the JSON number contract.
pub fn project_durable_record(
    observation: &SourceObservation,
    state: &DomainSpanState,
) -> Result<DurableRecordV1, ContractError> {
    if observation.trace_id != state.trace_id
        || observation.span_id != state.span_id
        || observation.parent_span_id != state.parent_span_id
        || state.kind != kind_for_event(&observation.event)
    {
        return Err(ContractError::ProjectionIdentityMismatch);
    }

    let (_, name, model, project) = event_projection(&observation.event);
    let attributes = project_attributes(observation, state);
    let record = DurableRecordV1 {
        schema_version: DURABLE_RECORD_VERSION.into(),
        record_type: "span".into(),
        trace_id: observation.trace_id.as_str().into(),
        span_id: observation.span_id.as_str().into(),
        parent_span_id: observation
            .parent_span_id
            .as_ref()
            .map(|id| id.as_str().into()),
        span_kind: state.kind,
        name: name.into(),
        start_time_unix_ms: exact_json_integer(state.timing.start_unix_ms)?,
        end_time_unix_ms: state
            .timing
            .end_unix_ms
            .map(exact_json_integer)
            .transpose()?,
        status: StatusV1 {
            code: status_for_lifecycle(state.lifecycle),
        },
        agent: AgentV1 {
            name: Some(source_name(observation.source).into()),
            version: None,
            model: model.map(str::to_owned),
        },
        project: ProjectV1 {
            name: project.map(str::to_owned),
            repo_path: None,
        },
        attributes,
        metrics: metrics_from_usage(&state.token_usage)?,
        content: ContentV1::default(),
        redaction: RedactionV1 {
            applied: true,
            count: 4,
            fields: vec![
                "prompt".into(),
                "output".into(),
                "tool_input".into(),
                "tool_output".into(),
            ],
        },
    };
    record.validate()?;
    Ok(record)
}

/// Returns a stable hash of the privacy-safe durable projection for one source observation.
///
/// # Errors
///
/// Returns [`ContractError`] when the observation cannot be projected or sanitized.
pub fn canonical_observation_payload_hash(
    observation: &SourceObservation,
) -> Result<String, ContractError> {
    let state = DomainSpanState {
        trace_id: observation.trace_id.clone(),
        span_id: observation.span_id.clone(),
        parent_span_id: observation.parent_span_id.clone(),
        kind: kind_for_event(&observation.event),
        lifecycle: observation.lifecycle,
        correlation: observation.correlation.clone(),
        timing: observation.timing,
        token_usage: observation.token_usage,
    };
    let record = sanitize_durable_record(&project_durable_record(observation, &state)?)?;
    let encoded = serde_json::to_vec(&record).map_err(|_| ContractError::InvalidDurableHeader)?;
    let mut hash = Sha256::new();
    hash.update(encoded);
    let mut output = String::from("sha256:");
    for byte in hash.finalize() {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    Ok(output)
}

fn project_attributes(observation: &SourceObservation, state: &DomainSpanState) -> AttributesV1 {
    let (event_type, _, _, _) = event_projection(&observation.event);
    let mut attributes = AttributesV1 {
        source: Some(ScalarValueV1::String(
            source_name(observation.source).into(),
        )),
        event_type: Some(ScalarValueV1::String(event_type.into())),
        session_id: scalar_id(
            state
                .correlation
                .session_id
                .as_ref()
                .map(agent_observability_domain::SessionId::as_str),
        ),
        turn_id: scalar_id(
            state
                .correlation
                .turn_id
                .as_ref()
                .map(agent_observability_domain::TurnId::as_str),
        ),
        request_id: scalar_id(
            state
                .correlation
                .request_id
                .as_ref()
                .map(agent_observability_domain::RequestId::as_str),
        ),
        call_id: scalar_id(
            state
                .correlation
                .operation_id
                .as_ref()
                .map(agent_observability_domain::OperationId::as_str),
        ),
        permission_id: scalar_id(
            state
                .correlation
                .permission_id
                .as_ref()
                .map(agent_observability_domain::PermissionId::as_str),
        ),
        compaction_id: scalar_id(
            state
                .correlation
                .compaction_id
                .as_ref()
                .map(agent_observability_domain::CompactionId::as_str),
        ),
        ..AttributesV1::default()
    };
    match &observation.event {
        ObservationEvent::ToolOperation { tool_name, phase } => {
            attributes.tool_name = scalar_string(tool_name.as_deref());
            attributes.phase = scalar_string(phase.as_deref());
        }
        ObservationEvent::Permission { decision } => {
            attributes.decision = scalar_string(decision.as_deref());
        }
        ObservationEvent::Compaction { trigger } => {
            attributes.trigger = scalar_string(trigger.as_deref());
        }
        ObservationEvent::Session { .. }
        | ObservationEvent::Turn
        | ObservationEvent::ModelRequest { .. } => {}
    }
    attributes
}

impl DurableRecordV1 {
    /// Projects an observation through the closed privacy boundary.
    ///
    /// # Errors
    ///
    /// Returns [`ContractError`] when the observation and reduced state disagree or cannot be
    /// represented by the durable JSON contract.
    pub fn from_observation(
        observation: &SourceObservation,
        state: &DomainSpanState,
    ) -> Result<Self, ContractError> {
        project_durable_record(observation, state)
    }
}

fn scalar_id(value: Option<&str>) -> Option<ScalarValueV1> {
    value.map(|id| ScalarValueV1::String(id.into()))
}

/// Returns the stable privacy projection for an opaque identifier.
#[must_use]
pub fn hash_opaque_identifier(value: &str) -> String {
    const PREFIX: &str = "id:sha256:";
    if value.starts_with(PREFIX) {
        return value.into();
    }
    let mut hash = Sha256::new();
    hash.update(value.as_bytes());
    let mut output = String::from(PREFIX);
    for byte in hash.finalize() {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

/// Applies the mandatory durable privacy boundary to a v1 record.
///
/// # Errors
///
/// Returns [`ContractError`] when the sanitized record violates the closed wire contract.
pub fn sanitize_durable_record(input: &DurableRecordV1) -> Result<DurableRecordV1, ContractError> {
    let mut record = input.clone();
    let mut fields = Vec::new();
    hash_record_id(&mut record.trace_id, "trace_id", &mut fields);
    hash_record_id(&mut record.span_id, "span_id", &mut fields);
    if let Some(parent) = &mut record.parent_span_id {
        hash_record_id(parent, "parent_span_id", &mut fields);
    }
    sanitize_optional_text(&mut record.agent.name, "agent.name", &mut fields);
    sanitize_optional_text(&mut record.agent.version, "agent.version", &mut fields);
    sanitize_optional_text(&mut record.agent.model, "agent.model", &mut fields);
    sanitize_optional_text(&mut record.project.name, "project.name", &mut fields);
    sanitize_optional_text(
        &mut record.project.repo_path,
        "project.repo_path",
        &mut fields,
    );
    sanitize_attributes(&mut record.attributes, &mut fields);
    let safe_name = redact_sensitive_text(&record.name, "name");
    if safe_name != record.name {
        record.name = safe_name;
        fields.push("name".into());
    }
    for (present, field) in [
        (record.content.prompt.is_some(), "content.prompt"),
        (record.content.output.is_some(), "content.output"),
        (record.content.tool_input.is_some(), "content.tool_input"),
        (record.content.tool_output.is_some(), "content.tool_output"),
    ] {
        if present {
            fields.push(field.into());
        }
    }
    record.content = ContentV1::default();
    record.redaction.applied = record.redaction.applied || !fields.is_empty();
    record.redaction.count = record
        .redaction
        .count
        .saturating_add(u64::try_from(fields.len()).unwrap_or(u64::MAX));
    record.redaction.fields.extend(fields);
    record.redaction.fields.sort();
    record.redaction.fields.dedup();
    record.validate()?;
    Ok(record)
}

fn hash_record_id(value: &mut String, field: &str, fields: &mut Vec<String>) {
    let hashed = hash_opaque_identifier(value);
    if hashed != *value {
        *value = hashed;
        fields.push(field.into());
    }
}

fn sanitize_optional_text(value: &mut Option<String>, key: &str, fields: &mut Vec<String>) {
    let Some(current) = value else {
        return;
    };
    let safe = redact_sensitive_text(current, key);
    if safe != *current {
        *current = safe;
        fields.push(key.into());
    }
}

fn sanitize_scalar(value: &mut Option<ScalarValueV1>, key: &str, fields: &mut Vec<String>) {
    let Some(ScalarValueV1::String(current)) = value else {
        return;
    };
    let safe = if matches!(
        key,
        "session_id" | "turn_id" | "request_id" | "call_id" | "permission_id" | "compaction_id"
    ) {
        hash_opaque_identifier(current)
    } else {
        redact_sensitive_text(current, key)
    };
    if safe != *current {
        *current = safe;
        fields.push(format!("attributes.{key}"));
    }
}

fn sanitize_attributes(attributes: &mut AttributesV1, fields: &mut Vec<String>) {
    sanitize_scalar(&mut attributes.source, "source", fields);
    sanitize_scalar(&mut attributes.event_type, "event_type", fields);
    sanitize_scalar(&mut attributes.envelope_type, "envelope_type", fields);
    sanitize_scalar(&mut attributes.session_id, "session_id", fields);
    sanitize_scalar(&mut attributes.turn_id, "turn_id", fields);
    sanitize_scalar(&mut attributes.request_id, "request_id", fields);
    sanitize_scalar(&mut attributes.call_id, "call_id", fields);
    sanitize_scalar(&mut attributes.tool_name, "tool_name", fields);
    sanitize_scalar(&mut attributes.phase, "phase", fields);
    sanitize_scalar(&mut attributes.exit_code, "exit_code", fields);
    sanitize_scalar(&mut attributes.sandbox, "sandbox", fields);
    sanitize_scalar(&mut attributes.approval, "approval", fields);
    sanitize_scalar(&mut attributes.permission_id, "permission_id", fields);
    sanitize_scalar(&mut attributes.decision, "decision", fields);
    sanitize_scalar(&mut attributes.command_kind, "command_kind", fields);
    sanitize_scalar(&mut attributes.compaction_id, "compaction_id", fields);
    sanitize_scalar(&mut attributes.trigger, "trigger", fields);
}

/// Redacts secret-bearing or path-bearing free text without retaining the sensitive fragment.
#[must_use]
pub fn redact_sensitive_text(value: &str, key: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let normalized_whitespace = lower.split_whitespace().collect::<Vec<_>>().join(" ");
    let key = key.to_ascii_lowercase();
    let sensitive_key = [
        "authorization",
        "cookie",
        "credential",
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "access_key",
        "private_key",
        "refresh_token",
        "session_key",
    ]
    .iter()
    .any(|candidate| key.contains(candidate));
    let secret_assignment = [
        "authorization",
        "cookie",
        "password",
        "passwd",
        "secret",
        "token",
        "id_token",
        "api_key",
        "api-key",
        "access_token",
        "refresh_token",
    ]
    .iter()
    .any(|candidate| lower.contains(candidate))
        && (lower.contains('=') || lower.contains(':'));
    if sensitive_key
        || secret_assignment
        || value.chars().any(char::is_control)
        || [
            "authorization:",
            "authorization=",
            "cookie:",
            "cookie=",
            "password=",
            "password:",
            "passwd=",
            "secret=",
            "secret:",
            "api_key=",
            "api-key=",
            "access_token=",
            "refresh_token=",
            "private key",
        ]
        .iter()
        .any(|pattern| lower.contains(pattern))
        || normalized_whitespace.contains("bearer ")
    {
        return "[redacted]".into();
    }
    let sensitive_extension = std::path::Path::new(&lower)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| matches!(extension, "pem" | "key"));
    if lower == ".env"
        || lower.starts_with(".env.")
        || lower.contains("/.env")
        || sensitive_extension
        || lower.contains(".pem ")
        || lower.contains(".key ")
        || lower.contains(".tfstate")
        || lower.contains(".tfvars")
        || (key.contains("path") && (value.contains('/') || value.contains('\\')))
    {
        return "[redacted path]".into();
    }
    value.into()
}

fn scalar_string(value: Option<&str>) -> Option<ScalarValueV1> {
    value.map(|value| ScalarValueV1::String(value.into()))
}

fn source_name(source: AgentSource) -> &'static str {
    match source {
        AgentSource::Codex => "codex",
        AgentSource::ClaudeCode => "claude-code",
        AgentSource::Cursor => "cursor",
    }
}

fn event_projection(
    event: &ObservationEvent,
) -> (&'static str, &'static str, Option<&str>, Option<&str>) {
    match event {
        ObservationEvent::Session { model, project } => {
            ("session", "session", model.as_deref(), project.as_deref())
        }
        ObservationEvent::Turn => ("turn", "turn", None, None),
        ObservationEvent::ModelRequest { model } => {
            ("model_request", "llm.request", model.as_deref(), None)
        }
        ObservationEvent::ToolOperation { .. } => ("tool_operation", "tool.execution", None, None),
        ObservationEvent::Permission { .. } => ("permission", "permission", None, None),
        ObservationEvent::Compaction { .. } => ("compaction", "compaction", None, None),
    }
}

fn kind_for_event(event: &ObservationEvent) -> SpanKind {
    match event {
        ObservationEvent::Session { .. } => SpanKind::AgentSession,
        ObservationEvent::Turn => SpanKind::Turn,
        ObservationEvent::ModelRequest { .. } => SpanKind::LlmRequest,
        ObservationEvent::ToolOperation { .. } => SpanKind::ToolExecution,
        ObservationEvent::Permission { .. } => SpanKind::Permission,
        ObservationEvent::Compaction { .. } => SpanKind::Compaction,
    }
}

fn status_for_lifecycle(lifecycle: LifecycleState) -> StatusCode {
    match lifecycle {
        LifecycleState::Observed | LifecycleState::Running => StatusCode::Unset,
        LifecycleState::Completed => StatusCode::Ok,
        LifecycleState::Failed | LifecycleState::Interrupted => StatusCode::Error,
    }
}

fn metrics_from_usage(usage: &TokenUsage) -> Result<MetricsV1, ContractError> {
    Ok(MetricsV1 {
        input_tokens: usage.input.map(exact_json_integer).transpose()?,
        output_tokens: usage.output.map(exact_json_integer).transpose()?,
        cached_input_tokens: usage.cached_input.map(exact_json_integer).transpose()?,
        cache_creation_input_tokens: usage
            .cache_creation_input
            .map(exact_json_integer)
            .transpose()?,
        reasoning_output_tokens: usage.reasoning_output.map(exact_json_integer).transpose()?,
        total_tokens: usage.total.map(exact_json_integer).transpose()?,
        total_input_tokens: usage.total_input.map(exact_json_integer).transpose()?,
        total_output_tokens: usage.total_output.map(exact_json_integer).transpose()?,
        total_cached_input_tokens: usage
            .total_cached_input
            .map(exact_json_integer)
            .transpose()?,
        total_reasoning_output_tokens: usage
            .total_reasoning_output
            .map(exact_json_integer)
            .transpose()?,
        total_accumulated_tokens: usage
            .total_accumulated
            .map(exact_json_integer)
            .transpose()?,
        context_window_tokens: usage.context_window.map(exact_json_integer).transpose()?,
        input_tokens_before: usage.input_before.map(exact_json_integer).transpose()?,
        input_tokens_after: usage.input_after.map(exact_json_integer).transpose()?,
        ..MetricsV1::default()
    })
}

fn exact_json_integer(value: u64) -> Result<f64, ContractError> {
    const MAX_EXACT_JSON_INTEGER: u64 = (1_u64 << f64::MANTISSA_DIGITS) - 1;
    if value > MAX_EXACT_JSON_INTEGER {
        return Err(ContractError::IntegerPrecisionLoss);
    }
    #[allow(clippy::cast_precision_loss)]
    let projected = value as f64;
    Ok(projected)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum JsonValue {
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ScalarValueV1 {
    Boolean(bool),
    Number(f64),
    String(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StatusV1 {
    #[serde(with = "status_code_serde")]
    pub code: StatusCode,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AttributesV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub envelope_type: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_id: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_kind: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compaction_id: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<ScalarValueV1>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_output_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_input_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_output_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_cached_input_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_reasoning_output_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_accumulated_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens_before: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens_after: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<JsonValue>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RedactionV1 {
    pub applied: bool,
    pub count: u64,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DurableRecordV1 {
    pub schema_version: String,
    pub record_type: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    #[serde(with = "span_kind_serde")]
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReportFiltersV1 {
    pub repos: Vec<String>,
    pub sessions: Vec<String>,
    pub turns: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub models: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CostComponentV1 {
    pub tokens: f64,
    pub rate_per_1m: f64,
    pub estimated_cost: f64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CostDetailV1 {
    pub assumption: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incomplete_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unknown_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub semantic_errors: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub components: BTreeMap<String, CostComponentV1>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CostEstimateV1 {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub rate_table: RateTableRefV1,
    pub cost: CostDetailV1,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RateTableRefV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ReportMetricsV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_output_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_input_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_output_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_cached_input_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_reasoning_output_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_accumulated_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window_tokens: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReportAgentV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReportAttributesV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub envelope_type: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<ScalarValueV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<ScalarValueV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ReportSpanV1 {
    pub schema_version: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    #[serde(with = "span_kind_serde")]
    pub kind: SpanKind,
    pub name: String,
    #[serde(with = "status_code_serde")]
    pub status: StatusCode,
    pub start_time_unix_ms: f64,
    pub end_time_unix_ms: Option<f64>,
    pub repo: String,
    pub agent: ReportAgentV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    pub attributes: ReportAttributesV1,
    pub metrics: ReportMetricsV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost: Option<f64>,
    pub cost: CostEstimateV1,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ReportDtoV1 {
    pub schema_version: String,
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
        self.expect("rate_table", "agent_observability.rate_table.v1")?;
        self.expect(
            "retention_archive",
            "agent_observability.retention_archive.v1",
        )?;
        self.expect("local_runtime_config", "local_runtime.v2")?;
        self.expect("durable_schema", "contracts/durable-record-v1.schema.json")?;
        self.expect("report_schema", "contracts/report-dto-v1.schema.json")?;
        self.expect("rate_table_schema", "contracts/rate-table-v1.schema.json")?;
        self.expect(
            "retention_archive_schema",
            "contracts/retention-archive-entry-v1.schema.json",
        )?;
        self.expect(
            "local_runtime_config_schema",
            "contracts/local-runtime-config-v2.schema.json",
        )?;
        self.expect(
            "local_runtime_config_fixture",
            "contracts/local-runtime-config-v2.fixture.json",
        )?;
        self.expect(
            "local_runtime_config_parity",
            "contracts/local-runtime-config-v2.parity.json",
        )?;
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
    ProjectionIdentityMismatch,
    IntegerPrecisionLoss,
    EndBeforeStart,
    InvalidDurableHeader,
    MissingDurableIdentity,
    InvalidReportVersion,
    EmptyOptionalString,
    InvalidCostStatus,
    NonFiniteNumber,
    NegativeMetric,
    InvalidCapabilityManifest,
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
            Self::ProjectionIdentityMismatch => {
                formatter.write_str("source observation and domain state do not match")
            }
            Self::IntegerPrecisionLoss => {
                formatter.write_str("integer cannot be represented exactly by the JSON contract")
            }
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
            Self::InvalidCapabilityManifest => {
                formatter.write_str("invalid adapter capability manifest")
            }
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
    use super::{
        ADAPTER_CAPABILITY_V1, AdapterCapabilityManifestV1, CONTRACT_MANIFEST, ContractManifest,
        DURABLE_RECORD_SCHEMA, LOCAL_RUNTIME_CONFIG_SCHEMA, RATE_TABLE_SCHEMA, REPORT_DTO_SCHEMA,
        RETENTION_ARCHIVE_SCHEMA, redact_sensitive_text,
    };

    #[test]
    fn shared_manifest_and_closed_schemas_match_release_boundary() {
        let manifest = ContractManifest::parse(CONTRACT_MANIFEST).expect("manifest parses");
        manifest
            .validate_release_boundary()
            .expect("release boundary matches");
        for schema in [
            DURABLE_RECORD_SCHEMA,
            REPORT_DTO_SCHEMA,
            RATE_TABLE_SCHEMA,
            RETENTION_ARCHIVE_SCHEMA,
            LOCAL_RUNTIME_CONFIG_SCHEMA,
        ] {
            assert!(schema.contains("\"additionalProperties\": false"));
        }
    }

    #[test]
    fn adapter_capability_v1_has_closed_ownership_and_privacy() {
        let manifest = AdapterCapabilityManifestV1::parse_and_validate(ADAPTER_CAPABILITY_V1)
            .expect("capability manifest validates");
        let codex = manifest
            .entries
            .iter()
            .find(|entry| entry.adapter_family == "codex")
            .expect("Codex capability exists");
        assert_eq!(codex.support_status, "supported");
        assert_eq!(codex.product_versions.oldest, "0.150.1");
        assert_eq!(codex.platforms, ["macos"]);
        assert_eq!(codex.profiles, ["standalone"]);
        assert!(!codex.privacy.content_fields_accepted);
        assert!(!codex.privacy.raw_identifiers_durable);
        let claude = manifest
            .entries
            .iter()
            .find(|entry| entry.adapter_family == "claude-code")
            .expect("Claude Code capability exists");
        assert_eq!(claude.support_status, "supported");
        assert_eq!(claude.product_versions.oldest, "2.1.248");
        assert_eq!(claude.product_versions.newest, "2.1.248");
        assert!(!claude.privacy.content_fields_accepted);
        assert!(!claude.privacy.raw_identifiers_durable);
        let cursor = manifest
            .entries
            .iter()
            .find(|entry| entry.adapter_family == "cursor")
            .expect("Cursor capability exists");
        assert_eq!(cursor.support_status, "supported");
        assert!(!cursor.privacy.content_fields_accepted);
        assert!(!cursor.privacy.raw_identifiers_durable);
    }

    #[test]
    fn secret_redaction_covers_generic_tokens_cookies_and_whitespace() {
        for value in [
            "token=RAW_SECRET",
            "id_token: RAW_SECRET",
            "Cookie: session=RAW_SECRET",
            "Set-Cookie=RAW_SECRET",
            "Bearer\tRAW_SECRET",
        ] {
            assert_eq!(redact_sensitive_text(value, "tool_name"), "[redacted]");
        }
        assert_eq!(
            redact_sensitive_text("token budget", "tool_name"),
            "token budget"
        );
        assert_eq!(redact_sensitive_text("value", "auth_token"), "[redacted]");
    }
}
