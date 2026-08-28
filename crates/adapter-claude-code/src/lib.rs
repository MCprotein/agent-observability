//! Bounded, content-free Claude Code OTel/hook handoff adapter.

use agent_observability_contracts::{
    AdapterDispositionKind, AgentSource, ObservationEvent, SourceCheckpoint, SourceObservation,
    canonical_observation_payload_hash,
};
use agent_observability_domain::{
    CompactionId, CorrelationIds, LifecycleState, ObservationId, OperationId, PermissionId,
    RequestId, SessionId, SourceCursor, SourceGeneration, SpanId, Timing, TokenUsage, TraceId,
    TurnId,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fmt::{self, Display, Formatter};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

pub const HANDOFF_SCHEMA_VERSION: &str = "claude_handoff.v1";
pub const MAX_HANDOFF_BYTES: u64 = 1024 * 1024;
pub const MAX_HANDOFF_LINES: usize = 4096;
pub const MAX_HANDOFF_LINE_BYTES: usize = 64 * 1024;
const KNOWN_CLAUDE_MODELS: &[&str] = &[
    "claude-test",
    "claude-haiku-4-5",
    "claude-opus-4-6",
    "claude-sonnet-4-6",
    "claude-opus-5",
    "claude-sonnet-5",
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceSurface {
    OtelLog,
    Hook,
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
    UnsupportedPlatform,
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
            Self::Io(_) => "Claude Code handoff I/O failure",
            Self::HandoffTooLarge => "Claude Code handoff exceeds the byte limit",
            Self::TooManyRecords => "Claude Code handoff exceeds the record limit",
            Self::RecordTooLarge => "Claude Code handoff record exceeds the byte limit",
            Self::InvalidJson => "Claude Code handoff contains invalid JSON",
            Self::InvalidSchema => "Claude Code handoff schema is unsupported",
            Self::InvalidCursorSequence => "Claude Code handoff cursor sequence is invalid",
            Self::UnsupportedPlatform => "Claude Code file handoff requires a Unix platform",
            Self::InsecurePermissions => "Claude Code handoff permissions are too broad",
            Self::SymbolicLink => "Claude Code handoff must not be a symbolic link",
            Self::InvalidFileType => "Claude Code handoff must be a regular file",
            Self::InvalidIdentifier => "Claude Code handoff contains an invalid identifier",
            Self::InvalidFieldType => "Claude Code handoff contains an invalid field value",
            Self::InvalidTiming => "Claude Code handoff contains invalid timing",
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
    if !file_platform_supported() {
        return Err(AdapterError::UnsupportedPlatform);
    }
    let path = path.as_ref();
    let path_metadata = fs::symlink_metadata(path)?;
    if path_metadata.file_type().is_symlink() {
        return Err(AdapterError::SymbolicLink);
    }
    if !path_metadata.is_file() {
        return Err(AdapterError::InvalidFileType);
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || !same_file_identity(&path_metadata, &metadata) {
        return Err(AdapterError::InvalidFileType);
    }
    if metadata.len() > MAX_HANDOFF_BYTES {
        return Err(AdapterError::HandoffTooLarge);
    }
    private_file_permissions(&metadata)?;
    let mut input = String::new();
    file.take(MAX_HANDOFF_BYTES + 1)
        .read_to_string(&mut input)?;
    if input.len() as u64 > MAX_HANDOFF_BYTES {
        return Err(AdapterError::HandoffTooLarge);
    }
    parse_handoff_jsonl(&input)
}

#[cfg(unix)]
const fn file_platform_supported() -> bool {
    true
}

#[cfg(not(unix))]
const fn file_platform_supported() -> bool {
    false
}

#[cfg(unix)]
fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(unix))]
fn same_file_identity(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
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
    Err(AdapterError::UnsupportedPlatform)
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
        source: AgentSource::ClaudeCode,
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
    _record_index: usize,
) -> Result<Mapping, AdapterError> {
    match (record.surface, record.event_name.as_str()) {
        (SourceSurface::Hook, "SessionStart") => session_observation(record, previous_cursor),
        (SourceSurface::OtelLog, "claude_code.user_prompt") => {
            turn_observation(record, previous_cursor, LifecycleState::Running)
        }
        (SourceSurface::OtelLog, "claude_code.api_request") => {
            request_observation(record, previous_cursor)
        }
        (SourceSurface::OtelLog, "claude_code.tool_decision") => {
            permission_observation(record, previous_cursor)
        }
        (SourceSurface::OtelLog, "claude_code.tool_result") => {
            tool_observation(record, previous_cursor)
        }
        (SourceSurface::OtelLog, "claude_code.compaction") => {
            compaction_observation(record, previous_cursor)
        }
        (
            SourceSurface::OtelLog,
            "claude_code.assistant_response"
            | "claude_code.api_request_body"
            | "claude_code.api_response_body",
        ) => Ok(Mapping::Diagnostic(DiagnosticCode::ContentEventIgnored)),
        (SourceSurface::Hook, "Stop") => {
            turn_observation(record, previous_cursor, LifecycleState::Completed)
        }
        (SourceSurface::Hook, "StopFailure") => {
            turn_observation(record, previous_cursor, LifecycleState::Failed)
        }
        (SourceSurface::Hook | SourceSurface::OtelLog, _) => {
            Ok(Mapping::Diagnostic(DiagnosticCode::UnsupportedEvent))
        }
    }
}

fn session_observation(
    record: &HandoffRecord,
    previous_cursor: Option<String>,
) -> Result<Mapping, AdapterError> {
    let session = required_string(&record.attributes, "session_id")?;
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
    lifecycle: LifecycleState,
) -> Result<Mapping, AdapterError> {
    let session = required_string(&record.attributes, "session_id")?;
    let turn = required_string(&record.attributes, "prompt_id")?;
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
        lifecycle,
        TokenUsage::default(),
        None,
    )
}

