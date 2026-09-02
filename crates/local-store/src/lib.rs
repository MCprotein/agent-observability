//! Private, replayable `SQLite` authority for standalone observations.

use agent_observability_application::reduce_span_state;
use agent_observability_contracts::{
    AdapterDispositionCode, AdapterDispositionKind, DurableRecordV1, ObservationEvent,
    RETENTION_ARCHIVE_VERSION, SourceCheckpoint, SourceObservation,
    canonical_observation_payload_hash, hash_opaque_identifier, project_durable_record,
    sanitize_durable_record,
};
use agent_observability_domain::{
    CompactionId, CorrelationIds, DomainSpanState, LifecycleState, OperationId, PermissionId,
    RequestId, SessionId, SpanId, SpanKind, Timing, TokenUsage, TraceId, TurnId,
};
use fs2::FileExt;
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt::{self, Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const DB_NAME: &str = "local-store.sqlite3";
const PROJECTION_NAME: &str = "observations.jsonl";
const STORE_OPEN_LOCK_NAME: &str = ".store-open.lock";
const REPORT_RENDER_LOCK_NAME: &str = ".report-render.lock";
pub const LOCAL_STORE_SCHEMA_VERSION: &str = "local_state.v4";
const REPORT_GENERATION_KEY: &str = "report_generation";
const REPORT_ACKNOWLEDGED_GENERATION_KEY: &str = "report_acknowledged_generation";
const REPORT_VISIT_BATCH_SIZE: i64 = 128;
const CODEX_CORRELATION_KEY_PREFIX: &str = "codex_request_correlation.v1:";
const MAX_CODEX_CORRELATION_STATE_BYTES: usize = 512 * 1024;
const MAX_CODEX_PENDING_CORRELATIONS: usize = 1024;
const MAX_CODEX_RECENTLY_COMPLETED_CORRELATIONS: usize = 1024;
const MAX_EXPIRED_SPAN_GUARDS: u64 = 100_000;
const MAX_RETENTION_RECEIPTS: u64 = 1_024;
const MAX_ADAPTER_DISPOSITIONS: u64 = 100_000;
const MIN_ARCHIVE_RECORDS: u32 = 1;
const MAX_ARCHIVE_RECORDS: u32 = 100_000;
const MIN_ARCHIVE_BYTES: u64 = 64 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ARCHIVE_TEMP_COLLISIONS: usize = 64;
const MAX_PRIVATE_DIRECTORY_ENTRIES: usize = 4_096;
const MAX_STALE_PROJECTION_TEMPS: usize = 1024;
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
        "CREATE TABLE observations (event_id TEXT PRIMARY KEY, source TEXT NOT NULL, generation TEXT NOT NULL, observation_id TEXT NOT NULL, trace_id TEXT NOT NULL, observed_at_unix_ms TEXT NOT NULL, payload_hash TEXT NOT NULL, projected_json TEXT NOT NULL, UNIQUE(source, generation, observation_id))",
    ),
    (
        "table",
        "source_inputs",
        "CREATE TABLE source_inputs (source TEXT NOT NULL, generation TEXT NOT NULL, cursor TEXT NOT NULL, event_id TEXT NOT NULL REFERENCES observations(event_id), payload_hash TEXT NOT NULL, PRIMARY KEY(source, generation, cursor))",
    ),
    (
        "table",
        "expired_span_states",
        "CREATE TABLE expired_span_states (guard_seq INTEGER PRIMARY KEY AUTOINCREMENT, span_id TEXT NOT NULL UNIQUE, canonical_state_hash TEXT NOT NULL)",
    ),
    (
        "table",
        "retention_receipts",
        "CREATE TABLE retention_receipts (plan_id TEXT PRIMARY KEY, cutoff_unix_ms TEXT NOT NULL, traces INTEGER NOT NULL, observations INTEGER NOT NULL, records INTEGER NOT NULL, archive_bytes INTEGER NOT NULL, truncated INTEGER NOT NULL CHECK(truncated IN (0,1)), archive_path_hash TEXT NOT NULL, archive_sha256 TEXT NOT NULL, compacted INTEGER NOT NULL CHECK(compacted IN (0,1)))",
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
    (
        "index",
        "observations_trace_idx",
        "CREATE INDEX observations_trace_idx ON observations(trace_id, observed_at_unix_ms, event_id)",
    ),
    (
        "index",
        "records_trace_idx",
        "CREATE INDEX records_trace_idx ON records(trace_id, commit_seq)",
    ),
    (
        "index",
        "topology_trace_idx",
        "CREATE INDEX topology_trace_idx ON topology(trace_id, unresolved)",
    ),
];
static PROJECTION_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrashPoint {
    BeforeCommit,
    AfterCommit,
    BeforeProjectionRename,
    AfterProjectionRename,
    BeforeRetentionCommit,
    AfterRetentionCommit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IngestStatus {
    Committed,
    Duplicate,
    Suppressed,
}

#[derive(Clone, Debug)]
pub struct ReportSnapshot {
    pub generation: u64,
    pub records: Vec<DurableRecordV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReportVisit {
    pub generation: u64,
    pub records: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReportStatus {
    pub generation: u64,
    pub acknowledged_generation: u64,
}

#[derive(Debug)]
pub struct ReportRenderGuard {
    _file: File,
}

impl ReportStatus {
    #[must_use]
    pub const fn pending(self) -> bool {
        self.generation != self.acknowledged_generation
    }
}

#[derive(Clone, Copy, Debug)]
pub enum StoreBatchItem<'a> {
    Observation(&'a SourceObservation),
    Disposition {
        checkpoint: &'a SourceCheckpoint,
        disposition: AdapterDispositionKind,
        code: AdapterDispositionCode,
        canonical_payload_hash: Option<&'a str>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexCorrelationStateV1 {
    schema_version: String,
    next_sequence: u64,
    pending: Vec<CodexPendingCorrelationV1>,
    #[serde(default)]
    recently_completed: Vec<CodexCompletedOfficialRetryV1>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexPendingCorrelationV1 {
    source_generation_hash: String,
    conversation_hash: String,
    model_hash: String,
    correlation_id: String,
    #[serde(default)]
    official_retry_identity: Option<String>,
    inserted_at_unix_ms: u64,
    sequence: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexCompletedOfficialRetryV1 {
    source_generation_hash: String,
    conversation_hash: String,
    model_hash: String,
    official_retry_identity: String,
    completed_at_unix_ms: u64,
    sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionPlan {
    pub plan_id: String,
    pub cutoff_unix_ms: u64,
    pub traces: u64,
    pub observations: u64,
    pub records: u64,
    pub archive_bytes: u64,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionResult {
    pub plan: RetentionPlan,
    pub archive_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "entry_type", rename_all = "snake_case")]
enum RetentionArchiveEntry {
    Header {
        schema_version: String,
        plan_id: String,
        cutoff_unix_ms: u64,
    },
    Record {
        record: Box<DurableRecordV1>,
    },
    Footer {
        traces: u64,
        records: u64,
        records_sha256: String,
    },
}

#[derive(Debug)]
struct RetentionSelection {
    plan: RetentionPlan,
    trace_ids: Vec<String>,
    span_guards: Vec<(String, String)>,
}

#[derive(Debug)]
struct RetentionReceipt {
    plan: RetentionPlan,
    archive_path: PathBuf,
    archive_sha256: String,
    compacted: bool,
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
    StaleRetentionPlan,
    RetentionBoundsTooSmall,
    InvalidRetentionBounds,
    PendingRetentionRecovery,
    MigrationAdmissionRequired,
    ReportSnapshotChanged,
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
            Self::StaleRetentionPlan => "retention plan is stale",
            Self::RetentionBoundsTooSmall => {
                "retention bounds cannot fit every eligible complete trace in one pass"
            }
            Self::InvalidRetentionBounds => "retention bounds are outside the supported range",
            Self::PendingRetentionRecovery => {
                "a previous retention pass must be recovered before starting another"
            }
            Self::MigrationAdmissionRequired => {
                "legacy local store migration requires admitted temporary disk headroom"
            }
            Self::ReportSnapshotChanged => "report snapshot changed while it was being visited",
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
    /// Opens or creates private transactional state and repairs its JSONL projection when needed.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] for insecure permissions, incompatible state, or I/O failure.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::open_internal(dir.as_ref(), None, true)
    }

    /// Opens the store while allowing a legacy schema rewrite within admitted temporary bytes.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::MigrationAdmissionRequired`] before migration when the admitted
    /// temporary workspace is smaller than the conservative full-rewrite requirement.
    pub fn open_with_migration_headroom(
        dir: impl AsRef<Path>,
        admitted_temporary_bytes: u64,
    ) -> Result<Self, StoreError> {
        Self::open_internal(dir.as_ref(), Some(admitted_temporary_bytes), true)
    }

    /// Opens the store while allowing migration but defers non-authoritative JSONL repair.
    ///
    /// This keeps collector availability independent from projection filesystem work. Callers that
    /// require the JSONL artifact can repair it later through [`Self::repair_projection_if_needed`].
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] for insecure permissions, incompatible state, migration admission,
    /// or authoritative `SQLite` failure.
    pub fn open_with_migration_headroom_deferred_projection(
        dir: impl AsRef<Path>,
        admitted_temporary_bytes: u64,
    ) -> Result<Self, StoreError> {
        Self::open_internal(dir.as_ref(), Some(admitted_temporary_bytes), false)
    }

    /// Opens an already initialized current-schema store without creating, migrating, or repairing
    /// artifacts. This is intended for concurrent projection consumers such as report rendering.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store is missing, insecure, or not on the current schema.
    pub fn open_current(dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let dir = dir.as_ref();
        fs::symlink_metadata(dir)?;
        private_dir(dir)?;
        let dir = fs::canonicalize(dir)?;
        let db_path = dir.join(DB_NAME);
        private_file(&db_path)?;
        let db = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.pragma_update(None, "foreign_keys", true)?;
        db.pragma_update(None, "synchronous", "FULL")?;
        let schema = db
            .query_row(
                "SELECT value FROM metadata WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .map_err(|_| StoreError::SchemaMismatch)?;
        if schema != LOCAL_STORE_SCHEMA_VERSION {
            return Err(StoreError::SchemaMismatch);
        }
        metadata_generation(&db, REPORT_GENERATION_KEY)?;
        metadata_generation(&db, REPORT_ACKNOWLEDGED_GENERATION_KEY)?;
        Ok(Self { dir, db })
    }

    fn open_internal(
        dir: &Path,
        admitted_temporary_bytes: Option<u64>,
        repair_projection: bool,
    ) -> Result<Self, StoreError> {
        private_dir(dir)?;
        let dir = fs::canonicalize(dir)?;
        let _open_guard = acquire_private_lock(&dir, STORE_OPEN_LOCK_NAME)?;
        let db_path = dir.join(DB_NAME);
        match private_create_new(&db_path) {
            Ok(file) => file.sync_all()?,
            Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists => {
                private_file(&db_path)?;
            }
            Err(error) => return Err(error),
        }
        let db = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.pragma_update(None, "journal_mode", "DELETE")?;
        db.pragma_update(None, "foreign_keys", true)?;
        db.pragma_update(None, "synchronous", "FULL")?;
        db.pragma_update(None, "auto_vacuum", "INCREMENTAL")?;
        initialize_empty_schema(&db)?;
        let schema = db.query_row(
            "SELECT value FROM metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        );
        let schema: String = schema.map_err(|_| StoreError::SchemaMismatch)?;
        if matches!(
            schema.as_str(),
            "local_state.v1" | "local_state.v2" | "local_state.v3"
        ) {
            let database_bytes = fs::metadata(&db_path)?.len();
            let required_workspace = database_bytes.saturating_mul(2);
            if admitted_temporary_bytes.is_none_or(|bytes| bytes < required_workspace) {
                return Err(StoreError::MigrationAdmissionRequired);
            }
            migrate_to_v4(&db)?;
        } else if schema != LOCAL_STORE_SCHEMA_VERSION {
            return Err(StoreError::SchemaMismatch);
        }
        validate_schema(&db)?;
        ensure_report_metadata(&db)?;
        let store = Self { dir, db };
        if repair_projection {
            store.repair_projection_if_needed()?;
        }
        Ok(store)
    }

    /// Repairs the JSONL projection only when it is missing or marked dirty.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when projection state cannot be validated or rebuilt.
    pub fn repair_projection_if_needed(&self) -> Result<bool, StoreError> {
        let projection_path = self.projection_path();
        let projection_missing = match fs::symlink_metadata(&projection_path) {
            Ok(_) => {
                private_file(&projection_path)?;
                false
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => true,
            Err(error) => return Err(error.into()),
        };
        let projection_dirty = self.projection_dirty()?;
        if projection_missing || projection_dirty {
            self.rebuild_projection()?;
            return Ok(true);
        }
        Ok(false)
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

    /// Atomically commits an ordered observation batch while deferring projection rebuild.
    ///
    /// Every observation shares one transaction. A failure rolls back the entire batch, including
    /// cursor advancement, so callers can retry the same ordered input without partial progress.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::ingest`].
    pub fn ingest_batch_deferred_projection(
        &mut self,
        observations: &[SourceObservation],
    ) -> Result<Vec<IngestStatus>, StoreError> {
        let items = observations
            .iter()
            .map(StoreBatchItem::Observation)
            .collect::<Vec<_>>();
        self.ingest_ordered_batch_at(&items, None)
    }

    /// Atomically commits ordered observations and content-free dispositions while deferring the
    /// replayable projection rebuild.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::ingest`] and [`Self::ingest_disposition`].
    pub fn ingest_ordered_batch_deferred_projection(
        &mut self,
        items: &[StoreBatchItem<'_>],
    ) -> Result<Vec<IngestStatus>, StoreError> {
        self.ingest_ordered_batch_at(items, None)
    }

    /// Atomically commits a Codex batch and its bounded privacy-safe correlation snapshot.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::ingest_ordered_batch_deferred_projection`] and rejects
    /// malformed or non-private correlation snapshots.
    pub fn ingest_codex_batch_with_correlation_state_deferred_projection(
        &mut self,
        items: &[StoreBatchItem<'_>],
        source_generation: &str,
        correlation_state_json: &str,
    ) -> Result<Vec<IngestStatus>, StoreError> {
        self.ingest_ordered_batch_with_correlation_state_at(
            items,
            source_generation,
            correlation_state_json,
            None,
        )
    }

    /// Loads the private correlation snapshot for one Codex source generation.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] for invalid identifiers, malformed state, or storage failure.
    pub fn codex_request_correlation_state(
        &self,
        source_generation: &str,
    ) -> Result<Option<String>, StoreError> {
        let key = codex_correlation_key(source_generation)?;
        let value = self
            .db
            .query_row("SELECT value FROM metadata WHERE key=?1", [key], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        if let Some(value) = value.as_deref() {
            validate_codex_correlation_state(value, source_generation)?;
        }
        Ok(value)
    }

    fn ingest_ordered_batch_at(
        &mut self,
        items: &[StoreBatchItem<'_>],
        crash: Option<CrashPoint>,
    ) -> Result<Vec<IngestStatus>, StoreError> {
        self.ingest_ordered_batch_at_inner(items, None, crash)
    }

    fn ingest_ordered_batch_with_correlation_state_at(
        &mut self,
        items: &[StoreBatchItem<'_>],
        source_generation: &str,
        correlation_state_json: &str,
        crash: Option<CrashPoint>,
    ) -> Result<Vec<IngestStatus>, StoreError> {
        validate_codex_correlation_state(correlation_state_json, source_generation)?;
        let key = codex_correlation_key(source_generation)?;
        self.ingest_ordered_batch_at_inner(items, Some((&key, correlation_state_json)), crash)
    }

    fn ingest_ordered_batch_at_inner(
        &mut self,
        items: &[StoreBatchItem<'_>],
        correlation_state: Option<(&str, &str)>,
        crash: Option<CrashPoint>,
    ) -> Result<Vec<IngestStatus>, StoreError> {
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let mut statuses = Vec::with_capacity(items.len());
        for item in items {
            statuses.push(match item {
                StoreBatchItem::Observation(observation) => {
                    Self::ingest_in_transaction(&tx, observation, None)?
                }
                StoreBatchItem::Disposition {
                    checkpoint,
                    disposition,
                    code,
                    canonical_payload_hash,
                } => Self::ingest_disposition_in_transaction(
                    &tx,
                    checkpoint,
                    *disposition,
                    *code,
                    *canonical_payload_hash,
                )?,
            });
        }
        if let Some((key, value)) = correlation_state {
            tx.execute(
                "INSERT INTO metadata(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, value],
            )?;
        }
        if crash == Some(CrashPoint::BeforeCommit) {
            return Err(StoreError::Crash(CrashPoint::BeforeCommit));
        }
        tx.commit()?;
        if crash == Some(CrashPoint::AfterCommit) {
            return Err(StoreError::Crash(CrashPoint::AfterCommit));
        }
        Ok(statuses)
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
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let status = Self::ingest_disposition_in_transaction(
            &tx,
            checkpoint,
            disposition,
            code,
            canonical_payload_hash,
        )?;
        tx.commit()?;
        Ok(status)
    }

    fn ingest_disposition_in_transaction(
        tx: &Transaction<'_>,
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
        if cursor_exists(tx, "source_inputs", source, &generation, cursor)? {
            return Err(StoreError::PayloadConflict);
        }
        if !checkpoint_cursor_matches(tx, checkpoint)? {
            return Err(StoreError::CursorConflict);
        }
        tx.execute(
            "INSERT INTO adapter_dispositions(source,generation,cursor,disposition,code,payload_hash) VALUES (?1,?2,?3,?4,?5,?6)",
            params![source, generation, cursor, disposition.as_str(), code.as_str(), payload_hash],
        )?;
        advance_checkpoint_cursor(tx, checkpoint)?;
        prune_adapter_dispositions(tx)?;
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

    fn ingest_at(
        &mut self,
        observation: &SourceObservation,
        crash: Option<CrashPoint>,
        rebuild_projection: bool,
    ) -> Result<IngestStatus, StoreError> {
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let status = Self::ingest_in_transaction(&tx, observation, crash)?;
        tx.commit()?;
        if crash == Some(CrashPoint::AfterCommit) {
            return Err(StoreError::Crash(CrashPoint::AfterCommit));
        }
        self.rebuild_if_enabled(rebuild_projection, crash)?;
        Ok(status)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the ordered cursor, dedupe, and crash-point transaction stays contiguous"
    )]
    fn ingest_in_transaction(
        tx: &Transaction<'_>,
        observation: &SourceObservation,
        crash: Option<CrashPoint>,
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
        if let Some((disposition, code, existing_hash)) =
            existing_disposition(tx, source, &generation, cursor)?
        {
            if disposition == AdapterDispositionKind::Suppressed.as_str()
                && code == AdapterDispositionCode::DuplicateObservation.as_str()
                && existing_hash == payload_hash
            {
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
            return Ok(IngestStatus::Duplicate);
        }
        if !cursor_matches(tx, observation)? {
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
            ensure_static_record_compatibility(tx, state.span_id.as_str(), &incoming_record)?;
            let existing_state = state_from_json(&existing_state_json)?;
            if same_canonical_observation(&existing_state, &state) {
                insert_duplicate_disposition(tx, observation)?;
                if crash == Some(CrashPoint::BeforeCommit) {
                    return Err(StoreError::Crash(CrashPoint::BeforeCommit));
                }
                mark_projection_dirty(tx)?;
                return Ok(IngestStatus::Suppressed);
            }
        }
        if let Some(existing_hash) = tx
            .query_row(
                "SELECT canonical_state_hash FROM expired_span_states WHERE span_id=?1",
                [state.span_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            if existing_hash != canonical_state_hash(&state)? {
                return Err(StoreError::PayloadConflict);
            }
            insert_duplicate_disposition(tx, observation)?;
            if crash == Some(CrashPoint::BeforeCommit) {
                return Err(StoreError::Crash(CrashPoint::BeforeCommit));
            }
            mark_projection_dirty(tx)?;
            return Ok(IngestStatus::Suppressed);
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
            advance_cursor(tx, observation)?;
            mark_projection_dirty(tx)?;
            if crash == Some(CrashPoint::BeforeCommit) {
                return Err(StoreError::Crash(CrashPoint::BeforeCommit));
            }
            return Ok(IngestStatus::Duplicate);
        }
        let existing = load_state(tx, state.span_id.as_str())?;
        if existing.is_some() {
            ensure_static_record_compatibility(tx, state.span_id.as_str(), &incoming_record)?;
        }
        let reduced = reduce_span_state(existing.as_ref(), state).map_err(map_reduction_error)?;
        let projection_observation = projection_observation(observation, &reduced);
        let record = sanitize_durable_record(
            &project_durable_record(
                &projection_observation,
                &projection_state(&projection_observation, &reduced),
            )
            .map_err(|_| StoreError::InvalidObservation)?,
        )
        .map_err(|_| StoreError::InvalidObservation)?;
        let record_json = serde_json::to_string(&record)?;
        let state_json = state_to_json(&reduced)?;
        let parent = reduced.parent_span_id.as_ref().map(SpanId::as_str);
        let unresolved = match parent {
            Some(parent_id) => i32::from(!topology_contains(tx, parent_id)?),
            None => 0,
        };
        tx.execute("INSERT INTO observations(event_id, source, generation, observation_id, trace_id, observed_at_unix_ms, payload_hash, projected_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![event_id, source, generation, hash_opaque_identifier(observation.observation_id.as_str()), reduced.trace_id.as_str(), record_observed_at_millis(&incoming_record)?, payload_hash, projected_json])?;
        tx.execute("INSERT INTO source_inputs(source, generation, cursor, event_id, payload_hash) VALUES (?1,?2,?3,?4,?5)", params![source, generation, cursor, event_id, payload_hash])?;
        tx.execute("INSERT INTO records(span_id, trace_id, parent_span_id, kind, state_json, record_json) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(span_id) DO UPDATE SET state_json=excluded.state_json, record_json=excluded.record_json", params![reduced.span_id.as_str(), reduced.trace_id.as_str(), parent, kind_name(reduced.kind), state_json, record_json])?;
        tx.execute("INSERT INTO topology(span_id, trace_id, parent_span_id, kind, unresolved) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(span_id) DO UPDATE SET unresolved=excluded.unresolved", params![reduced.span_id.as_str(), reduced.trace_id.as_str(), parent, kind_name(reduced.kind), unresolved])?;
        validate_topology_chain(tx, reduced.span_id.as_str())?;
        validate_direct_children(tx, reduced.span_id.as_str())?;
        tx.execute(
            "UPDATE topology SET unresolved=0 WHERE parent_span_id=?1",
            [reduced.span_id.as_str()],
        )?;
        tx.execute(
            "INSERT INTO delivery_outcomes(event_id, outcome) VALUES (?1, 'not_applicable')",
            [event_id.as_str()],
        )?;
        advance_cursor(tx, observation)?;
        mark_projection_dirty(tx)?;
        advance_report_generation(tx)?;
        if crash == Some(CrashPoint::BeforeCommit) {
            return Err(StoreError::Crash(CrashPoint::BeforeCommit));
        }
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
        tx.execute(
            "UPDATE metadata SET value='0' WHERE key='projection_dirty'",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn projection_dirty(&self) -> Result<bool, StoreError> {
        let value = self.db.query_row(
            "SELECT value FROM metadata WHERE key='projection_dirty'",
            [],
            |row| row.get::<_, String>(0),
        )?;
        match value.as_str() {
            "0" => Ok(false),
            "1" => Ok(true),
            _ => Err(StoreError::SchemaMismatch),
        }
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

    /// Reads one consistent, ordered snapshot of current reduced durable records.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the authoritative transaction cannot be read or a stored record
    /// no longer satisfies its typed JSON contract.
    pub fn current_records(&self) -> Result<Vec<DurableRecordV1>, StoreError> {
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
        let records = {
            let mut statement =
                tx.prepare("SELECT record_json FROM records ORDER BY commit_seq")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            rows.map(|row| {
                let json = row?;
                serde_json::from_str(&json).map_err(StoreError::Json)
            })
            .collect::<Result<Vec<_>, _>>()?
        };
        tx.commit()?;
        Ok(records)
    }

    /// Reads current records and their report generation from one consistent snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the snapshot or stored record contract cannot be read.
    pub fn report_snapshot(&self) -> Result<ReportSnapshot, StoreError> {
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
        let generation = metadata_generation(&tx, REPORT_GENERATION_KEY)?;
        let records = {
            let mut statement =
                tx.prepare("SELECT record_json FROM records ORDER BY commit_seq")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            rows.map(|row| {
                let json = row?;
                serde_json::from_str(&json).map_err(StoreError::Json)
            })
            .collect::<Result<Vec<_>, _>>()?
        };
        tx.commit()?;
        Ok(ReportSnapshot {
            generation,
            records,
        })
    }

    /// Visits one consistent, ordered report snapshot without retaining all source records.
    ///
    /// Rows are copied in bounded transactions and visited after each read transaction closes, so
    /// projection work cannot hold a `SQLite` read lock. A generation fence rejects a multi-batch
    /// visit if a writer commits between batches. The returned count belongs to the visited
    /// generation and is not queried separately.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the snapshot or a stored record cannot be read.
    pub fn visit_report_snapshot(
        &self,
        mut visit: impl FnMut(usize, DurableRecordV1),
    ) -> Result<ReportVisit, StoreError> {
        let mut expected_generation = None;
        let mut expected_records = None;
        let mut last_commit_seq = 0_i64;
        let mut visited = 0_usize;

        loop {
            let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
            let generation = metadata_generation(&tx, REPORT_GENERATION_KEY)?;
            if expected_generation.is_some_and(|expected| expected != generation) {
                return Err(StoreError::ReportSnapshotChanged);
            }
            let generation = *expected_generation.get_or_insert(generation);
            let records = if let Some(records) = expected_records {
                records
            } else {
                let count = tx.query_row("SELECT COUNT(*) FROM records", [], |row| {
                    row.get::<_, i64>(0)
                })?;
                let count = usize::try_from(count).map_err(|_| StoreError::SchemaMismatch)?;
                expected_records = Some(count);
                count
            };
            let batch = {
                let mut statement = tx.prepare(
                    "SELECT commit_seq, record_json FROM records WHERE commit_seq > ?1 ORDER BY commit_seq LIMIT ?2",
                )?;
                let rows = statement
                    .query_map(params![last_commit_seq, REPORT_VISIT_BATCH_SIZE], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                    })?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            tx.commit()?;

            if batch.is_empty() {
                if visited == records {
                    return Ok(ReportVisit {
                        generation,
                        records,
                    });
                }
                return Err(StoreError::SchemaMismatch);
            }
            for (commit_seq, json) in batch {
                if commit_seq <= last_commit_seq || visited >= records {
                    return Err(StoreError::SchemaMismatch);
                }
                let record = serde_json::from_str(&json).map_err(StoreError::Json)?;
                visit(visited, record);
                visited = visited.checked_add(1).ok_or(StoreError::SchemaMismatch)?;
                last_commit_seq = commit_seq;
            }
            if visited == records {
                if metadata_generation(&self.db, REPORT_GENERATION_KEY)? != generation {
                    return Err(StoreError::ReportSnapshotChanged);
                }
                return Ok(ReportVisit {
                    generation,
                    records,
                });
            }
        }
    }

    /// Returns the durable report generation state.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when generation metadata is missing, invalid, or cannot be read.
    pub fn report_status(&self) -> Result<ReportStatus, StoreError> {
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
        let status = ReportStatus {
            generation: metadata_generation(&tx, REPORT_GENERATION_KEY)?,
            acknowledged_generation: metadata_generation(&tx, REPORT_ACKNOWLEDGED_GENERATION_KEY)?,
        };
        tx.commit()?;
        if status.acknowledged_generation > status.generation {
            return Err(StoreError::SchemaMismatch);
        }
        Ok(status)
    }

    /// Serializes report artifact publication across local processes.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the private render lock cannot be safely acquired.
    pub fn acquire_report_render_guard(&self) -> Result<ReportRenderGuard, StoreError> {
        let file = acquire_private_lock(&self.dir, REPORT_RENDER_LOCK_NAME)?;
        Ok(ReportRenderGuard { _file: file })
    }

    /// Acknowledges a report only when authority is still at the rendered generation.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when durable generation metadata cannot be updated transactionally.
    pub fn acknowledge_report_generation(&self, generation: u64) -> Result<bool, StoreError> {
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let current = metadata_generation(&tx, REPORT_GENERATION_KEY)?;
        if current != generation {
            tx.commit()?;
            return Ok(false);
        }
        tx.execute(
            "UPDATE metadata SET value=?1 WHERE key=?2",
            params![generation.to_string(), REPORT_ACKNOWLEDGED_GENERATION_KEY],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// Plans a bounded archive-and-prune pass without changing authority or projections.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the authoritative snapshot is invalid or cannot be read.
    pub fn retention_plan(
        &self,
        cutoff_unix_ms: u64,
        max_archive_records: u32,
        max_archive_bytes: u64,
    ) -> Result<RetentionPlan, StoreError> {
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let selection =
            select_retention(&tx, cutoff_unix_ms, max_archive_records, max_archive_bytes)?;
        tx.commit()?;
        Ok(selection.plan)
    }

    /// Writes one private archive and physically expires the exact selected payloads.
    ///
    /// Active cursors and bounded canonical span-state replay guards are preserved.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when selection, private archive creation, the transaction, or
    /// projection repair fails.
    pub fn apply_retention(
        &self,
        cutoff_unix_ms: u64,
        max_archive_records: u32,
        max_archive_bytes: u64,
        expected_plan_id: &str,
        archive_path: &Path,
    ) -> Result<RetentionResult, StoreError> {
        self.apply_retention_at(
            cutoff_unix_ms,
            max_archive_records,
            max_archive_bytes,
            expected_plan_id,
            archive_path,
            None,
        )
    }

    fn apply_retention_at(
        &self,
        cutoff_unix_ms: u64,
        max_archive_records: u32,
        max_archive_bytes: u64,
        expected_plan_id: &str,
        archive_path: &Path,
        crash: Option<CrashPoint>,
    ) -> Result<RetentionResult, StoreError> {
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        if let Some(receipt) = load_retention_receipt(&tx, expected_plan_id, archive_path)? {
            tx.commit()?;
            validate_retention_archive(archive_path, &receipt)?;
            self.finish_retention_receipt(&receipt)?;
            return Ok(RetentionResult {
                plan: receipt.plan,
                archive_path: Some(receipt.archive_path),
            });
        }
        if pending_retention_receipt_exists(&tx)? {
            return Err(StoreError::PendingRetentionRecovery);
        }
        let selection =
            select_retention(&tx, cutoff_unix_ms, max_archive_records, max_archive_bytes)?;
        if selection.plan.plan_id != expected_plan_id {
            return Err(StoreError::StaleRetentionPlan);
        }
        if selection.plan.truncated {
            return Err(StoreError::RetentionBoundsTooSmall);
        }
        if selection.plan.traces == 0 {
            tx.commit()?;
            return Ok(RetentionResult {
                plan: selection.plan,
                archive_path: None,
            });
        }
        tx.execute_batch(
            "CREATE TEMP TABLE retention_selected_traces(ordinal INTEGER PRIMARY KEY, trace_id TEXT NOT NULL UNIQUE)",
        )?;
        for (ordinal, trace_id) in selection.trace_ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO retention_selected_traces(ordinal, trace_id) VALUES (?1, ?2)",
                params![
                    i64::try_from(ordinal).map_err(|_| StoreError::SchemaMismatch)?,
                    trace_id
                ],
            )?;
        }
        let archive_sha256 = write_archive(&tx, archive_path, &selection)?;
        insert_retention_receipt(&tx, &selection.plan, archive_path, &archive_sha256)?;
        for (span_id, canonical_state_hash) in &selection.span_guards {
            tx.execute(
                "INSERT OR REPLACE INTO expired_span_states(span_id, canonical_state_hash) VALUES (?1, ?2)",
                params![span_id, canonical_state_hash],
            )?;
        }
        prune_expired_span_guards(&tx)?;
        tx.execute_batch(
            "DELETE FROM delivery_outcomes WHERE event_id IN (SELECT event_id FROM observations WHERE trace_id IN (SELECT trace_id FROM retention_selected_traces));
             DELETE FROM source_inputs WHERE event_id IN (SELECT event_id FROM observations WHERE trace_id IN (SELECT trace_id FROM retention_selected_traces));
             DELETE FROM observations WHERE trace_id IN (SELECT trace_id FROM retention_selected_traces);
             DELETE FROM topology WHERE trace_id IN (SELECT trace_id FROM retention_selected_traces);
             DELETE FROM records WHERE trace_id IN (SELECT trace_id FROM retention_selected_traces);
             DROP TABLE retention_selected_traces;",
        )?;
        mark_projection_dirty(&tx)?;
        advance_report_generation(&tx)?;
        if crash == Some(CrashPoint::BeforeRetentionCommit) {
            return Err(StoreError::Crash(CrashPoint::BeforeRetentionCommit));
        }
        tx.commit()?;
        if crash == Some(CrashPoint::AfterRetentionCommit) {
            return Err(StoreError::Crash(CrashPoint::AfterRetentionCommit));
        }
        let receipt = RetentionReceipt {
            plan: selection.plan.clone(),
            archive_path: archive_path.to_path_buf(),
            archive_sha256,
            compacted: false,
        };
        self.finish_retention_receipt(&receipt)?;
        Ok(RetentionResult {
            plan: selection.plan,
            archive_path: Some(archive_path.to_path_buf()),
        })
    }

    fn finish_retention_receipt(&self, receipt: &RetentionReceipt) -> Result<(), StoreError> {
        if !receipt.compacted {
            incremental_vacuum(&self.db, receipt.plan.archive_bytes)?;
            self.rebuild_projection()?;
        }
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        if !receipt.compacted {
            tx.execute(
                "UPDATE retention_receipts SET compacted=1 WHERE plan_id=?1",
                [&receipt.plan.plan_id],
            )?;
        }
        prune_retention_receipts(&tx)?;
        tx.commit()?;
        Ok(())
    }
}

fn codex_correlation_key(source_generation: &str) -> Result<String, StoreError> {
    if source_generation.is_empty() || source_generation.len() > 512 {
        return Err(StoreError::InvalidObservation);
    }
    Ok(format!(
        "{CODEX_CORRELATION_KEY_PREFIX}{}",
        hash_opaque_identifier(source_generation)
    ))
}

fn validate_codex_correlation_state(
    input: &str,
    source_generation: &str,
) -> Result<(), StoreError> {
    if input.len() > MAX_CODEX_CORRELATION_STATE_BYTES {
        return Err(StoreError::InvalidObservation);
    }
    let state: CodexCorrelationStateV1 = serde_json::from_str(input)?;
    let expected_generation = hash_opaque_identifier(source_generation);
    if state.schema_version != "codex_request_correlation.v1"
        || state.pending.len() > MAX_CODEX_PENDING_CORRELATIONS
        || state.recently_completed.len() > MAX_CODEX_RECENTLY_COMPLETED_CORRELATIONS
        || state
            .pending
            .len()
            .checked_add(state.recently_completed.len())
            .is_none_or(|len| len > MAX_CODEX_PENDING_CORRELATIONS)
        || state.pending.iter().any(|pending| {
            pending.source_generation_hash != expected_generation
                || !is_private_identifier_hash(&pending.conversation_hash)
                || !is_private_identifier_hash(&pending.model_hash)
                || !is_private_identifier_hash(&pending.correlation_id)
                || pending
                    .official_retry_identity
                    .as_deref()
                    .is_some_and(|identity| !is_private_identifier_hash(identity))
                || pending.sequence >= state.next_sequence
                || pending.inserted_at_unix_ms == 0
        })
        || state.recently_completed.iter().any(|completed| {
            completed.source_generation_hash != expected_generation
                || !is_private_identifier_hash(&completed.conversation_hash)
                || !is_private_identifier_hash(&completed.model_hash)
                || !is_private_identifier_hash(&completed.official_retry_identity)
                || completed.sequence >= state.next_sequence
                || completed.completed_at_unix_ms == 0
        })
    {
        return Err(StoreError::InvalidObservation);
    }
    Ok(())
}

fn is_private_identifier_hash(value: &str) -> bool {
    value.len() == 74
        && value.starts_with("id:sha256:")
        && value[10..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn prune_expired_span_guards(tx: &Transaction<'_>) -> Result<(), StoreError> {
    tx.execute(
        "DELETE FROM expired_span_states WHERE guard_seq NOT IN (SELECT guard_seq FROM expired_span_states ORDER BY guard_seq DESC LIMIT ?1)",
        [i64::try_from(MAX_EXPIRED_SPAN_GUARDS).map_err(|_| StoreError::SchemaMismatch)?],
    )?;
    Ok(())
}

fn prune_retention_receipts(db: &Connection) -> Result<(), StoreError> {
    db.execute(
        "DELETE FROM retention_receipts WHERE compacted=1 AND rowid NOT IN (SELECT rowid FROM retention_receipts WHERE compacted=1 ORDER BY rowid DESC LIMIT ?1)",
        [i64::try_from(MAX_RETENTION_RECEIPTS).map_err(|_| StoreError::SchemaMismatch)?],
    )?;
    Ok(())
}

fn pending_retention_receipt_exists(tx: &Transaction<'_>) -> Result<bool, StoreError> {
    tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM retention_receipts WHERE compacted=0)",
        [],
        |row| row.get(0),
    )
    .map_err(StoreError::from)
}

fn prune_adapter_dispositions(tx: &Transaction<'_>) -> Result<(), StoreError> {
    tx.execute(
        "DELETE FROM adapter_dispositions WHERE rowid NOT IN (SELECT rowid FROM adapter_dispositions ORDER BY rowid DESC LIMIT ?1)",
        [i64::try_from(MAX_ADAPTER_DISPOSITIONS).map_err(|_| StoreError::SchemaMismatch)?],
    )?;
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "bounded trace-group selection and its byte accounting stay visibly contiguous"
)]
fn select_retention(
    tx: &Transaction<'_>,
    cutoff_unix_ms: u64,
    max_archive_records: u32,
    max_archive_bytes: u64,
) -> Result<RetentionSelection, StoreError> {
    validate_retention_bounds(max_archive_records, max_archive_bytes)?;
    let mut trace_ids = Vec::new();
    let mut span_guards = Vec::new();
    let mut plan_inputs = Vec::new();
    let mut record_bytes = 0_u64;
    let mut observation_count = 0_u64;
    let mut record_count = 0_u64;
    let mut truncated = false;
    let mut statement = tx.prepare(
        "SELECT trace_id FROM observations GROUP BY trace_id HAVING MAX(observed_at_unix_ms) < ?1 ORDER BY MAX(observed_at_unix_ms), trace_id",
    )?;
    let candidates = statement.query_map([ordered_millis(cutoff_unix_ms)], |row| {
        row.get::<_, String>(0)
    })?;
    'candidates: for trace_id in candidates {
        let trace_id = trace_id?;
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM topology WHERE trace_id=?1 AND unresolved=1)",
            [&trace_id],
            |row| row.get::<_, bool>(0),
        )? {
            continue;
        }
        let group_records = tx
            .query_row(
                "SELECT COUNT(*) FROM records WHERE trace_id=?1",
                [&trace_id],
                |row| row.get::<_, i64>(0),
            )?
            .cast_unsigned();
        let next_records = record_count
            .checked_add(group_records)
            .ok_or(StoreError::SchemaMismatch)?;
        if group_records == 0 {
            continue;
        }
        if next_records > u64::from(max_archive_records) {
            truncated = true;
            break;
        }
        let header_bytes = encoded_line_len(&RetentionArchiveEntry::Header {
            schema_version: RETENTION_ARCHIVE_VERSION.into(),
            plan_id: "0".repeat(64),
            cutoff_unix_ms,
        })?;
        let footer_bytes = encoded_line_len(&RetentionArchiveEntry::Footer {
            traces: u64::try_from(trace_ids.len() + 1).map_err(|_| StoreError::SchemaMismatch)?,
            records: next_records,
            records_sha256: "0".repeat(64),
        })?;
        let fixed_bytes = record_bytes
            .checked_add(header_bytes)
            .and_then(|bytes| bytes.checked_add(footer_bytes))
            .ok_or(StoreError::SchemaMismatch)?;
        let group_byte_limit = max_archive_bytes.saturating_sub(fixed_bytes);
        if fixed_bytes > max_archive_bytes {
            truncated = true;
            break;
        }
        let mut group_span_guards = Vec::new();
        let mut group_bytes = 0_u64;
        let mut records = tx.prepare(
            "SELECT span_id, state_json, record_json FROM records WHERE trace_id=?1 ORDER BY commit_seq",
        )?;
        let rows = records.query_map([&trace_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (span_id, state_json, record_json) = row?;
            let state = state_from_json(&state_json)?;
            let record: DurableRecordV1 = serde_json::from_str(&record_json)?;
            let entry = RetentionArchiveEntry::Record {
                record: Box::new(record),
            };
            let next_group_bytes = group_bytes
                .checked_add(encoded_line_len(&entry)?)
                .ok_or(StoreError::SchemaMismatch)?;
            if next_group_bytes > group_byte_limit {
                truncated = true;
                break 'candidates;
            }
            group_bytes = next_group_bytes;
            group_span_guards.push((span_id, canonical_state_hash(&state)?));
        }
        drop(records);
        if u64::try_from(group_span_guards.len()).map_err(|_| StoreError::SchemaMismatch)?
            != group_records
        {
            return Err(StoreError::SchemaMismatch);
        }
        let mut group_observations = 0_u64;
        let mut observation_hash = Sha256::new();
        observation_hash.update(b"agent-observability-retention-trace-events-v1");
        let mut observations = tx.prepare(
            "SELECT event_id, payload_hash FROM observations WHERE trace_id=?1 ORDER BY observed_at_unix_ms, event_id",
        )?;
        let rows = observations.query_map([&trace_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (event_id, payload_hash) = row?;
            group_observations = group_observations
                .checked_add(1)
                .ok_or(StoreError::SchemaMismatch)?;
            for value in [&event_id, &payload_hash] {
                observation_hash.update(
                    u64::try_from(value.len())
                        .map_err(|_| StoreError::SchemaMismatch)?
                        .to_be_bytes(),
                );
                observation_hash.update(value.as_bytes());
            }
        }
        drop(observations);
        let group_plan_input = format!("{trace_id}:{}", hex_digest(observation_hash.finalize()));
        record_bytes = record_bytes
            .checked_add(group_bytes)
            .ok_or(StoreError::SchemaMismatch)?;
        observation_count += group_observations;
        record_count += group_records;
        plan_inputs.push(group_plan_input);
        trace_ids.push(trace_id);
        span_guards.extend(group_span_guards);
    }
    let plan_id = retention_plan_id(cutoff_unix_ms, &plan_inputs);
    let archive_bytes = if trace_ids.is_empty() {
        0
    } else {
        let header_bytes = encoded_line_len(&RetentionArchiveEntry::Header {
            schema_version: RETENTION_ARCHIVE_VERSION.into(),
            plan_id: plan_id.clone(),
            cutoff_unix_ms,
        })?;
        let footer_bytes = encoded_line_len(&RetentionArchiveEntry::Footer {
            traces: u64::try_from(trace_ids.len()).map_err(|_| StoreError::SchemaMismatch)?,
            records: record_count,
            records_sha256: "0".repeat(64),
        })?;
        header_bytes
            .checked_add(record_bytes)
            .and_then(|bytes| bytes.checked_add(footer_bytes))
            .ok_or(StoreError::SchemaMismatch)?
    };
    if archive_bytes > max_archive_bytes {
        return Err(StoreError::SchemaMismatch);
    }
    Ok(RetentionSelection {
        plan: RetentionPlan {
            plan_id,
            cutoff_unix_ms,
            traces: u64::try_from(trace_ids.len()).map_err(|_| StoreError::SchemaMismatch)?,
            observations: observation_count,
            records: record_count,
            archive_bytes,
            truncated,
        },
        trace_ids,
        span_guards,
    })
}

fn validate_retention_bounds(
    max_archive_records: u32,
    max_archive_bytes: u64,
) -> Result<(), StoreError> {
    if !(MIN_ARCHIVE_RECORDS..=MAX_ARCHIVE_RECORDS).contains(&max_archive_records)
        || !(MIN_ARCHIVE_BYTES..=MAX_ARCHIVE_BYTES).contains(&max_archive_bytes)
    {
        return Err(StoreError::InvalidRetentionBounds);
    }
    Ok(())
}

fn retention_plan_id(cutoff_unix_ms: u64, inputs: &[String]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"agent-observability-retention-plan-v1");
    hash.update(cutoff_unix_ms.to_be_bytes());
    for input in inputs {
        hash.update((input.len() as u64).to_be_bytes());
        hash.update(input.as_bytes());
    }
    hex_digest(hash.finalize())
}

fn canonical_state_hash(state: &DomainSpanState) -> Result<String, StoreError> {
    let mut hash = Sha256::new();
    hash.update(b"agent-observability-expired-span-state-v1");
    hash.update(state_to_json(state)?.as_bytes());
    Ok(hex_digest(hash.finalize()))
}

fn archive_path_hash(path: &Path) -> String {
    digest_components(&[
        "agent-observability-retention-archive-path-v1",
        &path.to_string_lossy(),
    ])
}

fn insert_retention_receipt(
    tx: &Transaction<'_>,
    plan: &RetentionPlan,
    archive_path: &Path,
    archive_sha256: &str,
) -> Result<(), StoreError> {
    tx.execute(
        "INSERT INTO retention_receipts(plan_id, cutoff_unix_ms, traces, observations, records, archive_bytes, truncated, archive_path_hash, archive_sha256, compacted) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,0)",
        params![
            plan.plan_id,
            ordered_millis(plan.cutoff_unix_ms),
            i64::try_from(plan.traces).map_err(|_| StoreError::SchemaMismatch)?,
            i64::try_from(plan.observations).map_err(|_| StoreError::SchemaMismatch)?,
            i64::try_from(plan.records).map_err(|_| StoreError::SchemaMismatch)?,
            i64::try_from(plan.archive_bytes).map_err(|_| StoreError::SchemaMismatch)?,
            i64::from(plan.truncated),
            archive_path_hash(archive_path),
            archive_sha256,
        ],
    )?;
    Ok(())
}

fn load_retention_receipt(
    tx: &Transaction<'_>,
    plan_id: &str,
    archive_path: &Path,
) -> Result<Option<RetentionReceipt>, StoreError> {
    let row = tx
        .query_row(
            "SELECT cutoff_unix_ms, traces, observations, records, archive_bytes, truncated, archive_path_hash, archive_sha256, compacted FROM retention_receipts WHERE plan_id=?1",
            [plan_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, bool>(8)?,
                ))
            },
        )
        .optional()?;
    let Some((
        cutoff,
        traces,
        observations,
        records,
        archive_bytes,
        truncated,
        stored_path_hash,
        archive_sha256,
        compacted,
    )) = row
    else {
        return Ok(None);
    };
    if stored_path_hash != archive_path_hash(archive_path) {
        return Ok(None);
    }
    let parse_unsigned = |value: i64| u64::try_from(value).map_err(|_| StoreError::SchemaMismatch);
    Ok(Some(RetentionReceipt {
        plan: RetentionPlan {
            plan_id: plan_id.into(),
            cutoff_unix_ms: cutoff.parse().map_err(|_| StoreError::SchemaMismatch)?,
            traces: parse_unsigned(traces)?,
            observations: parse_unsigned(observations)?,
            records: parse_unsigned(records)?,
            archive_bytes: parse_unsigned(archive_bytes)?,
            truncated,
        },
        archive_path: archive_path.into(),
        archive_sha256,
        compacted,
    }))
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    let mut output = String::with_capacity(bytes.as_ref().len() * 2);
    for byte in bytes.as_ref() {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn encoded_line_len(entry: &RetentionArchiveEntry) -> Result<u64, StoreError> {
    let length = serde_json::to_vec(entry)?.len();
    u64::try_from(length)
        .ok()
        .and_then(|length| length.checked_add(1))
        .ok_or(StoreError::SchemaMismatch)
}

fn ordered_millis(value: u64) -> String {
    format!("{value:020}")
}

fn record_observed_at_millis(record: &DurableRecordV1) -> Result<String, StoreError> {
    let observed_at = record.end_time_unix_ms.unwrap_or(record.start_time_unix_ms);
    if !observed_at.is_finite()
        || observed_at < 0.0
        || observed_at.fract() != 0.0
        || observed_at > 9_007_199_254_740_991.0
    {
        return Err(StoreError::SchemaMismatch);
    }
    let millis = format!("{observed_at:.0}")
        .parse::<u64>()
        .map_err(|_| StoreError::SchemaMismatch)?;
    Ok(ordered_millis(millis))
}

fn incremental_vacuum(db: &Connection, reclaimed_bytes: u64) -> Result<(), StoreError> {
    let page_size: i64 = db.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    let page_size = u64::try_from(page_size).map_err(|_| StoreError::SchemaMismatch)?;
    if page_size == 0 {
        return Err(StoreError::SchemaMismatch);
    }
    let pages = reclaimed_bytes
        .saturating_mul(16)
        .saturating_add(page_size - 1)
        .checked_div(page_size)
        .ok_or(StoreError::SchemaMismatch)?;
    if pages > 0 {
        let mut statement = db.prepare(&format!("PRAGMA incremental_vacuum({pages})"))?;
        let mut rows = statement.query([])?;
        while rows.next()?.is_some() {}
    }
    Ok(())
}

fn write_archive(
    tx: &Transaction<'_>,
    path: &Path,
    selection: &RetentionSelection,
) -> Result<String, StoreError> {
    let parent = path.parent().ok_or(StoreError::InvalidPath)?;
    private_dir(parent)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(StoreError::InvalidPath)?;
    let _directory_lock = lock_archive_directory(parent)?;
    cleanup_stale_archive_temps(parent, name)?;
    let (temporary, mut file) = create_archive_temp(parent, name, &PROJECTION_TEMP_SEQUENCE)?;
    let result = (|| -> Result<String, StoreError> {
        let mut archive_hash = Sha256::new();
        write_archive_entry(
            &mut file,
            &mut archive_hash,
            &RetentionArchiveEntry::Header {
                schema_version: RETENTION_ARCHIVE_VERSION.into(),
                plan_id: selection.plan.plan_id.clone(),
                cutoff_unix_ms: selection.plan.cutoff_unix_ms,
            },
        )?;
        let mut records_hash = Sha256::new();
        let mut records = 0_u64;
        let mut statement = tx.prepare(
            "SELECT records.record_json FROM retention_selected_traces JOIN records ON records.trace_id=retention_selected_traces.trace_id ORDER BY retention_selected_traces.ordinal, records.commit_seq",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            let record: DurableRecordV1 = serde_json::from_str(&row?)?;
            let entry = RetentionArchiveEntry::Record {
                record: Box::new(record),
            };
            let encoded = serde_json::to_vec(&entry)?;
            file.write_all(&encoded)?;
            file.write_all(b"\n")?;
            records_hash.update(&encoded);
            records_hash.update(b"\n");
            archive_hash.update(&encoded);
            archive_hash.update(b"\n");
            records = records.checked_add(1).ok_or(StoreError::SchemaMismatch)?;
        }
        if records != selection.plan.records {
            return Err(StoreError::SchemaMismatch);
        }
        write_archive_entry(
            &mut file,
            &mut archive_hash,
            &RetentionArchiveEntry::Footer {
                traces: selection.plan.traces,
                records,
                records_sha256: hex_digest(records_hash.finalize()),
            },
        )?;
        file.sync_all()?;
        if file.metadata()?.len() != selection.plan.archive_bytes {
            return Err(StoreError::SchemaMismatch);
        }
        fs::hard_link(&temporary, path)?;
        fs::remove_file(&temporary)?;
        private_file(path)?;
        File::open(parent)?.sync_all()?;
        Ok(hex_digest(archive_hash.finalize()))
    })();
    if result.is_err() {
        drop(file);
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn lock_archive_directory(parent: &Path) -> Result<File, StoreError> {
    let path = parent.join(".agent-observability.retention.lock");
    if let Ok(metadata) = fs::symlink_metadata(&path)
        && metadata.file_type().is_symlink()
    {
        return Err(StoreError::Symlink);
    }
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(no_follow_flag());
    }
    let file = options.open(path)?;
    private_open_file(&file)?;
    file.try_lock_exclusive()?;
    Ok(file)
}

fn acquire_private_lock(parent: &Path, name: &str) -> Result<File, StoreError> {
    let path = parent.join(name);
    if let Ok(metadata) = fs::symlink_metadata(&path)
        && metadata.file_type().is_symlink()
    {
        return Err(StoreError::Symlink);
    }
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(no_follow_flag());
    }
    let file = options.open(path)?;
    private_open_file(&file)?;
    file.lock_exclusive()?;
    Ok(file)
}

fn write_archive_entry(
    file: &mut File,
    archive_hash: &mut Sha256,
    entry: &RetentionArchiveEntry,
) -> Result<(), StoreError> {
    let encoded = serde_json::to_vec(entry)?;
    file.write_all(&encoded)?;
    file.write_all(b"\n")?;
    archive_hash.update(&encoded);
    archive_hash.update(b"\n");
    Ok(())
}

fn create_archive_temp(
    parent: &Path,
    archive_name: &str,
    sequence: &AtomicU64,
) -> Result<(PathBuf, File), StoreError> {
    for _ in 0..MAX_ARCHIVE_TEMP_COLLISIONS {
        let sequence = sequence.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".{archive_name}.retention.tmp.{}.{sequence}",
            std::process::id()
        ));
        match private_create_new(&path) {
            Ok(file) => return Ok((path, file)),
            Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(StoreError::Io(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "archive temporary-file collision limit exceeded",
    )))
}

fn cleanup_stale_archive_temps(parent: &Path, archive_name: &str) -> Result<(), StoreError> {
    let prefix = format!(".{archive_name}.retention.tmp.");
    for (index, entry) in fs::read_dir(parent)?.enumerate() {
        if index >= MAX_PRIVATE_DIRECTORY_ENTRIES {
            return Err(StoreError::InvalidPath);
        }
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(suffix) = name.strip_prefix(&prefix) else {
            continue;
        };
        let Some((pid, sequence)) = suffix.split_once('.') else {
            continue;
        };
        if pid.is_empty()
            || sequence.is_empty()
            || !pid.bytes().all(|byte| byte.is_ascii_digit())
            || !sequence.bytes().all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        let path = entry.path();
        match private_file(&path) {
            Ok(()) => fs::remove_file(path)?,
            Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn validate_retention_archive(path: &Path, receipt: &RetentionReceipt) -> Result<(), StoreError> {
    let plan = &receipt.plan;
    private_file(path)?;
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() != plan.archive_bytes {
        return Err(StoreError::SchemaMismatch);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o777 != 0o600 {
            return Err(StoreError::InsecurePermissions);
        }
    }
    private_file(path)?;

    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut line_number = 0_u64;
    let mut record_count = 0_u64;
    let mut trace_ids = BTreeSet::new();
    let mut records_hash = Sha256::new();
    let mut archive_hash = Sha256::new();
    let mut footer_seen = false;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        line_number = line_number
            .checked_add(1)
            .ok_or(StoreError::SchemaMismatch)?;
        if !line.ends_with('\n') {
            return Err(StoreError::SchemaMismatch);
        }
        archive_hash.update(line.as_bytes());
        let entry: RetentionArchiveEntry = serde_json::from_str(&line)?;
        match entry {
            RetentionArchiveEntry::Header {
                schema_version,
                plan_id,
                cutoff_unix_ms,
            } if line_number == 1
                && schema_version == RETENTION_ARCHIVE_VERSION
                && plan_id == plan.plan_id
                && cutoff_unix_ms == plan.cutoff_unix_ms => {}
            RetentionArchiveEntry::Record { record } if line_number > 1 && !footer_seen => {
                record.validate().map_err(|_| StoreError::SchemaMismatch)?;
                record_count = record_count
                    .checked_add(1)
                    .ok_or(StoreError::SchemaMismatch)?;
                trace_ids.insert(record.trace_id.clone());
                records_hash.update(line.as_bytes());
            }
            RetentionArchiveEntry::Footer {
                traces,
                records,
                records_sha256,
            } if line_number > 1
                && !footer_seen
                && traces == plan.traces
                && records == plan.records
                && records_sha256 == hex_digest(records_hash.finalize_reset()) =>
            {
                footer_seen = true;
            }
            _ => return Err(StoreError::SchemaMismatch),
        }
    }
    if !footer_seen
        || record_count != plan.records
        || u64::try_from(trace_ids.len()).map_err(|_| StoreError::SchemaMismatch)? != plan.traces
        || hex_digest(archive_hash.finalize()) != receipt.archive_sha256
    {
        return Err(StoreError::SchemaMismatch);
    }
    Ok(())
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

fn projection_observation(
    observation: &SourceObservation,
    reduced: &DomainSpanState,
) -> SourceObservation {
    let mut projected = observation.clone();
    if let ObservationEvent::ToolOperation { phase, .. } = &mut projected.event {
        *phase = match reduced.lifecycle {
            LifecycleState::Running => Some("start".into()),
            LifecycleState::Completed => Some("result".into()),
            LifecycleState::Failed | LifecycleState::Interrupted => Some("failure".into()),
            LifecycleState::Observed => phase.clone(),
        };
    }
    projected
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
    advance_cursor(tx, observation)?;
    prune_adapter_dispositions(tx)
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

fn load_state(tx: &Transaction<'_>, span_id: &str) -> Result<Option<DomainSpanState>, StoreError> {
    let state = tx
        .query_row(
            "SELECT state_json FROM records WHERE span_id=?1",
            [span_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    state.map(|json| state_from_json(&json)).transpose()
}

fn topology_contains(tx: &Transaction<'_>, span_id: &str) -> Result<bool, StoreError> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM topology WHERE span_id=?1)",
        [span_id],
        |row| row.get::<_, bool>(0),
    )?)
}

fn validate_topology_chain(tx: &Transaction<'_>, span_id: &str) -> Result<(), StoreError> {
    const MAX_TOPOLOGY_DEPTH: usize = 4_096;
    let mut states = Vec::new();
    let mut seen = BTreeSet::new();
    let mut current = Some(span_id.to_owned());
    while let Some(id) = current {
        if states.len() >= MAX_TOPOLOGY_DEPTH || !seen.insert(id.clone()) {
            return Err(StoreError::InvalidObservation);
        }
        let Some(state) = load_state(tx, &id)? else {
            break;
        };
        current = state
            .parent_span_id
            .as_ref()
            .map(|parent| parent.as_str().to_owned());
        states.push(state);
    }
    agent_observability_domain::validate_topology(&states).map_err(map_reduction_error)?;
    Ok(())
}

fn validate_direct_children(tx: &Transaction<'_>, parent_span_id: &str) -> Result<(), StoreError> {
    let mut statement = tx.prepare(
        "SELECT span_id FROM topology WHERE parent_span_id=?1 ORDER BY span_id LIMIT 4097",
    )?;
    let rows = statement.query_map([parent_span_id], |row| row.get::<_, String>(0))?;
    let children = rows.collect::<Result<Vec<_>, _>>()?;
    if children.len() > 4_096 {
        return Err(StoreError::InvalidObservation);
    }
    drop(statement);
    for child in children {
        validate_topology_chain(tx, &child)?;
    }
    Ok(())
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

#[cfg(unix)]
fn private_open_file(file: &File) -> Result<(), StoreError> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(StoreError::InvalidPath);
    }
    if metadata.permissions().mode() & 0o777 != 0o600 {
        return Err(StoreError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn private_open_file(file: &File) -> Result<(), StoreError> {
    if !file.metadata()?.is_file() {
        return Err(StoreError::InvalidPath);
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "android"))]
const fn no_follow_flag() -> i32 {
    0x20_000
}

#[cfg(target_os = "macos")]
const fn no_follow_flag() -> i32 {
    0x100
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
    let mut stale = Vec::new();
    for (index, entry) in fs::read_dir(dir)?.enumerate() {
        if index >= MAX_PRIVATE_DIRECTORY_ENTRIES {
            return Err(StoreError::SchemaMismatch);
        }
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(&prefix) {
            continue;
        }
        stale.push(entry.path());
        if stale.len() > MAX_STALE_PROJECTION_TEMPS {
            return Err(StoreError::SchemaMismatch);
        }
    }
    for path in stale {
        private_file(&path)?;
        fs::remove_file(path)?;
    }
    Ok(())
}

fn mark_projection_dirty(tx: &Transaction<'_>) -> Result<(), StoreError> {
    tx.execute(
        "UPDATE metadata SET value='1' WHERE key='projection_dirty'",
        [],
    )?;
    Ok(())
}

fn metadata_generation(db: &Connection, key: &str) -> Result<u64, StoreError> {
    let value: String = db
        .query_row("SELECT value FROM metadata WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .map_err(|_| StoreError::SchemaMismatch)?;
    value.parse().map_err(|_| StoreError::SchemaMismatch)
}

fn advance_report_generation(tx: &Transaction<'_>) -> Result<(), StoreError> {
    let generation = metadata_generation(tx, REPORT_GENERATION_KEY)?
        .checked_add(1)
        .ok_or(StoreError::SchemaMismatch)?;
    tx.execute(
        "UPDATE metadata SET value=?1 WHERE key=?2",
        params![generation.to_string(), REPORT_GENERATION_KEY],
    )?;
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
        tx.execute(
            "INSERT INTO metadata(key, value) VALUES ('projection_dirty', '1')",
            [],
        )?;
        tx.execute(
            "INSERT INTO metadata(key, value) VALUES (?1, '0')",
            [REPORT_GENERATION_KEY],
        )?;
        tx.execute(
            "INSERT INTO metadata(key, value) VALUES (?1, '0')",
            [REPORT_ACKNOWLEDGED_GENERATION_KEY],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn ensure_report_metadata(db: &Connection) -> Result<(), StoreError> {
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let generation = tx
        .query_row(
            "SELECT value FROM metadata WHERE key=?1",
            [REPORT_GENERATION_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let acknowledged = tx
        .query_row(
            "SELECT value FROM metadata WHERE key=?1",
            [REPORT_ACKNOWLEDGED_GENERATION_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    match (generation, acknowledged) {
        (None, None) => {
            tx.execute(
                "INSERT INTO metadata(key, value) VALUES (?1, '1')",
                [REPORT_GENERATION_KEY],
            )?;
            tx.execute(
                "INSERT INTO metadata(key, value) VALUES (?1, '0')",
                [REPORT_ACKNOWLEDGED_GENERATION_KEY],
            )?;
        }
        (Some(generation), Some(acknowledged)) => {
            let generation = generation
                .parse::<u64>()
                .map_err(|_| StoreError::SchemaMismatch)?;
            let acknowledged = acknowledged
                .parse::<u64>()
                .map_err(|_| StoreError::SchemaMismatch)?;
            if acknowledged > generation {
                return Err(StoreError::SchemaMismatch);
            }
        }
        _ => return Err(StoreError::SchemaMismatch),
    }
    tx.commit()?;
    Ok(())
}

fn migrate_to_v4(db: &Connection) -> Result<(), StoreError> {
    db.pragma_update(None, "foreign_keys", false)?;
    let result = prune_legacy_adapter_dispositions(db)
        .and_then(|()| db.execute_batch("VACUUM").map_err(StoreError::from))
        .and_then(|()| migrate_to_v4_inner(db));
    db.pragma_update(None, "foreign_keys", true)?;
    result
}

fn prune_legacy_adapter_dispositions(db: &Connection) -> Result<(), StoreError> {
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let version: String = tx
        .query_row(
            "SELECT value FROM metadata WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| StoreError::SchemaMismatch)?;
    if !matches!(version.as_str(), "local_state.v2" | "local_state.v3") {
        tx.commit()?;
        return Ok(());
    }
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='adapter_dispositions')",
        [],
        |row| row.get(0),
    )?;
    if exists {
        prune_adapter_dispositions(&tx)?;
    }
    tx.commit()?;
    Ok(())
}

fn migrate_to_v4_inner(db: &Connection) -> Result<(), StoreError> {
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let version: String = tx
        .query_row(
            "SELECT value FROM metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| StoreError::SchemaMismatch)?;
    if version == LOCAL_STORE_SCHEMA_VERSION {
        tx.commit()?;
        return Ok(());
    }
    if matches!(version.as_str(), "local_state.v2" | "local_state.v3") {
        tx.execute(
            "INSERT OR REPLACE INTO metadata(key, value) VALUES ('projection_dirty', '1')",
            [],
        )?;
    } else if version == "local_state.v1" {
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
            "INSERT OR REPLACE INTO metadata(key, value) VALUES ('projection_dirty', '1')",
            [],
        )?;
    } else {
        return Err(StoreError::SchemaMismatch);
    }
    prune_adapter_dispositions(&tx)?;
    tx.execute_batch(
        "CREATE TABLE observations_v4 (event_id TEXT PRIMARY KEY, source TEXT NOT NULL, generation TEXT NOT NULL, observation_id TEXT NOT NULL, trace_id TEXT NOT NULL, observed_at_unix_ms TEXT NOT NULL, payload_hash TEXT NOT NULL, projected_json TEXT NOT NULL, UNIQUE(source, generation, observation_id))",
    )?;
    {
        let mut statement = tx.prepare(
            "SELECT event_id, source, generation, observation_id, payload_hash, projected_json FROM observations",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        for row in rows {
            let (event_id, source, generation, observation_id, payload_hash, projected_json) = row?;
            let record: DurableRecordV1 = serde_json::from_str(&projected_json)?;
            tx.execute(
                "INSERT INTO observations_v4(event_id, source, generation, observation_id, trace_id, observed_at_unix_ms, payload_hash, projected_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![event_id, source, generation, observation_id, record.trace_id, record_observed_at_millis(&record)?, payload_hash, projected_json],
            )?;
        }
    }
    tx.execute_batch(
        "DROP TABLE observations; ALTER TABLE observations_v4 RENAME TO observations",
    )?;
    create_v4_retention_objects(&tx)?;
    tx.execute(
        "UPDATE metadata SET value=?1 WHERE key='schema_version'",
        [LOCAL_STORE_SCHEMA_VERSION],
    )?;
    tx.commit()?;
    Ok(())
}

fn create_v4_retention_objects(tx: &Transaction<'_>) -> Result<(), StoreError> {
    for name in [
        "expired_span_states",
        "retention_receipts",
        "observations_trace_idx",
        "records_trace_idx",
        "topology_trace_idx",
    ] {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name=?1)",
            [name],
            |row| row.get(0),
        )?;
        if exists {
            continue;
        }
        let sql = SCHEMA_OBJECTS
            .iter()
            .find(|(_, object, _)| *object == name)
            .map(|(_, _, sql)| *sql)
            .ok_or(StoreError::SchemaMismatch)?;
        tx.execute_batch(sql)?;
    }
    Ok(())
}

fn validate_schema(db: &Connection) -> Result<(), StoreError> {
    let auto_vacuum: i64 = db.query_row("PRAGMA auto_vacuum", [], |row| row.get(0))?;
    if auto_vacuum != 2 {
        return Err(StoreError::SchemaMismatch);
    }
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
    validate_table_columns(db)?;
    let foreign_key_failure = db
        .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
        .optional()?;
    if foreign_key_failure.is_some() {
        return Err(StoreError::SchemaMismatch);
    }
    Ok(())
}

fn validate_table_columns(db: &Connection) -> Result<(), StoreError> {
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
                "trace_id",
                "observed_at_unix_ms",
                "payload_hash",
                "projected_json",
            ],
        ),
        (
            "source_inputs",
            &["source", "generation", "cursor", "event_id", "payload_hash"],
        ),
        (
            "expired_span_states",
            &["guard_seq", "span_id", "canonical_state_hash"],
        ),
        (
            "retention_receipts",
            &[
                "plan_id",
                "cutoff_unix_ms",
                "traces",
                "observations",
                "records",
                "archive_bytes",
                "truncated",
                "archive_path_hash",
                "archive_sha256",
                "compacted",
            ],
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
    Ok(())
}

fn normalize_schema_sql(sql: &str) -> String {
    sql.chars()
        .filter(|character| !character.is_ascii_whitespace() && *character != '"')
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

    fn correlation_snapshot(request_id: &str) -> String {
        serde_json::json!({
            "schema_version": "codex_request_correlation.v1",
            "next_sequence": 2,
            "pending": [{
                "source_generation_hash": hash_opaque_identifier("generation"),
                "conversation_hash": hash_opaque_identifier("conversation"),
                "model_hash": hash_opaque_identifier("model"),
                "correlation_id": hash_opaque_identifier(request_id),
                "official_retry_identity": hash_opaque_identifier(request_id),
                "inserted_at_unix_ms": 100,
                "sequence": 0
            }],
            "recently_completed": [{
                "source_generation_hash": hash_opaque_identifier("generation"),
                "conversation_hash": hash_opaque_identifier("conversation"),
                "model_hash": hash_opaque_identifier("model"),
                "official_retry_identity": hash_opaque_identifier(&format!("{request_id}-completed")),
                "completed_at_unix_ms": 101,
                "sequence": 1
            }]
        })
        .to_string()
    }

    fn completed_correlation_snapshot(request_id: &str) -> String {
        serde_json::json!({
            "schema_version": "codex_request_correlation.v1",
            "next_sequence": 1,
            "pending": [],
            "recently_completed": [{
                "source_generation_hash": hash_opaque_identifier("generation"),
                "conversation_hash": hash_opaque_identifier("conversation"),
                "model_hash": hash_opaque_identifier("model"),
                "official_retry_identity": hash_opaque_identifier(request_id),
                "completed_at_unix_ms": 100,
                "sequence": 0
            }]
        })
        .to_string()
    }

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agent-observability-local-store-{label}-{}",
            std::process::id()
        ))
    }

    fn downgrade_to_historical_schema(database: &Path, version: &str) {
        let connection = Connection::open(database).unwrap();
        connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        connection
            .execute_batch(
                "DROP INDEX observations_trace_idx;
                 DROP INDEX records_trace_idx;
                 DROP INDEX topology_trace_idx;
                 DROP TABLE expired_span_states;
                 DROP TABLE retention_receipts;
                 CREATE TABLE observations_historical (event_id TEXT PRIMARY KEY, source TEXT NOT NULL, generation TEXT NOT NULL, observation_id TEXT NOT NULL, payload_hash TEXT NOT NULL, projected_json TEXT NOT NULL, UNIQUE(source, generation, observation_id));
                 INSERT INTO observations_historical(event_id, source, generation, observation_id, payload_hash, projected_json) SELECT event_id, source, generation, observation_id, payload_hash, projected_json FROM observations;
                 DROP TABLE observations;
                 ALTER TABLE observations_historical RENAME TO observations;",
            )
            .unwrap();
        match version {
            "local_state.v1" => connection
                .execute_batch(
                    "DROP TABLE adapter_dispositions;
                     DELETE FROM metadata WHERE key='projection_dirty';",
                )
                .unwrap(),
            "local_state.v2" => {
                connection
                    .execute("DELETE FROM metadata WHERE key='projection_dirty'", [])
                    .unwrap();
            }
            "local_state.v3" => {}
            _ => panic!("unsupported historical schema fixture"),
        }
        connection
            .execute(
                "UPDATE metadata SET value=?1 WHERE key='schema_version'",
                [version],
            )
            .unwrap();
        connection
            .pragma_update(None, "auto_vacuum", "NONE")
            .unwrap();
        connection.execute_batch("VACUUM").unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA auto_vacuum", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
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
    fn clean_reopen_does_not_rewrite_projection() {
        let dir = temp_dir("clean-reopen");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store.ingest(&observation("1", "session", None)).unwrap();
        let projection = store.projection_path();
        let before = fs::metadata(&projection).unwrap().modified().unwrap();
        assert!(!store.repair_projection_if_needed().unwrap());
        drop(store);
        let reopened = LocalStore::open(&dir).unwrap();
        assert_eq!(
            fs::metadata(reopened.projection_path())
                .unwrap()
                .modified()
                .unwrap(),
            before
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn current_open_requires_existing_schema_without_repairing_projection() {
        let missing = temp_dir("current-open-missing");
        let _ = fs::remove_dir_all(&missing);
        assert!(matches!(
            LocalStore::open_current(&missing),
            Err(StoreError::Io(ref error)) if error.kind() == io::ErrorKind::NotFound
        ));
        assert!(!missing.exists());

        let dir = temp_dir("current-open-no-repair");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store.ingest(&observation("1", "session", None)).unwrap();
        let projection = store.projection_path();
        fs::remove_file(&projection).unwrap();
        drop(store);

        let reopened = LocalStore::open_current(&dir).unwrap();
        assert!(reopened.report_status().unwrap().pending());
        assert!(!projection.exists());
        assert!(reopened.repair_projection_if_needed().unwrap());
        assert!(projection.is_file());
        let _ = fs::remove_dir_all(&dir);

        let lightweight = temp_dir("current-open-skips-full-schema-audit");
        let _ = fs::remove_dir_all(&lightweight);
        let store = LocalStore::open(&lightweight).unwrap();
        let database = store.database_path();
        drop(store);
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch("CREATE TABLE report_consumer_probe(value INTEGER);")
            .unwrap();
        drop(connection);
        let reopened = LocalStore::open_current(&lightweight).unwrap();
        assert!(!reopened.report_status().unwrap().pending());
        drop(reopened);
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch("DROP TABLE report_consumer_probe;")
            .unwrap();
        let _ = fs::remove_dir_all(&lightweight);

        let legacy = temp_dir("current-open-legacy");
        let _ = fs::remove_dir_all(&legacy);
        let store = LocalStore::open(&legacy).unwrap();
        let database = store.database_path();
        drop(store);
        downgrade_to_historical_schema(&database, "local_state.v3");
        assert!(matches!(
            LocalStore::open_current(&legacy),
            Err(StoreError::SchemaMismatch)
        ));
        let connection = Connection::open(database).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM metadata WHERE key='schema_version'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "local_state.v3"
        );
        let _ = fs::remove_dir_all(&legacy);
    }

    #[test]
    fn admitted_open_can_defer_non_authoritative_projection_repair() {
        let dir = temp_dir("admitted-open-deferred-projection");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store
            .ingest_ordered_batch_deferred_projection(&[StoreBatchItem::Observation(&observation(
                "1", "session", None,
            ))])
            .unwrap();
        let projection = store.projection_path();
        fs::remove_file(&projection).unwrap();
        let database_bytes = fs::metadata(store.database_path()).unwrap().len();
        drop(store);

        let reopened = LocalStore::open_with_migration_headroom_deferred_projection(
            &dir,
            database_bytes.saturating_mul(2),
        )
        .unwrap();
        assert_eq!(reopened.record_count().unwrap(), 1);
        assert!(reopened.report_status().unwrap().pending());
        assert!(!projection.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn deferred_projection_is_repaired_from_dirty_state_on_reopen() {
        let dir = temp_dir("deferred-dirty");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store
            .ingest_deferred_projection(&observation("1", "session", None))
            .unwrap();
        let database = store.database_path();
        assert_eq!(fs::read_to_string(store.projection_path()).unwrap(), "");
        drop(store);
        let connection = Connection::open(database).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM metadata WHERE key='projection_dirty'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "1"
        );
        drop(connection);
        let reopened = LocalStore::open(&dir).unwrap();
        assert_eq!(
            fs::read_to_string(reopened.projection_path())
                .unwrap()
                .lines()
                .count(),
            1
        );
        let connection = Connection::open(reopened.database_path()).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM metadata WHERE key='projection_dirty'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "0"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn deferred_batch_commits_ordered_cursors_atomically() {
        let dir = temp_dir("deferred-batch");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let batch = [
            observation("1", "session", None),
            observation_after("2", Some("1"), "turn", Some("session")),
        ];
        assert_eq!(
            store.ingest_batch_deferred_projection(&batch).unwrap(),
            [IngestStatus::Committed, IngestStatus::Committed]
        );
        assert_eq!(store.observation_count().unwrap(), 2);
        assert_eq!(
            store.cursor("codex", "generation").unwrap().as_deref(),
            Some("2")
        );
        assert_eq!(fs::read_to_string(store.projection_path()).unwrap(), "");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn deferred_batch_rolls_back_cursor_and_records_on_failure() {
        let dir = temp_dir("deferred-batch-rollback");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let batch = [
            observation("1", "session", None),
            observation_after("2", None, "turn", Some("session")),
        ];
        assert!(matches!(
            store.ingest_batch_deferred_projection(&batch),
            Err(StoreError::CursorConflict)
        ));
        assert_eq!(store.observation_count().unwrap(), 0);
        assert_eq!(store.source_input_count().unwrap(), 0);
        assert_eq!(store.cursor("codex", "generation").unwrap(), None);
        let repaired = [
            observation("1", "session", None),
            observation_after("2", Some("1"), "turn", Some("session")),
        ];
        assert!(store.ingest_batch_deferred_projection(&repaired).is_ok());
        assert_eq!(store.observation_count().unwrap(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ordered_mixed_batch_commits_and_replays_in_one_cursor_namespace() {
        let dir = temp_dir("ordered-mixed-batch");
        let _ = fs::remove_dir_all(&dir);
        let first = observation("1", "session", None);
        let disposition = SourceCheckpoint {
            source: AgentSource::Codex,
            source_generation: SourceGeneration::parse("generation").unwrap(),
            previous_source_cursor: Some(SourceCursor::parse("1").unwrap()),
            source_cursor: SourceCursor::parse("2").unwrap(),
        };
        let last = observation_after("3", Some("2"), "turn", Some("session"));
        let items = [
            StoreBatchItem::Observation(&first),
            StoreBatchItem::Disposition {
                checkpoint: &disposition,
                disposition: AdapterDispositionKind::Diagnostic,
                code: AdapterDispositionCode::ContentEventIgnored,
                canonical_payload_hash: None,
            },
            StoreBatchItem::Observation(&last),
        ];
        let mut store = LocalStore::open(&dir).unwrap();

        assert_eq!(
            store
                .ingest_ordered_batch_deferred_projection(&items)
                .unwrap(),
            [
                IngestStatus::Committed,
                IngestStatus::Committed,
                IngestStatus::Committed
            ]
        );
        assert_eq!(store.observation_count().unwrap(), 2);
        assert_eq!(store.disposition_count().unwrap(), 1);
        assert_eq!(
            store.cursor("codex", "generation").unwrap().as_deref(),
            Some("3")
        );
        assert_eq!(
            store
                .ingest_ordered_batch_deferred_projection(&items)
                .unwrap(),
            [
                IngestStatus::Duplicate,
                IngestStatus::Duplicate,
                IngestStatus::Duplicate
            ]
        );
        assert_eq!(store.observation_count().unwrap(), 2);
        assert_eq!(store.disposition_count().unwrap(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ordered_mixed_batch_rolls_back_dispositions_and_observations() {
        let dir = temp_dir("ordered-mixed-rollback");
        let _ = fs::remove_dir_all(&dir);
        let first = observation("1", "session", None);
        let disposition = SourceCheckpoint {
            source: AgentSource::Codex,
            source_generation: SourceGeneration::parse("generation").unwrap(),
            previous_source_cursor: Some(SourceCursor::parse("1").unwrap()),
            source_cursor: SourceCursor::parse("2").unwrap(),
        };
        let items = [
            StoreBatchItem::Observation(&first),
            StoreBatchItem::Disposition {
                checkpoint: &disposition,
                disposition: AdapterDispositionKind::Diagnostic,
                code: AdapterDispositionCode::ContentEventIgnored,
                canonical_payload_hash: Some("invalid"),
            },
        ];
        let mut store = LocalStore::open(&dir).unwrap();

        assert!(matches!(
            store.ingest_ordered_batch_deferred_projection(&items),
            Err(StoreError::InvalidObservation)
        ));
        assert_eq!(store.observation_count().unwrap(), 0);
        assert_eq!(store.disposition_count().unwrap(), 0);
        assert_eq!(store.cursor("codex", "generation").unwrap(), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn deferred_batch_crash_boundaries_reopen_without_partial_progress() {
        for (label, point, expected) in [
            ("batch-before-commit", CrashPoint::BeforeCommit, 0),
            ("batch-after-commit", CrashPoint::AfterCommit, 2),
        ] {
            let dir = temp_dir(label);
            let _ = fs::remove_dir_all(&dir);
            let batch = [
                observation("1", "session", None),
                observation_after("2", Some("1"), "turn", Some("session")),
            ];
            let items = batch
                .iter()
                .map(StoreBatchItem::Observation)
                .collect::<Vec<_>>();
            let mut store = LocalStore::open(&dir).unwrap();
            assert!(matches!(
                store.ingest_ordered_batch_at(&items, Some(point)),
                Err(StoreError::Crash(actual)) if actual == point
            ));
            drop(store);
            let mut reopened = LocalStore::open(&dir).unwrap();
            assert_eq!(reopened.observation_count().unwrap(), expected);
            if expected == 0 {
                assert!(reopened.ingest_batch_deferred_projection(&batch).is_ok());
            } else {
                assert_eq!(
                    reopened.ingest_batch_deferred_projection(&batch).unwrap(),
                    [IngestStatus::Duplicate, IngestStatus::Duplicate]
                );
            }
            assert_eq!(reopened.observation_count().unwrap(), 2);
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn correlation_snapshot_and_cursor_share_crash_atomicity() {
        for (label, point, committed) in [
            ("correlation-before-commit", CrashPoint::BeforeCommit, false),
            ("correlation-after-commit", CrashPoint::AfterCommit, true),
        ] {
            let dir = temp_dir(label);
            let _ = fs::remove_dir_all(&dir);
            let item = observation("1", "session", None);
            let items = [StoreBatchItem::Observation(&item)];
            let snapshot = correlation_snapshot("request-private");
            let mut store = LocalStore::open(&dir).unwrap();
            assert!(matches!(
                store.ingest_ordered_batch_with_correlation_state_at(
                    &items,
                    "generation",
                    &snapshot,
                    Some(point),
                ),
                Err(StoreError::Crash(actual)) if actual == point
            ));
            drop(store);

            let reopened = LocalStore::open(&dir).unwrap();
            assert_eq!(
                reopened.cursor("codex", "generation").unwrap().as_deref(),
                committed.then_some("1")
            );
            assert_eq!(
                reopened
                    .codex_request_correlation_state("generation")
                    .unwrap()
                    .as_deref(),
                committed.then_some(snapshot.as_str())
            );
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn correlation_snapshot_rejects_raw_or_cross_generation_identifiers() {
        let dir = temp_dir("correlation-privacy-validation");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let item = observation("1", "session", None);
        let items = [StoreBatchItem::Observation(&item)];
        let raw = correlation_snapshot("request-private").replace(
            &hash_opaque_identifier("conversation"),
            "private@example.com",
        );
        assert!(matches!(
            store.ingest_codex_batch_with_correlation_state_deferred_projection(
                &items,
                "generation",
                &raw,
            ),
            Err(StoreError::InvalidObservation)
        ));
        let raw_official_identity = correlation_snapshot("request-private").replace(
            &hash_opaque_identifier("request-private"),
            "raw-official-request-id",
        );
        assert!(matches!(
            store.ingest_codex_batch_with_correlation_state_deferred_projection(
                &items,
                "generation",
                &raw_official_identity,
            ),
            Err(StoreError::InvalidObservation)
        ));
        let raw_completed_identity = completed_correlation_snapshot("request-private").replace(
            &hash_opaque_identifier("request-private"),
            "raw-completed-request-id",
        );
        assert!(matches!(
            store.ingest_codex_batch_with_correlation_state_deferred_projection(
                &items,
                "generation",
                &raw_completed_identity,
            ),
            Err(StoreError::InvalidObservation)
        ));
        assert_eq!(store.cursor("codex", "generation").unwrap(), None);
        assert_eq!(
            store
                .codex_request_correlation_state("other-generation")
                .unwrap(),
            None
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
        for round in 0..8 {
            let dir = temp_dir(&format!("concurrent-first-open-{round}"));
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
        assert_eq!(
            fs::metadata(dir.join(STORE_OPEN_LOCK_NAME))
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

        let lock_store = root.join("lock-store");
        fs::create_dir(&lock_store).unwrap();
        fs::set_permissions(&lock_store, fs::Permissions::from_mode(0o700)).unwrap();
        symlink(&target_file, lock_store.join(STORE_OPEN_LOCK_NAME)).unwrap();
        let lock_result = LocalStore::open(&lock_store);
        assert!(
            matches!(&lock_result, Err(StoreError::Symlink)),
            "{lock_result:?}"
        );

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
        downgrade_to_historical_schema(&database, "local_state.v1");
        fs::remove_file(&projection).unwrap();
        let required_workspace = fs::metadata(&database).unwrap().len().saturating_mul(2);

        assert!(matches!(
            LocalStore::open(&dir),
            Err(StoreError::MigrationAdmissionRequired)
        ));
        assert!(matches!(
            LocalStore::open_with_migration_headroom(&dir, required_workspace.saturating_sub(1)),
            Err(StoreError::MigrationAdmissionRequired)
        ));
        let mut store = LocalStore::open_with_migration_headroom(&dir, required_workspace).unwrap();
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
    fn v2_store_migrates_to_v4_and_repairs_projection() {
        let dir = temp_dir("v2-migration");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store.ingest(&observation("1", "session", None)).unwrap();
        let database = store.database_path();
        let projection = store.projection_path();
        drop(store);
        downgrade_to_historical_schema(&database, "local_state.v2");
        fs::remove_file(&projection).unwrap();

        let reopened = LocalStore::open_with_migration_headroom(&dir, u64::MAX).unwrap();
        assert_eq!(reopened.observation_count().unwrap(), 1);
        assert_eq!(fs::read_to_string(&projection).unwrap().lines().count(), 1);
        let version: String = reopened
            .db
            .query_row(
                "SELECT value FROM metadata WHERE key='schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, LOCAL_STORE_SCHEMA_VERSION);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn v3_store_migrates_to_v4_and_repairs_projection() {
        let dir = temp_dir("v3-migration");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store.ingest(&observation("1", "session", None)).unwrap();
        let database = store.database_path();
        let projection = store.projection_path();
        drop(store);
        downgrade_to_historical_schema(&database, "local_state.v3");
        let mut connection = Connection::open(&database).unwrap();
        let tx = connection.transaction().unwrap();
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO adapter_dispositions(source,generation,cursor,disposition,code,payload_hash) VALUES ('codex','generation',?1,'diagnostic','unsupported_event','hash')",
                )
                .unwrap();
            for index in 0..=MAX_ADAPTER_DISPOSITIONS {
                insert.execute([format!("migration-{index}")]).unwrap();
            }
        }
        tx.commit().unwrap();
        drop(connection);
        fs::remove_file(&projection).unwrap();

        let reopened = LocalStore::open_with_migration_headroom(&dir, u64::MAX).unwrap();
        let connection = Connection::open(reopened.database_path()).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM metadata WHERE key='schema_version'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "local_state.v4"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM metadata WHERE key='projection_dirty'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "0"
        );
        assert_eq!(fs::read_to_string(&projection).unwrap().lines().count(), 1);
        assert_eq!(
            count(&reopened.db, "adapter_dispositions").unwrap(),
            MAX_ADAPTER_DISPOSITIONS
        );
        assert!(
            !reopened
                .db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM adapter_dispositions WHERE cursor='migration-0')",
                    [],
                    |row| row.get::<_, bool>(0)
                )
                .unwrap()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_v3_data_migration_preserves_authority_and_can_retry() {
        let dir = temp_dir("v3-migration-retry");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store.ingest(&observation("1", "session", None)).unwrap();
        let database = store.database_path();
        let valid_record: String = store
            .db
            .query_row("SELECT projected_json FROM observations", [], |row| {
                row.get(0)
            })
            .unwrap();
        drop(store);
        downgrade_to_historical_schema(&database, "local_state.v3");

        let connection = Connection::open(&database).unwrap();
        connection
            .execute("UPDATE observations SET projected_json='{'", [])
            .unwrap();
        drop(connection);
        assert!(matches!(
            LocalStore::open_with_migration_headroom(&dir, u64::MAX),
            Err(StoreError::Json(_))
        ));

        let connection = Connection::open(&database).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM metadata WHERE key='schema_version'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "local_state.v3"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM observations", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
        assert!(
            !connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='retention_receipts')",
                    [],
                    |row| row.get::<_, bool>(0)
                )
                .unwrap()
        );
        connection
            .execute("UPDATE observations SET projected_json=?1", [valid_record])
            .unwrap();
        drop(connection);

        let reopened = LocalStore::open_with_migration_headroom(&dir, u64::MAX).unwrap();
        assert_eq!(reopened.observation_count().unwrap(), 1);
        assert_eq!(reopened.record_count().unwrap(), 1);
        assert_eq!(
            reopened
                .db
                .query_row("PRAGMA auto_vacuum", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            2
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    #[allow(clippy::too_many_lines)]
    fn retention_archives_complete_expired_traces_and_preserves_replay_guards() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("retention-apply");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let session = observation("1", "session", None);
        let turn = observation_after("2", Some("1"), "turn", Some("session"));
        store.ingest(&session).unwrap();
        store.ingest(&turn).unwrap();
        let before = fs::read(store.projection_path()).unwrap();
        let plan = store.retention_plan(100, 100, 1_048_576).unwrap();
        assert_eq!(plan.traces, 1);
        assert_eq!(plan.observations, 2);
        assert_eq!(plan.records, 2);
        assert!(!plan.truncated);
        assert_eq!(fs::read(store.projection_path()).unwrap(), before);

        let archive = dir.join("archives/expired.jsonl");
        assert!(matches!(
            store.apply_retention(100, 100, 1_048_576, "stale", &archive),
            Err(StoreError::StaleRetentionPlan)
        ));
        assert!(!archive.exists());
        let result = store
            .apply_retention(100, 100, 1_048_576, &plan.plan_id, &archive)
            .unwrap();
        assert_eq!(result.archive_path.as_deref(), Some(archive.as_path()));
        assert_eq!(
            fs::metadata(&archive).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let archive_body = fs::read_to_string(&archive).unwrap();
        let archive_lines = archive_body.lines().collect::<Vec<_>>();
        assert_eq!(archive_lines.len(), 4);
        assert!(archive_body.contains(RETENTION_ARCHIVE_VERSION));
        assert!(archive_body.contains("\"entry_type\":\"footer\""));
        assert!(archive_body.contains("\"records_sha256\""));
        assert!(!archive_body.contains("\"event_id\""));
        assert!(!archive_body.contains("\"generation\""));
        assert!(!archive_body.contains("\"payload_hash\""));
        let footer: serde_json::Value = serde_json::from_str(archive_lines[3]).unwrap();
        let mut digest = Sha256::new();
        for line in &archive_lines[1..3] {
            digest.update(line.as_bytes());
            digest.update(b"\n");
        }
        assert_eq!(
            footer["records_sha256"].as_str().unwrap(),
            hex_digest(digest.finalize())
        );
        assert_eq!(store.observation_count().unwrap(), 0);
        assert_eq!(store.source_input_count().unwrap(), 0);
        assert_eq!(store.outcome_count().unwrap(), 0);
        assert_eq!(store.record_count().unwrap(), 0);
        assert_eq!(
            store.cursor("codex", "generation").unwrap().as_deref(),
            Some("2")
        );
        assert!(
            fs::read_to_string(store.projection_path())
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            store.ingest(&turn),
            Err(StoreError::CursorConflict)
        ));
        let mut changed = turn.clone();
        changed.timing = Timing::new(1, Some(3)).unwrap();
        assert!(matches!(
            store.ingest(&changed),
            Err(StoreError::CursorConflict)
        ));
        let semantic_duplicate = observation_after("3", Some("2"), "turn", Some("session"));
        assert_eq!(
            store.ingest(&semantic_duplicate).unwrap(),
            IngestStatus::Suppressed
        );
        assert_eq!(store.record_count().unwrap(), 0);
        let mut conflicting_state = observation_after("4", Some("3"), "turn", Some("session"));
        conflicting_state.timing = Timing::new(1, Some(3)).unwrap();
        assert!(matches!(
            store.ingest(&conflicting_state),
            Err(StoreError::PayloadConflict)
        ));
        assert_eq!(
            store.cursor("codex", "generation").unwrap().as_deref(),
            Some("3")
        );
        let expired_spans: u64 = store
            .db
            .query_row("SELECT COUNT(*) FROM expired_span_states", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
            .cast_unsigned();
        assert_eq!(expired_spans, 2);
        let free_pages: u64 = store
            .db
            .query_row("PRAGMA freelist_count", [], |row| row.get::<_, i64>(0))
            .unwrap()
            .cast_unsigned();
        assert_eq!(free_pages, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn connection_opened_before_retention_cannot_resurrect_expired_span() {
        let dir = temp_dir("retention-two-connections");
        let _ = fs::remove_dir_all(&dir);
        let mut collector = LocalStore::open(&dir).unwrap();
        let mut retention = LocalStore::open(&dir).unwrap();
        retention
            .ingest(&observation("1", "session", None))
            .unwrap();
        retention
            .ingest(&observation_after("2", Some("1"), "turn", Some("session")))
            .unwrap();
        let plan = retention.retention_plan(100, 100, 1_048_576).unwrap();
        retention
            .apply_retention(
                100,
                100,
                1_048_576,
                &plan.plan_id,
                &dir.join("archive/expired.jsonl"),
            )
            .unwrap();

        let replay = observation_after("3", Some("2"), "turn", Some("session"));
        assert_eq!(collector.ingest(&replay).unwrap(), IngestStatus::Suppressed);
        assert_eq!(collector.record_count().unwrap(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn report_generation_tracks_record_mutations_and_exact_acknowledgement() {
        let dir = temp_dir("report-generation");
        let _ = fs::remove_dir_all(&dir);
        let mut writer = LocalStore::open(&dir).unwrap();
        assert_eq!(
            writer.report_status().unwrap(),
            ReportStatus {
                generation: 0,
                acknowledged_generation: 0,
            }
        );
        writer.ingest(&observation("1", "session", None)).unwrap();
        let snapshot = writer.report_snapshot().unwrap();
        assert_eq!(snapshot.generation, 1);
        let mut visited = Vec::new();
        let visit = writer
            .visit_report_snapshot(|index, record| visited.push((index, record)))
            .unwrap();
        assert_eq!(visit.generation, snapshot.generation);
        assert_eq!(visit.records, snapshot.records.len());
        assert_eq!(visited.len(), snapshot.records.len());
        assert_eq!(visited[0].0, 0);
        assert_eq!(visited[0].1, snapshot.records[0]);
        assert_eq!(snapshot.records.len(), 1);

        let mut concurrent_writer = LocalStore::open(&dir).unwrap();
        concurrent_writer
            .ingest(&observation_after("2", Some("1"), "turn", Some("session")))
            .unwrap();
        assert!(
            !writer
                .acknowledge_report_generation(snapshot.generation)
                .unwrap()
        );
        assert!(writer.report_status().unwrap().pending());

        let latest = writer.report_snapshot().unwrap();
        assert_eq!(latest.generation, 2);
        assert_eq!(latest.records.len(), 2);
        assert!(
            writer
                .acknowledge_report_generation(latest.generation)
                .unwrap()
        );
        assert!(!writer.report_status().unwrap().pending());

        let plan = writer.retention_plan(100, 100, 1_048_576).unwrap();
        writer
            .apply_retention(
                100,
                100,
                1_048_576,
                &plan.plan_id,
                &dir.join("archive/expired.jsonl"),
            )
            .unwrap();
        let status = writer.report_status().unwrap();
        assert_eq!(status.generation, 3);
        assert_eq!(status.acknowledged_generation, 2);
        assert!(status.pending());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn report_visitor_releases_read_lock_before_slow_projection() {
        let dir = temp_dir("report-visitor-concurrent-write");
        let _ = fs::remove_dir_all(&dir);
        let mut seed = LocalStore::open(&dir).unwrap();
        seed.ingest(&observation("1", "session", None)).unwrap();
        drop(seed);
        let reader = LocalStore::open_current(&dir).unwrap();
        let mut visited = Vec::new();
        let writer_dir = dir.clone();
        let (start_tx, start_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            start_rx.recv().unwrap();
            let mut writer = LocalStore::open(&writer_dir).unwrap();
            let result = writer.ingest(&observation_after("2", Some("1"), "turn", Some("session")));
            done_tx.send(result).unwrap();
        });

        let error = reader
            .visit_report_snapshot(|index, record| {
                visited.push((index, record));
                if index == 0 {
                    start_tx.send(()).unwrap();
                    done_rx
                        .recv_timeout(Duration::from_secs(2))
                        .expect("writer must commit while the visitor is active")
                        .unwrap();
                    std::thread::sleep(Duration::from_millis(100));
                }
            })
            .unwrap_err();
        writer.join().unwrap();

        assert!(matches!(error, StoreError::ReportSnapshotChanged));
        assert_eq!(visited.len(), 1);
        assert_eq!(reader.report_status().unwrap().generation, 2);
        assert!(reader.report_status().unwrap().pending());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn report_visitor_rejects_a_generation_change_between_batches() {
        for initial_records in [128_u64, 129, 256, 257] {
            let dir = temp_dir(&format!(
                "report-visitor-generation-fence-{initial_records}"
            ));
            let _ = fs::remove_dir_all(&dir);
            let mut writer = LocalStore::open(&dir).unwrap();
            writer.ingest(&observation("1", "session", None)).unwrap();
            for ordinal in 2..=initial_records {
                let cursor = ordinal.to_string();
                let previous = (ordinal - 1).to_string();
                writer
                    .ingest(&observation_after(
                        &cursor,
                        Some(&previous),
                        &format!("turn-{ordinal}"),
                        Some("session"),
                    ))
                    .unwrap();
            }
            let reader = LocalStore::open_current(&dir).unwrap();
            let next = initial_records + 1;
            let final_index = usize::try_from(initial_records).unwrap() - 1;

            let error = reader
                .visit_report_snapshot(|index, _record| {
                    if index == final_index {
                        writer
                            .ingest(&observation_after(
                                &next.to_string(),
                                Some(&initial_records.to_string()),
                                &format!("turn-{next}"),
                                Some("session"),
                            ))
                            .unwrap();
                    }
                })
                .unwrap_err();

            assert!(matches!(error, StoreError::ReportSnapshotChanged));
            assert_eq!(reader.report_status().unwrap().generation, next);
            let _ = fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn report_visits_and_sustained_writes_do_not_hold_sqlite_locks() {
        let dir = temp_dir("report-visitor-sustained-writes");
        let _ = fs::remove_dir_all(&dir);
        let mut seed = LocalStore::open(&dir).unwrap();
        seed.ingest(&observation("1", "session", None)).unwrap();
        drop(seed);

        let writer_dir = dir.clone();
        let start = std::sync::Arc::new(std::sync::Barrier::new(2));
        let writer_start = std::sync::Arc::clone(&start);
        let committed_writes = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let writer_committed_writes = std::sync::Arc::clone(&committed_writes);
        let writer = std::thread::spawn(move || {
            writer_start.wait();
            let mut writer = LocalStore::open(&writer_dir).unwrap();
            let mut max_write = Duration::ZERO;
            for ordinal in 2..=257 {
                let cursor = ordinal.to_string();
                let previous = (ordinal - 1).to_string();
                let write_started = std::time::Instant::now();
                writer
                    .ingest(&observation_after(
                        &cursor,
                        Some(&previous),
                        &format!("turn-{ordinal}"),
                        Some("session"),
                    ))
                    .unwrap();
                max_write = max_write.max(write_started.elapsed());
                writer_committed_writes.fetch_add(1, std::sync::atomic::Ordering::Release);
                std::thread::sleep(Duration::from_micros(250));
            }
            max_write
        });

        let reader = LocalStore::open_current(&dir).unwrap();
        start.wait();
        let mut overlapping_visits = 0_u64;
        let mut writes_during_visits = 0_u64;
        while !writer.is_finished() {
            overlapping_visits += 1;
            let writes_before = committed_writes.load(std::sync::atomic::Ordering::Acquire);
            match reader.visit_report_snapshot(|_, _| {
                std::thread::sleep(Duration::from_micros(100));
            }) {
                Ok(_) | Err(StoreError::ReportSnapshotChanged) => {}
                Err(error) => panic!("report visitor failed during sustained writes: {error}"),
            }
            let writes_after = committed_writes.load(std::sync::atomic::Ordering::Acquire);
            writes_during_visits =
                writes_during_visits.saturating_add(writes_after.saturating_sub(writes_before));
        }
        let max_write = writer.join().unwrap();
        assert!(overlapping_visits > 0);
        assert!(
            writes_during_visits >= 16,
            "only {writes_during_visits} writes completed during report visits"
        );
        assert!(
            max_write < Duration::from_secs(2),
            "one writer approached the SQLite busy timeout: {max_write:?}"
        );

        let mut visited = 0;
        let final_visit = reader
            .visit_report_snapshot(|index, _| {
                assert_eq!(index, visited);
                visited += 1;
            })
            .unwrap();
        assert_eq!(final_visit.records, 257);
        assert_eq!(visited, 257);
        assert_eq!(final_visit.generation, 257);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn existing_v4_store_without_report_metadata_reopens_pending() {
        let dir = temp_dir("report-generation-v4-backfill");
        let _ = fs::remove_dir_all(&dir);
        let store = LocalStore::open(&dir).unwrap();
        let database = store.database_path();
        drop(store);
        let connection = Connection::open(database).unwrap();
        connection
            .execute(
                "DELETE FROM metadata WHERE key IN (?1, ?2)",
                params![REPORT_GENERATION_KEY, REPORT_ACKNOWLEDGED_GENERATION_KEY],
            )
            .unwrap();
        drop(connection);

        let reopened = LocalStore::open(&dir).unwrap();
        assert_eq!(
            reopened.report_status().unwrap(),
            ReportStatus {
                generation: 1,
                acknowledged_generation: 0,
            }
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_keeps_mixed_age_traces_and_reports_bounded_truncation() {
        let dir = temp_dir("retention-bounds");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let old = observation("1", "session", None);
        let mut recent = observation_after("2", Some("1"), "turn", Some("session"));
        recent.timing = Timing::new(200, Some(201)).unwrap();
        store.ingest(&old).unwrap();
        store.ingest(&recent).unwrap();
        assert_eq!(store.retention_plan(100, 100, 1_048_576).unwrap().traces, 0);

        let mut expired = observation_after("3", Some("2"), "other", None);
        expired.trace_id = TraceId::parse("other-trace").unwrap();
        store.ingest(&expired).unwrap();
        assert!(matches!(
            store.retention_plan(100, 1, 1),
            Err(StoreError::InvalidRetentionBounds)
        ));
        let archive = dir.join("archive/too-small.jsonl");
        assert!(matches!(
            store.apply_retention(100, 1, 1, "invalid", &archive),
            Err(StoreError::InvalidRetentionBounds)
        ));
        assert!(!archive.exists());
        assert_eq!(store.record_count().unwrap(), 3);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_bounds_are_inclusive_and_never_split_a_trace() {
        let dir = temp_dir("retention-exact-bounds");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store.ingest(&observation("1", "session", None)).unwrap();
        store
            .ingest(&observation_after("2", Some("1"), "turn", Some("session")))
            .unwrap();
        let generous = store.retention_plan(100, 100, 1_048_576).unwrap();
        assert_eq!(generous.records, 2);
        let exact = store.retention_plan(100, 2, MIN_ARCHIVE_BYTES).unwrap();
        assert_eq!(exact.traces, 1);
        assert_eq!(exact.records, 2);
        assert_eq!(exact.archive_bytes, generous.archive_bytes);
        assert_eq!(store.retention_plan(100, 1, 1_048_576).unwrap().traces, 0);
        assert!(matches!(
            store.retention_plan(100, 2, MIN_ARCHIVE_BYTES - 1),
            Err(StoreError::InvalidRetentionBounds)
        ));
        assert_eq!(store.record_count().unwrap(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_limits_are_enforced_at_the_store_boundary() {
        let dir = temp_dir("retention-limit-boundary");
        let _ = fs::remove_dir_all(&dir);
        let store = LocalStore::open(&dir).unwrap();
        for (records, bytes) in [
            (MIN_ARCHIVE_RECORDS, MIN_ARCHIVE_BYTES),
            (MAX_ARCHIVE_RECORDS, MAX_ARCHIVE_BYTES),
        ] {
            assert_eq!(store.retention_plan(0, records, bytes).unwrap().traces, 0);
        }
        for (records, bytes) in [
            (0, MIN_ARCHIVE_BYTES),
            (MAX_ARCHIVE_RECORDS + 1, MIN_ARCHIVE_BYTES),
            (MIN_ARCHIVE_RECORDS, MIN_ARCHIVE_BYTES - 1),
            (MIN_ARCHIVE_RECORDS, MAX_ARCHIVE_BYTES + 1),
        ] {
            assert!(matches!(
                store.retention_plan(u64::MAX, records, bytes),
                Err(StoreError::InvalidRetentionBounds)
            ));
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pending_retention_receipt_blocks_a_new_pass_until_recovered() {
        let dir = temp_dir("retention-pending-receipt");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store.ingest(&observation("1", "session", None)).unwrap();
        let first = store.retention_plan(100, 100, 1_048_576).unwrap();
        let first_archive = dir.join("archive/first.jsonl");
        assert!(matches!(
            store.apply_retention_at(
                100,
                100,
                1_048_576,
                &first.plan_id,
                &first_archive,
                Some(CrashPoint::AfterRetentionCommit)
            ),
            Err(StoreError::Crash(CrashPoint::AfterRetentionCommit))
        ));

        let mut second = observation_after("2", Some("1"), "other", None);
        second.trace_id = TraceId::parse("other-trace").unwrap();
        store.ingest(&second).unwrap();
        let second_plan = store.retention_plan(100, 100, 1_048_576).unwrap();
        let second_archive = dir.join("archive/second.jsonl");
        assert!(matches!(
            store.apply_retention(100, 100, 1_048_576, &second_plan.plan_id, &second_archive),
            Err(StoreError::PendingRetentionRecovery)
        ));
        store
            .apply_retention(100, 100, 1_048_576, &first.plan_id, &first_archive)
            .unwrap();
        store
            .apply_retention(100, 100, 1_048_576, &second_plan.plan_id, &second_archive)
            .unwrap();
        assert_eq!(store.record_count().unwrap(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_retention_callers_publish_one_archive_and_one_receipt() {
        let dir = temp_dir("retention-concurrent-callers");
        let _ = fs::remove_dir_all(&dir);
        let mut planner = LocalStore::open(&dir).unwrap();
        planner.ingest(&observation("1", "session", None)).unwrap();
        let plan = planner.retention_plan(100, 100, 1_048_576).unwrap();
        drop(planner);

        let first = LocalStore::open(&dir).unwrap();
        let second = LocalStore::open(&dir).unwrap();
        let archive = dir.join("archive/shared.jsonl");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let mut handles = Vec::new();
        for store in [first, second] {
            let plan_id = plan.plan_id.clone();
            let archive = archive.clone();
            let barrier = std::sync::Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                store.apply_retention(100, 100, 1_048_576, &plan_id, &archive)
            }));
        }
        for handle in handles {
            let result = handle.join().unwrap().unwrap();
            assert_eq!(result.plan, plan);
            assert_eq!(result.archive_path.as_deref(), Some(archive.as_path()));
        }

        let reopened = LocalStore::open(&dir).unwrap();
        assert_eq!(reopened.record_count().unwrap(), 0);
        assert_eq!(count(&reopened.db, "retention_receipts").unwrap(), 1);
        assert!(archive.is_file());
        let archive_name = archive.file_name().unwrap().to_str().unwrap();
        let temp_prefix = format!(".{archive_name}.retention.tmp.");
        assert_eq!(
            fs::read_dir(archive.parent().unwrap())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&temp_prefix))
                .count(),
            0
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_stops_before_an_oversized_oldest_trace_without_splitting_it() {
        let dir = temp_dir("retention-oversized-trace");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        for index in 1..=11_u8 {
            let cursor = index.to_string();
            let previous = (index > 1).then(|| (index - 1).to_string());
            let mut running = observation_after(
                &cursor,
                previous.as_deref(),
                &format!("running-{index}"),
                None,
            );
            running.trace_id = TraceId::parse("a-running").unwrap();
            running.lifecycle = LifecycleState::Running;
            store.ingest(&running).unwrap();
        }
        let mut terminal = observation_after("12", Some("11"), "terminal", None);
        terminal.trace_id = TraceId::parse("z-terminal").unwrap();
        store.ingest(&terminal).unwrap();

        let plan = store.retention_plan(100, 1, 1_048_576).unwrap();
        assert_eq!(plan.traces, 0);
        assert_eq!(plan.records, 0);
        assert!(plan.truncated);
        assert_eq!(store.record_count().unwrap(), 12);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_rejects_a_truncated_plan_without_applying_its_selected_prefix() {
        let dir = temp_dir("retention-truncated-prefix");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let mut first = observation("1", "first", None);
        first.trace_id = TraceId::parse("first-trace").unwrap();
        first.timing = Timing::new(1, Some(2)).unwrap();
        store.ingest(&first).unwrap();
        let mut second = observation_after("2", Some("1"), "second", None);
        second.trace_id = TraceId::parse("second-trace").unwrap();
        second.timing = Timing::new(3, Some(4)).unwrap();
        store.ingest(&second).unwrap();

        let plan = store.retention_plan(100, 1, 1_048_576).unwrap();
        assert_eq!(plan.traces, 1);
        assert!(plan.truncated);
        let archive = dir.join("archive/truncated.jsonl");
        assert!(matches!(
            store.apply_retention(100, 1, 1_048_576, &plan.plan_id, &archive),
            Err(StoreError::RetentionBoundsTooSmall)
        ));
        assert_eq!(store.record_count().unwrap(), 2);
        assert_eq!(store.observation_count().unwrap(), 2);
        assert!(!archive.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_expires_stale_incomplete_traces_but_keeps_cutoff_equal_traces() {
        let dir = temp_dir("retention-pins");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        let mut incomplete = observation("1", "session", None);
        incomplete.lifecycle = LifecycleState::Running;
        incomplete.timing = Timing::new(1, None).unwrap();
        store.ingest(&incomplete).unwrap();
        assert_eq!(store.retention_plan(100, 100, 1_048_576).unwrap().traces, 1);

        let mut cutoff_equal = observation_after("2", Some("1"), "equal", None);
        cutoff_equal.trace_id = TraceId::parse("equal-trace").unwrap();
        cutoff_equal.timing = Timing::new(100, Some(100)).unwrap();
        store.ingest(&cutoff_equal).unwrap();

        let mut running_with_end = observation_after("3", Some("2"), "running", None);
        running_with_end.trace_id = TraceId::parse("running-trace").unwrap();
        running_with_end.lifecycle = LifecycleState::Running;
        store.ingest(&running_with_end).unwrap();
        assert_eq!(store.retention_plan(100, 100, 1_048_576).unwrap().traces, 2);
        assert_eq!(store.record_count().unwrap(), 3);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn retention_archive_publication_fails_closed_without_overwrite() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("retention-no-overwrite");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store.ingest(&observation("1", "session", None)).unwrap();
        let plan = store.retention_plan(100, 100, 1_048_576).unwrap();
        let archive_dir = dir.join("archive");
        fs::create_dir(&archive_dir).unwrap();
        fs::set_permissions(&archive_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let archive = archive_dir.join("existing.jsonl");
        fs::write(&archive, b"sentinel\n").unwrap();
        fs::set_permissions(&archive, fs::Permissions::from_mode(0o600)).unwrap();

        assert!(matches!(
            store.apply_retention(100, 100, 1_048_576, &plan.plan_id, &archive),
            Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists
        ));
        assert_eq!(fs::read(&archive).unwrap(), b"sentinel\n");
        assert_eq!(store.observation_count().unwrap(), 1);
        assert_eq!(store.record_count().unwrap(), 1);
        assert_eq!(
            store
                .db
                .query_row("SELECT COUNT(*) FROM retention_receipts", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_stores_cannot_clean_an_active_archive_temporary_file() {
        use std::sync::mpsc;

        let root = temp_dir("retention-destination-lock");
        let _ = fs::remove_dir_all(&root);
        let mut first = LocalStore::open(root.join("first-store")).unwrap();
        let mut second = LocalStore::open(root.join("second-store")).unwrap();
        first.ingest(&observation("1", "first", None)).unwrap();
        second.ingest(&observation("1", "second", None)).unwrap();
        let first_plan = first.retention_plan(100, 100, 1_048_576).unwrap();
        let second_plan = second.retention_plan(100, 100, 1_048_576).unwrap();
        let archive_dir = root.join("archive");
        private_dir(&archive_dir).unwrap();
        let archive_name = "shared.jsonl";
        let archive = archive_dir.join(archive_name);
        let active_temp = archive_dir.join(format!(
            ".{archive_name}.retention.tmp.{}.999",
            std::process::id()
        ));
        drop(private_create_new(&active_temp).unwrap());

        let (locked_tx, locked_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let lock_dir = archive_dir.clone();
        let holder = std::thread::spawn(move || {
            let _lock = lock_archive_directory(&lock_dir).unwrap();
            locked_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        locked_rx.recv().unwrap();

        assert!(matches!(
            second.apply_retention(
                100,
                100,
                1_048_576,
                &second_plan.plan_id,
                &archive
            ),
            Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock
        ));
        assert!(active_temp.is_file());
        assert_eq!(second.record_count().unwrap(), 1);

        release_tx.send(()).unwrap();
        holder.join().unwrap();
        first
            .apply_retention(100, 100, 1_048_576, &first_plan.plan_id, &archive)
            .unwrap();
        assert!(!active_temp.exists());
        assert_eq!(first.record_count().unwrap(), 0);
        second
            .apply_retention(
                100,
                100,
                1_048_576,
                &second_plan.plan_id,
                &archive_dir.join("other.jsonl"),
            )
            .unwrap();
        assert_eq!(second.record_count().unwrap(), 0);
        assert_eq!(
            fs::read_dir(&archive_dir)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name() == ".agent-observability.retention.lock")
                .count(),
            1
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    #[allow(clippy::too_many_lines)]
    fn retention_archive_paths_permissions_and_stale_temps_fail_closed() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let dir = temp_dir("retention-path-safety");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store.ingest(&observation("1", "session", None)).unwrap();
        let plan = store.retention_plan(100, 100, 1_048_576).unwrap();

        let broad_parent = dir.join("broad");
        fs::create_dir(&broad_parent).unwrap();
        fs::set_permissions(&broad_parent, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            store.apply_retention(
                100,
                100,
                1_048_576,
                &plan.plan_id,
                &broad_parent.join("archive.jsonl")
            ),
            Err(StoreError::InsecurePermissions)
        ));

        let private_parent = dir.join("private");
        fs::create_dir(&private_parent).unwrap();
        fs::set_permissions(&private_parent, fs::Permissions::from_mode(0o700)).unwrap();
        let sentinel = private_parent.join("sentinel");
        fs::write(&sentinel, b"sentinel\n").unwrap();
        fs::set_permissions(&sentinel, fs::Permissions::from_mode(0o600)).unwrap();
        let link = private_parent.join("link.jsonl");
        symlink(&sentinel, &link).unwrap();
        assert!(matches!(
            store.apply_retention(100, 100, 1_048_576, &plan.plan_id, &link),
            Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists
        ));
        assert_eq!(fs::read(&sentinel).unwrap(), b"sentinel\n");

        let directory_target = private_parent.join("directory.jsonl");
        fs::create_dir(&directory_target).unwrap();
        assert!(matches!(
            store.apply_retention(100, 100, 1_048_576, &plan.plan_id, &directory_target),
            Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists
        ));

        let wrong_parent = private_parent.join("not-a-directory");
        fs::write(&wrong_parent, b"sentinel\n").unwrap();
        fs::set_permissions(&wrong_parent, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(
            store.apply_retention(
                100,
                100,
                1_048_576,
                &plan.plan_id,
                &wrong_parent.join("archive.jsonl")
            ),
            Err(StoreError::InvalidPath)
        ));

        assert_eq!(store.observation_count().unwrap(), 1);
        assert_eq!(store.record_count().unwrap(), 1);
        assert_eq!(
            store
                .db
                .query_row("SELECT COUNT(*) FROM retention_receipts", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );

        let stale_sequence = AtomicU64::new(0);
        let stale = private_parent.join(format!(
            ".fresh.jsonl.retention.tmp.{}.0",
            std::process::id()
        ));
        fs::write(&stale, b"stale\n").unwrap();
        fs::set_permissions(&stale, fs::Permissions::from_mode(0o600)).unwrap();
        let (fresh, file) =
            create_archive_temp(&private_parent, "fresh.jsonl", &stale_sequence).unwrap();
        drop(file);
        assert!(fresh.ends_with(format!(
            ".fresh.jsonl.retention.tmp.{}.1",
            std::process::id()
        )));
        assert_eq!(fs::read(&stale).unwrap(), b"stale\n");
        fs::remove_file(fresh).unwrap();

        let exhausted_sequence = AtomicU64::new(0);
        let mut exhausted = Vec::new();
        for index in 0..MAX_ARCHIVE_TEMP_COLLISIONS {
            let path = private_parent.join(format!(
                ".exhausted.jsonl.retention.tmp.{}.{index}",
                std::process::id()
            ));
            fs::write(&path, b"stale\n").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            exhausted.push(path);
        }
        assert!(matches!(
            create_archive_temp(
                &private_parent,
                "exhausted.jsonl",
                &exhausted_sequence
            ),
            Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists
        ));
        for path in exhausted {
            assert_eq!(fs::read(&path).unwrap(), b"stale\n");
            fs::remove_file(path).unwrap();
        }

        let archive = private_parent.join("fresh.jsonl");
        store
            .apply_retention(100, 100, 1_048_576, &plan.plan_id, &archive)
            .unwrap();
        assert!(archive.is_file());
        assert!(!stale.exists());
        assert_eq!(store.observation_count().unwrap(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_pins_unresolved_topology_until_parent_arrives() {
        let dir = temp_dir("retention-unresolved");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store
            .ingest(&observation("1", "turn", Some("session")))
            .unwrap();
        assert_eq!(store.unresolved_parent_count().unwrap(), 1);
        assert_eq!(store.retention_plan(100, 100, 1_048_576).unwrap().traces, 0);
        store
            .ingest(&observation_after("2", Some("1"), "session", None))
            .unwrap();
        assert_eq!(store.unresolved_parent_count().unwrap(), 0);
        assert_eq!(store.retention_plan(100, 100, 1_048_576).unwrap().traces, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_rejects_a_real_plan_after_authority_changes() {
        let dir = temp_dir("retention-stale-authority");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store.ingest(&observation("1", "session", None)).unwrap();
        let plan = store.retention_plan(100, 100, 1_048_576).unwrap();
        store
            .ingest(&observation_after("2", Some("1"), "turn", Some("session")))
            .unwrap();
        let archive = dir.join("archive/stale.jsonl");
        assert!(matches!(
            store.apply_retention(100, 100, 1_048_576, &plan.plan_id, &archive),
            Err(StoreError::StaleRetentionPlan)
        ));
        assert!(!archive.exists());
        assert_eq!(store.observation_count().unwrap(), 2);
        assert_eq!(store.record_count().unwrap(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_crash_boundaries_leave_recoverable_authority_and_archive() {
        for crash in [
            CrashPoint::BeforeRetentionCommit,
            CrashPoint::AfterRetentionCommit,
        ] {
            let dir = temp_dir(&format!("retention-crash-{crash:?}"));
            let _ = fs::remove_dir_all(&dir);
            let mut store = LocalStore::open(&dir).unwrap();
            store.ingest(&observation("1", "session", None)).unwrap();
            let plan = store.retention_plan(100, 100, 1_048_576).unwrap();
            let archive = dir.join("archive/expired.jsonl");
            assert!(matches!(
                store.apply_retention_at(
                    100,
                    100,
                    1_048_576,
                    &plan.plan_id,
                    &archive,
                    Some(crash),
                ),
                Err(StoreError::Crash(point)) if point == crash
            ));
            assert!(archive.is_file());
            drop(store);

            let reopened = LocalStore::open(&dir).unwrap();
            let expected = u64::from(crash == CrashPoint::BeforeRetentionCommit);
            assert_eq!(reopened.observation_count().unwrap(), expected);
            assert_eq!(reopened.record_count().unwrap(), expected);
            assert_eq!(
                fs::read_to_string(reopened.projection_path())
                    .unwrap()
                    .lines()
                    .count(),
                usize::try_from(expected).unwrap()
            );
            let retry_archive = dir.join("archive/retry.jsonl");
            if crash == CrashPoint::BeforeRetentionCommit {
                assert!(matches!(
                    reopened.apply_retention(
                        100,
                        100,
                        1_048_576,
                        &plan.plan_id,
                        &archive,
                    ),
                    Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists
                ));
                assert_eq!(reopened.record_count().unwrap(), 1);
                reopened
                    .apply_retention(100, 100, 1_048_576, &plan.plan_id, &retry_archive)
                    .unwrap();
                assert_eq!(reopened.record_count().unwrap(), 0);
                assert!(retry_archive.is_file());
            } else {
                let retried = reopened
                    .apply_retention(100, 100, 1_048_576, &plan.plan_id, &archive)
                    .unwrap();
                assert_eq!(retried.plan, plan);
                assert_eq!(retried.archive_path.as_deref(), Some(archive.as_path()));
                assert!(!retry_archive.exists());
                let compacted: bool = reopened
                    .db
                    .query_row(
                        "SELECT compacted FROM retention_receipts WHERE plan_id=?1",
                        [&plan.plan_id],
                        |row| row.get(0),
                    )
                    .unwrap();
                assert!(compacted);
            }
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[cfg(unix)]
    #[test]
    fn retention_receipt_recovery_rejects_a_corrupted_archive() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("retention-corrupt-recovery");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store.ingest(&observation("1", "session", None)).unwrap();
        let plan = store.retention_plan(100, 100, 1_048_576).unwrap();
        let archive = dir.join("archive/expired.jsonl");
        assert!(matches!(
            store.apply_retention_at(
                100,
                100,
                1_048_576,
                &plan.plan_id,
                &archive,
                Some(CrashPoint::AfterRetentionCommit),
            ),
            Err(StoreError::Crash(CrashPoint::AfterRetentionCommit))
        ));
        let original = fs::read_to_string(&archive).unwrap();
        let mut lines = original.lines().map(str::to_owned).collect::<Vec<String>>();
        let mut record: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
        record["record"]["name"] = serde_json::Value::String("xession".into());
        lines[1] = serde_json::to_string(&record).unwrap();
        let mut record_hash = Sha256::new();
        record_hash.update(lines[1].as_bytes());
        record_hash.update(b"\n");
        let mut footer: serde_json::Value = serde_json::from_str(&lines[2]).unwrap();
        footer["records_sha256"] = serde_json::Value::String(hex_digest(record_hash.finalize()));
        lines[2] = serde_json::to_string(&footer).unwrap();
        let substituted = format!("{}\n", lines.join("\n"));
        assert_eq!(substituted.len(), original.len());
        fs::write(&archive, substituted).unwrap();
        fs::set_permissions(&archive, fs::Permissions::from_mode(0o600)).unwrap();
        drop(store);

        let reopened = LocalStore::open(&dir).unwrap();
        assert!(matches!(
            reopened.apply_retention(100, 100, 1_048_576, &plan.plan_id, &archive),
            Err(StoreError::SchemaMismatch)
        ));
        assert_eq!(reopened.record_count().unwrap(), 0);
        assert!(
            !reopened
                .db
                .query_row(
                    "SELECT compacted FROM retention_receipts WHERE plan_id=?1",
                    [&plan.plan_id],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_vacuum_reclaims_database_bytes() {
        let dir = temp_dir("retention-reclaims-bytes");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        for index in 0..256_u16 {
            let cursor = (u32::from(index) + 1).to_string();
            let previous = (index > 0).then(|| u32::from(index).to_string());
            let mut item =
                observation_after(&cursor, previous.as_deref(), &format!("span-{index}"), None);
            item.trace_id = TraceId::parse(format!("trace-{index}")).unwrap();
            store.ingest_deferred_projection(&item).unwrap();
        }
        store.rebuild_projection().unwrap();
        let before = fs::metadata(store.database_path()).unwrap().len();
        let plan = store.retention_plan(100, 1_000, 10_485_760).unwrap();
        assert_eq!(plan.traces, 256);
        let archive = dir.join("archive/all.jsonl");
        store
            .apply_retention(100, 1_000, 10_485_760, &plan.plan_id, &archive)
            .unwrap();
        let after = fs::metadata(store.database_path()).unwrap().len();
        assert!(after < before, "before={before}, after={after}");
        assert_eq!(store.record_count().unwrap(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn expired_span_replay_guards_keep_only_the_newest_bounded_horizon() {
        let dir = temp_dir("retention-guard-bound");
        let _ = fs::remove_dir_all(&dir);
        let store = LocalStore::open(&dir).unwrap();
        let tx = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate).unwrap();
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO expired_span_states(span_id, canonical_state_hash) VALUES (?1, ?2)",
                )
                .unwrap();
            for index in 0..=MAX_EXPIRED_SPAN_GUARDS {
                insert
                    .execute(params![format!("span-{index}"), format!("hash-{index}")])
                    .unwrap();
            }
        }
        prune_expired_span_guards(&tx).unwrap();
        tx.commit().unwrap();
        assert_eq!(
            count(&store.db, "expired_span_states").unwrap(),
            MAX_EXPIRED_SPAN_GUARDS
        );
        assert!(
            store
                .db
                .query_row(
                    "SELECT 1 FROM expired_span_states WHERE span_id='span-0'",
                    [],
                    |_| Ok(())
                )
                .optional()
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .db
                .query_row(
                    "SELECT 1 FROM expired_span_states WHERE span_id=?1",
                    [format!("span-{MAX_EXPIRED_SPAN_GUARDS}")],
                    |_| Ok(())
                )
                .optional()
                .unwrap()
                .is_some()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn disposition_and_completed_receipt_ledgers_keep_only_the_newest_bounds() {
        let dir = temp_dir("bounded-ledgers");
        let _ = fs::remove_dir_all(&dir);
        let store = LocalStore::open(&dir).unwrap();
        let tx = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate).unwrap();
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO adapter_dispositions(source,generation,cursor,disposition,code,payload_hash) VALUES ('codex','generation',?1,'diagnostic','unsupported_event','hash')",
                )
                .unwrap();
            for index in 0..=MAX_ADAPTER_DISPOSITIONS {
                insert.execute([format!("cursor-{index}")]).unwrap();
            }
        }
        prune_adapter_dispositions(&tx).unwrap();
        tx.commit().unwrap();
        assert_eq!(
            count(&store.db, "adapter_dispositions").unwrap(),
            MAX_ADAPTER_DISPOSITIONS
        );
        assert!(
            !store
                .db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM adapter_dispositions WHERE cursor='cursor-0')",
                    [],
                    |row| row.get::<_, bool>(0)
                )
                .unwrap()
        );

        let tx = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate).unwrap();
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO retention_receipts(plan_id,cutoff_unix_ms,traces,observations,records,archive_bytes,truncated,archive_path_hash,archive_sha256,compacted) VALUES (?1,'00000000000000000000',1,1,1,1,0,'path','archive',1)",
                )
                .unwrap();
            for index in 0..=MAX_RETENTION_RECEIPTS {
                insert.execute([format!("plan-{index}")]).unwrap();
            }
        }
        tx.commit().unwrap();
        store
            .finish_retention_receipt(&RetentionReceipt {
                plan: RetentionPlan {
                    plan_id: format!("plan-{MAX_RETENTION_RECEIPTS}"),
                    cutoff_unix_ms: 0,
                    traces: 1,
                    observations: 1,
                    records: 1,
                    archive_bytes: 1,
                    truncated: false,
                },
                archive_path: PathBuf::from("unused"),
                archive_sha256: "unused".into(),
                compacted: true,
            })
            .unwrap();
        assert_eq!(
            count(&store.db, "retention_receipts").unwrap(),
            MAX_RETENTION_RECEIPTS
        );
        assert!(
            !store
                .db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM retention_receipts WHERE plan_id='plan-0')",
                    [],
                    |row| row.get::<_, bool>(0)
                )
                .unwrap()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_projection_temp_cleanup_fails_closed_above_bound() {
        let dir = temp_dir("temp-bound");
        let _ = fs::remove_dir_all(&dir);
        let store = LocalStore::open(&dir).unwrap();
        let database = store.database_path();
        drop(store);
        for index in 0..=MAX_STALE_PROJECTION_TEMPS {
            private_create_new(&dir.join(format!(".{PROJECTION_NAME}.tmp.test.{index}"))).unwrap();
        }
        let connection = Connection::open(database).unwrap();
        connection
            .execute(
                "UPDATE metadata SET value='1' WHERE key='projection_dirty'",
                [],
            )
            .unwrap();
        drop(connection);
        assert!(matches!(
            LocalStore::open(&dir),
            Err(StoreError::SchemaMismatch)
        ));
        assert_eq!(
            fs::read_dir(&dir)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!(".{PROJECTION_NAME}.tmp.")))
                .count(),
            MAX_STALE_PROJECTION_TEMPS + 1
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_projection_temp_cleanup_bounds_total_directory_traversal() {
        let dir = temp_dir("temp-directory-bound");
        let _ = fs::remove_dir_all(&dir);
        let store = LocalStore::open(&dir).unwrap();
        drop(store);
        for index in 0..MAX_PRIVATE_DIRECTORY_ENTRIES {
            drop(private_create_new(&dir.join(format!("unrelated-{index}"))).unwrap());
        }
        assert!(matches!(
            remove_stale_projection_temps(&dir),
            Err(StoreError::SchemaMismatch)
        ));
        assert!(dir.join("unrelated-0").is_file());
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

        let mut reopened = LocalStore::open_with_migration_headroom(&dir, u64::MAX).unwrap();
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

    #[test]
    fn current_records_reads_an_ordered_typed_snapshot_from_authority() {
        let dir = temp_dir("current-records");
        let _ = fs::remove_dir_all(&dir);
        let mut store = LocalStore::open(&dir).unwrap();
        store.ingest(&observation("1", "session", None)).unwrap();
        store
            .ingest(&observation_after("2", Some("1"), "turn", Some("session")))
            .unwrap();

        let records = store.current_records().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].span_kind, SpanKind::AgentSession);
        assert_eq!(records[1].span_kind, SpanKind::Turn);
        assert_eq!(
            records[1].parent_span_id.as_deref(),
            Some(records[0].span_id.as_str())
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
