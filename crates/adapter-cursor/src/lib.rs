//! Bounded, content-free Cursor Hook v1 handoff adapter.

use agent_observability_contracts::{
    AdapterDispositionKind, AgentSource, ObservationEvent, SourceCheckpoint, SourceObservation,
    canonical_observation_payload_hash,
};
use agent_observability_domain::{
    CorrelationIds, LifecycleState, ObservationId, OperationId, SessionId, SourceCursor,
    SourceGeneration, SpanId, Timing, TokenUsage, TraceId, TurnId,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fmt::{self, Display, Formatter};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

pub const HANDOFF_SCHEMA_VERSION: &str = "cursor_handoff.v1";
pub const MAX_HANDOFF_BYTES: u64 = 1024 * 1024;
pub const MAX_HANDOFF_LINES: usize = 4096;
pub const MAX_HANDOFF_LINE_BYTES: usize = 64 * 1024;
const KNOWN_MODELS: &[&str] = &["cursor-test", "gpt-4o", "claude-3.5-sonnet", "sonnet-4"];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceSurface {
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
            AdapterItem::Observation(v) => Some(v.as_ref()),
            AdapterItem::Disposition(_) => None,
        })
    }
    pub fn diagnostics(&self) -> impl Iterator<Item = &AdapterDiagnostic> {
        self.items.iter().filter_map(|item| match item {
            AdapterItem::Disposition(v) => Some(v),
            AdapterItem::Observation(_) => None,
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
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Io(_) => "Cursor handoff I/O failure",
            Self::HandoffTooLarge => "Cursor handoff exceeds the byte limit",
            Self::TooManyRecords => "Cursor handoff exceeds the record limit",
            Self::RecordTooLarge => "Cursor handoff record exceeds the byte limit",
            Self::InvalidJson => "Cursor handoff contains invalid JSON",
            Self::InvalidSchema => "Cursor handoff schema is unsupported",
            Self::InvalidCursorSequence => "Cursor handoff cursor sequence is invalid",
            Self::UnsupportedPlatform => "Cursor handoff requires Unix",
            Self::InsecurePermissions => "Cursor handoff permissions are too broad",
            Self::SymbolicLink => "Cursor handoff must not be a symbolic link",
            Self::InvalidFileType => "Cursor handoff must be a regular file",
            Self::InvalidIdentifier => "Cursor handoff contains an invalid identifier",
            Self::InvalidFieldType => "Cursor handoff contains an invalid field value",
            Self::InvalidTiming => "Cursor handoff contains invalid timing",
        })
    }
}
impl std::error::Error for AdapterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}
impl From<io::Error> for AdapterError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Reads and validates a bounded private Unix regular-file handoff.
///
/// # Errors
///
/// Returns an error for unsafe file metadata, bounds, malformed JSON, schema drift, or cursor
/// sequence violations.
pub fn read_handoff_file(path: impl AsRef<Path>) -> Result<AdapterBatch, AdapterError> {
    if !cfg!(unix) {
        return Err(AdapterError::UnsupportedPlatform);
    }
    let path = path.as_ref();
    let before = fs::symlink_metadata(path)?;
    if before.file_type().is_symlink() {
        return Err(AdapterError::SymbolicLink);
    }
    if !before.is_file() {
        return Err(AdapterError::InvalidFileType);
    }
    private_permissions(&before)?;
    let file = File::open(path)?;
    let after = file.metadata()?;
    if !after.is_file() || !same_identity(&before, &after) {
        return Err(AdapterError::InvalidFileType);
    }
    private_permissions(&after)?;
    if after.len() > MAX_HANDOFF_BYTES {
        return Err(AdapterError::HandoffTooLarge);
    }
    let mut input = String::new();
    file.take(MAX_HANDOFF_BYTES + 1)
        .read_to_string(&mut input)?;
    if input.len() as u64 > MAX_HANDOFF_BYTES {
        return Err(AdapterError::HandoffTooLarge);
    }
    parse_handoff_jsonl(&input)
}

#[cfg(unix)]
fn same_identity(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}
#[cfg(not(unix))]
fn same_identity(_: &fs::Metadata, _: &fs::Metadata) -> bool {
    false
}
#[cfg(unix)]
fn private_permissions(m: &fs::Metadata) -> Result<(), AdapterError> {
    use std::os::unix::fs::PermissionsExt;
    if m.permissions().mode() & 0o077 != 0 {
        Err(AdapterError::InsecurePermissions)
    } else {
        Ok(())
    }
}
#[cfg(not(unix))]
fn private_permissions(_: &fs::Metadata) -> Result<(), AdapterError> {
    Err(AdapterError::UnsupportedPlatform)
}

