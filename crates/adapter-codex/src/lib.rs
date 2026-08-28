//! Bounded, content-free Codex OTel/notify handoff adapter.

use agent_observability_contracts::{
    AdapterDispositionKind, AgentSource, ObservationEvent, SourceCheckpoint, SourceObservation,
    canonical_observation_payload_hash,
};
use agent_observability_domain::{
    CorrelationIds, LifecycleState, ObservationId, OperationId, PermissionId, RequestId, SessionId,
    SourceCursor, SourceGeneration, SpanId, Timing, TokenUsage, TraceId, TurnId,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io;
use std::path::Path;

pub const HANDOFF_SCHEMA_VERSION: &str = "codex_handoff.v1";
pub const MAX_HANDOFF_BYTES: u64 = 1024 * 1024;
pub const MAX_HANDOFF_LINES: usize = 4096;
pub const MAX_HANDOFF_LINE_BYTES: usize = 64 * 1024;
const KNOWN_CODEX_MODELS: &[&str] = &[
    "gpt-test",
    "gpt-5.4",
    "gpt-5.5",
    "gpt-5.6-luna",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceSurface {
    OtelLog,
    Notify,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HandoffRecord {
    schema_version: String,
    source_generation: String,
    previous_cursor: Option<String>,
    cursor: String,
    surface: SourceSurface,
    received_at_unix_ms: u64,
    event_name: String,
    #[serde(default)]
    attributes: BTreeMap<String, Value>,
}

pub use agent_observability_contracts::AdapterDispositionCode as DiagnosticCode;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterDiagnostic {
    pub record_index: usize,
    pub code: DiagnosticCode,
    pub checkpoint: SourceCheckpoint,
    pub disposition: AdapterDispositionKind,
    pub payload_hash: Option<String>,
}

#[derive(Debug)]
pub enum AdapterItem {
    Observation(Box<SourceObservation>),
    Disposition(AdapterDiagnostic),
}

#[derive(Debug, Default)]
pub struct AdapterBatch {
    pub items: Vec<AdapterItem>,
}

impl AdapterBatch {
    pub fn observations(&self) -> impl Iterator<Item = &SourceObservation> {
        self.items.iter().filter_map(|item| match item {
            AdapterItem::Observation(observation) => Some(observation.as_ref()),
            AdapterItem::Disposition(_) => None,
        })
    }

    pub fn diagnostics(&self) -> impl Iterator<Item = &AdapterDiagnostic> {
        self.items.iter().filter_map(|item| match item {
            AdapterItem::Observation(_) => None,
            AdapterItem::Disposition(diagnostic) => Some(diagnostic),
        })
    }
}

#[derive(Debug)]
pub enum AdapterError {
    Io(io::Error),
    HandoffTooLarge,
    TooManyRecords,
    RecordTooLarge,
    InvalidJson,
    InvalidSchema,
    InvalidCursorSequence,
    InsecurePermissions,
    SymbolicLink,
    InvalidFileType,
    InvalidIdentifier,
    InvalidFieldType,
    InvalidTiming,
}

impl Display for AdapterError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Io(_) => "Codex handoff I/O failure",
            Self::HandoffTooLarge => "Codex handoff exceeds the byte limit",
            Self::TooManyRecords => "Codex handoff exceeds the record limit",
            Self::RecordTooLarge => "Codex handoff record exceeds the byte limit",
            Self::InvalidJson => "Codex handoff contains invalid JSON",
            Self::InvalidSchema => "Codex handoff schema is unsupported",
            Self::InvalidCursorSequence => "Codex handoff cursor sequence is invalid",
            Self::InsecurePermissions => "Codex handoff permissions are too broad",
            Self::SymbolicLink => "Codex handoff must not be a symbolic link",
            Self::InvalidFileType => "Codex handoff must be a regular file",
            Self::InvalidIdentifier => "Codex handoff contains an invalid identifier",
            Self::InvalidFieldType => "Codex handoff contains an invalid field value",
            Self::InvalidTiming => "Codex handoff contains invalid timing",
        })
    }
}

impl std::error::Error for AdapterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for AdapterError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Reads a private regular file and parses its bounded JSONL handoff.
///
/// # Errors
///
/// Returns [`AdapterError`] for unsafe paths, oversized input, malformed JSON, invalid cursor
/// order, or invalid canonical identifiers.
pub fn read_handoff_file(path: impl AsRef<Path>) -> Result<AdapterBatch, AdapterError> {
    let path = path.as_ref();
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(AdapterError::SymbolicLink);
    }
    if !metadata.is_file() {
        return Err(AdapterError::InvalidFileType);
    }
    if metadata.len() > MAX_HANDOFF_BYTES {
        return Err(AdapterError::HandoffTooLarge);
    }
    private_file_permissions(&metadata)?;
    let input = fs::read_to_string(path)?;
    parse_handoff_jsonl(&input)
}

