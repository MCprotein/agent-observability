//! Private, replayable `SQLite` authority for standalone observations.

use agent_observability_application::reduce_observation_state;
use agent_observability_contracts::{
    AdapterDispositionCode, AdapterDispositionKind, DurableRecordV1, SourceCheckpoint,
    SourceObservation, canonical_observation_payload_hash, hash_opaque_identifier,
    project_durable_record, sanitize_durable_record,
};
use agent_observability_domain::{
    CompactionId, CorrelationIds, DomainSpanState, LifecycleState, OperationId, PermissionId,
    RequestId, SessionId, SpanId, SpanKind, Timing, TokenUsage, TraceId, TurnId,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::{self, Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const DB_NAME: &str = "local-store.sqlite3";
const PROJECTION_NAME: &str = "observations.jsonl";
pub const LOCAL_STORE_SCHEMA_VERSION: &str = "local_state.v2";
const SCHEMA_OBJECTS: &[(&str, &str, &str)] = &[
    (
        "table",
        "metadata",
        "CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
    ),
    (
        "table",
        "source_cursors",
        "CREATE TABLE source_cursors (source TEXT NOT NULL, generation TEXT NOT NULL, cursor TEXT NOT NULL, PRIMARY KEY(source, generation))",
    ),
    (
        "table",
        "observations",
        "CREATE TABLE observations (event_id TEXT PRIMARY KEY, source TEXT NOT NULL, generation TEXT NOT NULL, observation_id TEXT NOT NULL, payload_hash TEXT NOT NULL, projected_json TEXT NOT NULL, UNIQUE(source, generation, observation_id))",
    ),
    (
        "table",
        "source_inputs",
        "CREATE TABLE source_inputs (source TEXT NOT NULL, generation TEXT NOT NULL, cursor TEXT NOT NULL, event_id TEXT NOT NULL REFERENCES observations(event_id), payload_hash TEXT NOT NULL, PRIMARY KEY(source, generation, cursor))",
    ),
    (
        "table",
        "records",
        "CREATE TABLE records (commit_seq INTEGER PRIMARY KEY AUTOINCREMENT, span_id TEXT NOT NULL UNIQUE, trace_id TEXT NOT NULL, parent_span_id TEXT, kind TEXT NOT NULL, state_json TEXT NOT NULL, record_json TEXT NOT NULL)",
    ),
    (
        "table",
        "topology",
        "CREATE TABLE topology (span_id TEXT PRIMARY KEY, trace_id TEXT NOT NULL, parent_span_id TEXT, kind TEXT NOT NULL, unresolved INTEGER NOT NULL CHECK(unresolved IN (0,1)))",
    ),
    (
        "table",
        "delivery_outcomes",
        "CREATE TABLE delivery_outcomes (event_id TEXT PRIMARY KEY REFERENCES observations(event_id), outcome TEXT NOT NULL CHECK(outcome = 'not_applicable'))",
    ),
    (
        "table",
        "adapter_dispositions",
        "CREATE TABLE adapter_dispositions (source TEXT NOT NULL, generation TEXT NOT NULL, cursor TEXT NOT NULL, disposition TEXT NOT NULL CHECK(disposition IN ('diagnostic','suppressed')), code TEXT NOT NULL, payload_hash TEXT NOT NULL, PRIMARY KEY(source, generation, cursor))",
    ),
    (
        "index",
        "topology_parent_idx",
        "CREATE INDEX topology_parent_idx ON topology(parent_span_id)",
    ),
];
static PROJECTION_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrashPoint {
    BeforeCommit,
    AfterCommit,
    BeforeProjectionRename,
    AfterProjectionRename,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IngestStatus {
    Committed,
    Duplicate,
    Suppressed,
}

/// Returns the stable identity used for replay deduplication.
#[must_use]
pub fn stable_event_id(observation: &SourceObservation) -> String {
    digest_components(&[
        "agent-observability-event-v1",
        source_name(observation),
        observation.source_generation.as_str(),
        observation.observation_id.as_str(),
    ])
}

#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    InvalidObservation,
    CursorConflict,
    PayloadConflict,
    TopologyConflict,
    SchemaMismatch,
    Crash(CrashPoint),
    InsecurePermissions,
    Symlink,
    InvalidPath,
}

impl Display for StoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Io(_) => "local store I/O failure",
            Self::Sqlite(_) => "local store database failure",
            Self::Json(_) => "local store JSON failure",
            Self::InvalidObservation => "invalid observation",
            Self::CursorConflict => "source cursor conflict",
            Self::PayloadConflict => "observation payload conflict",
            Self::TopologyConflict => "observation topology conflict",
            Self::SchemaMismatch => "local store schema or integrity mismatch",
            Self::Crash(_) => "injected local store crash",
            Self::InsecurePermissions => "local store permissions are too broad",
            Self::Symlink => "local store paths must not be symbolic links",
            Self::InvalidPath => "local store path has the wrong filesystem type",
        })
    }
}
impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Sqlite(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for StoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}
impl From<serde_json::Error> for StoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug)]
pub struct LocalStore {
    dir: PathBuf,
    db: Connection,
}

