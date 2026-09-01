//! Bounded, content-free Codex OTel/notify handoff adapter.

use agent_observability_contracts::{
    AdapterDispositionKind, AgentSource, ObservationEvent, SourceCheckpoint, SourceObservation,
    canonical_observation_payload_hash, hash_opaque_identifier,
};
use agent_observability_domain::{
    CorrelationIds, LifecycleState, ObservationId, OperationId, PermissionId, RequestId, SessionId,
    SourceCursor, SourceGeneration, SpanId, Timing, TokenUsage, TraceId, TurnId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io;
use std::path::Path;

pub const HANDOFF_SCHEMA_VERSION: &str = "codex_handoff.v1";
pub const MAX_HANDOFF_BYTES: u64 = 1024 * 1024;
pub const MAX_HANDOFF_LINES: usize = 4096;
pub const MAX_HANDOFF_LINE_BYTES: usize = 64 * 1024;
pub const MAX_OTLP_LOG_RECORDS: usize = 4096;
pub const MAX_PENDING_OTLP_REQUESTS: usize = 1024;
pub const MAX_PERSISTED_OTLP_CORRELATION_BYTES: usize = 512 * 1024;
pub const OTLP_REQUEST_CORRELATION_TTL_MS: u64 = 5 * 60 * 1000;
const KNOWN_CODEX_MODELS: &[&str] = &[
    "gpt-test",
    "gpt-5.4",
    "gpt-5.5",
    "gpt-5.6-luna",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
];

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceSurface {
    OtelLog,
    Notify,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

type OtlpRequestPairKey = (String, String, String);

#[derive(Clone, Debug)]
struct PendingOtlpRequest {
    correlation_id: String,
    official_retry_identity: Option<String>,
    inserted_at_unix_ms: u64,
    sequence: u64,
    current_record_index: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedOtlpRequestCorrelationState {
    schema_version: String,
    next_sequence: u64,
    pending: Vec<PersistedPendingOtlpRequest>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedPendingOtlpRequest {
    source_generation_hash: String,
    conversation_hash: String,
    model_hash: String,
    correlation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    official_retry_identity: Option<String>,
    inserted_at_unix_ms: u64,
    sequence: u64,
}

/// Bounded, content-free correlation state for Codex OTLP requests split across HTTP exports.
#[derive(Clone, Debug, Default)]
pub struct OtlpRequestCorrelationState {
    pending: BTreeMap<OtlpRequestPairKey, VecDeque<PendingOtlpRequest>>,
    next_sequence: u64,
}

impl OtlpRequestCorrelationState {
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.values().map(VecDeque::len).sum()
    }

    /// Restores bounded, privacy-projected correlation state and expires stale entries.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] when the snapshot is malformed, oversized, or violates bounds.
    pub fn from_persisted_json(input: &str, now_unix_ms: u64) -> Result<Self, AdapterError> {
        if input.len() > MAX_PERSISTED_OTLP_CORRELATION_BYTES {
            return Err(AdapterError::HandoffTooLarge);
        }
        let persisted: PersistedOtlpRequestCorrelationState =
            serde_json::from_str(input).map_err(|_| AdapterError::InvalidJson)?;
        if persisted.schema_version != "codex_request_correlation.v1"
            || persisted.pending.len() > MAX_PENDING_OTLP_REQUESTS
        {
            return Err(AdapterError::InvalidSchema);
        }
        let mut state = Self {
            pending: BTreeMap::new(),
            next_sequence: persisted.next_sequence,
        };
        for pending in persisted.pending {
            if !is_private_hash(&pending.source_generation_hash)
                || !is_private_hash(&pending.conversation_hash)
                || !is_private_hash(&pending.model_hash)
                || !is_private_hash(&pending.correlation_id)
                || pending
                    .official_retry_identity
                    .as_deref()
                    .is_some_and(|identity| !is_private_hash(identity))
                || pending.sequence >= state.next_sequence
            {
                return Err(AdapterError::InvalidSchema);
            }
            state.push(
                (
                    pending.source_generation_hash,
                    pending.conversation_hash,
                    pending.model_hash,
                ),
                PendingOtlpRequest {
                    correlation_id: pending.correlation_id,
                    official_retry_identity: pending.official_retry_identity,
                    inserted_at_unix_ms: pending.inserted_at_unix_ms,
                    sequence: pending.sequence,
                    current_record_index: None,
                },
            );
        }
        state.expire(now_unix_ms);
        Ok(state)
    }

    /// Serializes only hashed identifiers and bounded scalar correlation state.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] if the bounded snapshot cannot be encoded.
    pub fn to_persisted_json(&self) -> Result<String, AdapterError> {
        let pending = self
            .pending
            .iter()
            .flat_map(|(key, queue)| {
                queue.iter().map(|request| PersistedPendingOtlpRequest {
                    source_generation_hash: key.0.clone(),
                    conversation_hash: key.1.clone(),
                    model_hash: key.2.clone(),
                    correlation_id: request.correlation_id.clone(),
                    official_retry_identity: request.official_retry_identity.clone(),
                    inserted_at_unix_ms: request.inserted_at_unix_ms,
                    sequence: request.sequence,
                })
            })
            .collect();
        let encoded = serde_json::to_string(&PersistedOtlpRequestCorrelationState {
            schema_version: "codex_request_correlation.v1".into(),
            next_sequence: self.next_sequence,
            pending,
        })
        .map_err(|_| AdapterError::InvalidJson)?;
        if encoded.len() > MAX_PERSISTED_OTLP_CORRELATION_BYTES {
            return Err(AdapterError::HandoffTooLarge);
        }
        Ok(encoded)
    }

    fn expire(&mut self, now_unix_ms: u64) {
        self.pending.retain(|_, queue| {
            queue.retain(|pending| {
                now_unix_ms.saturating_sub(pending.inserted_at_unix_ms)
                    < OTLP_REQUEST_CORRELATION_TTL_MS
            });
            !queue.is_empty()
        });
    }

    fn push(&mut self, key: OtlpRequestPairKey, pending: PendingOtlpRequest) {
        self.pending.entry(key).or_default().push_back(pending);
        while self.pending_len() > MAX_PENDING_OTLP_REQUESTS {
            self.evict_oldest();
        }
    }

    fn contains_official_retry(
        &self,
        key: &OtlpRequestPairKey,
        official_retry_identity: &str,
    ) -> bool {
        self.pending.get(key).is_some_and(|queue| {
            queue.iter().any(|pending| {
                pending.official_retry_identity.as_deref() == Some(official_retry_identity)
            })
        })
    }

    fn evict_oldest(&mut self) {
        let oldest_key = self
            .pending
            .iter()
            .filter_map(|(key, queue)| queue.front().map(|pending| (pending.sequence, key)))
            .min_by(Ord::cmp)
            .map(|(_, key)| key.clone());
        let Some(key) = oldest_key else {
            return;
        };
        let remove_key = self.pending.get_mut(&key).is_some_and(|queue| {
            queue.pop_front();
            queue.is_empty()
        });
        if remove_key {
            self.pending.remove(&key);
        }
    }

    fn next_sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        if let Some(next) = self.next_sequence.checked_add(1) {
            self.next_sequence = next;
        } else {
            self.pending.clear();
            self.next_sequence = 1;
        }
        sequence
    }

    fn finish_batch(&mut self) {
        self.pending.retain(|_, queue| {
            for pending in queue.iter_mut() {
                pending.current_record_index = None;
            }
            !queue.is_empty()
        });
    }
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

/// Decodes one bounded OTLP/HTTP JSON logs request into the existing content-free Codex adapter
/// boundary. Only explicitly owned scalar fields are copied; prompt text, tool arguments/output,
/// account identity, and unknown attributes are discarded before canonical mapping.
///
/// `first_cursor` must be the next monotonic cursor for `source_generation`. The returned cursor is
/// the last consumed cursor, or `previous_cursor` when the request contains no log records.
///
/// # Errors
///
/// Returns [`AdapterError`] for oversized or malformed OTLP JSON, invalid cursor values, or a
/// request containing more than [`MAX_OTLP_LOG_RECORDS`] records.
pub fn parse_otlp_http_json(
    input: &[u8],
    source_generation: &str,
    previous_cursor: Option<&str>,
    first_cursor: u64,
    fallback_received_at_unix_ms: u64,
) -> Result<(AdapterBatch, Option<String>), AdapterError> {
    parse_otlp_http_json_with_state(
        input,
        source_generation,
        previous_cursor,
        first_cursor,
        fallback_received_at_unix_ms,
        &mut OtlpRequestCorrelationState::default(),
    )
}

/// Stateful variant of [`parse_otlp_http_json`] for collectors that receive one OTLP export at a
/// time. State changes are applied only when the complete export maps successfully.
///
/// # Errors
///
/// Returns the same bounded decode and mapping errors as [`parse_otlp_http_json`].
pub fn parse_otlp_http_json_with_state(
    input: &[u8],
    source_generation: &str,
    previous_cursor: Option<&str>,
    first_cursor: u64,
    fallback_received_at_unix_ms: u64,
    state: &mut OtlpRequestCorrelationState,
) -> Result<(AdapterBatch, Option<String>), AdapterError> {
    if input.len() as u64 > MAX_HANDOFF_BYTES {
        return Err(AdapterError::HandoffTooLarge);
    }
    if source_generation.is_empty() || first_cursor == 0 {
        return Err(AdapterError::InvalidCursorSequence);
    }
    let root: Value = serde_json::from_slice(input).map_err(|_| AdapterError::InvalidJson)?;
    let resource_logs = root
        .get("resourceLogs")
        .and_then(Value::as_array)
        .ok_or(AdapterError::InvalidJson)?;
    let mut records = Vec::new();
    let mut next_cursor = first_cursor;
    let mut prior = previous_cursor.map(str::to_owned);
    for resource in resource_logs {
        let scope_logs = resource
            .get("scopeLogs")
            .and_then(Value::as_array)
            .ok_or(AdapterError::InvalidJson)?;
        for scope in scope_logs {
            let log_records = scope
                .get("logRecords")
                .and_then(Value::as_array)
                .ok_or(AdapterError::InvalidJson)?;
            for log_record in log_records {
                if records.len() >= MAX_OTLP_LOG_RECORDS {
                    return Err(AdapterError::TooManyRecords);
                }
                let cursor = next_cursor.to_string();
                next_cursor = next_cursor
                    .checked_add(1)
                    .ok_or(AdapterError::InvalidCursorSequence)?;
                let attributes = otlp_attributes(log_record)?;
                let event_name = attributes
                    .get("event.name")
                    .and_then(Value::as_str)
                    .ok_or(AdapterError::InvalidFieldType)?
                    .to_owned();
                let received_at_unix_ms = log_record
                    .get("timeUnixNano")
                    .and_then(otlp_u64)
                    .map_or(fallback_received_at_unix_ms, |value| value / 1_000_000);
                let canonical = canonical_otlp_attributes(&event_name, &attributes)?;
                records.push(HandoffRecord {
                    schema_version: HANDOFF_SCHEMA_VERSION.into(),
                    source_generation: source_generation.into(),
                    previous_cursor: prior.clone(),
                    cursor: cursor.clone(),
                    surface: SourceSurface::OtelLog,
                    received_at_unix_ms,
                    event_name,
                    attributes: canonical,
                });
                prior = Some(cursor);
            }
        }
    }
    let mut next_state = state.clone();
    correlate_otlp_request_pairs(
        &mut records,
        source_generation,
        fallback_received_at_unix_ms,
        &mut next_state,
    )?;
    let mut jsonl = String::new();
    for record in &records {
        jsonl.push_str(&serde_json::to_string(record).map_err(|_| AdapterError::InvalidJson)?);
        jsonl.push('\n');
    }
    let batch = parse_handoff_jsonl(&jsonl)?;
    *state = next_state;
    Ok((batch, prior))
}

/// Projects the raw Codex `agent-turn-complete` notify argument without retaining content fields.
///
/// # Errors
///
/// Returns [`AdapterError`] when the bounded JSON payload lacks the supported type or identifiers.
pub fn parse_notify_json(
    input: &[u8],
    source_generation: &str,
    previous_cursor: Option<&str>,
    cursor: u64,
    received_at_unix_ms: u64,
) -> Result<AdapterBatch, AdapterError> {
    if input.len() as u64 > MAX_HANDOFF_LINE_BYTES as u64 {
        return Err(AdapterError::RecordTooLarge);
    }
    let payload: Value = serde_json::from_slice(input).map_err(|_| AdapterError::InvalidJson)?;
    let event_name = payload
        .get("type")
        .and_then(Value::as_str)
        .filter(|value| *value == "agent-turn-complete")
        .ok_or(AdapterError::InvalidSchema)?;
    let thread_id = payload
        .get("thread-id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(AdapterError::InvalidIdentifier)?;
    let turn_id = payload
        .get("turn-id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(AdapterError::InvalidIdentifier)?;
    let mut attributes = BTreeMap::new();
    attributes.insert("thread_id".into(), Value::String(thread_id.into()));
    attributes.insert("turn_id".into(), Value::String(turn_id.into()));
    let record = HandoffRecord {
        schema_version: HANDOFF_SCHEMA_VERSION.into(),
        source_generation: source_generation.into(),
        previous_cursor: previous_cursor.map(str::to_owned),
        cursor: cursor.to_string(),
        surface: SourceSurface::Notify,
        received_at_unix_ms,
        event_name: event_name.into(),
        attributes,
    };
    let json = serde_json::to_string(&record).map_err(|_| AdapterError::InvalidJson)?;
    parse_handoff_jsonl(&json)
}

fn otlp_attributes(record: &Value) -> Result<BTreeMap<String, Value>, AdapterError> {
    let values = record
        .get("attributes")
        .and_then(Value::as_array)
        .ok_or(AdapterError::InvalidJson)?;
    let mut attributes = BTreeMap::new();
    for attribute in values {
        let key = attribute
            .get("key")
            .and_then(Value::as_str)
            .ok_or(AdapterError::InvalidJson)?;
        let value = attribute
            .get("value")
            .and_then(otlp_any_value)
            .ok_or(AdapterError::InvalidFieldType)?;
        attributes.insert(key.to_owned(), value);
    }
    Ok(attributes)
}

fn otlp_any_value(value: &Value) -> Option<Value> {
    if let Some(value) = value.get("stringValue").and_then(Value::as_str) {
        return Some(Value::String(value.to_owned()));
    }
    if let Some(value) = value.get("boolValue").and_then(Value::as_bool) {
        return Some(Value::Bool(value));
    }
    if let Some(value) = value.get("intValue").and_then(otlp_u64) {
        return Some(Value::Number(value.into()));
    }
    value
        .get("doubleValue")
        .and_then(Value::as_f64)
        .and_then(serde_json::Number::from_f64)
        .map(Value::Number)
}

fn otlp_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}

fn canonical_otlp_attributes(
    event_name: &str,
    source: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, AdapterError> {
    let mut output = BTreeMap::new();
    copy_string(source, &mut output, "conversation.id", "conversation_id")?;
    copy_string(source, &mut output, "turn.id", "turn_id")?;
    copy_string(source, &mut output, "model", "model")?;
    copy_string(source, &mut output, "event.kind", "kind")?;
    copy_string(source, &mut output, "tool_name", "tool_name")?;
    copy_string(source, &mut output, "call_id", "call_id")?;
    copy_string(source, &mut output, "decision", "decision")?;
    copy_u64(source, &mut output, "duration_ms", "duration_ms")?;
    copy_u64(source, &mut output, "input_token_count", "input_tokens")?;
    copy_u64(source, &mut output, "output_token_count", "output_tokens")?;
    copy_u64(
        source,
        &mut output,
        "cached_token_count",
        "cached_input_tokens",
    )?;
    copy_u64(
        source,
        &mut output,
        "reasoning_token_count",
        "reasoning_output_tokens",
    )?;
    copy_u64(source, &mut output, "tool_token_count", "total_tokens")?;

    if matches!(event_name, "codex.api_request" | "codex.sse_event") {
        copy_string(source, &mut output, "auth.request_id", "request_id")?;
    }
    let success = source
        .get("success")
        .and_then(otlp_bool)
        .or_else(|| success_from_status(source))
        .unwrap_or_else(|| !source.contains_key("error.message"));
    output.insert("success".into(), Value::Bool(success));
    Ok(output)
}

fn correlate_otlp_request_pairs(
    records: &mut [HandoffRecord],
    source_generation: &str,
    now_unix_ms: u64,
    state: &mut OtlpRequestCorrelationState,
) -> Result<(), AdapterError> {
    state.expire(now_unix_ms);
    for index in 0..records.len() {
        let event_name = records[index].event_name.as_str();
        let is_completed = event_name == "codex.sse_event"
            && optional_string(&records[index].attributes, "kind")?.as_deref()
                == Some("response.completed");
        if event_name != "codex.api_request" && !is_completed {
            continue;
        }
        let Some((conversation_id, model)) = otlp_request_pair_key(&records[index].attributes)?
        else {
            continue;
        };
        let key = (
            hash_opaque_identifier(source_generation),
            hash_opaque_identifier(&conversation_id),
            hash_opaque_identifier(&model),
        );
        if event_name == "codex.api_request" {
            let request_id = optional_string(&records[index].attributes, "request_id")?;
            let official_retry_identity = request_id.as_deref().map(hash_opaque_identifier);
            let correlation_id = official_retry_identity.clone().unwrap_or_else(|| {
                stable_id(
                    "id:sha256",
                    &[
                        "codex-local-request-v1",
                        source_generation,
                        &conversation_id,
                        &model,
                        &records[index].cursor,
                    ],
                )
            });
            records[index]
                .attributes
                .insert("request_id".into(), Value::String(correlation_id.clone()));
            if optional_bool(&records[index].attributes, "success")? != Some(true) {
                continue;
            }
            if official_retry_identity
                .as_deref()
                .is_some_and(|identity| state.contains_official_retry(&key, identity))
            {
                continue;
            }
            let sequence = state.next_sequence();
            state.push(
                key,
                PendingOtlpRequest {
                    correlation_id,
                    official_retry_identity,
                    inserted_at_unix_ms: now_unix_ms,
                    sequence,
                    current_record_index: Some(index),
                },
            );
            continue;
        }
        let pending = state.pending.get_mut(&key).and_then(VecDeque::pop_front);
        let remove_key = state.pending.get(&key).is_some_and(VecDeque::is_empty);
        if remove_key {
            state.pending.remove(&key);
        }
        let Some(pending) = pending else {
            continue;
        };
        let completed_request_id = optional_string(&records[index].attributes, "request_id")?;
        let request_id = match completed_request_id {
            Some(completed) if hash_opaque_identifier(&completed) == pending.correlation_id => {
                Some(completed)
            }
            Some(_) => None,
            None => Some(pending.correlation_id.clone()),
        };
        if let Some(request_id) = request_id {
            if let Some(api_index) = pending.current_record_index {
                records[api_index]
                    .attributes
                    .insert("request_id".into(), Value::String(request_id.clone()));
            }
            records[index]
                .attributes
                .insert("request_id".into(), Value::String(request_id));
        } else {
            if let Some(api_index) = pending.current_record_index {
                records[api_index].attributes.remove("request_id");
            }
            records[index].attributes.remove("request_id");
        }
    }
    state.finish_batch();
    Ok(())
}

fn is_private_hash(value: &str) -> bool {
    value.len() == 74
        && value.starts_with("id:sha256:")
        && value[10..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn otlp_request_pair_key(
    attributes: &BTreeMap<String, Value>,
) -> Result<Option<(String, String)>, AdapterError> {
    let conversation_id = optional_string(attributes, "conversation_id")?;
    let model = optional_string(attributes, "model")?;
    Ok(conversation_id.zip(model))
}

fn copy_string(
    source: &BTreeMap<String, Value>,
    output: &mut BTreeMap<String, Value>,
    source_key: &str,
    output_key: &str,
) -> Result<(), AdapterError> {
    let Some(value) = source.get(source_key) else {
        return Ok(());
    };
    let value = value.as_str().ok_or(AdapterError::InvalidFieldType)?;
    if value.is_empty() {
        return Err(AdapterError::InvalidIdentifier);
    }
    output.insert(output_key.into(), Value::String(value.into()));
    Ok(())
}

fn copy_u64(
    source: &BTreeMap<String, Value>,
    output: &mut BTreeMap<String, Value>,
    source_key: &str,
    output_key: &str,
) -> Result<(), AdapterError> {
    let Some(value) = source.get(source_key) else {
        return Ok(());
    };
    let value = otlp_u64(value).ok_or(AdapterError::InvalidFieldType)?;
    output.insert(output_key.into(), Value::Number(value.into()));
    Ok(())
}

fn otlp_bool(value: &Value) -> Option<bool> {
    value
        .as_bool()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn success_from_status(source: &BTreeMap<String, Value>) -> Option<bool> {
    let status = source.get("http.response.status_code").and_then(otlp_u64)?;
    Some((200..=299).contains(&status))
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
    let turn = optional_string(&record.attributes, "turn_id")?;
    let request = required_string(&record.attributes, "request_id")?;
    let correlation = CorrelationIds {
        session_id: Some(parse_identifier::<SessionId>(&session)?),
        turn_id: turn
            .as_deref()
            .map(parse_identifier::<TurnId>)
            .transpose()?,
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
        turn.as_deref(),
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
    let turn = optional_string(&record.attributes, "turn_id")?;
    let operation = required_string(&record.attributes, "call_id")?;
    let correlation = CorrelationIds {
        session_id: Some(parse_identifier::<SessionId>(&session)?),
        turn_id: turn
            .as_deref()
            .map(parse_identifier::<TurnId>)
            .transpose()?,
        operation_id: Some(parse_identifier::<OperationId>(&operation)?),
        ..CorrelationIds::default()
    };
    build_observation(
        record,
        previous_cursor,
        &session,
        turn.as_deref(),
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
    let turn = optional_string(&record.attributes, "turn_id")?;
    let permission = required_string(&record.attributes, "call_id")?;
    let decision = canonical_decision(&record.attributes)?;
    let correlation = CorrelationIds {
        session_id: Some(parse_identifier::<SessionId>(&session)?),
        turn_id: turn
            .as_deref()
            .map(parse_identifier::<TurnId>)
            .transpose()?,
        permission_id: Some(parse_identifier::<PermissionId>(&permission)?),
        ..CorrelationIds::default()
    };
    build_observation(
        record,
        previous_cursor,
        &session,
        turn.as_deref(),
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
        None if leaf == "session" => (session_span, None),
        None => (
            stable_id("codex-span", &[session, leaf]),
            Some(session_span),
        ),
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
    if KNOWN_CODEX_MODELS.contains(&model.as_str()) {
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
        MAX_HANDOFF_LINES, MAX_OTLP_LOG_RECORDS, MAX_PENDING_OTLP_REQUESTS,
        OTLP_REQUEST_CORRELATION_TTL_MS, OtlpRequestCorrelationState, parse_handoff_jsonl,
        parse_notify_json, parse_otlp_http_json, parse_otlp_http_json_with_state,
        read_handoff_file,
    };
    use agent_observability_contracts::ObservationEvent;
    use agent_observability_contracts::hash_opaque_identifier;
    use agent_observability_domain::LifecycleState;
    use std::fs;

    const FIXTURE: &str = include_str!("../tests/fixtures/codex-handoff.jsonl");
    const EXPECTED_PROJECTION: &str = include_str!("../tests/fixtures/codex-projection.jsonl");
    const EXPECTED_HANDOFF_HASH: &str =
        "sha256:0b30a1810b6e34152310691a3a660ecf33e98d4940fc63fe9b340811241f526c";
    const EXPECTED_PROJECTION_HASH: &str =
        "sha256:6dc1fa2ad7837c0e9ac2dcd6ac0dca52da1b0c11872db203d53ae742c97ee45a";

    fn otlp_api_request(conversation_id: &str, request_id: &str) -> Vec<u8> {
        format!(
            r#"{{"resourceLogs":[{{"scopeLogs":[{{"logRecords":[{{"attributes":[
              {{"key":"event.name","value":{{"stringValue":"codex.api_request"}}}},
              {{"key":"conversation.id","value":{{"stringValue":"{conversation_id}"}}}},
              {{"key":"model","value":{{"stringValue":"gpt-test"}}}},
              {{"key":"auth.request_id","value":{{"stringValue":"{request_id}"}}}}
            ]}}]}}]}}]}}"#
        )
        .into_bytes()
    }

    fn otlp_api_request_without_id(conversation_id: &str) -> Vec<u8> {
        format!(
            r#"{{"resourceLogs":[{{"scopeLogs":[{{"logRecords":[{{"attributes":[
              {{"key":"event.name","value":{{"stringValue":"codex.api_request"}}}},
              {{"key":"conversation.id","value":{{"stringValue":"{conversation_id}"}}}},
              {{"key":"model","value":{{"stringValue":"gpt-test"}}}}
            ]}}]}}]}}]}}"#
        )
        .into_bytes()
    }

    fn otlp_completed_response(conversation_id: &str) -> Vec<u8> {
        format!(
            r#"{{"resourceLogs":[{{"scopeLogs":[{{"logRecords":[{{"attributes":[
              {{"key":"event.name","value":{{"stringValue":"codex.sse_event"}}}},
              {{"key":"conversation.id","value":{{"stringValue":"{conversation_id}"}}}},
              {{"key":"model","value":{{"stringValue":"gpt-test"}}}},
              {{"key":"event.kind","value":{{"stringValue":"response.completed"}}}}
            ]}}]}}]}}]}}"#
        )
        .into_bytes()
    }

    fn only_request_id(batch: &super::AdapterBatch) -> Option<&str> {
        batch
            .observations()
            .find(|observation| matches!(observation.event, ObservationEvent::ModelRequest { .. }))
            .and_then(|observation| observation.correlation.request_id.as_ref())
            .map(agent_observability_domain::RequestId::as_str)
    }

    #[test]
    fn current_codex_otlp_json_is_allowlisted_before_canonical_mapping() {
        let input = br#"{
          "resourceLogs": [{"scopeLogs": [{"logRecords": [
            {"timeUnixNano":"1787875200000000000","attributes":[
              {"key":"event.name","value":{"stringValue":"codex.conversation_starts"}},
              {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
              {"key":"model","value":{"stringValue":"gpt-5.6-sol"}},
              {"key":"user.email","value":{"stringValue":"SECRET_EMAIL@example.com"}}
            ]},
            {"timeUnixNano":"1787875200100000000","attributes":[
              {"key":"event.name","value":{"stringValue":"codex.api_request"}},
              {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
              {"key":"model","value":{"stringValue":"gpt-5.6-sol"}},
              {"key":"attempt","value":{"intValue":"1"}},
              {"key":"http.response.status_code","value":{"intValue":"200"}},
              {"key":"duration_ms","value":{"stringValue":"100"}},
              {"key":"auth.request_id","value":{"stringValue":"request-native-1"}}
            ]},
            {"timeUnixNano":"1787875200200000000","attributes":[
              {"key":"event.name","value":{"stringValue":"codex.sse_event"}},
              {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
              {"key":"event.kind","value":{"stringValue":"response.completed"}},
              {"key":"model","value":{"stringValue":"gpt-5.6-sol"}},
              {"key":"input_token_count","value":{"stringValue":"100"}},
              {"key":"output_token_count","value":{"stringValue":"25"}},
              {"key":"cached_token_count","value":{"intValue":"10"}},
              {"key":"reasoning_token_count","value":{"intValue":"5"}},
              {"key":"tool_token_count","value":{"stringValue":"125"}}
            ]},
            {"timeUnixNano":"1787875200300000000","attributes":[
              {"key":"event.name","value":{"stringValue":"codex.tool_result"}},
              {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
              {"key":"tool_name","value":{"stringValue":"exec_command"}},
              {"key":"call_id","value":{"stringValue":"call-1"}},
              {"key":"success","value":{"stringValue":"true"}},
              {"key":"arguments","value":{"stringValue":"RAW_ARGUMENT_SECRET"}},
              {"key":"output","value":{"stringValue":"RAW_OUTPUT_SECRET"}}
            ]}
          ]}]}]
        }"#;
        let (batch, cursor) = parse_otlp_http_json(input, "codex-0.151.0", None, 1, 0).unwrap();
        assert_eq!(cursor.as_deref(), Some("4"));
        assert_eq!(batch.observations().count(), 4);
        assert_eq!(batch.diagnostics().count(), 0);
        let requests = batch
            .observations()
            .filter(|observation| {
                matches!(observation.event, ObservationEvent::ModelRequest { .. })
            })
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].correlation.request_id,
            requests[1].correlation.request_id
        );
        assert_eq!(
            requests[0]
                .correlation
                .request_id
                .as_ref()
                .unwrap()
                .as_str(),
            hash_opaque_identifier("request-native-1")
        );
        assert_eq!(requests[1].token_usage.input, Some(100));
        assert_eq!(requests[1].token_usage.output, Some(25));
        assert_eq!(requests[1].token_usage.total, Some(125));
        assert!(
            requests
                .iter()
                .all(|request| request.correlation.turn_id.is_none())
        );
        let debug = format!("{batch:?}");
        for secret in [
            "SECRET_EMAIL@example.com",
            "RAW_ARGUMENT_SECRET",
            "RAW_OUTPUT_SECRET",
        ] {
            assert!(!debug.contains(secret));
        }
    }

    #[test]
    fn native_request_pair_without_official_request_id_gets_local_correlation() {
        let input = br#"{
          "resourceLogs": [{"scopeLogs": [{"logRecords": [
            {"timeUnixNano":"1787875200000000000","attributes":[
              {"key":"event.name","value":{"stringValue":"codex.api_request"}},
              {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
              {"key":"model","value":{"stringValue":"gpt-5.6-sol"}},
              {"key":"attempt","value":{"intValue":"1"}},
              {"key":"http.response.status_code","value":{"intValue":"200"}},
              {"key":"duration_ms","value":{"stringValue":"100"}},
              {"key":"error.message","value":{"stringValue":"RAW_API_ERROR_SECRET"}}
            ]},
            {"timeUnixNano":"1787875200100000000","attributes":[
              {"key":"event.name","value":{"stringValue":"codex.sse_event"}},
              {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
              {"key":"event.kind","value":{"stringValue":"response.completed"}},
              {"key":"model","value":{"stringValue":"gpt-5.6-sol"}},
              {"key":"input_token_count","value":{"stringValue":"100"}},
              {"key":"output_token_count","value":{"stringValue":"25"}},
              {"key":"response.body","value":{"stringValue":"RAW_RESPONSE_SECRET"}}
            ]}
          ]}]}]
        }"#;
        let (batch, cursor) = parse_otlp_http_json(input, "codex-0.151.0", None, 41, 0).unwrap();
        assert_eq!(cursor.as_deref(), Some("42"));
        let requests = batch.observations().collect::<Vec<_>>();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].correlation.request_id,
            requests[1].correlation.request_id
        );
        assert!(
            requests[0]
                .correlation
                .request_id
                .as_ref()
                .unwrap()
                .as_str()
                .starts_with("id:sha256:")
        );
        assert_eq!(batch.diagnostics().count(), 0);
        let debug = format!("{batch:?}");
        assert!(!debug.contains("otlp-41"));
        assert!(!debug.contains("otlp-42"));
        assert!(!debug.contains("RAW_API_ERROR_SECRET"));
        assert!(!debug.contains("RAW_RESPONSE_SECRET"));
    }

    #[test]
    fn stateful_correlation_pairs_split_exports_in_fifo_order() {
        let mut state = OtlpRequestCorrelationState::default();
        for (cursor, request_id) in [(1, "request-1"), (2, "request-2")] {
            let (batch, _) = parse_otlp_http_json_with_state(
                &otlp_api_request("conversation-1", request_id),
                "generation-a",
                None,
                cursor,
                100,
                &mut state,
            )
            .unwrap();
            assert_eq!(
                only_request_id(&batch),
                Some(hash_opaque_identifier(request_id).as_str())
            );
        }
        assert_eq!(state.pending_len(), 2);

        for (cursor, request_id) in [(3, "request-1"), (4, "request-2")] {
            let (batch, _) = parse_otlp_http_json_with_state(
                &otlp_completed_response("conversation-1"),
                "generation-a",
                None,
                cursor,
                101,
                &mut state,
            )
            .unwrap();
            assert_eq!(
                only_request_id(&batch),
                Some(hash_opaque_identifier(request_id).as_str())
            );
        }
        assert_eq!(state.pending_len(), 0);
    }

    #[test]
    fn successful_official_retry_is_idempotent_across_restart_and_completion() {
        let first_request = otlp_api_request("conversation-1", "request-1");
        let mut state = OtlpRequestCorrelationState::default();
        parse_otlp_http_json_with_state(&first_request, "generation-a", None, 1, 100, &mut state)
            .unwrap();
        assert_eq!(state.pending_len(), 1);

        let before_retry = state.to_persisted_json().unwrap();
        parse_otlp_http_json_with_state(&first_request, "generation-a", None, 1, 101, &mut state)
            .unwrap();
        assert_eq!(state.pending_len(), 1);
        assert_eq!(state.to_persisted_json().unwrap(), before_retry);

        let persisted = state.to_persisted_json().unwrap();
        let mut restored =
            OtlpRequestCorrelationState::from_persisted_json(&persisted, 102).unwrap();
        let (retry, _) = parse_otlp_http_json_with_state(
            &first_request,
            "generation-a",
            None,
            1,
            103,
            &mut restored,
        )
        .unwrap();
        assert_eq!(
            only_request_id(&retry),
            Some(hash_opaque_identifier("request-1").as_str())
        );
        assert_eq!(restored.pending_len(), 1);
        assert_eq!(restored.to_persisted_json().unwrap(), persisted);

        let (completed, _) = parse_otlp_http_json_with_state(
            &otlp_completed_response("conversation-1"),
            "generation-a",
            Some("1"),
            2,
            104,
            &mut restored,
        )
        .unwrap();
        assert_eq!(
            only_request_id(&completed),
            Some(hash_opaque_identifier("request-1").as_str())
        );
        assert_eq!(restored.pending_len(), 0);

        parse_otlp_http_json_with_state(
            &otlp_api_request("conversation-1", "request-2"),
            "generation-a",
            Some("2"),
            3,
            105,
            &mut restored,
        )
        .unwrap();
        let (subsequent, _) = parse_otlp_http_json_with_state(
            &otlp_completed_response("conversation-1"),
            "generation-a",
            Some("3"),
            4,
            106,
            &mut restored,
        )
        .unwrap();
        assert_eq!(
            only_request_id(&subsequent),
            Some(hash_opaque_identifier("request-2").as_str())
        );
        assert_eq!(restored.pending_len(), 0);
    }

    #[test]
    fn no_id_retry_keeps_cursor_identity_without_official_deduplication() {
        let request = otlp_api_request_without_id("conversation-1");
        let mut state = OtlpRequestCorrelationState::default();
        let mut request_ids = Vec::new();
        for _ in 0..2 {
            let (batch, _) =
                parse_otlp_http_json_with_state(&request, "generation-a", None, 7, 100, &mut state)
                    .unwrap();
            request_ids.push(only_request_id(&batch).unwrap().to_owned());
        }
        assert_eq!(request_ids[0], request_ids[1]);
        assert_eq!(state.pending_len(), 2);

        parse_otlp_http_json_with_state(
            &otlp_completed_response("conversation-1"),
            "generation-a",
            Some("7"),
            8,
            101,
            &mut state,
        )
        .unwrap();
        assert_eq!(state.pending_len(), 1);
    }

    #[test]
    fn correlation_isolated_by_generation_and_restart_fails_closed() {
        let mut state = OtlpRequestCorrelationState::default();
        parse_otlp_http_json_with_state(
            &otlp_api_request("conversation-1", "request-1"),
            "generation-a",
            None,
            1,
            100,
            &mut state,
        )
        .unwrap();

        for (generation, state) in [
            ("generation-b", &mut state),
            ("generation-a", &mut OtlpRequestCorrelationState::default()),
        ] {
            let (batch, _) = parse_otlp_http_json_with_state(
                &otlp_completed_response("conversation-1"),
                generation,
                None,
                2,
                101,
                state,
            )
            .unwrap();
            assert_eq!(batch.observations().count(), 0);
            assert_eq!(
                batch.diagnostics().next().map(|diagnostic| diagnostic.code),
                Some(DiagnosticCode::MissingCorrelation)
            );
        }
    }

    #[test]
    fn correlation_expires_and_never_exceeds_hard_capacity() {
        let mut state = OtlpRequestCorrelationState::default();
        parse_otlp_http_json_with_state(
            &otlp_api_request("expired", "request-expired"),
            "generation-a",
            None,
            1,
            100,
            &mut state,
        )
        .unwrap();
        let (expired, _) = parse_otlp_http_json_with_state(
            &otlp_completed_response("expired"),
            "generation-a",
            None,
            2,
            100 + OTLP_REQUEST_CORRELATION_TTL_MS,
            &mut state,
        )
        .unwrap();
        assert_eq!(expired.observations().count(), 0);
        assert_eq!(state.pending_len(), 0);

        for index in 0..=MAX_PENDING_OTLP_REQUESTS {
            parse_otlp_http_json_with_state(
                &otlp_api_request(
                    &format!("conversation-{index}"),
                    &format!("request-{index}"),
                ),
                "generation-a",
                None,
                index as u64 + 3,
                200,
                &mut state,
            )
            .unwrap();
        }
        assert_eq!(state.pending_len(), MAX_PENDING_OTLP_REQUESTS);

        let (evicted, _) = parse_otlp_http_json_with_state(
            &otlp_completed_response("conversation-0"),
            "generation-a",
            None,
            2_000,
            201,
            &mut state,
        )
        .unwrap();
        assert_eq!(evicted.observations().count(), 0);
        assert_eq!(
            evicted
                .diagnostics()
                .next()
                .map(|diagnostic| diagnostic.code),
            Some(DiagnosticCode::MissingCorrelation)
        );
        let (retained, _) = parse_otlp_http_json_with_state(
            &otlp_completed_response(&format!("conversation-{MAX_PENDING_OTLP_REQUESTS}")),
            "generation-a",
            None,
            2_001,
            201,
            &mut state,
        )
        .unwrap();
        assert_eq!(
            only_request_id(&retained),
            Some(hash_opaque_identifier(&format!("request-{MAX_PENDING_OTLP_REQUESTS}")).as_str())
        );
    }

    #[test]
    fn failed_retry_is_observed_but_does_not_consume_completion_fifo() {
        let input = br#"{
          "resourceLogs": [{"scopeLogs": [{"logRecords": [
            {"timeUnixNano":"100000000","attributes":[
              {"key":"event.name","value":{"stringValue":"codex.api_request"}},
              {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
              {"key":"model","value":{"stringValue":"gpt-test"}},
              {"key":"auth.request_id","value":{"stringValue":"request-retry"}},
              {"key":"http.response.status_code","value":{"intValue":"500"}}
            ]},
            {"timeUnixNano":"101000000","attributes":[
              {"key":"event.name","value":{"stringValue":"codex.api_request"}},
              {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
              {"key":"model","value":{"stringValue":"gpt-test"}},
              {"key":"auth.request_id","value":{"stringValue":"request-retry"}},
              {"key":"http.response.status_code","value":{"intValue":"200"}}
            ]},
            {"timeUnixNano":"102000000","attributes":[
              {"key":"event.name","value":{"stringValue":"codex.sse_event"}},
              {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
              {"key":"model","value":{"stringValue":"gpt-test"}},
              {"key":"event.kind","value":{"stringValue":"response.completed"}}
            ]},
            {"timeUnixNano":"103000000","attributes":[
              {"key":"event.name","value":{"stringValue":"codex.api_request"}},
              {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
              {"key":"model","value":{"stringValue":"gpt-test"}},
              {"key":"http.response.status_code","value":{"intValue":"200"}}
            ]}
          ]}]}]
        }"#;
        let mut state = OtlpRequestCorrelationState::default();
        let (batch, _) =
            parse_otlp_http_json_with_state(input, "generation-a", None, 1, 100, &mut state)
                .unwrap();
        let requests = batch.observations().collect::<Vec<_>>();
        assert_eq!(requests.len(), 4);
        assert_eq!(requests[0].lifecycle, LifecycleState::Failed);
        assert_ne!(requests[0].observation_id, requests[1].observation_id);
        assert_ne!(requests[0].source_cursor, requests[1].source_cursor);
        assert_eq!(
            requests[1].correlation.request_id,
            requests[2].correlation.request_id
        );
        assert_eq!(
            requests[0].correlation.request_id,
            requests[1].correlation.request_id
        );
        assert_ne!(
            requests[1].correlation.request_id,
            requests[3].correlation.request_id
        );
        assert_eq!(state.pending_len(), 1);
    }

    #[test]
    fn persisted_correlation_state_is_bounded_private_and_restartable() {
        let mut state = OtlpRequestCorrelationState::default();
        parse_otlp_http_json_with_state(
            &otlp_api_request("PRIVATE_CONVERSATION", "PRIVATE_REQUEST"),
            "PRIVATE_GENERATION",
            None,
            1,
            100,
            &mut state,
        )
        .unwrap();
        let persisted = state.to_persisted_json().unwrap();
        for raw in [
            "PRIVATE_CONVERSATION",
            "PRIVATE_REQUEST",
            "PRIVATE_GENERATION",
        ] {
            assert!(!persisted.contains(raw));
        }
        let expired = OtlpRequestCorrelationState::from_persisted_json(
            &persisted,
            100 + OTLP_REQUEST_CORRELATION_TTL_MS,
        )
        .unwrap();
        assert_eq!(expired.pending_len(), 0);
        let mut restored =
            OtlpRequestCorrelationState::from_persisted_json(&persisted, 101).unwrap();
        let (completed, _) = parse_otlp_http_json_with_state(
            &otlp_completed_response("PRIVATE_CONVERSATION"),
            "PRIVATE_GENERATION",
            Some("1"),
            2,
            102,
            &mut restored,
        )
        .unwrap();
        assert_eq!(
            only_request_id(&completed),
            Some(hash_opaque_identifier("PRIVATE_REQUEST").as_str())
        );
        assert_eq!(restored.pending_len(), 0);
    }

    #[test]
    fn raw_notify_discards_content_and_cwd_before_mapping() {
        let input = br#"{
          "type":"agent-turn-complete",
          "thread-id":"conversation-1",
          "turn-id":"turn-1",
          "cwd":"/SECRET/PRIVATE/REPO",
          "input-messages":["RAW_PROMPT_SECRET"],
          "last-assistant-message":"RAW_ASSISTANT_SECRET"
        }"#;
        let batch = parse_notify_json(input, "codex-notify-v1", None, 1, 123).unwrap();
        let turn = batch.observations().next().unwrap();
        assert!(matches!(turn.event, ObservationEvent::Turn));
        assert_eq!(
            turn.correlation.turn_id.as_ref().unwrap().as_str(),
            "turn-1"
        );
        let debug = format!("{batch:?}");
        for secret in [
            "/SECRET/PRIVATE/REPO",
            "RAW_PROMPT_SECRET",
            "RAW_ASSISTANT_SECRET",
        ] {
            assert!(!debug.contains(secret));
        }
    }

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
        assert_eq!(batch.observations().count(), 1);
        assert_eq!(batch.diagnostics().count(), 2);

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
    fn otlp_record_bound_accepts_exact_limit_and_rejects_next_record() {
        let record =
            r#"{"attributes":[{"key":"event.name","value":{"stringValue":"codex.user_prompt"}}]}"#;
        let records = std::iter::repeat_n(record, MAX_OTLP_LOG_RECORDS)
            .collect::<Vec<_>>()
            .join(",");
        let exact =
            format!(r#"{{"resourceLogs":[{{"scopeLogs":[{{"logRecords":[{records}]}}]}}]}}"#);
        assert!(parse_otlp_http_json(exact.as_bytes(), "codex-test", None, 1, 0).is_ok());

        let over = format!(
            r#"{{"resourceLogs":[{{"scopeLogs":[{{"logRecords":[{records},{record}]}}]}}]}}"#
        );
        assert!(matches!(
            parse_otlp_http_json(over.as_bytes(), "codex-test", None, 1, 0),
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