fn request_observation(
    record: &HandoffRecord,
    previous_cursor: Option<String>,
) -> Result<Mapping, AdapterError> {
    let session = required_string(&record.attributes, "session_id")?;
    let turn = required_string(&record.attributes, "prompt_id")?;
    let request = required_string(&record.attributes, "request_id")?;
    let correlation = CorrelationIds {
        session_id: Some(parse_identifier::<SessionId>(&session)?),
        turn_id: Some(parse_identifier::<TurnId>(&turn)?),
        request_id: Some(parse_identifier::<RequestId>(&request)?),
        ..CorrelationIds::default()
    };
    let usage = TokenUsage {
        input: optional_u64(&record.attributes, "input_tokens")?,
        output: optional_u64(&record.attributes, "output_tokens")?,
        cached_input: optional_u64(&record.attributes, "cache_read_tokens")?,
        cache_creation_input: optional_u64(&record.attributes, "cache_creation_tokens")?,
        ..TokenUsage::default()
    };
    let lifecycle = lifecycle_from_success(optional_bool(&record.attributes, "success")?);
    build_observation(
        record,
        previous_cursor,
        &session,
        Some(&turn),
        &format!("request:{request}"),
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
    let session = required_string(&record.attributes, "session_id")?;
    let turn = required_string(&record.attributes, "prompt_id")?;
    let operation = required_string(&record.attributes, "tool_use_id")?;
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
    let session = required_string(&record.attributes, "session_id")?;
    let turn = required_string(&record.attributes, "prompt_id")?;
    let permission = required_string(&record.attributes, "tool_use_id")?;
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

fn compaction_observation(
    record: &HandoffRecord,
    previous_cursor: Option<String>,
) -> Result<Mapping, AdapterError> {
    let session = required_string(&record.attributes, "session_id")?;
    let turn = required_string(&record.attributes, "prompt_id")?;
    let compaction = required_string(&record.attributes, "compaction_id")?;
    let correlation = CorrelationIds {
        session_id: Some(parse_identifier::<SessionId>(&session)?),
        turn_id: Some(parse_identifier::<TurnId>(&turn)?),
        compaction_id: Some(parse_identifier::<CompactionId>(&compaction)?),
        ..CorrelationIds::default()
    };
    build_observation(
        record,
        previous_cursor,
        &session,
        Some(&turn),
        &format!("compaction:{compaction}"),
        correlation,
        ObservationEvent::Compaction {
            trigger: Some(canonical_trigger(&record.attributes)?),
        },
        lifecycle_from_success(optional_bool(&record.attributes, "success")?),
        TokenUsage {
            input_before: optional_u64(&record.attributes, "pre_tokens")?,
            input_after: optional_u64(&record.attributes, "post_tokens")?,
            ..TokenUsage::default()
        },
        optional_u64(&record.attributes, "duration_ms")?,
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
    let trace_id = parse_identifier::<TraceId>(&stable_id("claude-code-trace", &[session]))?;
    let session_span = stable_id("claude-code-session", &[session]);
    let (span_id, parent_span_id) = match turn {
        None => (session_span, None),
        Some(turn_id) if leaf == "turn" => (
            stable_id("claude-code-turn", &[session, turn_id]),
            Some(session_span),
        ),
        Some(turn_id) => (
            stable_id("claude-code-span", &[session, turn_id, leaf]),
            Some(stable_id("claude-code-turn", &[session, turn_id])),
        ),
    };
    let end = record.received_at_unix_ms;
    let start = end
        .checked_sub(duration_ms.unwrap_or(0))
        .ok_or(AdapterError::InvalidTiming)?;
    let timing = Timing::new(start, Some(end)).map_err(|_| AdapterError::InvalidTiming)?;
    let observation_id = stable_id(
        "claude-code-observation",
        &[
            &record.source_generation,
            &record.cursor,
            match record.surface {
                SourceSurface::OtelLog => "otel_log",
                SourceSurface::Hook => "hook",
            },
            &record.event_name,
        ],
    );
    Ok(Mapping::Observation(Box::new(SourceObservation {
        source: AgentSource::ClaudeCode,
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
    if KNOWN_CLAUDE_MODELS.contains(&model.as_str()) {
        Ok(Some(model))
    } else {
        Err(AdapterError::InvalidFieldType)
    }
}

fn canonical_tool_name(
    attributes: &BTreeMap<String, Value>,
) -> Result<Option<String>, AdapterError> {
    let Some(tool) = optional_string(attributes, "tool_name")? else {
        return Ok(None);
    };
    let category = match tool.as_str() {
        "Bash" | "PowerShell" => "shell",
        "Edit" | "Write" | "NotebookEdit" => "edit",
        "Read" | "Glob" | "Grep" => "read",
        "WebFetch" | "WebSearch" => "web",
        "Agent" | "Task" => "agent",
        "mcp_tool" => "mcp",
        _ => "other",
    };
    Ok(Some(category.into()))
}

fn canonical_decision(attributes: &BTreeMap<String, Value>) -> Result<String, AdapterError> {
    match optional_string(attributes, "decision")?.as_deref() {
        Some("accept") => Ok("approved".into()),
        Some("reject") => Ok("denied".into()),
        _ => Err(AdapterError::InvalidFieldType),
    }
}

fn canonical_trigger(attributes: &BTreeMap<String, Value>) -> Result<String, AdapterError> {
    match optional_string(attributes, "trigger")?.as_deref() {
        Some("auto") => Ok("auto".into()),
        Some("manual") => Ok("manual".into()),
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
    CompactionId,
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

    const FIXTURE: &str = include_str!("../tests/fixtures/claude-handoff.jsonl");
    const EXPECTED_PROJECTION: &str = include_str!("../tests/fixtures/claude-projection.jsonl");
    const EXPECTED_HANDOFF_HASH: &str =
        "sha256:316ae843b2d0d20ae5ad25a7507fb9e16d025d6f4442b9775048b789c9560a44";
    const EXPECTED_PROJECTION_HASH: &str =
        "sha256:a65f5439b7fb563547ac9daa311f71b799f34842e14e21c0451be2956c3bd4fc";
    const RAW_SENTINELS: &[&str] = &[
        "RAW_PROMPT_SECRET",
        "RAW_RESPONSE_SECRET",
        "RAW_COMMAND_SECRET",
        "RAW_TOOL_INPUT_SECRET",
        "RAW_TOOL_ERROR_SECRET",
        "RAW_ERROR_SECRET",
        "RAW_ASSISTANT_SECRET",
        "RAW_COMPACTION_ERROR",
        "RAW_CONTENT_EVENT_SECRET",
        "RAW_UNKNOWN_SECRET",
        "raw@example.invalid",
        "/RAW/PRIVATE/PATH",
        "/RAW/PRIVATE/TRANSCRIPT",
    ];

    #[test]
    fn maps_otel_primary_and_hook_lifecycle_with_fixed_precedence() {
        let batch = parse_handoff_jsonl(FIXTURE).expect("fixture parses");
        assert_eq!(batch.observations().count(), 7);
        let diagnostics = batch.diagnostics().collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 3);
        assert_eq!(diagnostics[0].code, DiagnosticCode::ContentEventIgnored);
        assert_eq!(diagnostics[1].code, DiagnosticCode::UnsupportedEvent);
        assert_eq!(diagnostics[2].code, DiagnosticCode::DuplicateObservation);

        let failed = batch.observations().find(|observation| {
            matches!(observation.event, ObservationEvent::Turn)
                && observation.lifecycle == LifecycleState::Failed
        });
        assert!(failed.is_some());

        let request = batch
            .observations()
            .find(|observation| matches!(observation.event, ObservationEvent::ModelRequest { .. }))
            .unwrap();
        assert_eq!(request.token_usage.input, Some(120));
        assert_eq!(request.token_usage.cached_input, Some(40));
        assert_eq!(request.token_usage.cache_creation_input, Some(10));

        let compaction = batch
            .observations()
            .find(|observation| matches!(observation.event, ObservationEvent::Compaction { .. }))
            .unwrap();
        assert_eq!(compaction.token_usage.input_before, Some(120_000));
        assert_eq!(compaction.token_usage.input_after, Some(64_000));
    }

    #[test]
    fn accepts_out_of_order_timestamps_without_reordering_source_cursors() {
        let batch = parse_handoff_jsonl(FIXTURE).unwrap();
        let failed = batch
            .observations()
            .find(|observation| {
                matches!(observation.event, ObservationEvent::Turn)
                    && observation.lifecycle == LifecycleState::Failed
            })
            .unwrap();
        let request = batch
            .observations()
            .find(|observation| matches!(observation.event, ObservationEvent::ModelRequest { .. }))
            .unwrap();
        assert!(failed.timing.end_unix_ms > request.timing.end_unix_ms);
        assert_eq!(failed.source_cursor.as_str(), "3");
        assert_eq!(
            request.previous_source_cursor.as_ref().unwrap().as_str(),
            "3"
        );
    }

    #[test]
    fn hook_supplement_cannot_override_otel_primary_usage_fields() {
        let input = concat!(
            "{\"schema_version\":\"claude_handoff.v1\",\"source_generation\":\"claude-code-2.1.248\",\"previous_cursor\":null,\"cursor\":\"1\",\"surface\":\"hook\",\"received_at_unix_ms\":100,\"event_name\":\"SessionStart\",\"attributes\":{\"session_id\":\"session-1\",\"model\":\"claude-test\"}}\n",
            "{\"schema_version\":\"claude_handoff.v1\",\"source_generation\":\"claude-code-2.1.248\",\"previous_cursor\":\"1\",\"cursor\":\"2\",\"surface\":\"hook\",\"received_at_unix_ms\":300,\"event_name\":\"Stop\",\"attributes\":{\"session_id\":\"session-1\",\"prompt_id\":\"prompt-1\",\"model\":\"RAW_HOOK_MODEL\",\"input_tokens\":999,\"tool_name\":\"RAW_HOOK_TOOL\",\"decision\":\"reject\"}}\n",
            "{\"schema_version\":\"claude_handoff.v1\",\"source_generation\":\"claude-code-2.1.248\",\"previous_cursor\":\"2\",\"cursor\":\"3\",\"surface\":\"otel_log\",\"received_at_unix_ms\":200,\"event_name\":\"claude_code.api_request\",\"attributes\":{\"session_id\":\"session-1\",\"prompt_id\":\"prompt-1\",\"request_id\":\"request-1\",\"model\":\"claude-sonnet-5\",\"input_tokens\":42,\"success\":true}}"
        );
        let batch = parse_handoff_jsonl(input).unwrap();
        let requests = batch
            .observations()
            .filter(|observation| {
                matches!(observation.event, ObservationEvent::ModelRequest { .. })
            })
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].token_usage.input, Some(42));
        assert!(matches!(
            &requests[0].event,
            ObservationEvent::ModelRequest { model } if model.as_deref() == Some("claude-sonnet-5")
        ));
        let debug = format!("{batch:?}");
        assert!(!debug.contains("RAW_HOOK_MODEL"));
        assert!(!debug.contains("RAW_HOOK_TOOL"));
    }

    #[test]
    fn missing_prompt_id_and_unknown_model_are_fixed_diagnostics() {
        let input = concat!(
            "{\"schema_version\":\"claude_handoff.v1\",\"source_generation\":\"claude-code-2.1.248\",\"previous_cursor\":null,\"cursor\":\"1\",\"surface\":\"hook\",\"received_at_unix_ms\":100,\"event_name\":\"Stop\",\"attributes\":{\"session_id\":\"session-1\"}}\n",
            "{\"schema_version\":\"claude_handoff.v1\",\"source_generation\":\"claude-code-2.1.248\",\"previous_cursor\":\"1\",\"cursor\":\"2\",\"surface\":\"otel_log\",\"received_at_unix_ms\":200,\"event_name\":\"claude_code.api_request\",\"attributes\":{\"session_id\":\"session-1\",\"prompt_id\":\"prompt-1\",\"request_id\":\"request-1\",\"model\":\"RAW_UNKNOWN_MODEL\"}}"
        );
        let batch = parse_handoff_jsonl(input).unwrap();
        let diagnostics = batch.diagnostics().collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].code, DiagnosticCode::MissingCorrelation);
        assert_eq!(diagnostics[1].code, DiagnosticCode::InvalidFieldType);
        assert!(!format!("{batch:?}").contains("RAW_UNKNOWN_MODEL"));
    }

    #[test]
    fn duration_underflow_is_a_fixed_diagnostic() {
        let input = "{\"schema_version\":\"claude_handoff.v1\",\"source_generation\":\"claude-code-2.1.248\",\"previous_cursor\":null,\"cursor\":\"1\",\"surface\":\"otel_log\",\"received_at_unix_ms\":10,\"event_name\":\"claude_code.api_request\",\"attributes\":{\"session_id\":\"session-1\",\"prompt_id\":\"prompt-1\",\"request_id\":\"request-1\",\"model\":\"claude-test\",\"duration_ms\":11}}";
        let batch = parse_handoff_jsonl(input).unwrap();
        assert_eq!(batch.observations().count(), 0);
        assert_eq!(
            batch.diagnostics().next().unwrap().code,
            DiagnosticCode::InvalidFieldType
        );
    }

    #[test]
    fn content_and_identity_never_cross_the_adapter_boundary() {
        let debug = format!("{:?}", parse_handoff_jsonl(FIXTURE).unwrap());
        for secret in RAW_SENTINELS {
            assert!(!debug.contains(secret));
        }
    }

    #[test]
    fn rejects_cursor_gaps_and_bounded_input_overflow() {
        let gap = FIXTURE.replacen("\"previous_cursor\":\"1\"", "\"previous_cursor\":null", 1);
        assert!(matches!(
            parse_handoff_jsonl(&gap),
            Err(AdapterError::InvalidCursorSequence)
        ));
        assert!(matches!(
            parse_handoff_jsonl(&"x".repeat(usize::try_from(MAX_HANDOFF_BYTES).unwrap() + 1)),
            Err(AdapterError::HandoffTooLarge)
        ));
        let oversized_line = format!("{{\"padding\":\"{}\"}}", "x".repeat(MAX_HANDOFF_LINE_BYTES));
        assert!(matches!(
            parse_handoff_jsonl(&oversized_line),
            Err(AdapterError::RecordTooLarge)
        ));
        let too_many = "\n".repeat(MAX_HANDOFF_LINES + 1);
        assert!(matches!(
            parse_handoff_jsonl(&too_many),
            Err(AdapterError::TooManyRecords)
        ));
    }

    #[test]
    fn truncated_tail_fails_closed_without_returning_a_partial_batch() {
        let truncated = format!("{}\n{{\"schema_version\":", FIXTURE.lines().next().unwrap());
        assert!(matches!(
            parse_handoff_jsonl(&truncated),
            Err(AdapterError::InvalidJson)
        ));
    }

    #[test]
    fn source_generation_rotation_has_an_independent_cursor_scope() {
        let input = concat!(
            "{\"schema_version\":\"claude_handoff.v1\",\"source_generation\":\"claude-code-2.1.248-a\",\"previous_cursor\":null,\"cursor\":\"1\",\"surface\":\"hook\",\"received_at_unix_ms\":100,\"event_name\":\"SessionStart\",\"attributes\":{\"session_id\":\"session-1\",\"model\":\"claude-test\"}}\n",
            "{\"schema_version\":\"claude_handoff.v1\",\"source_generation\":\"claude-code-2.1.248-b\",\"previous_cursor\":null,\"cursor\":\"1\",\"surface\":\"hook\",\"received_at_unix_ms\":200,\"event_name\":\"SessionStart\",\"attributes\":{\"session_id\":\"session-1\",\"model\":\"claude-test\"}}"
        );
        let batch = parse_handoff_jsonl(input).unwrap();
        let observations = batch.observations().collect::<Vec<_>>();
        assert_eq!(observations.len(), 2);
        assert_ne!(
            observations[0].source_generation,
            observations[1].source_generation
        );
        assert_eq!(observations[0].source_cursor.as_str(), "1");
        assert_eq!(observations[1].source_cursor.as_str(), "1");
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

    #[cfg(unix)]
    #[test]
    fn file_reader_requires_a_private_regular_file() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let root = std::env::temp_dir().join(format!(
            "agent-observability-claude-file-{}",
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
        assert_eq!(read_handoff_file(&path).unwrap().observations().count(), 7);
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
            read_handoff_file(&link),
            Err(AdapterError::SymbolicLink)
        ));
        assert!(matches!(
            read_handoff_file(&root),
            Err(AdapterError::InvalidFileType)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_store_projection_is_exact_and_private() {
        use agent_observability_local_store::LocalStore;

        let root = std::env::temp_dir().join(format!(
            "agent-observability-claude-projection-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut store = LocalStore::open(&root).unwrap();
        let batch = parse_handoff_jsonl(FIXTURE).unwrap();
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
        assert_eq!(durable, EXPECTED_PROJECTION);
        for secret in RAW_SENTINELS {
            assert!(!durable.contains(secret));
        }
        assert_eq!(store.observation_count().unwrap(), 7);
        assert_eq!(store.disposition_count().unwrap(), 3);
        assert_eq!(
            store
                .cursor("claude-code", "claude-code-2.1.248")
                .unwrap()
                .as_deref(),
            Some("10")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn capability_manifest_locks_the_fixture_hashes() {
        use agent_observability_contracts::{ADAPTER_CAPABILITY_V1, AdapterCapabilityManifestV1};
        use sha2::{Digest, Sha256};

        fn hash(input: &str) -> String {
            let digest = Sha256::digest(input.as_bytes());
            let mut output = String::from("sha256:");
            for byte in digest {
                use std::fmt::Write as _;
                write!(output, "{byte:02x}").unwrap();
            }
            output
        }

        let manifest = AdapterCapabilityManifestV1::parse_and_validate(ADAPTER_CAPABILITY_V1)
            .expect("capability manifest validates");
        let capability = manifest
            .entries
            .iter()
            .find(|entry| entry.adapter_family == "claude-code")
            .expect("Claude Code capability exists");
        assert_eq!(hash(FIXTURE), EXPECTED_HANDOFF_HASH);
        assert_eq!(hash(EXPECTED_PROJECTION), EXPECTED_PROJECTION_HASH);
        assert_eq!(
            capability.fixture_hashes.get("claude-handoff.jsonl"),
            Some(&EXPECTED_HANDOFF_HASH.to_owned())
        );
        assert_eq!(
            capability.fixture_hashes.get("claude-projection.jsonl"),
            Some(&EXPECTED_PROJECTION_HASH.to_owned())
        );
    }
}