impl LocalStore {
    /// Opens or creates private transactional state and rebuilds its JSONL projection.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] for insecure permissions, incompatible state, or I/O failure.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let dir = dir.as_ref().to_path_buf();
        private_dir(&dir)?;
        let db_path = dir.join(DB_NAME);
        match private_create_new(&db_path) {
            Ok(file) => file.sync_all()?,
            Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists => {
                private_file(&db_path)?;
            }
            Err(error) => return Err(error),
        }
        let db = Connection::open(&db_path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.pragma_update(None, "journal_mode", "DELETE")?;
        db.pragma_update(None, "foreign_keys", true)?;
        db.pragma_update(None, "synchronous", "FULL")?;
        initialize_empty_schema(&db)?;
        let schema = db.query_row(
            "SELECT value FROM metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        );
        let schema: String = schema.map_err(|_| StoreError::SchemaMismatch)?;
        if schema == "local_state.v1" {
            migrate_v1_to_v2(&db)?;
        } else if schema != LOCAL_STORE_SCHEMA_VERSION {
            return Err(StoreError::SchemaMismatch);
        }
        validate_schema(&db)?;
        let store = Self { dir, db };
        store.rebuild_projection()?;
        Ok(store)
    }

    /// Atomically accepts one source observation.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] for cursor, payload, lifecycle, topology, projection, or storage
    /// failures.
    pub fn ingest(&mut self, observation: &SourceObservation) -> Result<IngestStatus, StoreError> {
        self.ingest_at(observation, None, true)
    }

    /// Commits one observation while deferring the replayable JSONL rebuild to the batch owner.
    ///
    /// The `SQLite` transaction remains authoritative. Call [`Self::rebuild_projection`] after the
    /// final item; reopening the store also repairs a missing projection after interruption.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::ingest`].
    pub fn ingest_deferred_projection(
        &mut self,
        observation: &SourceObservation,
    ) -> Result<IngestStatus, StoreError> {
        self.ingest_at(observation, None, false)
    }

    /// Atomically records a content-free adapter diagnostic or suppression and advances its
    /// source cursor without creating a synthetic observation.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] for cursor conflicts, conflicting replay, or storage failure.
    pub fn ingest_disposition(
        &mut self,
        checkpoint: &SourceCheckpoint,
        disposition: AdapterDispositionKind,
        code: AdapterDispositionCode,
    ) -> Result<IngestStatus, StoreError> {
        self.ingest_disposition_with_payload(checkpoint, disposition, code, None)
    }

    /// Atomically records a disposition bound to an optional privacy-safe canonical payload hash.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] for invalid hashes, cursor conflicts, conflicting replay, or storage
    /// failure.
    pub fn ingest_disposition_with_payload(
        &mut self,
        checkpoint: &SourceCheckpoint,
        disposition: AdapterDispositionKind,
        code: AdapterDispositionCode,
        canonical_payload_hash: Option<&str>,
    ) -> Result<IngestStatus, StoreError> {
        let source = checkpoint.source.as_str();
        let generation = hash_opaque_identifier(checkpoint.source_generation.as_str());
        let cursor = checkpoint.source_cursor.as_str();
        if canonical_payload_hash.is_some_and(|value| {
            value.len() != 71
                || !value.starts_with("sha256:")
                || !value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            return Err(StoreError::InvalidObservation);
        }
        let payload_hash = canonical_payload_hash.map_or_else(
            || {
                disposition_payload_hash(
                    source,
                    checkpoint.source_generation.as_str(),
                    cursor,
                    disposition,
                    code,
                )
            },
            str::to_owned,
        );
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        if let Some((existing_disposition, existing_code, existing_hash)) = tx
            .query_row(
                "SELECT disposition, code, payload_hash FROM adapter_dispositions WHERE source=?1 AND generation=?2 AND cursor=?3",
                params![source, generation, cursor],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
            )
            .optional()?
        {
            if existing_disposition != disposition.as_str()
                || existing_code != code.as_str()
                || existing_hash != payload_hash
            {
                return Err(StoreError::PayloadConflict);
            }
            return Ok(IngestStatus::Duplicate);
        }
        if cursor_exists(&tx, "source_inputs", source, &generation, cursor)? {
            return Err(StoreError::PayloadConflict);
        }
        if !checkpoint_cursor_matches(&tx, checkpoint)? {
            return Err(StoreError::CursorConflict);
        }
        tx.execute(
            "INSERT INTO adapter_dispositions(source,generation,cursor,disposition,code,payload_hash) VALUES (?1,?2,?3,?4,?5,?6)",
            params![source, generation, cursor, disposition.as_str(), code.as_str(), payload_hash],
        )?;
        advance_checkpoint_cursor(&tx, checkpoint)?;
        tx.commit()?;
        Ok(IngestStatus::Committed)
    }

    /// Executes ingest while injecting a deterministic crash at the requested boundary.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Crash`] at the selected boundary or any normal ingest error.
    pub fn ingest_with_crash(
        &mut self,
        observation: &SourceObservation,
        crash: CrashPoint,
    ) -> Result<IngestStatus, StoreError> {
        self.ingest_at(observation, Some(crash), true)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the ordered cursor, dedupe, commit, and crash-point transaction stays contiguous"
    )]
    fn ingest_at(
        &mut self,
        observation: &SourceObservation,
        crash: Option<CrashPoint>,
        rebuild_projection: bool,
    ) -> Result<IngestStatus, StoreError> {
        let source = source_name(observation);
        let generation = private_source_generation(observation);
        let cursor = observation.source_cursor.as_str();
        let prepared = prepare_observation(observation)?;
        let state = prepared.state;
        let incoming_record = prepared.record;
        let projected_json = prepared.projected_json;
        let payload_hash = prepared.payload_hash;
        let event_id = prepared.event_id;
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        if let Some((disposition, code, existing_hash)) =
            existing_disposition(&tx, source, &generation, cursor)?
        {
            if disposition == AdapterDispositionKind::Suppressed.as_str()
                && code == AdapterDispositionCode::DuplicateObservation.as_str()
                && existing_hash == payload_hash
            {
                drop(tx);
                self.rebuild_if_enabled(rebuild_projection, None)?;
                return Ok(IngestStatus::Suppressed);
            }
            return Err(StoreError::PayloadConflict);
        }
        if let Some((existing_event, existing_hash)) = tx
            .query_row(
                "SELECT event_id, payload_hash FROM source_inputs WHERE source = ?1 AND generation = ?2 AND cursor = ?3",
                params![source, generation, cursor],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
        {
            if existing_event != event_id || existing_hash != payload_hash {
                return Err(StoreError::PayloadConflict);
            }
            drop(tx);
            self.rebuild_if_enabled(rebuild_projection, None)?;
            return Ok(IngestStatus::Duplicate);
        }
        if !cursor_matches(&tx, observation)? {
            return Err(StoreError::CursorConflict);
        }
        if let Some(existing_state_json) = tx
            .query_row(
                "SELECT state_json FROM records WHERE span_id=?1",
                [state.span_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            ensure_static_record_compatibility(&tx, state.span_id.as_str(), &incoming_record)?;
            let existing_state = state_from_json(&existing_state_json)?;
            if same_canonical_observation(&existing_state, &state) {
                insert_duplicate_disposition(&tx, observation)?;
                if crash == Some(CrashPoint::BeforeCommit) {
                    return Err(StoreError::Crash(CrashPoint::BeforeCommit));
                }
                tx.commit()?;
                if crash == Some(CrashPoint::AfterCommit) {
                    return Err(StoreError::Crash(CrashPoint::AfterCommit));
                }
                self.rebuild_if_enabled(rebuild_projection, crash)?;
                return Ok(IngestStatus::Suppressed);
            }
        }
        if let Some(existing) = tx
            .query_row(
                "SELECT payload_hash FROM observations WHERE event_id = ?1",
                [event_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            if existing != payload_hash {
                return Err(StoreError::PayloadConflict);
            }
            tx.execute(
                "INSERT INTO source_inputs(source, generation, cursor, event_id, payload_hash) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![source, generation, cursor, event_id, payload_hash],
            )?;
            advance_cursor(&tx, observation)?;
            if crash == Some(CrashPoint::BeforeCommit) {
                return Err(StoreError::Crash(CrashPoint::BeforeCommit));
            }
            tx.commit()?;
            if crash == Some(CrashPoint::AfterCommit) {
                return Err(StoreError::Crash(CrashPoint::AfterCommit));
            }
            self.rebuild_if_enabled(rebuild_projection, crash)?;
            return Ok(IngestStatus::Duplicate);
        }
        let mut states = load_states(&tx)?;
        if states
            .iter()
            .any(|existing| existing.span_id == state.span_id)
        {
            ensure_static_record_compatibility(&tx, state.span_id.as_str(), &incoming_record)?;
        }
        let reduced = reduce_observation_state(&mut states, state).map_err(map_reduction_error)?;
        let record = sanitize_durable_record(
            &project_durable_record(observation, &projection_state(observation, &reduced))
                .map_err(|_| StoreError::InvalidObservation)?,
        )
        .map_err(|_| StoreError::InvalidObservation)?;
        let record_json = serde_json::to_string(&record)?;
        let state_json = state_to_json(&reduced)?;
        let parent = reduced.parent_span_id.as_ref().map(SpanId::as_str);
        let unresolved = i32::from(
            parent.is_some()
                && !states
                    .iter()
                    .any(|state| Some(state.span_id.as_str()) == parent),
        );
        tx.execute("INSERT INTO observations(event_id, source, generation, observation_id, payload_hash, projected_json) VALUES (?1,?2,?3,?4,?5,?6)", params![event_id, source, generation, hash_opaque_identifier(observation.observation_id.as_str()), payload_hash, projected_json])?;
        tx.execute("INSERT INTO source_inputs(source, generation, cursor, event_id, payload_hash) VALUES (?1,?2,?3,?4,?5)", params![source, generation, cursor, event_id, payload_hash])?;
        tx.execute("INSERT INTO records(span_id, trace_id, parent_span_id, kind, state_json, record_json) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(span_id) DO UPDATE SET state_json=excluded.state_json, record_json=excluded.record_json", params![reduced.span_id.as_str(), reduced.trace_id.as_str(), parent, kind_name(reduced.kind), state_json, record_json])?;
        tx.execute("INSERT INTO topology(span_id, trace_id, parent_span_id, kind, unresolved) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(span_id) DO UPDATE SET unresolved=excluded.unresolved", params![reduced.span_id.as_str(), reduced.trace_id.as_str(), parent, kind_name(reduced.kind), unresolved])?;
        tx.execute("UPDATE topology SET unresolved = CASE WHEN parent_span_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM topology parent WHERE parent.span_id = topology.parent_span_id) THEN 1 ELSE 0 END", [])?;
        tx.execute(
            "INSERT INTO delivery_outcomes(event_id, outcome) VALUES (?1, 'not_applicable')",
            [event_id.as_str()],
        )?;
        advance_cursor(&tx, observation)?;
        if crash == Some(CrashPoint::BeforeCommit) {
            return Err(StoreError::Crash(CrashPoint::BeforeCommit));
        }
        tx.commit()?;
        if crash == Some(CrashPoint::AfterCommit) {
            return Err(StoreError::Crash(CrashPoint::AfterCommit));
        }
        self.rebuild_if_enabled(rebuild_projection, crash)?;
        Ok(IngestStatus::Committed)
    }

    fn rebuild_if_enabled(
        &self,
        enabled: bool,
        crash: Option<CrashPoint>,
    ) -> Result<(), StoreError> {
        if enabled {
            self.rebuild_projection_with_crash(crash)
        } else {
            Ok(())
        }
    }

    /// Rebuilds JSONL from committed current records.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when reading state or replacing the projection fails.
    pub fn rebuild_projection(&self) -> Result<(), StoreError> {
        self.rebuild_projection_with_crash(None)
    }

    fn rebuild_projection_with_crash(&self, crash: Option<CrashPoint>) -> Result<(), StoreError> {
        let final_path = self.dir.join(PROJECTION_NAME);
        match fs::symlink_metadata(&final_path) {
            Ok(_) => private_file(&final_path)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        remove_stale_projection_temps(&self.dir)?;
        let (tmp, mut file) = create_projection_temp(&self.dir)?;
        let mut stmt = tx.prepare("SELECT record_json FROM records ORDER BY commit_seq")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            writeln!(file, "{}", row?)?;
        }
        drop(stmt);
        file.sync_all()?;
        if crash == Some(CrashPoint::BeforeProjectionRename) {
            return Err(StoreError::Crash(CrashPoint::BeforeProjectionRename));
        }
        fs::rename(&tmp, &final_path)?;
        File::open(&self.dir)?.sync_all()?;
        private_file(&final_path)?;
        if crash == Some(CrashPoint::AfterProjectionRename) {
            return Err(StoreError::Crash(CrashPoint::AfterProjectionRename));
        }
        tx.commit()?;
        Ok(())
    }

    #[must_use]
    pub fn projection_path(&self) -> PathBuf {
        self.dir.join(PROJECTION_NAME)
    }
    #[must_use]
    pub fn database_path(&self) -> PathBuf {
        self.dir.join(DB_NAME)
    }
    /// Counts immutable accepted observations.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when state cannot be queried.
    pub fn observation_count(&self) -> Result<u64, StoreError> {
        count(&self.db, "observations")
    }
    /// Counts current reduced records.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when state cannot be queried.
    pub fn record_count(&self) -> Result<u64, StoreError> {
        count(&self.db, "records")
    }
    /// Counts accepted source cursor inputs.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when state cannot be queried.
    pub fn source_input_count(&self) -> Result<u64, StoreError> {
        count(&self.db, "source_inputs")
    }
    /// Counts explicit profile-neutral delivery outcomes.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when state cannot be queried.
    pub fn outcome_count(&self) -> Result<u64, StoreError> {
        count(&self.db, "delivery_outcomes")
    }
    /// Counts content-free adapter diagnostics and suppressions.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when state cannot be queried.
    pub fn disposition_count(&self) -> Result<u64, StoreError> {
        count(&self.db, "adapter_dispositions")
    }
    /// Counts unresolved out-of-order parent links.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when state cannot be queried.
    pub fn unresolved_parent_count(&self) -> Result<u64, StoreError> {
        count_where(&self.db, "topology", "unresolved = 1")
    }
    /// Reads the committed resumable cursor for one source generation.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when state cannot be queried.
    pub fn cursor(&self, source: &str, generation: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .db
            .query_row(
                "SELECT cursor FROM source_cursors WHERE source=?1 AND generation=?2",
                params![source, hash_opaque_identifier(generation)],
                |r| r.get(0),
            )
            .optional()?)
    }
    /// Returns observation, current-record, and delivery-outcome counts.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when state cannot be queried.
    pub fn counts(&self) -> Result<(u64, u64, u64), StoreError> {
        Ok((
            count(&self.db, "observations")?,
            count(&self.db, "records")?,
            count(&self.db, "delivery_outcomes")?,
        ))
    }
}

fn map_reduction_error(error: agent_observability_domain::DomainError) -> StoreError {
    let topology = matches!(
        &error,
        agent_observability_domain::DomainError::SelfParent { .. }
            | agent_observability_domain::DomainError::CrossTraceParent { .. }
            | agent_observability_domain::DomainError::InvalidParentKind { .. }
            | agent_observability_domain::DomainError::Cycle { .. }
    );
    drop(error);
    if topology {
        StoreError::TopologyConflict
    } else {
        StoreError::InvalidObservation
    }
}

#[derive(Debug)]
struct PreparedObservation {
    state: DomainSpanState,
    record: DurableRecordV1,
    projected_json: String,
    payload_hash: String,
    event_id: String,
}

fn prepare_observation(observation: &SourceObservation) -> Result<PreparedObservation, StoreError> {
    let raw_state = DomainSpanState {
        trace_id: observation.trace_id.clone(),
        span_id: observation.span_id.clone(),
        parent_span_id: observation.parent_span_id.clone(),
        kind: kind(&observation.event),
        lifecycle: observation.lifecycle,
        correlation: observation.correlation.clone(),
        timing: observation.timing,
        token_usage: observation.token_usage,
    };
    let record = sanitize_durable_record(
        &project_durable_record(observation, &raw_state)
            .map_err(|_| StoreError::InvalidObservation)?,
    )
    .map_err(|_| StoreError::InvalidObservation)?;
    let projected_json = serde_json::to_string(&record)?;
    Ok(PreparedObservation {
        state: private_state(&raw_state)?,
        record,
        payload_hash: canonical_observation_payload_hash(observation)
            .map_err(|_| StoreError::InvalidObservation)?,
        projected_json,
        event_id: stable_event_id(observation),
    })
}

fn private_state(state: &DomainSpanState) -> Result<DomainSpanState, StoreError> {
    Ok(DomainSpanState {
        trace_id: TraceId::parse(hash_opaque_identifier(state.trace_id.as_str()))
            .map_err(|_| StoreError::InvalidObservation)?,
        span_id: SpanId::parse(hash_opaque_identifier(state.span_id.as_str()))
            .map_err(|_| StoreError::InvalidObservation)?,
        parent_span_id: state
            .parent_span_id
            .as_ref()
            .map(|value| SpanId::parse(hash_opaque_identifier(value.as_str())))
            .transpose()
            .map_err(|_| StoreError::InvalidObservation)?,
        kind: state.kind,
        lifecycle: state.lifecycle,
        correlation: CorrelationIds {
            session_id: private_optional_id(
                state.correlation.session_id.as_ref().map(SessionId::as_str),
                SessionId::parse,
            )?,
            turn_id: private_optional_id(
                state.correlation.turn_id.as_ref().map(TurnId::as_str),
                TurnId::parse,
            )?,
            request_id: private_optional_id(
                state.correlation.request_id.as_ref().map(RequestId::as_str),
                RequestId::parse,
            )?,
            operation_id: private_optional_id(
                state
                    .correlation
                    .operation_id
                    .as_ref()
                    .map(OperationId::as_str),
                OperationId::parse,
            )?,
            permission_id: private_optional_id(
                state
                    .correlation
                    .permission_id
                    .as_ref()
                    .map(PermissionId::as_str),
                PermissionId::parse,
            )?,
            compaction_id: private_optional_id(
                state
                    .correlation
                    .compaction_id
                    .as_ref()
                    .map(CompactionId::as_str),
                CompactionId::parse,
            )?,
        },
        timing: state.timing,
        token_usage: state.token_usage,
    })
}

fn private_optional_id<T, E>(
    value: Option<&str>,
    parse: impl FnOnce(String) -> Result<T, E>,
) -> Result<Option<T>, StoreError> {
    value
        .map(|value| parse(hash_opaque_identifier(value)))
        .transpose()
        .map_err(|_| StoreError::InvalidObservation)
}

fn projection_state(observation: &SourceObservation, reduced: &DomainSpanState) -> DomainSpanState {
    DomainSpanState {
        trace_id: observation.trace_id.clone(),
        span_id: observation.span_id.clone(),
        parent_span_id: observation.parent_span_id.clone(),
        kind: reduced.kind,
        lifecycle: reduced.lifecycle,
        correlation: reduced.correlation.clone(),
        timing: reduced.timing,
        token_usage: reduced.token_usage,
    }
}

fn cursor_matches(
    tx: &Transaction<'_>,
    observation: &SourceObservation,
) -> Result<bool, StoreError> {
    let current: Option<String> = tx
        .query_row(
            "SELECT cursor FROM source_cursors WHERE source = ?1 AND generation = ?2",
            params![
                source_name(observation),
                private_source_generation(observation)
            ],
            |row| row.get(0),
        )
        .optional()?;
    let expected = observation
        .previous_source_cursor
        .as_ref()
        .map(agent_observability_domain::SourceCursor::as_str);
    Ok(
        observation.source_cursor.as_str() != expected.unwrap_or_default()
            && current.as_deref() == expected,
    )
}

fn cursor_exists(
    tx: &Transaction<'_>,
    table: &str,
    source: &str,
    generation: &str,
    cursor: &str,
) -> Result<bool, StoreError> {
    let sql = match table {
        "source_inputs" => {
            "SELECT 1 FROM source_inputs WHERE source=?1 AND generation=?2 AND cursor=?3"
        }
        "adapter_dispositions" => {
            "SELECT 1 FROM adapter_dispositions WHERE source=?1 AND generation=?2 AND cursor=?3"
        }
        _ => return Err(StoreError::SchemaMismatch),
    };
    Ok(tx
        .query_row(sql, params![source, generation, cursor], |_| Ok(()))
        .optional()?
        .is_some())
}

fn existing_disposition(
    tx: &Transaction<'_>,
    source: &str,
    generation: &str,
    cursor: &str,
) -> Result<Option<(String, String, String)>, StoreError> {
    tx.query_row(
        "SELECT disposition,code,payload_hash FROM adapter_dispositions WHERE source=?1 AND generation=?2 AND cursor=?3",
        params![source, generation, cursor],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()
    .map_err(StoreError::from)
}

fn same_canonical_observation(existing: &DomainSpanState, incoming: &DomainSpanState) -> bool {
    existing.trace_id == incoming.trace_id
        && existing.span_id == incoming.span_id
        && existing.parent_span_id == incoming.parent_span_id
        && existing.kind == incoming.kind
        && existing.lifecycle == incoming.lifecycle
        && existing.correlation == incoming.correlation
        && existing.timing == incoming.timing
        && existing.token_usage == incoming.token_usage
}

fn insert_duplicate_disposition(
    tx: &Transaction<'_>,
    observation: &SourceObservation,
) -> Result<(), StoreError> {
    let source = source_name(observation);
    let generation = private_source_generation(observation);
    let cursor = observation.source_cursor.as_str();
    let disposition = AdapterDispositionKind::Suppressed;
    let code = AdapterDispositionCode::DuplicateObservation;
    let payload_hash = canonical_observation_payload_hash(observation)
        .map_err(|_| StoreError::InvalidObservation)?;
    tx.execute(
        "INSERT INTO adapter_dispositions(source,generation,cursor,disposition,code,payload_hash) VALUES (?1,?2,?3,?4,?5,?6)",
        params![source, generation, cursor, disposition.as_str(), code.as_str(), payload_hash],
    )?;
    advance_cursor(tx, observation)
}

fn disposition_payload_hash(
    source: &str,
    source_generation: &str,
    cursor: &str,
    disposition: AdapterDispositionKind,
    code: AdapterDispositionCode,
) -> String {
    digest_components(&[
        "agent-observability-disposition-v1",
        source,
        source_generation,
        cursor,
        disposition.as_str(),
        code.as_str(),
    ])
}

fn load_states(tx: &Transaction<'_>) -> Result<Vec<DomainSpanState>, StoreError> {
    let mut stmt = tx.prepare("SELECT state_json FROM records ORDER BY span_id")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(state_from_json(&row?)?);
    }
    Ok(out)
}

#[derive(Debug, Deserialize, Serialize)]
struct StoredState {
    lifecycle: String,
    trace_id: String,
    span_id: String,
    parent_span_id: Option<String>,
    kind: String,
    correlation: StoredCorrelation,
    start_unix_ms: u64,
    end_unix_ms: Option<u64>,
    token_usage: StoredTokenUsage,
}

#[derive(Debug, Deserialize, Serialize)]
struct StoredCorrelation {
    session: Option<String>,
    turn: Option<String>,
    request: Option<String>,
    operation: Option<String>,
    permission: Option<String>,
    compaction: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StoredTokenUsage {
    input: Option<u64>,
    output: Option<u64>,
    cached_input: Option<u64>,
    cache_creation_input: Option<u64>,
    reasoning_output: Option<u64>,
    total: Option<u64>,
    total_input: Option<u64>,
    total_output: Option<u64>,
    total_cached_input: Option<u64>,
    total_reasoning_output: Option<u64>,
    total_accumulated: Option<u64>,
    context_window: Option<u64>,
    #[serde(default)]
    input_before: Option<u64>,
    #[serde(default)]
    input_after: Option<u64>,
}

fn state_to_json(state: &DomainSpanState) -> Result<String, StoreError> {
    Ok(serde_json::to_string(&StoredState {
        lifecycle: lifecycle_name(state.lifecycle).into(),
        trace_id: state.trace_id.as_str().into(),
        span_id: state.span_id.as_str().into(),
        parent_span_id: state
            .parent_span_id
            .as_ref()
            .map(|value| value.as_str().into()),
        kind: kind_name(state.kind).into(),
        correlation: StoredCorrelation {
            session: state
                .correlation
                .session_id
                .as_ref()
                .map(|value| value.as_str().into()),
            turn: state
                .correlation
                .turn_id
                .as_ref()
                .map(|value| value.as_str().into()),
            request: state
                .correlation
                .request_id
                .as_ref()
                .map(|value| value.as_str().into()),
            operation: state
                .correlation
                .operation_id
                .as_ref()
                .map(|value| value.as_str().into()),
            permission: state
                .correlation
                .permission_id
                .as_ref()
                .map(|value| value.as_str().into()),
            compaction: state
                .correlation
                .compaction_id
                .as_ref()
                .map(|value| value.as_str().into()),
        },
        start_unix_ms: state.timing.start_unix_ms,
        end_unix_ms: state.timing.end_unix_ms,
        token_usage: StoredTokenUsage {
            input: state.token_usage.input,
            output: state.token_usage.output,
            cached_input: state.token_usage.cached_input,
            cache_creation_input: state.token_usage.cache_creation_input,
            reasoning_output: state.token_usage.reasoning_output,
            total: state.token_usage.total,
            total_input: state.token_usage.total_input,
            total_output: state.token_usage.total_output,
            total_cached_input: state.token_usage.total_cached_input,
            total_reasoning_output: state.token_usage.total_reasoning_output,
            total_accumulated: state.token_usage.total_accumulated,
            context_window: state.token_usage.context_window,
            input_before: state.token_usage.input_before,
            input_after: state.token_usage.input_after,
        },
    })?)
}

fn state_from_json(value: &str) -> Result<DomainSpanState, StoreError> {
    let stored: StoredState = serde_json::from_str(value)?;
    Ok(DomainSpanState {
        trace_id: TraceId::parse(stored.trace_id).map_err(|_| StoreError::InvalidObservation)?,
        span_id: SpanId::parse(stored.span_id).map_err(|_| StoreError::InvalidObservation)?,
        parent_span_id: stored
            .parent_span_id
            .map(SpanId::parse)
            .transpose()
            .map_err(|_| StoreError::InvalidObservation)?,
        kind: parse_kind(&stored.kind)?,
        lifecycle: parse_lifecycle(&stored.lifecycle)?,
        correlation: CorrelationIds {
            session_id: parse_optional_id(stored.correlation.session, SessionId::parse)?,
            turn_id: parse_optional_id(stored.correlation.turn, TurnId::parse)?,
            request_id: parse_optional_id(stored.correlation.request, RequestId::parse)?,
            operation_id: parse_optional_id(stored.correlation.operation, OperationId::parse)?,
            permission_id: parse_optional_id(stored.correlation.permission, PermissionId::parse)?,
            compaction_id: parse_optional_id(stored.correlation.compaction, CompactionId::parse)?,
        },
        timing: Timing::new(stored.start_unix_ms, stored.end_unix_ms)
            .map_err(|_| StoreError::InvalidObservation)?,
        token_usage: TokenUsage {
            input: stored.token_usage.input,
            output: stored.token_usage.output,
            cached_input: stored.token_usage.cached_input,
            cache_creation_input: stored.token_usage.cache_creation_input,
            reasoning_output: stored.token_usage.reasoning_output,
            total: stored.token_usage.total,
            total_input: stored.token_usage.total_input,
            total_output: stored.token_usage.total_output,
            total_cached_input: stored.token_usage.total_cached_input,
            total_reasoning_output: stored.token_usage.total_reasoning_output,
            total_accumulated: stored.token_usage.total_accumulated,
            context_window: stored.token_usage.context_window,
            input_before: stored.token_usage.input_before,
            input_after: stored.token_usage.input_after,
        },
    })
}

fn parse_optional_id<T, E>(
    value: Option<String>,
    parse: impl FnOnce(String) -> Result<T, E>,
) -> Result<Option<T>, StoreError> {
    value
        .map(parse)
        .transpose()
        .map_err(|_| StoreError::InvalidObservation)
}

fn advance_cursor(tx: &Transaction<'_>, observation: &SourceObservation) -> Result<(), StoreError> {
    tx.execute(
        "INSERT INTO source_cursors(source,generation,cursor) VALUES (?1,?2,?3) ON CONFLICT(source,generation) DO UPDATE SET cursor=excluded.cursor",
        params![
            source_name(observation),
            private_source_generation(observation),
            observation.source_cursor.as_str()
        ],
    )?;
    Ok(())
}

fn checkpoint_cursor_matches(
    tx: &Transaction<'_>,
    checkpoint: &SourceCheckpoint,
) -> Result<bool, StoreError> {
    let current = tx
        .query_row(
            "SELECT cursor FROM source_cursors WHERE source=?1 AND generation=?2",
            params![
                checkpoint.source.as_str(),
                hash_opaque_identifier(checkpoint.source_generation.as_str())
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let expected = checkpoint
        .previous_source_cursor
        .as_ref()
        .map(agent_observability_domain::SourceCursor::as_str);
    Ok(
        checkpoint.source_cursor.as_str() != expected.unwrap_or_default()
            && current.as_deref() == expected,
    )
}

fn advance_checkpoint_cursor(
    tx: &Transaction<'_>,
    checkpoint: &SourceCheckpoint,
) -> Result<(), StoreError> {
    tx.execute(
        "INSERT INTO source_cursors(source,generation,cursor) VALUES (?1,?2,?3) ON CONFLICT(source,generation) DO UPDATE SET cursor=excluded.cursor",
        params![
            checkpoint.source.as_str(),
            hash_opaque_identifier(checkpoint.source_generation.as_str()),
            checkpoint.source_cursor.as_str()
        ],
    )?;
    Ok(())
}

fn private_source_generation(observation: &SourceObservation) -> String {
    hash_opaque_identifier(observation.source_generation.as_str())
}

fn ensure_static_record_compatibility(
    tx: &Transaction<'_>,
    state_span_id: &str,
    incoming: &DurableRecordV1,
) -> Result<(), StoreError> {
    let existing_json: String = tx.query_row(
        "SELECT record_json FROM records WHERE span_id = ?1",
        [state_span_id],
        |row| row.get(0),
    )?;
    let existing: DurableRecordV1 = serde_json::from_str(&existing_json)?;
    let compatible = existing.trace_id == incoming.trace_id
        && existing.parent_span_id == incoming.parent_span_id
        && existing.span_kind == incoming.span_kind
        && existing.name == incoming.name
        && existing.agent == incoming.agent
        && existing.project == incoming.project
        && existing.attributes.source == incoming.attributes.source
        && existing.attributes.event_type == incoming.attributes.event_type
        && existing.attributes.envelope_type == incoming.attributes.envelope_type
        && existing.attributes.tool_name == incoming.attributes.tool_name
        && existing.attributes.phase == incoming.attributes.phase
        && existing.attributes.decision == incoming.attributes.decision
        && existing.attributes.trigger == incoming.attributes.trigger;
    if !compatible {
        return Err(StoreError::PayloadConflict);
    }
    Ok(())
}

fn lifecycle_name(value: LifecycleState) -> &'static str {
    match value {
        LifecycleState::Observed => "observed",
        LifecycleState::Running => "running",
        LifecycleState::Completed => "completed",
        LifecycleState::Failed => "failed",
        LifecycleState::Interrupted => "interrupted",
    }
}

fn parse_lifecycle(value: &str) -> Result<LifecycleState, StoreError> {
    match value {
        "observed" => Ok(LifecycleState::Observed),
        "running" => Ok(LifecycleState::Running),
        "completed" => Ok(LifecycleState::Completed),
        "failed" => Ok(LifecycleState::Failed),
        "interrupted" => Ok(LifecycleState::Interrupted),
        _ => Err(StoreError::InvalidObservation),
    }
}

fn digest_components(components: &[&str]) -> String {
    let mut hash = Sha256::new();
    for component in components {
        let length = u64::try_from(component.len()).expect("component length fits u64");
        hash.update(length.to_be_bytes());
        hash.update(component.as_bytes());
    }
    let mut output = String::with_capacity(64);
    for byte in hash.finalize() {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}
fn source_name(o: &SourceObservation) -> &'static str {
    match o.source {
        agent_observability_contracts::AgentSource::Codex => "codex",
        agent_observability_contracts::AgentSource::ClaudeCode => "claude-code",
        agent_observability_contracts::AgentSource::Cursor => "cursor",
    }
}
fn kind(e: &agent_observability_contracts::ObservationEvent) -> SpanKind {
    match e {
        agent_observability_contracts::ObservationEvent::Session { .. } => SpanKind::AgentSession,
        agent_observability_contracts::ObservationEvent::Turn => SpanKind::Turn,
        agent_observability_contracts::ObservationEvent::ModelRequest { .. } => {
            SpanKind::LlmRequest
        }
        agent_observability_contracts::ObservationEvent::ToolOperation { .. } => {
            SpanKind::ToolExecution
        }
        agent_observability_contracts::ObservationEvent::Permission { .. } => SpanKind::Permission,
        agent_observability_contracts::ObservationEvent::Compaction { .. } => SpanKind::Compaction,
    }
}
fn kind_name(k: SpanKind) -> &'static str {
    match k {
        SpanKind::AgentSession => "agent.session",
        SpanKind::Turn => "turn",
        SpanKind::LlmRequest => "llm.request",
        SpanKind::ToolExecution => "tool.execution",
        SpanKind::Permission => "permission",
        SpanKind::Compaction => "compaction",
        SpanKind::Workstream => "workstream",
    }
}
fn parse_kind(value: &str) -> Result<SpanKind, StoreError> {
    match value {
        "workstream" => Ok(SpanKind::Workstream),
        "agent.session" => Ok(SpanKind::AgentSession),
        "turn" => Ok(SpanKind::Turn),
        "llm.request" => Ok(SpanKind::LlmRequest),
        "tool.execution" => Ok(SpanKind::ToolExecution),
        "permission" => Ok(SpanKind::Permission),
        "compaction" => Ok(SpanKind::Compaction),
        _ => Err(StoreError::InvalidObservation),
    }
}
fn private_dir(path: &Path) -> Result<(), StoreError> {
    let existed = ensure_directory_chain(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if existed && fs::metadata(path)?.permissions().mode() & 0o777 != 0o700 {
            return Err(StoreError::InsecurePermissions);
        }
    }
    Ok(())
}

fn ensure_directory_chain(path: &Path) -> Result<bool, StoreError> {
    if path
        .components()
        .any(|component| component == std::path::Component::ParentDir)
    {
        return Err(StoreError::InvalidPath);
    }
    let existed = match fs::symlink_metadata(path) {
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    if trusted_platform_symlink(&current) {
                        continue;
                    }
                    return Err(StoreError::Symlink);
                }
                if !metadata.is_dir() {
                    return Err(StoreError::InvalidPath);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                match create_private_dir(&current) {
                    Ok(()) => {}
                    Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists => {
                        let metadata = fs::symlink_metadata(&current)?;
                        if metadata.file_type().is_symlink() {
                            return Err(StoreError::Symlink);
                        }
                        if !metadata.is_dir() {
                            return Err(StoreError::InvalidPath);
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(existed)
}

#[cfg(target_os = "macos")]
fn trusted_platform_symlink(path: &Path) -> bool {
    matches!(path.to_str(), Some("/tmp" | "/var"))
}

#[cfg(not(target_os = "macos"))]
fn trusted_platform_symlink(_path: &Path) -> bool {
    false
}

fn create_private_dir(path: &Path) -> Result<(), StoreError> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    Ok(())
}
fn private_file(path: &Path) -> Result<(), StoreError> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(StoreError::Symlink);
    }
    if !fs::metadata(path)?.is_file() {
        return Err(StoreError::InvalidPath);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)?.permissions().mode() & 0o777 != 0o600 {
            return Err(StoreError::InsecurePermissions);
        }
    }
    Ok(())
}
#[cfg(not(unix))]
fn set_private_file(path: &Path) -> Result<(), StoreError> {
    let _ = path;
    Ok(())
}
fn private_create_new(path: &Path) -> Result<File, StoreError> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(not(unix))]
    set_private_file(path)?;
    Ok(file)
}

fn create_projection_temp(dir: &Path) -> Result<(PathBuf, File), StoreError> {
    loop {
        let sequence = PROJECTION_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(
            ".{PROJECTION_NAME}.tmp.{}.{sequence}",
            std::process::id()
        ));
        match private_create_new(&path) {
            Ok(file) => return Ok((path, file)),
            Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
}

fn remove_stale_projection_temps(dir: &Path) -> Result<(), StoreError> {
    let prefix = format!(".{PROJECTION_NAME}.tmp.");
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(&prefix) {
            continue;
        }
        let path = entry.path();
        private_file(&path)?;
        fs::remove_file(path)?;
    }
    Ok(())
}
fn count(db: &Connection, table: &str) -> Result<u64, StoreError> {
    Ok(db
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
            r.get::<_, i64>(0)
        })?
        .cast_unsigned())
}
fn count_where(db: &Connection, table: &str, predicate: &str) -> Result<u64, StoreError> {
    Ok(db
        .query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE {predicate}"),
            [],
            |r| r.get::<_, i64>(0),
        )?
        .cast_unsigned())
}