#[cfg(unix)]
fn private_file_permissions(metadata: &fs::Metadata) -> Result<(), AdapterError> {
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(AdapterError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn private_file_permissions(_metadata: &fs::Metadata) -> Result<(), AdapterError> {
    Ok(())
}

/// Parses the bounded handoff and returns content-free observations plus fixed-code diagnostics.
///
/// # Errors
///
/// Returns [`AdapterError`] for oversized input, malformed JSON, schema drift, cursor gaps, or
/// invalid canonical identifiers.
pub fn parse_handoff_jsonl(input: &str) -> Result<AdapterBatch, AdapterError> {
    if input.len() as u64 > MAX_HANDOFF_BYTES {
        return Err(AdapterError::HandoffTooLarge);
    }
    let mut batch = AdapterBatch::default();
    let mut raw_cursors = BTreeMap::<String, String>::new();
    let mut observation_positions = BTreeMap::<(String, String, String), usize>::new();
    for (index, line) in input.lines().enumerate() {
        if index >= MAX_HANDOFF_LINES {
            return Err(AdapterError::TooManyRecords);
        }
        if line.len() > MAX_HANDOFF_LINE_BYTES {
            return Err(AdapterError::RecordTooLarge);
        }
        if line.trim().is_empty() {
            continue;
        }
        let record: HandoffRecord =
            serde_json::from_str(line).map_err(|_| AdapterError::InvalidJson)?;
        if record.schema_version != HANDOFF_SCHEMA_VERSION {
            return Err(AdapterError::InvalidSchema);
        }
        validate_raw_cursor(&record, &raw_cursors)?;
        raw_cursors.insert(record.source_generation.clone(), record.cursor.clone());
        let checkpoint = checkpoint_from_record(&record)?;
        let mapping =
            match observation_from_record(&record, record.previous_cursor.clone(), index + 1) {
                Ok(mapping) => mapping,
                Err(AdapterError::InvalidIdentifier) => {
                    Mapping::Diagnostic(DiagnosticCode::MissingCorrelation)
                }
                Err(AdapterError::InvalidTiming | AdapterError::InvalidFieldType) => {
                    Mapping::Diagnostic(DiagnosticCode::InvalidFieldType)
                }
                Err(error) => return Err(error),
            };
        match mapping {
            Mapping::Observation(observation) => {
                let payload_hash = canonical_observation_payload_hash(&observation)
                    .map_err(|_| AdapterError::InvalidFieldType)?;
                let key = (
                    observation.source_generation.as_str().to_owned(),
                    observation.span_id.as_str().to_owned(),
                    payload_hash.clone(),
                );
                match observation_positions.entry(key) {
                    std::collections::btree_map::Entry::Occupied(_) => {
                        batch
                            .items
                            .push(AdapterItem::Disposition(AdapterDiagnostic {
                                record_index: index + 1,
                                code: DiagnosticCode::DuplicateObservation,
                                checkpoint,
                                disposition: AdapterDispositionKind::Suppressed,
                                payload_hash: Some(payload_hash),
                            }));
                    }
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(batch.items.len());
                        batch.items.push(AdapterItem::Observation(observation));
                    }
                }
            }
            Mapping::Diagnostic(code) => {
                batch
                    .items
                    .push(AdapterItem::Disposition(AdapterDiagnostic {
                        record_index: index + 1,
                        code,
                        checkpoint,
                        disposition: AdapterDispositionKind::Diagnostic,
                        payload_hash: None,
                    }));
            }
        }
    }
    Ok(batch)
}

fn checkpoint_from_record(record: &HandoffRecord) -> Result<SourceCheckpoint, AdapterError> {
    Ok(SourceCheckpoint {
        source: AgentSource::Codex,
        source_generation: parse_identifier::<SourceGeneration>(&record.source_generation)?,
        previous_source_cursor: record
            .previous_cursor
            .as_deref()
            .map(parse_identifier::<SourceCursor>)
            .transpose()?,
        source_cursor: parse_identifier::<SourceCursor>(&record.cursor)?,
    })
}

fn validate_raw_cursor(
    record: &HandoffRecord,
    cursors: &BTreeMap<String, String>,
) -> Result<(), AdapterError> {
    let expected = cursors.get(&record.source_generation).map(String::as_str);
    let previous = record.previous_cursor.as_deref();
    if record.cursor.is_empty()
        || previous == Some(record.cursor.as_str())
        || expected.is_some_and(|expected| previous != Some(expected))
    {
        return Err(AdapterError::InvalidCursorSequence);
    }
    Ok(())
}

enum Mapping {
    Observation(Box<SourceObservation>),
    Diagnostic(DiagnosticCode),
}

fn observation_from_record(
    record: &HandoffRecord,
    previous_cursor: Option<String>,
    record_index: usize,
) -> Result<Mapping, AdapterError> {
    match (record.surface, record.event_name.as_str()) {
        (SourceSurface::OtelLog, "codex.conversation_starts") => {
            session_observation(record, previous_cursor)
        }
        (SourceSurface::OtelLog, "codex.api_request") => {
            request_observation(record, previous_cursor, false)
        }
        (SourceSurface::OtelLog, "codex.sse_event") => {
            if optional_string(&record.attributes, "kind")?.as_deref() == Some("response.completed")
            {
                request_observation(record, previous_cursor, true)
            } else {
                Ok(Mapping::Diagnostic(DiagnosticCode::UnsupportedEventVariant))
            }
        }
        (SourceSurface::OtelLog, "codex.tool_decision") => {
            permission_observation(record, previous_cursor)
        }
        (SourceSurface::OtelLog, "codex.tool_result") => tool_observation(record, previous_cursor),
        (SourceSurface::OtelLog, "codex.user_prompt") => {
            Ok(Mapping::Diagnostic(DiagnosticCode::ContentEventIgnored))
        }
        (SourceSurface::Notify, "agent-turn-complete") => turn_observation(record, previous_cursor),
        (SourceSurface::Notify | SourceSurface::OtelLog, _) => {
            let _ = record_index;
            Ok(Mapping::Diagnostic(DiagnosticCode::UnsupportedEvent))
        }
    }
}

fn session_observation(
    record: &HandoffRecord,
    previous_cursor: Option<String>,
) -> Result<Mapping, AdapterError> {
    let session = required_string(&record.attributes, "conversation_id")?;
    let correlation = CorrelationIds {
        session_id: Some(parse_identifier::<SessionId>(&session)?),
        ..CorrelationIds::default()
    };
    build_observation(
        record,
        previous_cursor,
        &session,
        None,
        "session",
        correlation,
        ObservationEvent::Session {
            model: canonical_model(&record.attributes)?,
            project: None,
        },
        LifecycleState::Running,
        TokenUsage::default(),
        None,
    )
}

fn turn_observation(
    record: &HandoffRecord,
    previous_cursor: Option<String>,
) -> Result<Mapping, AdapterError> {
    let session = required_string(&record.attributes, "thread_id")?;
    let turn = required_string(&record.attributes, "turn_id")?;
    let correlation = CorrelationIds {
        session_id: Some(parse_identifier::<SessionId>(&session)?),
        turn_id: Some(parse_identifier::<TurnId>(&turn)?),
        ..CorrelationIds::default()
    };
    build_observation(
        record,
        previous_cursor,
        &session,
        Some(&turn),
        "turn",
        correlation,
        ObservationEvent::Turn,
        LifecycleState::Completed,
        TokenUsage::default(),
        None,
    )
}

fn request_observation(
    record: &HandoffRecord,
    previous_cursor: Option<String>,
    includes_usage: bool,
) -> Result<Mapping, AdapterError> {
    let session = required_string(&record.attributes, "conversation_id")?;
    let turn = required_string(&record.attributes, "turn_id")?;
    let request = required_string(&record.attributes, "request_id")?;
    let correlation = CorrelationIds {
        session_id: Some(parse_identifier::<SessionId>(&session)?),
        turn_id: Some(parse_identifier::<TurnId>(&turn)?),
        request_id: Some(parse_identifier::<RequestId>(&request)?),
        ..CorrelationIds::default()
    };
    let usage = if includes_usage {
        TokenUsage {
            input: optional_u64(&record.attributes, "input_tokens")?,
            output: optional_u64(&record.attributes, "output_tokens")?,
            cached_input: optional_u64(&record.attributes, "cached_input_tokens")?,
            reasoning_output: optional_u64(&record.attributes, "reasoning_output_tokens")?,
            total: optional_u64(&record.attributes, "total_tokens")?,
            ..TokenUsage::default()
        }
    } else {
        TokenUsage::default()
    };
    let lifecycle = lifecycle_from_success(optional_bool(&record.attributes, "success")?);
    build_observation(
        record,
        previous_cursor,
        &session,
        Some(&turn),
        &format!(
            "request:{request}:{}",
            if includes_usage { "response" } else { "api" }
        ),
        correlation,
        ObservationEvent::ModelRequest {
            model: canonical_model(&record.attributes)?,
        },
        lifecycle,
        usage,
        optional_u64(&record.attributes, "duration_ms")?,
    )
}

fn tool_observation(
    record: &HandoffRecord,
    previous_cursor: Option<String>,
) -> Result<Mapping, AdapterError> {
    let session = required_string(&record.attributes, "conversation_id")?;
    let turn = required_string(&record.attributes, "turn_id")?;
    let operation = required_string(&record.attributes, "call_id")?;
    let correlation = CorrelationIds {
        session_id: Some(parse_identifier::<SessionId>(&session)?),
        turn_id: Some(parse_identifier::<TurnId>(&turn)?),
        operation_id: Some(parse_identifier::<OperationId>(&operation)?),
        ..CorrelationIds::default()
    };
    build_observation(
        record,
        previous_cursor,
        &session,
        Some(&turn),
        &format!("tool:{operation}"),
        correlation,
        ObservationEvent::ToolOperation {
            tool_name: canonical_tool_name(&record.attributes)?,
            phase: Some("result".into()),
        },
        lifecycle_from_success(optional_bool(&record.attributes, "success")?),
        TokenUsage::default(),
        optional_u64(&record.attributes, "duration_ms")?,
    )
}

fn permission_observation(
    record: &HandoffRecord,
    previous_cursor: Option<String>,
) -> Result<Mapping, AdapterError> {
    let session = required_string(&record.attributes, "conversation_id")?;
    let turn = required_string(&record.attributes, "turn_id")?;
    let permission = required_string(&record.attributes, "call_id")?;
    let decision = canonical_decision(&record.attributes)?;
    let correlation = CorrelationIds {
        session_id: Some(parse_identifier::<SessionId>(&session)?),
        turn_id: Some(parse_identifier::<TurnId>(&turn)?),
        permission_id: Some(parse_identifier::<PermissionId>(&permission)?),
        ..CorrelationIds::default()
    };
    build_observation(
        record,
        previous_cursor,
        &session,
        Some(&turn),
        &format!("permission:{permission}"),
        correlation,
        ObservationEvent::Permission {
            decision: Some(decision.clone()),
        },
        if decision == "denied" {
            LifecycleState::Failed
        } else {
            LifecycleState::Completed
        },
        TokenUsage::default(),
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_observation(
    record: &HandoffRecord,
    previous_cursor: Option<String>,
    session: &str,
    turn: Option<&str>,
    leaf: &str,
    correlation: CorrelationIds,
    event: ObservationEvent,
    lifecycle: LifecycleState,
    token_usage: TokenUsage,
    duration_ms: Option<u64>,
) -> Result<Mapping, AdapterError> {
    let trace_id = parse_identifier::<TraceId>(&stable_id("codex-trace", &[session]))?;
    let session_span = stable_id("codex-session", &[session]);
    let (span_id, parent_span_id) = match turn {
        None => (session_span, None),
        Some(turn_id) if leaf == "turn" => (
            stable_id("codex-turn", &[session, turn_id]),
            Some(session_span),
        ),
        Some(turn_id) => (
            stable_id("codex-span", &[session, turn_id, leaf]),
            Some(stable_id("codex-turn", &[session, turn_id])),
        ),
    };
    let end = record.received_at_unix_ms;
    let start = end
        .checked_sub(duration_ms.unwrap_or(0))
        .ok_or(AdapterError::InvalidTiming)?;
    let timing = Timing::new(start, Some(end)).map_err(|_| AdapterError::InvalidTiming)?;
    let observation_id = stable_id(
        "codex-observation",
        &[
            &record.source_generation,
            &record.cursor,
            match record.surface {
                SourceSurface::OtelLog => "otel_log",
                SourceSurface::Notify => "notify",
            },
            &record.event_name,
        ],
    );
    Ok(Mapping::Observation(Box::new(SourceObservation {
        source: AgentSource::Codex,
        source_generation: parse_identifier::<SourceGeneration>(&record.source_generation)?,
        previous_source_cursor: previous_cursor
            .map(|value| parse_identifier::<SourceCursor>(&value))
            .transpose()?,
        source_cursor: parse_identifier::<SourceCursor>(&record.cursor)?,
        observation_id: parse_identifier::<ObservationId>(&observation_id)?,
        trace_id,
        span_id: parse_identifier::<SpanId>(&span_id)?,
        parent_span_id: parent_span_id
            .map(|value| parse_identifier::<SpanId>(&value))
            .transpose()?,
        correlation,
        event,
        lifecycle,
        timing,
        token_usage,
    })))
}

fn lifecycle_from_success(success: Option<bool>) -> LifecycleState {
    match success {
        Some(false) => LifecycleState::Failed,
        Some(true) => LifecycleState::Completed,
        None => LifecycleState::Observed,
    }
}

fn required_string(
    attributes: &BTreeMap<String, Value>,
    key: &str,
) -> Result<String, AdapterError> {
    optional_string(attributes, key)?.ok_or(AdapterError::InvalidIdentifier)
}

fn optional_string(
    attributes: &BTreeMap<String, Value>,
    key: &str,
) -> Result<Option<String>, AdapterError> {
    match attributes.get(key) {
        None => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value.clone())),
        Some(_) => Err(AdapterError::InvalidIdentifier),
    }
}

fn canonical_model(attributes: &BTreeMap<String, Value>) -> Result<Option<String>, AdapterError> {
    let Some(model) = optional_string(attributes, "model")? else {
        return Ok(None);
    };
    Ok(KNOWN_CODEX_MODELS
        .contains(&model.as_str())
        .then_some(model))
}

fn canonical_tool_name(
    attributes: &BTreeMap<String, Value>,
) -> Result<Option<String>, AdapterError> {
    let Some(tool) = optional_string(attributes, "tool_name")? else {
        return Ok(None);
    };
    let category = match tool.as_str() {
        "shell" | "exec_command" => "shell",
        "apply_patch" => "apply_patch",
        "web" | "web_search" => "web",
        "mcp" | "mcp_tool" => "mcp",
        _ => "other",
    };
    Ok(Some(category.into()))
}

fn canonical_decision(attributes: &BTreeMap<String, Value>) -> Result<String, AdapterError> {
    match optional_string(attributes, "decision")?.as_deref() {
        Some("approved" | "allowed") => Ok("approved".into()),
        Some("denied") => Ok("denied".into()),
        _ => Err(AdapterError::InvalidFieldType),
    }
}

fn optional_u64(
    attributes: &BTreeMap<String, Value>,
    key: &str,
) -> Result<Option<u64>, AdapterError> {
    match attributes.get(key) {
        None => Ok(None),
        Some(Value::Number(value)) => value.as_u64().ok_or(AdapterError::InvalidTiming).map(Some),
        Some(_) => Err(AdapterError::InvalidTiming),
    }
}

fn optional_bool(
    attributes: &BTreeMap<String, Value>,
    key: &str,
) -> Result<Option<bool>, AdapterError> {
    match attributes.get(key) {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(AdapterError::InvalidTiming),
    }
}

trait ParseIdentifier: Sized {
    fn parse(value: &str) -> Result<Self, agent_observability_domain::DomainError>;
}

macro_rules! parse_identifier_impl {
    ($($identifier:ty),+ $(,)?) => {
        $(impl ParseIdentifier for $identifier {
            fn parse(value: &str) -> Result<Self, agent_observability_domain::DomainError> {
                Self::parse(value)
            }
        })+
    };
}

parse_identifier_impl!(
    TraceId,
    SpanId,
    SessionId,
    TurnId,
    RequestId,
    OperationId,
    PermissionId,
    SourceCursor,
    SourceGeneration,
    ObservationId,
);

fn parse_identifier<T: ParseIdentifier>(value: &str) -> Result<T, AdapterError> {
    T::parse(value).map_err(|_| AdapterError::InvalidIdentifier)
}

fn stable_id(prefix: &str, components: &[&str]) -> String {
    let mut digest = Sha256::new();
    for component in components {
        digest.update(component.len().to_be_bytes());
        digest.update(component.as_bytes());
    }
    let bytes = digest.finalize();
    let mut encoded = String::with_capacity(prefix.len() + 1 + bytes.len() * 2);
    encoded.push_str(prefix);
    encoded.push(':');
    for byte in bytes {
        write!(encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{
        AdapterError, AdapterItem, DiagnosticCode, MAX_HANDOFF_BYTES, MAX_HANDOFF_LINE_BYTES,
        MAX_HANDOFF_LINES, parse_handoff_jsonl, read_handoff_file,
    };
    use agent_observability_contracts::ObservationEvent;
    use agent_observability_domain::LifecycleState;
    use std::fs;

    const FIXTURE: &str = include_str!("../tests/fixtures/codex-handoff.jsonl");
    const EXPECTED_PROJECTION: &str = include_str!("../tests/fixtures/codex-projection.jsonl");
    const EXPECTED_HANDOFF_HASH: &str =
        "sha256:0b30a1810b6e34152310691a3a660ecf33e98d4940fc63fe9b340811241f526c";
    const EXPECTED_PROJECTION_HASH: &str =
        "sha256:6dc1fa2ad7837c0e9ac2dcd6ac0dca52da1b0c11872db203d53ae742c97ee45a";

    #[test]
    fn maps_primary_and_supplement_sources_without_losing_timing_updates() {
        let batch = parse_handoff_jsonl(FIXTURE).expect("fixture parses");
        assert_eq!(batch.observations().count(), 6);
        let diagnostics = batch.diagnostics().collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].code, DiagnosticCode::ContentEventIgnored);
        assert_eq!(diagnostics[1].code, DiagnosticCode::UnsupportedEvent);
        let requests = batch
            .observations()
            .filter(|observation| {
                matches!(observation.event, ObservationEvent::ModelRequest { .. })
            })
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.token_usage.input == Some(100))
                .count(),
            1
        );
        let turns = batch
            .observations()
            .filter(|observation| matches!(observation.event, ObservationEvent::Turn))
            .collect::<Vec<_>>();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].lifecycle, LifecycleState::Completed);
        assert_eq!(turns[1].lifecycle, LifecycleState::Completed);
        assert!(turns[1].timing.end_unix_ms > turns[0].timing.end_unix_ms);
        assert_eq!(
            turns[0].previous_source_cursor.as_ref().unwrap().as_str(),
            "6"
        );
    }

    #[test]
    fn content_never_crosses_the_observation_or_diagnostic_boundary() {
        let batch = parse_handoff_jsonl(FIXTURE).unwrap();
        let debug = format!("{batch:?}");
        assert!(!debug.contains("RAW_PROMPT_SECRET"));
        assert!(!debug.contains("RAW_TOOL_OUTPUT_SECRET"));
    }

    #[test]
    fn tool_decision_maps_to_a_permission_without_copying_unowned_fields() {
        let input = r#"{"schema_version":"codex_handoff.v1","source_generation":"codex-0.150.1","previous_cursor":null,"cursor":"1","surface":"otel_log","received_at_unix_ms":1787875200000,"event_name":"codex.tool_decision","attributes":{"conversation_id":"conversation-1","turn_id":"turn-1","call_id":"call-1","decision":"denied","command":"RAW_COMMAND_SECRET"}}"#;
        let batch = parse_handoff_jsonl(input).unwrap();
        let permission = batch.observations().next().unwrap();
        assert!(matches!(
            &permission.event,
            ObservationEvent::Permission { decision } if decision.as_deref() == Some("denied")
        ));
        assert_eq!(permission.lifecycle, LifecycleState::Failed);
        assert!(!format!("{permission:?}").contains("RAW_COMMAND_SECRET"));
    }

    #[test]
    fn arbitrary_metadata_is_canonicalized_or_diagnosed_before_durable_write() {
        use agent_observability_local_store::LocalStore;

        let input = concat!(
            "{\"schema_version\":\"codex_handoff.v1\",\"source_generation\":\"codex-0.150.1\",\"previous_cursor\":null,\"cursor\":\"1\",\"surface\":\"otel_log\",\"received_at_unix_ms\":100,\"event_name\":\"codex.conversation_starts\",\"attributes\":{\"conversation_id\":\"conversation-1\",\"model\":\"RAW_PROMPT_SECRET\"}}\n",
            "{\"schema_version\":\"codex_handoff.v1\",\"source_generation\":\"codex-0.150.1\",\"previous_cursor\":\"1\",\"cursor\":\"2\",\"surface\":\"otel_log\",\"received_at_unix_ms\":110,\"event_name\":\"codex.tool_result\",\"attributes\":{\"conversation_id\":\"conversation-1\",\"turn_id\":\"turn-1\",\"call_id\":\"call-1\",\"tool_name\":\"RAW_TOOL_SECRET\",\"success\":true}}\n",
            "{\"schema_version\":\"codex_handoff.v1\",\"source_generation\":\"codex-0.150.1\",\"previous_cursor\":\"2\",\"cursor\":\"3\",\"surface\":\"otel_log\",\"received_at_unix_ms\":120,\"event_name\":\"codex.tool_decision\",\"attributes\":{\"conversation_id\":\"conversation-1\",\"turn_id\":\"turn-1\",\"call_id\":\"call-2\",\"decision\":\"RAW_DECISION_SECRET\"}}"
        );
        let batch = parse_handoff_jsonl(input).unwrap();
        assert_eq!(batch.observations().count(), 2);
        assert_eq!(batch.diagnostics().count(), 1);

        let root = std::env::temp_dir().join(format!(
            "agent-observability-codex-metadata-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut store = LocalStore::open(&root).unwrap();
        for item in &batch.items {
            match item {
                AdapterItem::Observation(observation) => {
                    store.ingest_deferred_projection(observation).unwrap();
                }
                AdapterItem::Disposition(diagnostic) => {
                    store
                        .ingest_disposition_with_payload(
                            &diagnostic.checkpoint,
                            diagnostic.disposition,
                            diagnostic.code,
                            diagnostic.payload_hash.as_deref(),
                        )
                        .unwrap();
                }
            }
        }
        store.rebuild_projection().unwrap();
        let durable = fs::read_to_string(store.projection_path()).unwrap();
        for secret in [
            "RAW_PROMPT_SECRET",
            "RAW_TOOL_SECRET",
            "RAW_DECISION_SECRET",
        ] {
            assert!(!durable.contains(secret));
        }
        assert!(durable.contains("other"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duration_underflow_and_unknown_permission_are_fixed_diagnostics() {
        let duration = r#"{"schema_version":"codex_handoff.v1","source_generation":"codex-0.150.1","previous_cursor":null,"cursor":"1","surface":"otel_log","received_at_unix_ms":10,"event_name":"codex.api_request","attributes":{"conversation_id":"conversation-1","turn_id":"turn-1","request_id":"request-1","duration_ms":20}}"#;
        let permission = r#"{"schema_version":"codex_handoff.v1","source_generation":"codex-0.150.1","previous_cursor":null,"cursor":"1","surface":"otel_log","received_at_unix_ms":10,"event_name":"codex.tool_decision","attributes":{"conversation_id":"conversation-1","turn_id":"turn-1","call_id":"call-1","decision":"unknown"}}"#;
        for input in [duration, permission] {
            let batch = parse_handoff_jsonl(input).unwrap();
            assert_eq!(batch.observations().count(), 0);
            assert_eq!(
                batch.diagnostics().next().unwrap().code,
                DiagnosticCode::InvalidFieldType
            );
        }
    }

    #[test]
    fn cursor_gaps_and_bounds_fail_closed() {
        let gap = FIXTURE.replacen("\"previous_cursor\":\"1\"", "\"previous_cursor\":null", 1);
        assert!(matches!(
            parse_handoff_jsonl(&gap),
            Err(AdapterError::InvalidCursorSequence)
        ));
        let oversized = "x".repeat(usize::try_from(MAX_HANDOFF_BYTES + 1).unwrap());
        assert!(matches!(
            parse_handoff_jsonl(&oversized),
            Err(AdapterError::HandoffTooLarge)
        ));
    }

    #[test]
    fn every_handoff_bound_accepts_its_limit_and_rejects_the_next_unit() {
        let bounded_line = format!("{}\n", " ".repeat(MAX_HANDOFF_LINE_BYTES - 1));
        let exact_bytes = bounded_line.repeat(16);
        assert_eq!(exact_bytes.len() as u64, MAX_HANDOFF_BYTES);
        assert!(parse_handoff_jsonl(&exact_bytes).is_ok());
        assert!(matches!(
            parse_handoff_jsonl(&(exact_bytes + "\n")),
            Err(AdapterError::HandoffTooLarge)
        ));

        let exact_line = " ".repeat(MAX_HANDOFF_LINE_BYTES);
        assert!(parse_handoff_jsonl(&exact_line).is_ok());
        assert!(matches!(
            parse_handoff_jsonl(&(exact_line + " ")),
            Err(AdapterError::RecordTooLarge)
        ));

        let exact_records = "\n".repeat(MAX_HANDOFF_LINES);
        assert!(parse_handoff_jsonl(&exact_records).is_ok());
        assert!(matches!(
            parse_handoff_jsonl(&(exact_records + "\n")),
            Err(AdapterError::TooManyRecords)
        ));
    }

    #[test]
    fn tail_checkpoint_is_allowed_but_duplicate_and_broken_cursors_fail_closed() {
        let tail = FIXTURE.lines().nth(2).unwrap();
        assert_eq!(parse_handoff_jsonl(tail).unwrap().observations().count(), 1);

        let duplicate = FIXTURE
            .lines()
            .take(2)
            .collect::<Vec<_>>()
            .join("\n")
            .replace("\"cursor\":\"2\"", "\"cursor\":\"1\"");
        assert!(matches!(
            parse_handoff_jsonl(&duplicate),
            Err(AdapterError::InvalidCursorSequence)
        ));

        let malformed = tail.replace("\"cursor\":\"3\"", "\"cursor\":\"\"");
        assert!(matches!(
            parse_handoff_jsonl(&malformed),
            Err(AdapterError::InvalidCursorSequence)
        ));
    }

    #[test]
    fn source_rotation_has_an_independent_cursor_and_dedupe_scope() {
        let first = FIXTURE.lines().next().unwrap();
        let second = first
            .replace("codex-0.150.1", "codex-0.150.1-rotated")
            .replace("\"cursor\":\"1\"", "\"cursor\":\"rotation-1\"");
        let batch = parse_handoff_jsonl(&format!("{first}\n{second}\n")).unwrap();
        assert_eq!(batch.observations().count(), 2);
        assert_eq!(batch.diagnostics().count(), 0);
        assert!(
            batch
                .observations()
                .all(|observation| observation.previous_source_cursor.is_none())
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_handoff_requires_private_permissions_and_rejects_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let root = std::env::temp_dir().join(format!(
            "agent-observability-codex-handoff-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("handoff.jsonl");
        fs::write(&path, FIXTURE).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            read_handoff_file(&path),
            Err(AdapterError::InsecurePermissions)
        ));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(read_handoff_file(&path).unwrap().observations().count(), 6);
        let oversized = root.join("oversized.jsonl");
        fs::write(
            &oversized,
            vec![b' '; usize::try_from(MAX_HANDOFF_BYTES + 1).unwrap()],
        )
        .unwrap();
        fs::set_permissions(&oversized, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(
            read_handoff_file(&oversized),
            Err(AdapterError::HandoffTooLarge)
        ));
        let link = root.join("handoff-link.jsonl");
        symlink(&path, &link).unwrap();
        assert!(matches!(
            read_handoff_file(link),
            Err(AdapterError::SymbolicLink)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn complete_batch_ingests_into_the_private_store() {
        use agent_observability_local_store::LocalStore;

        let root = std::env::temp_dir().join(format!(
            "agent-observability-codex-store-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut store = LocalStore::open(&root).unwrap();
        let batch = parse_handoff_jsonl(FIXTURE).unwrap();
        for item in &batch.items {
            match item {
                AdapterItem::Observation(observation) => {
                    store.ingest(observation).unwrap();
                }
                AdapterItem::Disposition(diagnostic) => {
                    store
                        .ingest_disposition_with_payload(
                            &diagnostic.checkpoint,
                            diagnostic.disposition,
                            diagnostic.code,
                            diagnostic.payload_hash.as_deref(),
                        )
                        .unwrap();
                }
            }
        }
        assert_eq!(store.observation_count().unwrap(), 6);
        assert_eq!(store.record_count().unwrap(), 5);
        assert_eq!(store.disposition_count().unwrap(), 2);
        assert_eq!(
            store.cursor("codex", "codex-0.150.1").unwrap().as_deref(),
            Some("8")
        );
        let durable = fs::read_to_string(store.projection_path()).unwrap();
        assert_eq!(durable, EXPECTED_PROJECTION);
        for secret in [
            "RAW_PROMPT_SECRET",
            "RAW_TOOL_OUTPUT_SECRET",
            "RAW_UNKNOWN_SECRET",
            "RAW_ASSISTANT_SECRET",
            "/RAW/PATH",
        ] {
            assert!(!durable.contains(secret));
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reopened_store_accepts_only_the_appended_tail() {
        use agent_observability_local_store::LocalStore;

        let root = std::env::temp_dir().join(format!(
            "agent-observability-codex-append-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut store = LocalStore::open(&root).unwrap();
        let prefix = FIXTURE.lines().take(2).collect::<Vec<_>>().join("\n");
        for item in parse_handoff_jsonl(&prefix).unwrap().items {
            match item {
                AdapterItem::Observation(observation) => {
                    store.ingest_deferred_projection(&observation).unwrap();
                }
                AdapterItem::Disposition(diagnostic) => {
                    store
                        .ingest_disposition_with_payload(
                            &diagnostic.checkpoint,
                            diagnostic.disposition,
                            diagnostic.code,
                            diagnostic.payload_hash.as_deref(),
                        )
                        .unwrap();
                }
            }
        }
        drop(store);
        let mut store = LocalStore::open(&root).unwrap();
        let tail = FIXTURE.lines().skip(2).collect::<Vec<_>>().join("\n");
        for item in parse_handoff_jsonl(&tail).unwrap().items {
            match item {
                AdapterItem::Observation(observation) => {
                    store.ingest_deferred_projection(&observation).unwrap();
                }
                AdapterItem::Disposition(diagnostic) => {
                    store
                        .ingest_disposition_with_payload(
                            &diagnostic.checkpoint,
                            diagnostic.disposition,
                            diagnostic.code,
                            diagnostic.payload_hash.as_deref(),
                        )
                        .unwrap();
                }
            }
        }
        store.rebuild_projection().unwrap();
        assert_eq!(store.observation_count().unwrap(), 6);
        assert_eq!(store.disposition_count().unwrap(), 2);
        assert_eq!(
            store.cursor("codex", "codex-0.150.1").unwrap().as_deref(),
            Some("8")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn capability_fixture_hashes_are_current() {
        use agent_observability_contracts::{ADAPTER_CAPABILITY_V1, AdapterCapabilityManifestV1};
        use sha2::{Digest, Sha256};

        fn fixture_hash(value: &str) -> String {
            let mut digest = Sha256::new();
            digest.update(value.as_bytes());
            let mut encoded = String::from("sha256:");
            for byte in digest.finalize() {
                use std::fmt::Write as _;
                write!(encoded, "{byte:02x}").unwrap();
            }
            encoded
        }

        let manifest = AdapterCapabilityManifestV1::parse_and_validate(ADAPTER_CAPABILITY_V1)
            .expect("capability parses");
        let codex = manifest
            .entries
            .iter()
            .find(|entry| entry.adapter_family == "codex")
            .unwrap();
        assert_eq!(fixture_hash(FIXTURE), EXPECTED_HANDOFF_HASH);
        assert_eq!(fixture_hash(EXPECTED_PROJECTION), EXPECTED_PROJECTION_HASH);
        assert_eq!(
            codex.fixture_hashes.get("codex-handoff.jsonl"),
            Some(&EXPECTED_HANDOFF_HASH.to_owned())
        );
        assert_eq!(
            codex.fixture_hashes.get("codex-projection.jsonl"),
            Some(&EXPECTED_PROJECTION_HASH.to_owned())
        );
    }
}