/// Parses a bounded Cursor JSONL handoff into content-free observations and fixed diagnostics.
///
/// # Errors
///
/// Returns an error for bounds, malformed JSON, schema drift, or cursor sequence violations.
pub fn parse_handoff_jsonl(input: &str) -> Result<AdapterBatch, AdapterError> {
    if input.len() as u64 > MAX_HANDOFF_BYTES {
        return Err(AdapterError::HandoffTooLarge);
    }
    let lines = input.lines().collect::<Vec<_>>();
    if lines.len() > MAX_HANDOFF_LINES {
        return Err(AdapterError::TooManyRecords);
    }
    for line in &lines {
        if line.len() > MAX_HANDOFF_LINE_BYTES {
            return Err(AdapterError::RecordTooLarge);
        }
    }
    let records = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| {
            serde_json::from_str::<HandoffRecord>(l)
                .map(|r| (i + 1, r))
                .map_err(|_| AdapterError::InvalidJson)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut cursors = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut batch = AdapterBatch::default();
    for (index, record) in records {
        if record.schema_version != HANDOFF_SCHEMA_VERSION {
            return Err(AdapterError::InvalidSchema);
        }
        match record.surface {
            SourceSurface::Hook => {}
        }
        validate_cursor(&record, &cursors)?;
        cursors.insert(record.source_generation.clone(), record.cursor.clone());
        let checkpoint = checkpoint(&record)?;
        let mapping = map_record(&record);
        match mapping {
            Mapping::Diagnostic(code) => {
                batch
                    .items
                    .push(AdapterItem::Disposition(AdapterDiagnostic {
                        record_index: index,
                        code,
                        checkpoint,
                        disposition: AdapterDispositionKind::Diagnostic,
                        payload_hash: None,
                    }));
            }
            Mapping::Observation(observation) => {
                let hash = canonical_observation_payload_hash(&observation)
                    .map_err(|_| AdapterError::InvalidFieldType)?;
                let key = (
                    observation.source_generation.as_str().to_owned(),
                    observation.span_id.as_str().to_owned(),
                    hash.clone(),
                );
                if seen.insert(key) {
                    batch.items.push(AdapterItem::Observation(observation));
                } else {
                    batch
                        .items
                        .push(AdapterItem::Disposition(AdapterDiagnostic {
                            record_index: index,
                            code: DiagnosticCode::DuplicateObservation,
                            checkpoint,
                            disposition: AdapterDispositionKind::Suppressed,
                            payload_hash: Some(hash),
                        }));
                }
            }
        }
    }
    Ok(batch)
}

enum Mapping {
    Observation(Box<SourceObservation>),
    Diagnostic(DiagnosticCode),
}
fn validate_cursor(
    r: &HandoffRecord,
    cursors: &BTreeMap<String, String>,
) -> Result<(), AdapterError> {
    let expected = cursors.get(&r.source_generation).map(String::as_str);
    if r.cursor.is_empty()
        || r.previous_cursor.as_deref() == Some(r.cursor.as_str())
        || expected.is_some_and(|v| r.previous_cursor.as_deref() != Some(v))
    {
        Err(AdapterError::InvalidCursorSequence)
    } else {
        Ok(())
    }
}
fn checkpoint(r: &HandoffRecord) -> Result<SourceCheckpoint, AdapterError> {
    Ok(SourceCheckpoint {
        source: AgentSource::Cursor,
        source_generation: parse::<SourceGeneration>(&r.source_generation)?,
        previous_source_cursor: r
            .previous_cursor
            .as_deref()
            .map(parse::<SourceCursor>)
            .transpose()?,
        source_cursor: parse::<SourceCursor>(&r.cursor)?,
    })
}
fn correlation_key(r: &HandoffRecord) -> Result<(String, String, String), AdapterError> {
    Ok((
        required(&r.attributes, "conversation_id")?,
        required(&r.attributes, "generation_id")?,
        required(&r.attributes, "tool_use_id")?,
    ))
}
fn is_generic_tool(e: &str) -> bool {
    matches!(e, "preToolUse" | "postToolUse" | "postToolUseFailure")
}
fn is_specific_tool(e: &str) -> bool {
    matches!(
        e,
        "beforeShellExecution"
            | "afterShellExecution"
            | "beforeMCPExecution"
            | "afterMCPExecution"
            | "beforeReadFile"
            | "afterFileEdit"
    )
}
fn map_record(r: &HandoffRecord) -> Mapping {
    match r.event_name.as_str() {
        "sessionStart" | "sessionEnd" => session(r),
        "beforeSubmitPrompt" | "stop" => turn(r),
        e if is_generic_tool(e) => tool(r),
        e if is_specific_tool(e) => Mapping::Diagnostic(DiagnosticCode::UnsupportedEventVariant),
        _ => Mapping::Diagnostic(if is_content_event(&r.event_name) {
            DiagnosticCode::ContentEventIgnored
        } else {
            DiagnosticCode::UnsupportedEvent
        }),
    }
}
fn is_content_event(e: &str) -> bool {
    matches!(
        e,
        "prompt" | "response" | "assistantResponse" | "userPrompt" | "message"
    )
}
fn session(r: &HandoffRecord) -> Mapping {
    let Ok(conversation) = required(&r.attributes, "conversation_id") else {
        return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation);
    };
    if required(&r.attributes, "generation_id").is_err() {
        return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation);
    }
    let Ok(session_id) = required(&r.attributes, "session_id") else {
        return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation);
    };
    if session_id != conversation {
        return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation);
    }
    let Ok(session_id) = parse::<SessionId>(&session_id) else {
        return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation);
    };
    let model = match model(&r.attributes) {
        Ok(v) => v,
        Err(c) => return Mapping::Diagnostic(c),
    };
    let session_end_reason = if r.event_name == "sessionEnd" {
        let Some(reason) = string(&r.attributes, "reason") else {
            return Mapping::Diagnostic(DiagnosticCode::InvalidFieldType);
        };
        if !matches!(
            reason.as_str(),
            "completed" | "aborted" | "error" | "window_close" | "user_close"
        ) {
            return Mapping::Diagnostic(DiagnosticCode::InvalidFieldType);
        }
        if required(&r.attributes, "final_status").is_err() {
            return Mapping::Diagnostic(DiagnosticCode::InvalidFieldType);
        }
        Some(reason)
    } else {
        None
    };
    let (lifecycle, duration) = if r.event_name == "sessionStart" {
        (LifecycleState::Running, 0)
    } else {
        let lifecycle = match session_end_reason.as_deref() {
            Some("completed") => LifecycleState::Completed,
            Some("error") => LifecycleState::Failed,
            Some("aborted" | "window_close" | "user_close") => LifecycleState::Interrupted,
            _ => return Mapping::Diagnostic(DiagnosticCode::InvalidFieldType),
        };
        let Ok(duration) = required_u64(&r.attributes, "duration_ms") else {
            return Mapping::Diagnostic(DiagnosticCode::InvalidFieldType);
        };
        (lifecycle, duration)
    };
    build(
        r,
        &conversation,
        None,
        "session",
        CorrelationIds {
            session_id: Some(session_id),
            ..Default::default()
        },
        ObservationEvent::Session {
            model,
            project: None,
        },
        lifecycle,
        duration,
    )
}
#[allow(clippy::many_single_char_names)]
fn turn(r: &HandoffRecord) -> Mapping {
    let (Ok(c), Ok(g)) = (
        required(&r.attributes, "conversation_id"),
        required(&r.attributes, "generation_id"),
    ) else {
        return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation);
    };
    let (Ok(s), Ok(t)) = (parse::<SessionId>(&c), parse::<TurnId>(&g)) else {
        return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation);
    };
    let lifecycle = if r.event_name == "beforeSubmitPrompt" {
        LifecycleState::Running
    } else {
        match string(&r.attributes, "status").as_deref() {
            Some("completed") => LifecycleState::Completed,
            Some("aborted") => LifecycleState::Interrupted,
            Some("error") => LifecycleState::Failed,
            _ => return Mapping::Diagnostic(DiagnosticCode::InvalidFieldType),
        }
    };
    build(
        r,
        &c,
        Some(&g),
        "turn",
        CorrelationIds {
            session_id: Some(s),
            turn_id: Some(t),
            ..Default::default()
        },
        ObservationEvent::Turn,
        lifecycle,
        0,
    )
}
#[allow(clippy::many_single_char_names)]
fn tool(r: &HandoffRecord) -> Mapping {
    let Ok((c, g, u)) = correlation_key(r) else {
        return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation);
    };
    let (Ok(s), Ok(t), Ok(o)) = (
        parse::<SessionId>(&c),
        parse::<TurnId>(&g),
        parse::<OperationId>(&u),
    ) else {
        return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation);
    };
    let Some(tool_name) = string(&r.attributes, "tool_name") else {
        return Mapping::Diagnostic(DiagnosticCode::InvalidFieldType);
    };
    let category = tool_category(r, Some(&tool_name));
    let phase = if r.event_name.starts_with("pre") || r.event_name.starts_with("before") {
        "start"
    } else if r.event_name == "postToolUseFailure" {
        "failure"
    } else {
        "result"
    };
    let lifecycle = if phase == "failure" {
        let Some(failure_type) = string(&r.attributes, "failure_type") else {
            return Mapping::Diagnostic(DiagnosticCode::InvalidFieldType);
        };
        if !matches!(
            failure_type.as_str(),
            "error" | "timeout" | "permission_denied"
        ) {
            return Mapping::Diagnostic(DiagnosticCode::InvalidFieldType);
        }
        match required_bool(&r.attributes, "is_interrupt") {
            Ok(true) => LifecycleState::Interrupted,
            Ok(false) => LifecycleState::Failed,
            Err(_) => return Mapping::Diagnostic(DiagnosticCode::InvalidFieldType),
        }
    } else if phase == "start" {
        LifecycleState::Running
    } else {
        LifecycleState::Completed
    };
    build(
        r,
        &c,
        Some(&g),
        &format!("tool:{u}"),
        CorrelationIds {
            session_id: Some(s),
            turn_id: Some(t),
            operation_id: Some(o),
            ..Default::default()
        },
        ObservationEvent::ToolOperation {
            tool_name: Some(category.into()),
            phase: Some(phase.into()),
        },
        lifecycle,
        if r.event_name == "preToolUse" {
            0
        } else {
            match required_u64(&r.attributes, "duration") {
                Ok(value) => value,
                Err(_) => return Mapping::Diagnostic(DiagnosticCode::InvalidFieldType),
            }
        },
    )
}
fn tool_category(r: &HandoffRecord, tool_name: Option<&str>) -> &'static str {
    if r.event_name.contains("Shell") {
        "shell"
    } else if r.event_name.contains("MCP") {
        "mcp"
    } else if r.event_name.contains("ReadFile") || r.event_name.contains("FileEdit") {
        "file"
    } else {
        match tool_name.map(str::to_ascii_lowercase).as_deref() {
            Some("shell" | "exec" | "terminal") => "shell",
            Some(value) if value == "mcp" || value.starts_with("mcp:") => "mcp",
            Some("read" | "write" | "edit" | "delete" | "grep" | "glob" | "search" | "file") => {
                "file"
            }
            Some("task" | "agent") => "agent",
            _ => "other",
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn build(
    r: &HandoffRecord,
    c: &str,
    turn: Option<&str>,
    leaf: &str,
    correlation: CorrelationIds,
    event: ObservationEvent,
    lifecycle: LifecycleState,
    duration: u64,
) -> Mapping {
    let end = r.received_at_unix_ms;
    let Some(start) = end.checked_sub(duration) else {
        return Mapping::Diagnostic(DiagnosticCode::InvalidFieldType);
    };
    let Ok(timing) = Timing::new(start, Some(end)) else {
        return Mapping::Diagnostic(DiagnosticCode::InvalidFieldType);
    };
    let session_span = stable_id("cursor-session", &[c]);
    let (span, parent) = match turn {
        None => (session_span.clone(), None),
        Some(g) if leaf == "turn" => (stable_id("cursor-turn", &[c, g]), Some(session_span)),
        Some(g) => (
            stable_id("cursor-span", &[c, g, leaf]),
            Some(stable_id("cursor-turn", &[c, g])),
        ),
    };
    let Ok(source_generation) = parse::<SourceGeneration>(&r.source_generation) else {
        return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation);
    };
    let previous_source_cursor = match r.previous_cursor.as_deref() {
        Some(value) => match parse::<SourceCursor>(value) {
            Ok(value) => Some(value),
            Err(_) => return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation),
        },
        None => None,
    };
    let Ok(source_cursor) = parse::<SourceCursor>(&r.cursor) else {
        return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation);
    };
    let Ok(observation_id) = parse::<ObservationId>(&stable_id(
        "cursor-observation",
        &[&r.source_generation, &r.cursor, &r.event_name],
    )) else {
        return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation);
    };
    let Ok(trace_id) = parse::<TraceId>(&stable_id("cursor-trace", &[c])) else {
        return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation);
    };
    let Ok(span_id) = parse::<SpanId>(&span) else {
        return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation);
    };
    let parent_span_id = match parent {
        Some(value) => match parse::<SpanId>(&value) {
            Ok(value) => Some(value),
            Err(_) => return Mapping::Diagnostic(DiagnosticCode::MissingCorrelation),
        },
        None => None,
    };
    Mapping::Observation(Box::new(SourceObservation {
        source: AgentSource::Cursor,
        source_generation,
        previous_source_cursor,
        source_cursor,
        observation_id,
        trace_id,
        span_id,
        parent_span_id,
        correlation,
        event,
        lifecycle,
        timing,
        token_usage: TokenUsage::default(),
    }))
}
fn required(a: &BTreeMap<String, Value>, k: &str) -> Result<String, AdapterError> {
    match a.get(k) {
        Some(Value::String(v)) if !v.is_empty() => Ok(v.clone()),
        _ => Err(AdapterError::InvalidIdentifier),
    }
}
fn string(a: &BTreeMap<String, Value>, k: &str) -> Option<String> {
    match a.get(k) {
        Some(Value::String(v)) if !v.is_empty() => Some(v.clone()),
        _ => None,
    }
}
fn model(a: &BTreeMap<String, Value>) -> Result<Option<String>, DiagnosticCode> {
    match a.get("model") {
        Some(Value::String(value)) if value.is_empty() => Err(DiagnosticCode::InvalidFieldType),
        Some(Value::String(value)) if KNOWN_MODELS.contains(&value.as_str()) => {
            Ok(Some(value.clone()))
        }
        None | Some(Value::String(_)) => Ok(None),
        Some(_) => Err(DiagnosticCode::InvalidFieldType),
    }
}
fn required_u64(a: &BTreeMap<String, Value>, k: &str) -> Result<u64, AdapterError> {
    match a.get(k) {
        Some(Value::Number(value)) => value.as_u64().ok_or(AdapterError::InvalidTiming),
        _ => Err(AdapterError::InvalidTiming),
    }
}
fn required_bool(a: &BTreeMap<String, Value>, k: &str) -> Result<bool, AdapterError> {
    match a.get(k) {
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(AdapterError::InvalidFieldType),
    }
}
trait Parse: Sized {
    fn parse(v: &str) -> Result<Self, agent_observability_domain::DomainError>;
}
macro_rules! parses { ($($t:ty),+ $(,)?) => { $(impl Parse for $t { fn parse(v: &str) -> Result<Self, agent_observability_domain::DomainError> { Self::parse(v) } })+ }; }
parses!(
    TraceId,
    SpanId,
    SessionId,
    TurnId,
    OperationId,
    SourceCursor,
    SourceGeneration,
    ObservationId
);
fn parse<T: Parse>(v: &str) -> Result<T, AdapterError> {
    T::parse(v).map_err(|_| AdapterError::InvalidIdentifier)
}
fn stable_id(prefix: &str, components: &[&str]) -> String {
    let mut d = Sha256::new();
    for c in components {
        d.update(c.len().to_be_bytes());
        d.update(c.as_bytes());
    }
    let mut s = format!("{prefix}:");
    for b in d.finalize() {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = include_str!("../tests/fixtures/cursor-handoff.jsonl");
    const HANDOFF_HASH: &str =
        "sha256:4cf6068af8bd16470d27d58efd85bd3a08bcf376717f7691b28700ebc49890f3";
    const PROJECTION_HASH: &str =
        "sha256:05dcb372927b809192ec9f307f1c537fdfb8753d9fb78ce02bc1742e8d4bef27";
    #[test]
    fn fixture_maps_cursor_events_and_is_content_free() {
        let b = parse_handoff_jsonl(FIXTURE).unwrap();
        assert_eq!(b.observations().count(), 7);
        assert_eq!(b.diagnostics().count(), 6);
        let debug = format!("{b:?}");
        for s in [
            "RAW_EMAIL",
            "RAW_PATH",
            "RAW_COMMAND",
            "RAW_PROMPT",
            "RAW_OUTPUT",
            "RAW_MCP",
        ] {
            assert!(!debug.contains(s));
        }
        assert!(b.observations().all(|o| o.source == AgentSource::Cursor));
    }
    #[test]
    fn generic_primary_ignores_uncorrelated_specific_and_failure_is_failed() {
        let b = parse_handoff_jsonl(FIXTURE).unwrap();
        assert!(
            b.diagnostics()
                .any(|d| d.code == DiagnosticCode::UnsupportedEventVariant)
        );
        assert!(
            b.observations()
                .any(|o| o.lifecycle == LifecycleState::Failed)
        );
        assert!(
            b.observations()
                .any(|o| o.lifecycle == LifecycleState::Interrupted)
        );
        let operation = b
            .observations()
            .filter(|observation| {
                observation
                    .correlation
                    .operation_id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == "tool-1")
            })
            .collect::<Vec<_>>();
        assert_eq!(operation.len(), 2);
        assert_eq!(operation[0].span_id, operation[1].span_id);
        let interrupted = FIXTURE
            .lines()
            .nth(4)
            .unwrap()
            .replace("\"is_interrupt\":false", "\"is_interrupt\":true");
        let interrupted = parse_handoff_jsonl(&interrupted).unwrap();
        assert_eq!(
            interrupted.observations().next().unwrap().lifecycle,
            LifecycleState::Interrupted
        );
    }
    #[test]
    fn bounds_rotation_tail_and_bad_fields_fail_closed() {
        let tail = FIXTURE.lines().nth(2).unwrap();
        assert!(parse_handoff_jsonl(tail).is_ok());
        let malformed_tail = format!("{tail}\n{{");
        assert!(matches!(
            parse_handoff_jsonl(&malformed_tail),
            Err(AdapterError::InvalidJson)
        ));
        let rotated = tail
            .replace("cursor-v3.17.21", "cursor-v3.17.21-rotated")
            .replace("\"cursor\":\"3\"", "\"cursor\":\"1\"");
        assert!(parse_handoff_jsonl(&rotated).is_ok());
        assert!(matches!(
            parse_handoff_jsonl(&"x".repeat(usize::try_from(MAX_HANDOFF_BYTES).unwrap() + 1)),
            Err(AdapterError::HandoffTooLarge)
        ));
        assert!(matches!(
            parse_handoff_jsonl(&format!("{}\n", "x".repeat(MAX_HANDOFF_LINE_BYTES + 1))),
            Err(AdapterError::RecordTooLarge)
        ));
        assert!(matches!(
            parse_handoff_jsonl(&"\n".repeat(MAX_HANDOFF_LINES + 1)),
            Err(AdapterError::TooManyRecords)
        ));
        let bad = tail.replace("\"generation_id\":\"generation-1\"", "\"generation_id\":7");
        let b = parse_handoff_jsonl(&bad).unwrap();
        assert_eq!(
            b.diagnostics().next().unwrap().code,
            DiagnosticCode::MissingCorrelation
        );
    }
    #[test]
    fn exact_bounds_and_utf8_bytes_are_enforced() {
        assert!(!matches!(
            parse_handoff_jsonl(&"x".repeat(usize::try_from(MAX_HANDOFF_BYTES).unwrap())),
            Err(AdapterError::HandoffTooLarge)
        ));
        assert!(matches!(
            parse_handoff_jsonl(&"x".repeat(MAX_HANDOFF_LINE_BYTES)),
            Err(AdapterError::InvalidJson)
        ));
        assert!(parse_handoff_jsonl(&" \n".repeat(MAX_HANDOFF_LINES)).is_ok());
        assert!(matches!(
            parse_handoff_jsonl(&" \n".repeat(MAX_HANDOFF_LINES + 1)),
            Err(AdapterError::TooManyRecords)
        ));
        assert!(matches!(
            parse_handoff_jsonl(&"é".repeat(MAX_HANDOFF_LINE_BYTES / 2 + 1)),
            Err(AdapterError::RecordTooLarge)
        ));
    }
    #[cfg(unix)]
    #[test]
    fn private_file_regular_and_symlink_checks() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = std::env::temp_dir().join(format!("cursor-adapter-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let p = root.join("handoff");
        fs::write(&p, FIXTURE).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            read_handoff_file(&p),
            Err(AdapterError::InsecurePermissions)
        ));
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(read_handoff_file(&p).is_ok());
        let l = root.join("link");
        symlink(&p, &l).unwrap();
        assert!(matches!(
            read_handoff_file(l),
            Err(AdapterError::SymbolicLink)
        ));
        let oversized = root.join("oversized");
        fs::write(
            &oversized,
            "x".repeat(usize::try_from(MAX_HANDOFF_BYTES).unwrap() + 1),
        )
        .unwrap();
        fs::set_permissions(&oversized, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(
            read_handoff_file(oversized),
            Err(AdapterError::HandoffTooLarge)
        ));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn durable_projection_fixture_is_redacted() {
        use agent_observability_local_store::LocalStore;
        let root = std::env::temp_dir().join(format!("cursor-projection-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut store = LocalStore::open(&root).unwrap();
        let b =
            parse_handoff_jsonl(&FIXTURE.lines().take(6).collect::<Vec<_>>().join("\n")).unwrap();
        for item in &b.items {
            match item {
                AdapterItem::Observation(o) => {
                    store.ingest_deferred_projection(o).unwrap();
                }
                AdapterItem::Disposition(d) => {
                    store
                        .ingest_disposition_with_payload(
                            &d.checkpoint,
                            d.disposition,
                            d.code,
                            d.payload_hash.as_deref(),
                        )
                        .unwrap();
                }
            }
        }
        store.rebuild_projection().unwrap();
        let text = fs::read_to_string(store.projection_path()).unwrap();
        assert!(!text.contains("RAW_"));
        assert_eq!(
            text,
            include_str!("../tests/fixtures/cursor-projection.jsonl")
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn persisted_generation_rotation_is_independent_and_all_store_files_are_private() {
        use agent_observability_local_store::{IngestStatus, LocalStore};
        let root = std::env::temp_dir().join(format!("cursor-rotation-{}", std::process::id()));
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
        let rotated = FIXTURE
            .lines()
            .nth(2)
            .unwrap()
            .replace("cursor-v3.17.21", "cursor-v3.17.21-rotated")
            .replace("\"previous_cursor\":\"2\"", "\"previous_cursor\":null")
            .replace("\"cursor\":\"3\"", "\"cursor\":\"1\"");
        let rotated_batch = parse_handoff_jsonl(&rotated).unwrap();
        let observation = rotated_batch.observations().next().unwrap();
        assert_eq!(
            store.ingest_deferred_projection(observation).unwrap(),
            IngestStatus::Committed
        );
        assert_eq!(
            store
                .cursor("cursor", "cursor-v3.17.21")
                .unwrap()
                .as_deref(),
            Some("13")
        );
        assert_eq!(
            store
                .cursor("cursor", "cursor-v3.17.21-rotated")
                .unwrap()
                .as_deref(),
            Some("1")
        );
        drop(store);
        for entry in fs::read_dir(&root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                let bytes = fs::read(path).unwrap();
                for secret in [
                    b"RAW_EMAIL".as_slice(),
                    b"RAW_PATH".as_slice(),
                    b"RAW_COMMAND".as_slice(),
                    b"RAW_PROMPT".as_slice(),
                    b"RAW_OUTPUT".as_slice(),
                    b"RAW_MCP".as_slice(),
                ] {
                    assert!(!bytes.windows(secret.len()).any(|window| window == secret));
                }
            }
        }
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn cursor_observation_recovers_across_store_crash_boundaries() {
        use agent_observability_local_store::{CrashPoint, IngestStatus, LocalStore, StoreError};
        let batch = parse_handoff_jsonl(FIXTURE).unwrap();
        let operation = batch
            .observations()
            .filter(|observation| {
                observation
                    .correlation
                    .operation_id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == "tool-1")
            })
            .collect::<Vec<_>>();
        assert_eq!(operation.len(), 2);
        let mut running = operation[0].clone();
        running.previous_source_cursor = None;
        running.source_cursor = SourceCursor::parse("1").unwrap();
        let mut failed = operation[1].clone();
        failed.previous_source_cursor = Some(SourceCursor::parse("1").unwrap());
        failed.source_cursor = SourceCursor::parse("2").unwrap();

        for point in [
            CrashPoint::BeforeCommit,
            CrashPoint::AfterCommit,
            CrashPoint::BeforeProjectionRename,
            CrashPoint::AfterProjectionRename,
        ] {
            let root = std::env::temp_dir().join(format!(
                "cursor-tool-crash-{point:?}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&root);
            let mut store = LocalStore::open(&root).unwrap();
            assert_eq!(store.ingest(&running).unwrap(), IngestStatus::Committed);
            assert!(matches!(
                store.ingest_with_crash(&failed, point),
                Err(StoreError::Crash(value)) if value == point
            ));
            drop(store);

            let mut reopened = LocalStore::open(&root).unwrap();
            let committed = point != CrashPoint::BeforeCommit;
            assert_eq!(reopened.record_count().unwrap(), 1);
            assert_eq!(
                reopened.observation_count().unwrap(),
                u64::from(committed) + 1
            );
            assert_eq!(
                reopened.ingest(&failed).unwrap(),
                if committed {
                    IngestStatus::Duplicate
                } else {
                    IngestStatus::Committed
                }
            );
            assert_eq!(reopened.record_count().unwrap(), 1);
            assert_eq!(reopened.observation_count().unwrap(), 2);
            let projection = fs::read_to_string(reopened.projection_path()).unwrap();
            assert_eq!(projection.lines().count(), 1);
            let record: Value = serde_json::from_str(projection.trim()).unwrap();
            assert_eq!(record["span_kind"], "tool.execution");
            assert_eq!(record["attributes"]["phase"], "failure");
            assert_eq!(record["status"]["code"], "error");
            assert_eq!(record["start_time_unix_ms"], 1_787_875_200_200_f64);
            assert_eq!(record["end_time_unix_ms"], 1_787_875_200_400_f64);
            let _ = fs::remove_dir_all(root);
        }
    }
    #[test]
    fn fixture_hashes_are_pinned_and_unknown_model_is_omitted() {
        use agent_observability_contracts::{ADAPTER_CAPABILITY_V1, AdapterCapabilityManifestV1};
        let mut d = Sha256::new();
        d.update(FIXTURE.as_bytes());
        let mut h = String::from("sha256:");
        for b in d.finalize() {
            write!(h, "{b:02x}").unwrap();
        }
        assert_eq!(h, HANDOFF_HASH);
        let projection = include_str!("../tests/fixtures/cursor-projection.jsonl");
        let mut d = Sha256::new();
        d.update(projection.as_bytes());
        let mut h = String::from("sha256:");
        for b in d.finalize() {
            write!(h, "{b:02x}").unwrap();
        }
        assert_eq!(h, PROJECTION_HASH);
        let manifest = AdapterCapabilityManifestV1::parse_and_validate(ADAPTER_CAPABILITY_V1)
            .expect("capability manifest validates");
        let cursor = manifest
            .entries
            .iter()
            .find(|entry| entry.adapter_family == "cursor")
            .expect("Cursor capability exists");
        assert_eq!(
            cursor.fixture_hashes.get("cursor-handoff.jsonl"),
            Some(&HANDOFF_HASH.to_owned())
        );
        assert_eq!(
            cursor.fixture_hashes.get("cursor-projection.jsonl"),
            Some(&PROJECTION_HASH.to_owned())
        );
        assert_eq!(
            cursor.correlation_keys,
            ["conversation_id", "generation_id", "tool_use_id"]
        );
        assert_eq!(cursor.surfaces.len(), 2);
        assert_eq!(cursor.surfaces[0].role, "primary");
        assert_eq!(
            cursor.surfaces[0].events,
            ["preToolUse", "postToolUse", "postToolUseFailure"]
        );
        assert!(
            cursor
                .fixture_ids
                .iter()
                .any(|id| id == "cursor-specific-tool-diagnostic-isolation-v1")
        );
        let input = r#"{"schema_version":"cursor_handoff.v1","source_generation":"cursor-v3.17.21","previous_cursor":null,"cursor":"1","surface":"hook","received_at_unix_ms":1,"event_name":"sessionStart","attributes":{"conversation_id":"c","generation_id":"g","session_id":"c","model":"unknown-model"}}"#;
        let b = parse_handoff_jsonl(input).unwrap();
        assert_eq!(b.diagnostics().count(), 0);
        let observation = b.observations().next().unwrap();
        assert!(matches!(
            &observation.event,
            ObservationEvent::Session { model: None, .. }
        ));
        let malformed = input.replace("\"unknown-model\"", "7");
        let malformed = parse_handoff_jsonl(&malformed).unwrap();
        assert_eq!(
            malformed.diagnostics().next().unwrap().code,
            DiagnosticCode::InvalidFieldType
        );
        let bad_reason = FIXTURE
            .lines()
            .nth(6)
            .unwrap()
            .replace("\"reason\":\"completed\"", "\"reason\":\"done\"");
        let bad_reason = parse_handoff_jsonl(&bad_reason).unwrap();
        assert_eq!(
            bad_reason.diagnostics().next().unwrap().code,
            DiagnosticCode::InvalidFieldType
        );
    }
}