fn initialize_empty_schema(db: &Connection) -> Result<(), StoreError> {
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let table_count: u64 = tx
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get::<_, i64>(0),
        )?
        .cast_unsigned();
    if table_count == 0 {
        for (_, _, sql) in SCHEMA_OBJECTS {
            tx.execute_batch(sql)?;
        }
        tx.execute(
            "INSERT INTO metadata(key, value) VALUES ('schema_version', ?1)",
            [LOCAL_STORE_SCHEMA_VERSION],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn migrate_v1_to_v2(db: &Connection) -> Result<(), StoreError> {
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let version: String = tx
        .query_row(
            "SELECT value FROM metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| StoreError::SchemaMismatch)?;
    if version == "local_state.v2" {
        tx.commit()?;
        return Ok(());
    }
    if version != "local_state.v1" {
        return Err(StoreError::SchemaMismatch);
    }
    let object_exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'adapter_dispositions')",
        [],
        |row| row.get(0),
    )?;
    if object_exists {
        return Err(StoreError::SchemaMismatch);
    }
    let disposition_sql = SCHEMA_OBJECTS
        .iter()
        .find(|(_, name, _)| *name == "adapter_dispositions")
        .map(|(_, _, sql)| *sql)
        .ok_or(StoreError::SchemaMismatch)?;
    tx.execute_batch(disposition_sql)?;
    tx.execute(
        "UPDATE metadata SET value='local_state.v2' WHERE key='schema_version'",
        [],
    )?;
    tx.commit()?;
    Ok(())
}

fn validate_schema(db: &Connection) -> Result<(), StoreError> {
    let integrity: String = db.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(StoreError::SchemaMismatch);
    }
    let object_count: i64 = db.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    if usize::try_from(object_count).ok() != Some(SCHEMA_OBJECTS.len()) {
        return Err(StoreError::SchemaMismatch);
    }
    for (kind, name, expected_sql) in SCHEMA_OBJECTS {
        let actual_sql: String = db
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = ?1 AND name = ?2",
                params![kind, name],
                |row| row.get(0),
            )
            .map_err(|_| StoreError::SchemaMismatch)?;
        if normalize_schema_sql(&actual_sql) != normalize_schema_sql(expected_sql) {
            return Err(StoreError::SchemaMismatch);
        }
    }
    for (table, expected) in [
        ("metadata", &["key", "value"][..]),
        ("source_cursors", &["source", "generation", "cursor"]),
        (
            "observations",
            &[
                "event_id",
                "source",
                "generation",
                "observation_id",
                "payload_hash",
                "projected_json",
            ],
        ),
        (
            "source_inputs",
            &["source", "generation", "cursor", "event_id", "payload_hash"],
        ),
        (
            "records",
            &[
                "commit_seq",
                "span_id",
                "trace_id",
                "parent_span_id",
                "kind",
                "state_json",
                "record_json",
            ],
        ),
        (
            "topology",
            &[
                "span_id",
                "trace_id",
                "parent_span_id",
                "kind",
                "unresolved",
            ],
        ),
        ("delivery_outcomes", &["event_id", "outcome"]),
        (
            "adapter_dispositions",
            &[
                "source",
                "generation",
                "cursor",
                "disposition",
                "code",
                "payload_hash",
            ],
        ),
    ] {
        let mut statement = db.prepare(&format!("PRAGMA table_info({table})"))?;
        let actual = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if actual != expected {
            return Err(StoreError::SchemaMismatch);
        }
    }
    let foreign_key_failure = db
        .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
        .optional()?;
    if foreign_key_failure.is_some() {
        return Err(StoreError::SchemaMismatch);
    }
    Ok(())
}

fn normalize_schema_sql(sql: &str) -> String {
    sql.chars()
        .filter(|character| !character.is_ascii_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_observability_contracts::{AgentSource, ObservationEvent};
    use agent_observability_domain::{
        CompactionId, CorrelationIds, ObservationId, SourceCursor, SourceGeneration, SpanId,
        Timing, TokenUsage, TraceId,
    };

    fn observation(cursor: &str, span: &str, parent: Option<&str>) -> SourceObservation {
        observation_after(cursor, None, span, parent)
    }

    fn observation_after(
        cursor: &str,
        previous: Option<&str>,
        span: &str,
        parent: Option<&str>,
    ) -> SourceObservation {
        SourceObservation {
            source: AgentSource::Codex,
            source_generation: SourceGeneration::parse("generation").unwrap(),
            previous_source_cursor: previous.map(|value| SourceCursor::parse(value).unwrap()),
            source_cursor: SourceCursor::parse(cursor).unwrap(),
            observation_id: ObservationId::parse(format!("observation-{cursor}")).unwrap(),
            trace_id: TraceId::parse("trace").unwrap(),
            span_id: SpanId::parse(span).unwrap(),
            parent_span_id: parent.map(|v| SpanId::parse(v).unwrap()),
            correlation: CorrelationIds::default(),
            event: if span == "session" {
                ObservationEvent::Session {
                    model: None,
                    project: None,
                }
            } else {
                ObservationEvent::Turn
            },
            lifecycle: LifecycleState::Completed,
            timing: Timing::new(1, Some(2)).unwrap(),
            token_usage: TokenUsage::default(),
        }
    }

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agent-observability-local-store-{label}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn crash_reopen_rebuilds_one_record_event_and_outcome() {
        let dir = temp_dir("crash");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let item = observation("1", "session", None);
        assert!(matches!(
            store.ingest_with_crash(&item, CrashPoint::AfterCommit),
            Err(StoreError::Crash(CrashPoint::AfterCommit))
        ));
        drop(store);
        let store = LocalStore::open(&dir).unwrap();
        assert_eq!(store.observation_count().unwrap(), 1);
        assert_eq!(store.outcome_count().unwrap(), 1);
        assert!(store.projection_path().metadata().unwrap().len() > 0);
        assert_eq!(
            store.cursor("codex", "generation").unwrap().as_deref(),
            Some("1")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_projection_crash_point_replays_without_duplicates() {
        for (label, point) in [
            ("before-commit", CrashPoint::BeforeCommit),
            ("after-commit", CrashPoint::AfterCommit),
            ("before-rename", CrashPoint::BeforeProjectionRename),
            ("after-rename", CrashPoint::AfterProjectionRename),
        ] {
            let dir = temp_dir(label);
            let _ = fs::remove_dir_all(&dir);
            let item = observation("1", "session", None);
            let mut store = LocalStore::open(&dir).unwrap();
            let result = store.ingest_with_crash(&item, point);
            assert!(matches!(result, Err(StoreError::Crash(value)) if value == point));
            drop(store);
            let mut reopened = LocalStore::open(&dir).unwrap();
            let expected = u64::from(point != CrashPoint::BeforeCommit);
            assert_eq!(reopened.observation_count().unwrap(), expected);
            assert_eq!(reopened.outcome_count().unwrap(), expected);
            if expected == 0 {
                assert_eq!(reopened.ingest(&item).unwrap(), IngestStatus::Committed);
            } else {
                assert_eq!(reopened.ingest(&item).unwrap(), IngestStatus::Duplicate);
            }
            assert_eq!(reopened.observation_count().unwrap(), expected.max(1));
            drop(reopened);
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn out_of_order_parent_is_explicitly_resolved() {
        let dir = temp_dir("topology");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let child = observation("2", "child", Some("session"));
        assert!(store.ingest(&child).is_ok());
        assert_eq!(store.unresolved_parent_count().unwrap(), 1);
        let parent = observation_after("3", Some("2"), "session", None);
        assert!(store.ingest(&parent).is_ok());
        assert_eq!(store.unresolved_parent_count().unwrap(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn replay_is_noop_and_payload_change_conflicts() {
        let dir = temp_dir("replay");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let item = observation("1", "session", None);
        assert_eq!(store.ingest(&item).unwrap(), IngestStatus::Committed);
        assert!(matches!(store.ingest(&item), Ok(IngestStatus::Duplicate)));
        let mut changed = observation("1", "session", None);
        changed.event = ObservationEvent::Session {
            model: Some("different".into()),
            project: None,
        };
        assert!(matches!(
            store.ingest(&changed),
            Err(StoreError::PayloadConflict)
        ));
        assert_eq!(store.observation_count().unwrap(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn observation_and_disposition_cursors_share_one_idempotency_namespace() {
        let dir = temp_dir("cross-kind-cursor");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let first = observation("1", "session", None);
        store.ingest(&first).unwrap();

        let reused = SourceCheckpoint {
            source: AgentSource::Codex,
            source_generation: SourceGeneration::parse("generation").unwrap(),
            previous_source_cursor: Some(SourceCursor::parse("1").unwrap()),
            source_cursor: SourceCursor::parse("1").unwrap(),
        };
        assert!(matches!(
            store.ingest_disposition(
                &reused,
                AdapterDispositionKind::Diagnostic,
                AdapterDispositionCode::UnsupportedEvent,
            ),
            Err(StoreError::PayloadConflict)
        ));

        let disposition = SourceCheckpoint {
            source_cursor: SourceCursor::parse("2").unwrap(),
            ..reused
        };
        store
            .ingest_disposition(
                &disposition,
                AdapterDispositionKind::Diagnostic,
                AdapterDispositionCode::UnsupportedEvent,
            )
            .unwrap();
        let second = observation_after("2", Some("1"), "turn", Some("session"));
        assert!(matches!(
            store.ingest(&second),
            Err(StoreError::PayloadConflict)
        ));
        assert_eq!(store.observation_count().unwrap(), 1);
        assert_eq!(store.disposition_count().unwrap(), 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn canonical_duplicate_at_a_new_cursor_becomes_a_durable_suppression() {
        let dir = temp_dir("semantic-duplicate");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let mut first = observation("1", "turn", None);
        first.timing = Timing::new(100, Some(100)).unwrap();
        store.ingest(&first).unwrap();

        let mut repeated = observation_after("2", Some("1"), "turn", None);
        repeated.timing = Timing::new(100, Some(100)).unwrap();
        assert_eq!(store.ingest(&repeated).unwrap(), IngestStatus::Suppressed);
        assert_eq!(store.observation_count().unwrap(), 1);
        assert_eq!(store.disposition_count().unwrap(), 1);
        assert_eq!(store.record_count().unwrap(), 1);
        assert_eq!(
            store.cursor("codex", "generation").unwrap().as_deref(),
            Some("2")
        );
        assert_eq!(store.ingest(&repeated).unwrap(), IngestStatus::Suppressed);
        let mut changed = repeated.clone();
        changed.token_usage.input = Some(1);
        assert!(matches!(
            store.ingest(&changed),
            Err(StoreError::PayloadConflict)
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn timing_change_at_a_new_cursor_reduces_instead_of_being_suppressed() {
        let dir = temp_dir("timing-update");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let mut first = observation("1", "turn", None);
        first.timing = Timing::new(100, Some(100)).unwrap();
        store.ingest(&first).unwrap();

        let mut later = observation_after("2", Some("1"), "turn", None);
        later.timing = Timing::new(100, Some(200)).unwrap();
        assert_eq!(store.ingest(&later).unwrap(), IngestStatus::Committed);
        assert_eq!(store.observation_count().unwrap(), 2);
        assert_eq!(store.disposition_count().unwrap(), 0);
        assert_eq!(store.record_count().unwrap(), 1);
        assert!(
            fs::read_to_string(store.projection_path())
                .unwrap()
                .contains("\"end_time_unix_ms\":200.0")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn lifecycle_updates_reduce_to_one_current_record() {
        let dir = temp_dir("lifecycle");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let mut running = observation("1", "session", None);
        running.lifecycle = LifecycleState::Running;
        running.timing = Timing::new(1, None).unwrap();
        store.ingest(&running).unwrap();

        let mut completed = observation_after("2", Some("1"), "session", None);
        completed.lifecycle = LifecycleState::Completed;
        completed.timing = Timing::new(1, Some(2)).unwrap();
        store.ingest(&completed).unwrap();

        assert_eq!(store.observation_count().unwrap(), 2);
        assert_eq!(store.source_input_count().unwrap(), 2);
        assert_eq!(store.record_count().unwrap(), 1);
        assert_eq!(store.outcome_count().unwrap(), 2);
        let projection = fs::read_to_string(store.projection_path()).unwrap();
        assert_eq!(projection.lines().count(), 1);
        assert!(projection.contains("\"code\":\"ok\""));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_event_at_new_cursor_becomes_a_suppression() {
        let dir = temp_dir("event-alias");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let first = observation("1", "session", None);
        store.ingest(&first).unwrap();
        let mut replay = first.clone();
        replay.previous_source_cursor = Some(SourceCursor::parse("1").unwrap());
        replay.source_cursor = SourceCursor::parse("2").unwrap();
        assert_eq!(store.ingest(&replay).unwrap(), IngestStatus::Suppressed);
        assert_eq!(store.observation_count().unwrap(), 1);
        assert_eq!(store.source_input_count().unwrap(), 1);
        assert_eq!(store.disposition_count().unwrap(), 1);
        assert_eq!(store.record_count().unwrap(), 1);
        assert_eq!(store.outcome_count().unwrap(), 1);
        assert_eq!(
            store.cursor("codex", "generation").unwrap().as_deref(),
            Some("2")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lifecycle_update_crashes_recover_the_whole_atomic_unit() {
        for (label, point) in [
            ("lifecycle-before-commit", CrashPoint::BeforeCommit),
            ("lifecycle-after-commit", CrashPoint::AfterCommit),
            (
                "lifecycle-before-rename",
                CrashPoint::BeforeProjectionRename,
            ),
            ("lifecycle-after-rename", CrashPoint::AfterProjectionRename),
        ] {
            let dir = temp_dir(label);
            let _ = fs::remove_dir_all(&dir);
            let mut store = LocalStore::open(&dir).unwrap();
            let mut running = observation("1", "session", None);
            running.lifecycle = LifecycleState::Running;
            running.timing = Timing::new(1, None).unwrap();
            store.ingest(&running).unwrap();

            let mut completed = observation_after("2", Some("1"), "session", None);
            completed.lifecycle = LifecycleState::Completed;
            completed.timing = Timing::new(1, Some(2)).unwrap();
            assert!(matches!(
                store.ingest_with_crash(&completed, point),
                Err(StoreError::Crash(value)) if value == point
            ));
            drop(store);

            let mut reopened = LocalStore::open(&dir).unwrap();
            let committed = point != CrashPoint::BeforeCommit;
            assert_eq!(
                reopened.observation_count().unwrap(),
                u64::from(committed) + 1
            );
            assert_eq!(
                reopened.source_input_count().unwrap(),
                u64::from(committed) + 1
            );
            assert_eq!(reopened.record_count().unwrap(), 1);
            assert_eq!(reopened.outcome_count().unwrap(), u64::from(committed) + 1);
            assert_eq!(
                reopened.cursor("codex", "generation").unwrap().as_deref(),
                Some(if committed { "2" } else { "1" })
            );
            let projection = fs::read_to_string(reopened.projection_path()).unwrap();
            assert_eq!(projection.lines().count(), 1);
            assert_eq!(projection.contains("\"code\":\"ok\""), committed);
            assert_eq!(
                reopened.ingest(&completed).unwrap(),
                if committed {
                    IngestStatus::Duplicate
                } else {
                    IngestStatus::Committed
                }
            );
            assert_eq!(reopened.observation_count().unwrap(), 2);
            assert_eq!(reopened.source_input_count().unwrap(), 2);
            assert_eq!(reopened.outcome_count().unwrap(), 2);
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn duplicate_alias_crashes_recover_cursor_without_duplicate_outcome() {
        for (label, point) in [
            ("alias-before-commit", CrashPoint::BeforeCommit),
            ("alias-after-commit", CrashPoint::AfterCommit),
            ("alias-before-rename", CrashPoint::BeforeProjectionRename),
            ("alias-after-rename", CrashPoint::AfterProjectionRename),
        ] {
            let dir = temp_dir(label);
            let _ = fs::remove_dir_all(&dir);
            let mut store = LocalStore::open(&dir).unwrap();
            let first = observation("1", "session", None);
            store.ingest(&first).unwrap();
            let mut alias = first.clone();
            alias.previous_source_cursor = Some(SourceCursor::parse("1").unwrap());
            alias.source_cursor = SourceCursor::parse("2").unwrap();
            assert!(matches!(
                store.ingest_with_crash(&alias, point),
                Err(StoreError::Crash(value)) if value == point
            ));
            drop(store);

            let mut reopened = LocalStore::open(&dir).unwrap();
            let committed = point != CrashPoint::BeforeCommit;
            assert_eq!(reopened.observation_count().unwrap(), 1);
            assert_eq!(reopened.source_input_count().unwrap(), 1);
            assert_eq!(reopened.disposition_count().unwrap(), u64::from(committed));
            assert_eq!(reopened.record_count().unwrap(), 1);
            assert_eq!(reopened.outcome_count().unwrap(), 1);
            assert_eq!(
                reopened.cursor("codex", "generation").unwrap().as_deref(),
                Some(if committed { "2" } else { "1" })
            );
            assert_eq!(reopened.ingest(&alias).unwrap(), IngestStatus::Suppressed);
            assert_eq!(reopened.source_input_count().unwrap(), 1);
            assert_eq!(reopened.disposition_count().unwrap(), 1);
            assert_eq!(reopened.outcome_count().unwrap(), 1);
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn same_store_suppression_retry_repairs_projection_after_every_crash_point() {
        for (label, point) in [
            ("suppression-retry-before-commit", CrashPoint::BeforeCommit),
            ("suppression-retry-after-commit", CrashPoint::AfterCommit),
            (
                "suppression-retry-before-rename",
                CrashPoint::BeforeProjectionRename,
            ),
            (
                "suppression-retry-after-rename",
                CrashPoint::AfterProjectionRename,
            ),
        ] {
            let dir = temp_dir(label);
            let _ = fs::remove_dir_all(&dir);
            let mut store = LocalStore::open(&dir).unwrap();
            let first = observation("1", "session", None);
            store.ingest(&first).unwrap();
            let clean_projection = fs::read_to_string(store.projection_path()).unwrap();
            let mut alias = first.clone();
            alias.previous_source_cursor = Some(SourceCursor::parse("1").unwrap());
            alias.source_cursor = SourceCursor::parse("2").unwrap();
            assert!(matches!(
                store.ingest_with_crash(&alias, point),
                Err(StoreError::Crash(value)) if value == point
            ));

            assert_eq!(store.ingest(&alias).unwrap(), IngestStatus::Suppressed);
            assert_eq!(
                fs::read_to_string(store.projection_path()).unwrap(),
                clean_projection
            );
            assert_eq!(store.observation_count().unwrap(), 1);
            assert_eq!(store.disposition_count().unwrap(), 1);
            assert_eq!(store.record_count().unwrap(), 1);
            assert_eq!(
                store.cursor("codex", "generation").unwrap().as_deref(),
                Some("2")
            );
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn duplicate_retry_repairs_projection_without_reopening() {
        let dir = temp_dir("duplicate-repairs-projection");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let item = observation("1", "session", None);
        assert!(matches!(
            store.ingest_with_crash(&item, CrashPoint::BeforeProjectionRename),
            Err(StoreError::Crash(CrashPoint::BeforeProjectionRename))
        ));
        assert_eq!(fs::read_to_string(store.projection_path()).unwrap(), "");
        assert_eq!(store.ingest(&item).unwrap(), IngestStatus::Duplicate);
        assert_eq!(
            fs::read_to_string(store.projection_path())
                .unwrap()
                .lines()
                .count(),
            1
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_writers_publish_a_complete_projection() {
        let dir = temp_dir("concurrent-projection");
        let _ = fs::remove_dir_all(&dir);
        drop(LocalStore::open(&dir).unwrap());
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let handles = (0..8)
            .map(|index| {
                let dir = dir.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut item = observation("1", &format!("span-{index}"), None);
                    item.source_generation =
                        SourceGeneration::parse(format!("generation-{index}")).unwrap();
                    item.observation_id =
                        ObservationId::parse(format!("observation-{index}")).unwrap();
                    barrier.wait();
                    LocalStore::open(dir).unwrap().ingest(&item).unwrap();
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }
        let store = LocalStore::open(&dir).unwrap();
        assert_eq!(store.observation_count().unwrap(), 8);
        assert_eq!(store.record_count().unwrap(), 8);
        assert_eq!(
            fs::read_to_string(store.projection_path())
                .unwrap()
                .lines()
                .count(),
            8
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_first_open_initializes_schema_once() {
        let dir = temp_dir("concurrent-first-open");
        let _ = fs::remove_dir_all(&dir);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(20));
        let handles = (0..20)
            .map(|_| {
                let dir = dir.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    LocalStore::open(dir).map(|store| store.counts().unwrap())
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            assert_eq!(handle.join().unwrap().unwrap(), (0, 0, 0));
        }
        LocalStore::open(&dir).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn private_permissions_are_enforced_without_mutating_existing_paths() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("permissions");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            LocalStore::open(&dir),
            Err(StoreError::InsecurePermissions)
        ));
        assert_eq!(
            fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o755
        );

        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let store = LocalStore::open(&dir).unwrap();
        assert_eq!(
            fs::metadata(store.database_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(store.projection_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let projection = store.projection_path();
        drop(store);
        fs::set_permissions(&projection, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            LocalStore::open(&dir),
            Err(StoreError::InsecurePermissions)
        ));
        assert_eq!(
            fs::metadata(&projection).unwrap().permissions().mode() & 0o777,
            0o644
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_store_and_artifact_paths_are_rejected() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let root = temp_dir("symlink");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let target_dir = root.join("target");
        fs::create_dir(&target_dir).unwrap();
        fs::set_permissions(&target_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let linked_dir = root.join("linked");
        symlink(&target_dir, &linked_dir).unwrap();
        assert!(matches!(
            LocalStore::open(&linked_dir),
            Err(StoreError::Symlink)
        ));

        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o700)).unwrap();
        let nested_root = root.join("nested");
        fs::create_dir(&nested_root).unwrap();
        fs::set_permissions(&nested_root, fs::Permissions::from_mode(0o700)).unwrap();
        symlink(&outside, nested_root.join("link")).unwrap();
        assert!(matches!(
            LocalStore::open(nested_root.join("link/store")),
            Err(StoreError::Symlink)
        ));
        assert!(!outside.join("store").exists());

        let store_dir = root.join("store");
        fs::create_dir(&store_dir).unwrap();
        fs::set_permissions(&store_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let target_file = root.join("target.sqlite3");
        private_create_new(&target_file).unwrap();
        symlink(&target_file, store_dir.join(DB_NAME)).unwrap();
        assert!(matches!(
            LocalStore::open(&store_dir),
            Err(StoreError::Symlink)
        ));

        let projection_store = root.join("projection-store");
        let store = LocalStore::open(&projection_store).unwrap();
        let projection = store.projection_path();
        drop(store);
        fs::remove_file(&projection).unwrap();
        symlink(root.join("missing-projection"), &projection).unwrap();
        assert!(matches!(
            LocalStore::open(&projection_store),
            Err(StoreError::Symlink)
        ));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn v1_store_migrates_and_commits_bounded_dispositions() {
        let dir = temp_dir("v1-migration");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let existing = observation("1", "session", None);
        store.ingest(&existing).unwrap();
        let database = store.database_path();
        let projection = store.projection_path();
        drop(store);
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch(
                "DROP TABLE adapter_dispositions;
                 UPDATE metadata SET value='local_state.v1' WHERE key='schema_version';",
            )
            .unwrap();
        drop(connection);
        fs::remove_file(&projection).unwrap();

        let mut store = LocalStore::open(&dir).unwrap();
        assert_eq!(store.observation_count().unwrap(), 1);
        assert_eq!(store.record_count().unwrap(), 1);
        assert_eq!(store.outcome_count().unwrap(), 1);
        assert_eq!(
            store.cursor("codex", "generation").unwrap().as_deref(),
            Some("1")
        );
        assert_eq!(fs::read_to_string(&projection).unwrap().lines().count(), 1);
        let checkpoint = SourceCheckpoint {
            source: AgentSource::Codex,
            source_generation: SourceGeneration::parse("generation").unwrap(),
            previous_source_cursor: Some(SourceCursor::parse("1").unwrap()),
            source_cursor: SourceCursor::parse("diagnostic-1").unwrap(),
        };
        assert_eq!(
            store
                .ingest_disposition(
                    &checkpoint,
                    AdapterDispositionKind::Diagnostic,
                    AdapterDispositionCode::UnsupportedEvent,
                )
                .unwrap(),
            IngestStatus::Committed
        );
        assert_eq!(
            store
                .ingest_disposition(
                    &checkpoint,
                    AdapterDispositionKind::Diagnostic,
                    AdapterDispositionCode::UnsupportedEvent,
                )
                .unwrap(),
            IngestStatus::Duplicate
        );
        assert_eq!(store.disposition_count().unwrap(), 1);
        assert_eq!(
            store.cursor("codex", "generation").unwrap().as_deref(),
            Some("diagnostic-1")
        );
        assert!(matches!(
            store.ingest_disposition(
                &checkpoint,
                AdapterDispositionKind::Suppressed,
                AdapterDispositionCode::DuplicateObservation,
            ),
            Err(StoreError::PayloadConflict)
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pre_v10_state_without_compaction_metrics_reopens_and_reduces_an_update() {
        let dir = temp_dir("pre-v10-compaction-state");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let mut existing = observation("1", "compaction", None);
        existing.event = ObservationEvent::Compaction {
            trigger: Some("auto".into()),
        };
        existing.correlation.compaction_id = Some(CompactionId::parse("compact-1").unwrap());
        store.ingest(&existing).unwrap();
        let database = store.database_path();
        drop(store);

        let connection = Connection::open(&database).unwrap();
        let state_json: String = connection
            .query_row("SELECT state_json FROM records", [], |row| row.get(0))
            .unwrap();
        let mut legacy: serde_json::Value = serde_json::from_str(&state_json).unwrap();
        let token_usage = legacy
            .get_mut("token_usage")
            .and_then(serde_json::Value::as_object_mut)
            .unwrap();
        token_usage.remove("input_before");
        token_usage.remove("input_after");
        let legacy_json = serde_json::to_string(&legacy).unwrap();
        assert!(!legacy_json.contains("input_before"));
        assert!(!legacy_json.contains("input_after"));
        connection
            .execute("UPDATE records SET state_json=?1", params![legacy_json])
            .unwrap();
        connection
            .execute_batch(
                "DROP TABLE adapter_dispositions;
                 UPDATE metadata SET value='local_state.v1' WHERE key='schema_version';",
            )
            .unwrap();
        drop(connection);

        let mut reopened = LocalStore::open(&dir).unwrap();
        let mut update = observation_after("2", Some("1"), "compaction", None);
        update.event = ObservationEvent::Compaction {
            trigger: Some("auto".into()),
        };
        update.correlation.compaction_id = Some(CompactionId::parse("compact-1").unwrap());
        update.token_usage.input_before = Some(120_000);
        update.token_usage.input_after = Some(64_000);
        assert_eq!(reopened.ingest(&update).unwrap(), IngestStatus::Committed);
        let projection = fs::read_to_string(reopened.projection_path()).unwrap();
        assert!(projection.contains("\"input_tokens_before\":120000.0"));
        assert!(projection.contains("\"input_tokens_after\":64000.0"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn incompatible_schema_version_fails_closed() {
        let dir = temp_dir("schema-version");
        let _ = fs::remove_dir_all(&dir);
        let store = LocalStore::open(&dir).unwrap();
        let database = store.database_path();
        drop(store);
        let connection = Connection::open(database).unwrap();
        connection
            .execute(
                "UPDATE metadata SET value='unsupported' WHERE key='schema_version'",
                [],
            )
            .unwrap();
        drop(connection);
        assert!(matches!(
            LocalStore::open(&dir),
            Err(StoreError::SchemaMismatch)
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn schema_missing_required_constraints_fails_closed() {
        let dir = temp_dir("schema-constraints");
        let _ = fs::remove_dir_all(&dir);
        let store = LocalStore::open(&dir).unwrap();
        let database = store.database_path();
        drop(store);
        let connection = Connection::open(database).unwrap();
        connection
            .execute_batch(
                "DROP TABLE records;
                 CREATE TABLE records (
                    commit_seq INTEGER,
                    span_id TEXT NOT NULL,
                    trace_id TEXT NOT NULL,
                    parent_span_id TEXT,
                    kind TEXT NOT NULL,
                    state_json TEXT NOT NULL,
                    record_json TEXT NOT NULL
                 );",
            )
            .unwrap();
        drop(connection);
        assert!(matches!(
            LocalStore::open(&dir),
            Err(StoreError::SchemaMismatch)
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn foreign_schema_object_is_not_treated_as_an_empty_database() {
        let dir = temp_dir("foreign-schema-object");
        let _ = fs::remove_dir_all(&dir);
        private_dir(&dir).unwrap();
        let database = dir.join(DB_NAME);
        private_create_new(&database).unwrap();
        let connection = Connection::open(&database).unwrap();
        connection
            .execute("CREATE VIEW foreign_view AS SELECT 1", [])
            .unwrap();
        drop(connection);
        assert!(matches!(
            LocalStore::open(&dir),
            Err(StoreError::SchemaMismatch)
        ));
        let connection = Connection::open(database).unwrap();
        let view_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='view' AND name='foreign_view'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(view_count, 1);
        drop(connection);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn durable_artifacts_do_not_contain_raw_opaque_identifiers() {
        let dir = temp_dir("private-identifiers");
        let _ = fs::remove_dir_all(&dir);
        let mut item = observation("1", "RAW_PRIVATE_SPAN", None);
        item.source_generation = SourceGeneration::parse("RAW_PRIVATE_GENERATION").unwrap();
        item.source_cursor = SourceCursor::parse("RAW_PRIVATE_CURSOR").unwrap();
        item.trace_id = TraceId::parse("RAW_PRIVATE_TRACE").unwrap();
        item.observation_id = ObservationId::parse("RAW_PRIVATE_OBSERVATION").unwrap();
        item.correlation.session_id = Some(SessionId::parse("RAW_PRIVATE_SESSION").unwrap());
        item.event = ObservationEvent::Session {
            model: Some("Authorization: Bearer RAW_PRIVATE_SECRET".into()),
            project: Some("/workspace/.env".into()),
        };
        let mut store = LocalStore::open(&dir).unwrap();
        store.ingest(&item).unwrap();
        let database = store.database_path();
        let projection = store.projection_path();
        drop(store);

        let database_body = String::from_utf8_lossy(&fs::read(&database).unwrap()).into_owned();
        assert!(database_body.contains("RAW_PRIVATE_CURSOR"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&database).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        for sentinel in [
            "RAW_PRIVATE_SPAN",
            "RAW_PRIVATE_TRACE",
            "RAW_PRIVATE_OBSERVATION",
            "RAW_PRIVATE_SESSION",
            "RAW_PRIVATE_GENERATION",
            "RAW_PRIVATE_SECRET",
            "/workspace/.env",
        ] {
            assert!(
                !database_body.contains(sentinel),
                "database leaked {sentinel}"
            );
        }

        let projection_body = String::from_utf8_lossy(&fs::read(projection).unwrap()).into_owned();
        for sentinel in [
            "RAW_PRIVATE_SPAN",
            "RAW_PRIVATE_TRACE",
            "RAW_PRIVATE_OBSERVATION",
            "RAW_PRIVATE_SESSION",
            "RAW_PRIVATE_GENERATION",
            "RAW_PRIVATE_CURSOR",
            "RAW_PRIVATE_SECRET",
            "/workspace/.env",
        ] {
            assert!(
                !projection_body.contains(sentinel),
                "projection leaked {sentinel}"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
