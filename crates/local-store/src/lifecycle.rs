use super::{
    LIFECYCLE_BACKFILL_COMPLETE, LIFECYCLE_BACKFILL_CURSOR_KEY, LIFECYCLE_SCAN_CURSOR_KEY,
    LocalStore, MAX_ARCHIVE_BYTES, MAX_ARCHIVE_RECORDS, MAX_EXPIRED_SPAN_GUARDS, MIN_ARCHIVE_BYTES,
    MIN_ARCHIVE_RECORDS, PROJECTION_NAME, StoreError, advance_report_generation,
    canonical_state_hash, hash_opaque_identifier, mark_projection_dirty, ordered_millis,
    private_file, prune_expired_span_guards, required_schema_text, state_from_json,
};
use agent_observability_contracts::{DurableRecordV1, sanitize_durable_record};
use agent_observability_domain::DomainSpanState;
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::time::Duration;

const DAY_MILLIS: u64 = 86_400_000;
const MAX_LIFECYCLE_DAYS: u16 = 3_650;
const MAX_TRACES_PER_PASS: u16 = 128;
const MAINTENANCE_BUSY_TIMEOUT: Duration = Duration::from_millis(100);
const NORMAL_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const ROW_OVERHEAD_BYTES: u64 = 512;
const PREFLIGHT_FIXED_BYTES: u64 = 2 * 1024 * 1024;
const MAX_INCREMENTAL_VACUUM_PAGES: u64 = 128;
const MAX_BACKFILL_ROWS_PER_PASS: u32 = 128;
const MAX_BACKFILL_BYTES_PER_PASS: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LifecycleRequest {
    pub now_unix_ms: u64,
    pub hot_days: u16,
    pub warm_days: u16,
    pub delete_after_days: u16,
    pub max_traces_per_pass: u16,
    pub max_archive_records: u32,
    pub max_archive_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LifecycleResult {
    pub scanned_traces: u16,
    pub moved_to_warm: u16,
    pub moved_to_cold: u16,
    pub deleted: u16,
    pub blocked: u16,
    pub deferred: u16,
    pub touched_records: u32,
    pub touched_bytes: u64,
    pub page_count_before: u64,
    pub page_count_after: u64,
    pub freelist_pages_before: u64,
    pub freelist_pages_after: u64,
    pub freelist_pages_reclaimed: u64,
    pub vacuum_pages_requested: u16,
    pub allocated_bytes_before: u64,
    pub allocated_bytes_after: u64,
    pub returned_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LifecyclePreflight {
    pub has_candidates: bool,
    pub has_backfill_pending: bool,
    pub maximum_touched_records: u32,
    pub maximum_touched_bytes: u64,
    pub logical_database_bytes: u64,
    pub required_temporary_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColdArchiveQuery {
    pub after_archive_seq: u64,
    pub max_traces: u16,
    pub max_records: u32,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColdArchiveTrace {
    pub archive_seq: u64,
    pub latest_observed_at_unix_ms: u64,
    pub archived_at_unix_ms: u64,
    pub archive_bytes: u64,
    pub records: Vec<DurableRecordV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColdArchivePage {
    pub traces: Vec<ColdArchiveTrace>,
    pub records: u32,
    pub bytes: u64,
    pub next_after_archive_seq: u64,
    pub has_more: bool,
    pub blocked: Option<ColdArchiveBlocked>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColdArchiveBlocked {
    pub archive_seq: u64,
    pub records: u32,
    pub bytes: u64,
}

impl ColdArchiveTrace {
    /// Serializes this validated cold trace as privacy-sanitized JSONL records.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when a record fails the durable privacy contract or serialization.
    pub fn records_jsonl(&self) -> Result<String, StoreError> {
        let mut output = Vec::new();
        for record in &self.records {
            let record = sanitize_durable_record(record).map_err(|_| StoreError::SchemaMismatch)?;
            serde_json::to_writer(&mut output, &record)?;
            output.write_all(b"\n")?;
        }
        String::from_utf8(output).map_err(|_| StoreError::SchemaMismatch)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tier {
    Hot,
    Warm,
    Cold,
}

#[derive(Clone, Debug)]
struct Candidate {
    tier: Tier,
    identity: String,
    latest_observed_at_unix_ms: u64,
    unresolved: bool,
    cursor_after: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LifecycleCursor {
    hot: String,
    warm: String,
    cold: String,
    #[serde(default)]
    next_tier: u8,
}

#[derive(Clone, Copy, Debug)]
struct Footprint {
    records: u32,
    bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Transition {
    Warm,
    Cold,
    Delete,
}

#[derive(Clone, Copy, Debug)]
enum CapacityAdmission {
    Admitted(Footprint),
    Blocked,
    Deferred,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ArchivedTracePreparation {
    NotArchived,
    Duplicate,
    Rehydrated,
}

pub(super) fn prepare_archived_trace(
    tx: &Transaction<'_>,
    incoming: &DomainSpanState,
) -> Result<ArchivedTracePreparation, StoreError> {
    let trace_key = hash_opaque_identifier(incoming.trace_id.as_str());
    let archived = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM lifecycle_trace_control WHERE trace_key=?1)",
        [&trace_key],
        |row| row.get::<_, bool>(0),
    )?;
    if !archived {
        return Ok(ArchivedTracePreparation::NotArchived);
    }
    let archived_state = tx
        .query_row(
            "SELECT state_json FROM lifecycle_span_control WHERE trace_key=?1 AND span_id=?2",
            params![trace_key, incoming.span_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(archived_state) = archived_state
        && canonical_state_hash(&state_from_json(&archived_state)?)?
            == canonical_state_hash(incoming)?
    {
        return Ok(ArchivedTracePreparation::Duplicate);
    }

    let (latest, record_count, estimated_bytes, trace_id) = tx.query_row(
        "SELECT c.latest_observed_at_unix_ms, c.record_count, c.estimated_bytes, s.trace_id FROM lifecycle_trace_control AS c JOIN lifecycle_span_control AS s ON s.trace_key=c.trace_key WHERE c.trace_key=?1 ORDER BY s.original_commit_seq LIMIT 1",
        [&trace_key],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, String>(3)?)),
    )?;
    if trace_id != incoming.trace_id.as_str() {
        return Err(StoreError::SchemaMismatch);
    }
    let restored = tx.execute(
        "INSERT INTO records(commit_seq, span_id, trace_id, parent_span_id, kind, state_json, record_json) SELECT original_commit_seq, span_id, trace_id, parent_span_id, kind, state_json, record_json FROM lifecycle_span_control WHERE trace_key=?1 ORDER BY original_commit_seq",
        [&trace_key],
    )?;
    if restored != usize::try_from(record_count).map_err(|_| StoreError::SchemaMismatch)? {
        return Err(StoreError::SchemaMismatch);
    }
    tx.execute(
        "INSERT INTO topology(span_id, trace_id, parent_span_id, kind, unresolved) SELECT child.span_id, child.trace_id, child.parent_span_id, child.kind, child.parent_span_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM lifecycle_span_control AS parent WHERE parent.trace_key=child.trace_key AND parent.span_id=child.parent_span_id) FROM lifecycle_span_control AS child WHERE child.trace_key=?1",
        [&trace_key],
    )?;
    tx.execute(
        "INSERT INTO hot_trace_index(trace_id, scan_key, latest_observed_at_unix_ms, unresolved, record_count, estimated_bytes, indexed_complete) VALUES (?1,?1,?2,EXISTS(SELECT 1 FROM topology WHERE trace_id=?1 AND unresolved=1),?3,?4,1)",
        params![trace_id, latest, record_count, estimated_bytes],
    )?;
    tx.execute(
        "DELETE FROM expired_span_states WHERE span_id IN (SELECT span_id FROM lifecycle_span_control WHERE trace_key=?1)",
        [&trace_key],
    )?;
    tx.execute("DELETE FROM warm_traces WHERE trace_key=?1", [&trace_key])?;
    tx.execute("DELETE FROM cold_traces WHERE trace_key=?1", [&trace_key])?;
    tx.execute(
        "DELETE FROM lifecycle_trace_control WHERE trace_key=?1",
        [&trace_key],
    )?;
    Ok(ArchivedTracePreparation::Rehydrated)
}

impl LocalStore {
    /// Returns conservative per-pass admission bounds for [`Self::maintain_lifecycle`].
    ///
    /// The store uses `SQLite`'s delete journal, not WAL. Admission covers the full logical
    /// database, the caller's touched-byte cap, and fixed framing headroom.
    /// `has_backfill_pending` is an O(1) metadata check so schedulers continue bounded legacy
    /// indexing even before `has_candidates` can use typed lifecycle metadata.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::InvalidRetentionBounds`] for an invalid request.
    pub fn lifecycle_preflight(
        &self,
        request: LifecycleRequest,
    ) -> Result<LifecyclePreflight, StoreError> {
        validate_request(request)?;
        let page_count = pragma_u64(&self.db, "page_count")?;
        let page_size = pragma_u64(&self.db, "page_size")?;
        let logical_database_bytes = page_count.saturating_mul(page_size);
        let backfill_cursor = required_schema_text(self.db.query_row(
            "SELECT value FROM metadata WHERE key=?1",
            [LIFECYCLE_BACKFILL_CURSOR_KEY],
            |row| row.get(0),
        ))?;
        Ok(LifecyclePreflight {
            has_candidates: lifecycle_has_candidates(&self.db, request)?,
            has_backfill_pending: backfill_cursor != LIFECYCLE_BACKFILL_COMPLETE,
            maximum_touched_records: request.max_archive_records,
            maximum_touched_bytes: request.max_archive_bytes,
            logical_database_bytes,
            required_temporary_bytes: logical_database_bytes
                .saturating_add(request.max_archive_bytes)
                .saturating_add(PREFLIGHT_FIXED_BYTES),
        })
    }

    /// Applies one bounded standalone hot/warm/cold lifecycle pass.
    ///
    /// Hot rows remain the ingest authority. The public warm query path retains only projected
    /// durable records and remains part of `current_records`, JSONL repair, and normal report
    /// snapshots. A separate private, non-queryable reducer control copy is retained through warm
    /// and cold so later input can atomically resume the trace. Cold public rows are uncompressed
    /// per-trace JSON blobs, are excluded from normal reports, and are available only through
    /// [`Self::query_cold_archives`]. Recent traces are protected by latest-observation age
    /// and unresolved traces are pinned; stale incomplete traces can age out. A later same-span
    /// semantic replay is suppressed. Changed or new input for a retained warm/cold trace restores
    /// its private reducer state atomically before normal reduction resumes.
    /// Every whole-trace transition commits independently, so a busy/error retry is idempotent and
    /// never leaves a partial trace. Maintenance marks projections dirty and never rebuilds JSONL.
    ///
    /// `max_archive_records` and `max_archive_bytes` cap all records and estimated authoritative
    /// bytes touched by the pass, including warm moves and expiry deletes.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] for invalid bounds, a busy database, corrupt tier data, or a failed
    /// transactional transition. Earlier whole-trace transitions may already be committed.
    pub fn maintain_lifecycle(
        &self,
        request: LifecycleRequest,
    ) -> Result<LifecycleResult, StoreError> {
        validate_request(request)?;
        self.db.busy_timeout(MAINTENANCE_BUSY_TIMEOUT)?;
        let result = (|| {
            self.db.pragma_update(None, "secure_delete", true)?;
            let before = storage_stats(&self.db, &self.database_path())?;
            self.maintain_lifecycle_inner(request)
                .and_then(|mut result| {
                    result.vacuum_pages_requested =
                        incremental_vacuum_bounded(&self.db, result.touched_bytes)?;
                    let after = storage_stats(&self.db, &self.database_path())?;
                    result.page_count_before = before.page_count;
                    result.page_count_after = after.page_count;
                    result.freelist_pages_before = before.freelist_pages;
                    result.freelist_pages_after = after.freelist_pages;
                    result.freelist_pages_reclaimed =
                        before.freelist_pages.saturating_sub(after.freelist_pages);
                    result.allocated_bytes_before = before.allocated_bytes;
                    result.allocated_bytes_after = after.allocated_bytes;
                    result.returned_bytes =
                        before.allocated_bytes.saturating_sub(after.allocated_bytes);
                    Ok(result)
                })
        })();
        let restore = self.db.busy_timeout(NORMAL_BUSY_TIMEOUT);
        match (result, restore) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error.into()),
        }
    }

    fn maintain_lifecycle_inner(
        &self,
        request: LifecycleRequest,
    ) -> Result<LifecycleResult, StoreError> {
        let backfill_blocked = backfill_hot_trace_index(&self.db, request)?;
        let hot_cutoff = cutoff(request.now_unix_ms, request.hot_days);
        let warm_cutoff = cutoff(request.now_unix_ms, request.warm_days);
        let delete_cutoff = cutoff(request.now_unix_ms, request.delete_after_days);
        let cursor = required_schema_text(self.db.query_row(
            "SELECT value FROM metadata WHERE key=?1",
            [LIFECYCLE_SCAN_CURSOR_KEY],
            |row| row.get(0),
        ))?;
        let cursor: LifecycleCursor = serde_json::from_str(&cursor)?;
        let candidates = lifecycle_candidates(
            &self.db,
            hot_cutoff,
            warm_cutoff,
            delete_cutoff,
            cursor,
            request.max_traces_per_pass,
        )?;
        let mut result = LifecycleResult {
            scanned_traces: 0,
            moved_to_warm: 0,
            moved_to_cold: 0,
            deleted: 0,
            blocked: backfill_blocked,
            deferred: 0,
            touched_records: 0,
            touched_bytes: 0,
            page_count_before: 0,
            page_count_after: 0,
            freelist_pages_before: 0,
            freelist_pages_after: 0,
            freelist_pages_reclaimed: 0,
            vacuum_pages_requested: 0,
            allocated_bytes_before: 0,
            allocated_bytes_after: 0,
            returned_bytes: 0,
        };

        for candidate in candidates {
            let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
            let Some(current) = reload_candidate(&tx, &candidate)? else {
                update_scan_cursor(&tx, &candidate.cursor_after)?;
                tx.commit()?;
                continue;
            };
            let transition = transition_for(&current, hot_cutoff, warm_cutoff, delete_cutoff);
            if transition.is_none() {
                update_scan_cursor(&tx, &candidate.cursor_after)?;
                tx.commit()?;
                continue;
            }
            result.scanned_traces = result
                .scanned_traces
                .checked_add(1)
                .ok_or(StoreError::SchemaMismatch)?;
            if current.unresolved {
                result.blocked = result.blocked.saturating_add(1);
                update_scan_cursor(&tx, &candidate.cursor_after)?;
                tx.commit()?;
                continue;
            }
            let footprint = trace_footprint(&tx, &current)?;
            let totals = match admit_capacity(&result, footprint, request)? {
                CapacityAdmission::Admitted(totals) => totals,
                disposition => {
                    match disposition {
                        CapacityAdmission::Blocked => {
                            result.blocked = result.blocked.saturating_add(1);
                        }
                        CapacityAdmission::Deferred => result.deferred += 1,
                        CapacityAdmission::Admitted(_) => unreachable!(),
                    }
                    update_scan_cursor(&tx, &candidate.cursor_after)?;
                    tx.commit()?;
                    continue;
                }
            };

            match transition.expect("checked above") {
                Transition::Warm => {
                    move_hot_to_warm(&tx, &current, request.now_unix_ms)?;
                    result.moved_to_warm = result.moved_to_warm.saturating_add(1);
                }
                Transition::Cold => {
                    invalidate_projection(&self.dir)?;
                    move_to_cold(&tx, &current, request.now_unix_ms)?;
                    result.moved_to_cold = result.moved_to_cold.saturating_add(1);
                }
                Transition::Delete => {
                    invalidate_projection(&self.dir)?;
                    purge_trace(&tx, &current)?;
                    result.deleted = result.deleted.saturating_add(1);
                }
            }
            result.touched_records = totals.records;
            result.touched_bytes = totals.bytes;
            mark_projection_dirty(&tx)?;
            advance_report_generation(&tx)?;
            update_scan_cursor(&tx, &candidate.cursor_after)?;
            tx.commit()?;
        }
        Ok(result)
    }

    /// Reads cold history through explicit record, byte, and trace bounds.
    ///
    /// Cold data is excluded from every normal report API. This method validates each stored
    /// uncompressed JSON blob back into the closed `DurableRecordV1` contract before returning it.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] for invalid bounds or corrupt cold data.
    pub fn query_cold_archives(
        &self,
        request: ColdArchiveQuery,
    ) -> Result<ColdArchivePage, StoreError> {
        validate_query(request)?;
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
        let after_archive_seq = i64::try_from(request.after_archive_seq)
            .map_err(|_| StoreError::InvalidRetentionBounds)?;
        let mut statement = tx.prepare(
            "SELECT archive_seq, latest_observed_at_unix_ms, archived_at_unix_ms, record_count, archive_bytes FROM cold_traces WHERE archive_seq>?1 ORDER BY archive_seq LIMIT ?2",
        )?;
        let limit = i64::from(request.max_traces) + 1;
        let rows = statement.query_map(params![after_archive_seq, limit], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        let mut traces = Vec::new();
        let mut records = 0_u32;
        let mut bytes = 0_u64;
        let mut has_more = false;
        let mut blocked = None;
        for row in rows {
            let (archive_seq, latest, archived, stored_records, stored_bytes) = row?;
            if traces.len() >= usize::from(request.max_traces) {
                has_more = true;
                break;
            }
            let archive_seq = u64::try_from(archive_seq).map_err(|_| StoreError::SchemaMismatch)?;
            let stored_records =
                u32::try_from(stored_records).map_err(|_| StoreError::SchemaMismatch)?;
            let stored_bytes =
                u64::try_from(stored_bytes).map_err(|_| StoreError::SchemaMismatch)?;
            let next_records = records
                .checked_add(stored_records)
                .ok_or(StoreError::SchemaMismatch)?;
            let next_bytes = bytes
                .checked_add(stored_bytes)
                .ok_or(StoreError::SchemaMismatch)?;
            if next_records > request.max_records || next_bytes > request.max_bytes {
                has_more = true;
                blocked = Some(ColdArchiveBlocked {
                    archive_seq,
                    records: stored_records,
                    bytes: stored_bytes,
                });
                break;
            }
            let blob: Vec<u8> = tx.query_row(
                "SELECT archive_blob FROM cold_traces WHERE archive_seq=?1",
                [i64::try_from(archive_seq).map_err(|_| StoreError::SchemaMismatch)?],
                |row| row.get(0),
            )?;
            if stored_bytes != u64::try_from(blob.len()).map_err(|_| StoreError::SchemaMismatch)? {
                return Err(StoreError::SchemaMismatch);
            }
            let decoded: Vec<DurableRecordV1> = serde_json::from_slice(&blob)?;
            if decoded.len()
                != usize::try_from(stored_records).map_err(|_| StoreError::SchemaMismatch)?
                || decoded.is_empty()
            {
                return Err(StoreError::SchemaMismatch);
            }
            for record in &decoded {
                record.validate().map_err(|_| StoreError::SchemaMismatch)?;
            }
            records = next_records;
            bytes = next_bytes;
            traces.push(ColdArchiveTrace {
                archive_seq,
                latest_observed_at_unix_ms: parse_millis(&latest)?,
                archived_at_unix_ms: parse_millis(&archived)?,
                archive_bytes: stored_bytes,
                records: decoded,
            });
        }
        drop(statement);
        tx.commit()?;
        let next_after_archive_seq = traces
            .last()
            .map_or(request.after_archive_seq, |trace| trace.archive_seq);
        Ok(ColdArchivePage {
            traces,
            records,
            bytes,
            next_after_archive_seq,
            has_more,
            blocked,
        })
    }
}

fn validate_request(request: LifecycleRequest) -> Result<(), StoreError> {
    if request.hot_days == 0
        || request.hot_days > request.warm_days
        || request.warm_days >= request.delete_after_days
        || request.delete_after_days > MAX_LIFECYCLE_DAYS
        || request.max_traces_per_pass == 0
        || request.max_traces_per_pass > MAX_TRACES_PER_PASS
        || request.max_archive_records < MIN_ARCHIVE_RECORDS
        || request.max_archive_records > MAX_ARCHIVE_RECORDS
        || request.max_archive_bytes < MIN_ARCHIVE_BYTES
        || request.max_archive_bytes > MAX_ARCHIVE_BYTES
    {
        return Err(StoreError::InvalidRetentionBounds);
    }
    Ok(())
}

fn validate_query(request: ColdArchiveQuery) -> Result<(), StoreError> {
    if request.after_archive_seq > i64::MAX.cast_unsigned()
        || request.max_traces == 0
        || request.max_traces > MAX_TRACES_PER_PASS
        || request.max_records == 0
        || request.max_records > MAX_ARCHIVE_RECORDS
        || request.max_bytes == 0
        || request.max_bytes > MAX_ARCHIVE_BYTES
    {
        return Err(StoreError::InvalidRetentionBounds);
    }
    Ok(())
}

fn cutoff(now_unix_ms: u64, days: u16) -> u64 {
    now_unix_ms.saturating_sub(u64::from(days).saturating_mul(DAY_MILLIS))
}

fn pragma_u64(db: &rusqlite::Connection, name: &str) -> Result<u64, StoreError> {
    let value = db.query_row(&format!("PRAGMA {name}"), [], |row| row.get::<_, i64>(0))?;
    u64::try_from(value).map_err(|_| StoreError::SchemaMismatch)
}

fn lifecycle_has_candidates(
    db: &rusqlite::Connection,
    request: LifecycleRequest,
) -> Result<bool, StoreError> {
    let hot_cutoff = ordered_millis(cutoff(request.now_unix_ms, request.hot_days));
    let warm_cutoff = ordered_millis(cutoff(request.now_unix_ms, request.warm_days));
    let delete_cutoff = ordered_millis(cutoff(request.now_unix_ms, request.delete_after_days));
    let indexed = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM hot_trace_index WHERE indexed_complete=1 AND latest_observed_at_unix_ms<=?1 LIMIT 1) OR EXISTS(SELECT 1 FROM warm_traces WHERE latest_observed_at_unix_ms<=?2 LIMIT 1) OR EXISTS(SELECT 1 FROM cold_traces WHERE latest_observed_at_unix_ms<=?3 LIMIT 1)",
        params![hot_cutoff, warm_cutoff, delete_cutoff],
        |row| row.get::<_, bool>(0),
    )?;
    if indexed {
        return Ok(true);
    }
    Ok(false)
}

#[derive(Debug, Deserialize, Serialize)]
struct BackfillState {
    done: bool,
    after_observation_rowid: i64,
    highwater_observation_rowid: i64,
    #[serde(default)]
    scan_complete: bool,
    #[serde(default)]
    blocked_traces: u16,
    active: Option<BackfillTrace>,
}

#[derive(Debug, Deserialize, Serialize)]
struct BackfillTrace {
    trace_id: String,
    observation_after_millis: String,
    observation_after_event_id: String,
    observation_count: u64,
    observation_bytes: u64,
    latest_observed_at_unix_ms: String,
    observations_done: bool,
    record_after_commit_seq: i64,
    record_count: u64,
    record_bytes: u64,
    #[serde(default)]
    blocked: bool,
}

impl BackfillTrace {
    fn new(trace_id: String) -> Self {
        Self {
            trace_id,
            observation_after_millis: String::new(),
            observation_after_event_id: String::new(),
            observation_count: 0,
            observation_bytes: 0,
            latest_observed_at_unix_ms: String::new(),
            observations_done: false,
            record_after_commit_seq: 0,
            record_count: 0,
            record_bytes: 0,
            blocked: false,
        }
    }
}

pub(super) fn invalidate_legacy_backfill_trace(
    tx: &Transaction<'_>,
    trace_id: &str,
) -> Result<(), StoreError> {
    let encoded = required_schema_text(tx.query_row(
        "SELECT value FROM metadata WHERE key=?1",
        [LIFECYCLE_BACKFILL_CURSOR_KEY],
        |row| row.get(0),
    ))?;
    if encoded == LIFECYCLE_BACKFILL_COMPLETE {
        return Ok(());
    }
    let mut state: BackfillState = serde_json::from_str(&encoded)?;
    if state.done {
        return Err(StoreError::SchemaMismatch);
    }
    if state.scan_complete {
        return Ok(());
    }
    if state.active.as_ref().map(|active| active.trace_id.as_str()) == Some(trace_id) {
        let blocked = state.active.as_ref().is_some_and(|active| active.blocked);
        let mut active = BackfillTrace::new(trace_id.to_owned());
        active.blocked = blocked;
        state.active = Some(active);
        tx.execute(
            "UPDATE metadata SET value=?1 WHERE key=?2",
            params![
                serde_json::to_string(&state)?,
                LIFECYCLE_BACKFILL_CURSOR_KEY
            ],
        )?;
        return Ok(());
    }
    if terminally_blocked_backfill_trace(tx, trace_id)? {
        return Ok(());
    }
    let future_legacy_observation = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM observations WHERE trace_id=?1 AND rowid>?2 AND rowid<=?3 LIMIT 1)",
        params![
            trace_id,
            state.after_observation_rowid,
            state.highwater_observation_rowid
        ],
        |row| row.get::<_, bool>(0),
    )?;
    if future_legacy_observation {
        return Ok(());
    }
    Err(StoreError::SchemaMismatch)
}

fn terminally_blocked_backfill_trace(
    db: &rusqlite::Connection,
    trace_id: &str,
) -> Result<bool, StoreError> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM hot_trace_index WHERE trace_id=?1 AND indexed_complete=0 AND record_count=0 AND estimated_bytes=0)",
        [trace_id],
        |row| row.get::<_, bool>(0),
    )?)
}

#[allow(
    clippy::too_many_lines,
    reason = "the bounded row cursor and its persisted accumulator form one transactional state machine"
)]
fn backfill_hot_trace_index(
    db: &rusqlite::Connection,
    request: LifecycleRequest,
) -> Result<u16, StoreError> {
    let observed_cursor = required_schema_text(db.query_row(
        "SELECT value FROM metadata WHERE key=?1",
        [LIFECYCLE_BACKFILL_CURSOR_KEY],
        |row| row.get(0),
    ))?;
    if observed_cursor == LIFECYCLE_BACKFILL_COMPLETE {
        return Ok(0);
    }
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let encoded = required_schema_text(tx.query_row(
        "SELECT value FROM metadata WHERE key=?1",
        [LIFECYCLE_BACKFILL_CURSOR_KEY],
        |row| row.get(0),
    ))?;
    if encoded == LIFECYCLE_BACKFILL_COMPLETE {
        tx.commit()?;
        return Ok(0);
    }
    let mut state: BackfillState = serde_json::from_str(&encoded)?;
    if state.done {
        return Err(StoreError::SchemaMismatch);
    }
    if state.scan_complete {
        tx.commit()?;
        return Ok(state.blocked_traces);
    }
    // The bundled SQLite's `octet_length(column)` reads byte length from cell
    // metadata. The row cursor bounds scans; the byte cap admits normal traces
    // and marks a larger whole trace terminally blocked without loading payloads.
    let mut remaining_rows = request.max_archive_records.min(MAX_BACKFILL_ROWS_PER_PASS);
    let mut remaining_bytes = MAX_BACKFILL_BYTES_PER_PASS;
    let mut completed_traces = 0_u16;
    while remaining_rows > 0 && completed_traces < request.max_traces_per_pass {
        if state.active.is_none() {
            let next = tx
                .query_row(
                    "SELECT rowid, trace_id FROM observations WHERE rowid>?1 AND rowid<=?2 ORDER BY rowid LIMIT 1",
                    params![
                        state.after_observation_rowid,
                        state.highwater_observation_rowid
                    ],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            let Some((rowid, trace_id)) = next else {
                if state.blocked_traces == 0 {
                    state.done = true;
                } else {
                    state.scan_complete = true;
                }
                break;
            };
            state.after_observation_rowid = rowid;
            let index_state = tx
                .query_row(
                    "SELECT indexed_complete, record_count=0 AND estimated_bytes=0 FROM hot_trace_index WHERE trace_id=?1",
                    [&trace_id],
                    |row| Ok((row.get::<_, bool>(0)?, row.get::<_, bool>(1)?)),
                )
                .optional()?;
            if index_state
                .is_some_and(|(complete, terminally_blocked)| complete || terminally_blocked)
            {
                completed_traces += 1;
                continue;
            }
            state.active = Some(BackfillTrace::new(trace_id));
        }
        let active = state.active.as_mut().ok_or(StoreError::SchemaMismatch)?;
        if !active.observations_done {
            let row = tx
                .query_row(
                    "SELECT observed_at_unix_ms, event_id, octet_length(projected_json)+octet_length(payload_hash)+?4 FROM observations WHERE trace_id=?1 AND (observed_at_unix_ms>?2 OR (observed_at_unix_ms=?2 AND event_id>?3)) ORDER BY observed_at_unix_ms, event_id LIMIT 1",
                    params![active.trace_id, active.observation_after_millis, active.observation_after_event_id, i64::try_from(ROW_OVERHEAD_BYTES).map_err(|_| StoreError::SchemaMismatch)?],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)),
                )
                .optional()?;
            if let Some((millis, event_id, bytes)) = row {
                let bytes = u64::try_from(bytes).map_err(|_| StoreError::SchemaMismatch)?;
                if !active.blocked
                    && bytes <= MAX_BACKFILL_BYTES_PER_PASS
                    && bytes > remaining_bytes
                {
                    break;
                }
                active.observation_after_millis.clone_from(&millis);
                active.observation_after_event_id = event_id;
                active.latest_observed_at_unix_ms = millis;
                active.observation_count = active.observation_count.saturating_add(1);
                remaining_rows -= 1;
                if bytes > MAX_BACKFILL_BYTES_PER_PASS {
                    if !active.blocked {
                        active.blocked = true;
                        state.blocked_traces = state.blocked_traces.saturating_add(1);
                    }
                } else if !active.blocked {
                    active.observation_bytes = active.observation_bytes.saturating_add(bytes);
                    remaining_bytes -= bytes;
                }
                continue;
            }
            active.observations_done = true;
        }
        let row = tx
            .query_row(
                "SELECT commit_seq, octet_length(state_json)+octet_length(record_json)+?3 FROM records WHERE trace_id=?1 AND commit_seq>?2 ORDER BY commit_seq LIMIT 1",
                params![active.trace_id, active.record_after_commit_seq, i64::try_from(ROW_OVERHEAD_BYTES).map_err(|_| StoreError::SchemaMismatch)?],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        if let Some((commit_seq, bytes)) = row {
            let bytes = u64::try_from(bytes).map_err(|_| StoreError::SchemaMismatch)?;
            if !active.blocked && bytes <= MAX_BACKFILL_BYTES_PER_PASS && bytes > remaining_bytes {
                break;
            }
            active.record_after_commit_seq = commit_seq;
            active.record_count = active.record_count.saturating_add(1);
            remaining_rows -= 1;
            if bytes > MAX_BACKFILL_BYTES_PER_PASS {
                if !active.blocked {
                    active.blocked = true;
                    state.blocked_traces = state.blocked_traces.saturating_add(1);
                }
            } else if !active.blocked {
                active.record_bytes = active.record_bytes.saturating_add(bytes);
                remaining_bytes -= bytes;
            }
            continue;
        }
        if active.blocked {
            tx.execute(
                "INSERT INTO hot_trace_index(trace_id, scan_key, latest_observed_at_unix_ms, unresolved, record_count, estimated_bytes, indexed_complete) VALUES (?1,?1,?2,0,0,0,0) ON CONFLICT(trace_id) DO UPDATE SET latest_observed_at_unix_ms=excluded.latest_observed_at_unix_ms, unresolved=0, record_count=0, estimated_bytes=0, indexed_complete=0",
                params![active.trace_id, active.latest_observed_at_unix_ms],
            )?;
            state.active = None;
            completed_traces += 1;
            continue;
        }
        if active.record_count == 0 || active.latest_observed_at_unix_ms.is_empty() {
            return Err(StoreError::SchemaMismatch);
        }
        tx.execute(
            "INSERT INTO hot_trace_index(trace_id, scan_key, latest_observed_at_unix_ms, unresolved, record_count, estimated_bytes, indexed_complete) VALUES (?1,?1,?2,EXISTS(SELECT 1 FROM topology WHERE trace_id=?1 AND unresolved=1),?3,?4,1) ON CONFLICT(trace_id) DO UPDATE SET latest_observed_at_unix_ms=excluded.latest_observed_at_unix_ms, unresolved=excluded.unresolved, record_count=excluded.record_count, estimated_bytes=excluded.estimated_bytes, indexed_complete=1 WHERE hot_trace_index.indexed_complete=0",
            params![active.trace_id, active.latest_observed_at_unix_ms, i64::try_from(active.record_count).map_err(|_| StoreError::SchemaMismatch)?, i64::try_from(active.observation_bytes.saturating_add(active.record_bytes)).map_err(|_| StoreError::SchemaMismatch)?],
        )?;
        state.active = None;
        completed_traces += 1;
    }
    let next = if state.done {
        LIFECYCLE_BACKFILL_COMPLETE.to_owned()
    } else {
        serde_json::to_string(&state)?
    };
    tx.execute(
        "UPDATE metadata SET value=?1 WHERE key=?2",
        params![next, LIFECYCLE_BACKFILL_CURSOR_KEY],
    )?;
    tx.commit()?;
    Ok(state.blocked_traces)
}

fn parse_millis(value: &str) -> Result<u64, StoreError> {
    value.parse().map_err(|_| StoreError::SchemaMismatch)
}

fn lifecycle_candidates(
    db: &rusqlite::Connection,
    hot_cutoff: u64,
    warm_cutoff: u64,
    delete_cutoff: u64,
    mut cursor: LifecycleCursor,
    limit: u16,
) -> Result<Vec<Candidate>, StoreError> {
    let mut candidates = Vec::with_capacity(usize::from(limit).saturating_mul(3));
    let mut remaining = usize::from(limit);
    for tier in tier_order(cursor.next_tier)? {
        if remaining == 0 {
            break;
        }
        let raw_cursor = match tier {
            Tier::Hot => &cursor.hot,
            Tier::Warm => &cursor.warm,
            Tier::Cold => &cursor.cold,
        };
        let rows = query_tier_window(db, tier, raw_cursor, remaining)?;
        for (identity, raw_key, latest, unresolved) in rows {
            match tier {
                Tier::Hot => cursor.hot.clone_from(&raw_key),
                Tier::Warm => cursor.warm.clone_from(&raw_key),
                Tier::Cold => cursor.cold.clone_from(&raw_key),
            }
            cursor.next_tier = next_tier(tier);
            let candidate = Candidate {
                tier,
                identity,
                latest_observed_at_unix_ms: parse_millis(&latest)?,
                unresolved,
                cursor_after: serde_json::to_string(&cursor)?,
            };
            let eligible =
                transition_for(&candidate, hot_cutoff, warm_cutoff, delete_cutoff).is_some();
            candidates.push(candidate);
            if eligible {
                remaining = remaining.saturating_sub(1);
            }
            if remaining == 0 {
                break;
            }
        }
    }
    Ok(candidates)
}

fn tier_order(next_tier: u8) -> Result<[Tier; 3], StoreError> {
    match next_tier {
        0 => Ok([Tier::Hot, Tier::Warm, Tier::Cold]),
        1 => Ok([Tier::Warm, Tier::Cold, Tier::Hot]),
        2 => Ok([Tier::Cold, Tier::Hot, Tier::Warm]),
        _ => Err(StoreError::SchemaMismatch),
    }
}

fn next_tier(tier: Tier) -> u8 {
    match tier {
        Tier::Hot => 1,
        Tier::Warm => 2,
        Tier::Cold => 0,
    }
}

fn query_tier_window(
    db: &rusqlite::Connection,
    tier: Tier,
    cursor: &str,
    limit: usize,
) -> Result<Vec<(String, String, String, bool)>, StoreError> {
    let mut rows = query_tier_range(db, tier, cursor, limit, true)?;
    let remaining = limit.saturating_sub(rows.len());
    if remaining > 0 && !cursor.is_empty() {
        rows.extend(query_tier_range(db, tier, cursor, remaining, false)?);
    }
    Ok(rows)
}

fn query_tier_range(
    db: &rusqlite::Connection,
    tier: Tier,
    cursor: &str,
    limit: usize,
    after: bool,
) -> Result<Vec<(String, String, String, bool)>, StoreError> {
    let comparison = if after { ">" } else { "<=" };
    let sql = match tier {
        Tier::Hot => format!(
            "SELECT trace_id, scan_key, latest_observed_at_unix_ms, unresolved FROM hot_trace_index WHERE scan_key {comparison} ?1 ORDER BY scan_key LIMIT ?2"
        ),
        Tier::Warm => format!(
            "SELECT trace_key, trace_key, latest_observed_at_unix_ms, 0 FROM warm_traces WHERE trace_key {comparison} ?1 ORDER BY trace_key LIMIT ?2"
        ),
        Tier::Cold => format!(
            "SELECT trace_key, trace_key, latest_observed_at_unix_ms, 0 FROM cold_traces WHERE trace_key {comparison} ?1 ORDER BY trace_key LIMIT ?2"
        ),
    };
    let mut statement = db.prepare(&sql)?;
    let limit = i64::try_from(limit).map_err(|_| StoreError::SchemaMismatch)?;
    let mapped = statement.query_map(params![cursor, limit], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, bool>(3)?,
        ))
    })?;
    mapped.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn reload_candidate(
    tx: &Transaction<'_>,
    candidate: &Candidate,
) -> Result<Option<Candidate>, StoreError> {
    let row = match candidate.tier {
        Tier::Hot => tx
            .query_row(
                "SELECT latest_observed_at_unix_ms, unresolved, 'h:' || scan_key FROM hot_trace_index WHERE trace_id=?1 AND indexed_complete=1",
                [&candidate.identity],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?, row.get::<_, String>(2)?)),
            )
            .optional()?,
        Tier::Warm => tx
            .query_row(
                "SELECT latest_observed_at_unix_ms, 0, 'w:' || trace_key FROM warm_traces WHERE trace_key=?1",
                [&candidate.identity],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?, row.get::<_, String>(2)?)),
            )
            .optional()?,
        Tier::Cold => tx
            .query_row(
                "SELECT latest_observed_at_unix_ms, 0, 'c:' || trace_key FROM cold_traces WHERE trace_key=?1",
                [&candidate.identity],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?, row.get::<_, String>(2)?)),
            )
            .optional()?,
    };
    row.map(|(latest, unresolved, _scan_key)| {
        Ok(Candidate {
            tier: candidate.tier,
            identity: candidate.identity.clone(),
            latest_observed_at_unix_ms: parse_millis(&latest)?,
            unresolved,
            cursor_after: candidate.cursor_after.clone(),
        })
    })
    .transpose()
}

fn transition_for(
    candidate: &Candidate,
    hot_cutoff: u64,
    warm_cutoff: u64,
    delete_cutoff: u64,
) -> Option<Transition> {
    let latest = candidate.latest_observed_at_unix_ms;
    if latest <= delete_cutoff {
        return Some(Transition::Delete);
    }
    match candidate.tier {
        Tier::Hot | Tier::Warm if latest <= warm_cutoff => Some(Transition::Cold),
        Tier::Hot if latest <= hot_cutoff => Some(Transition::Warm),
        Tier::Cold | Tier::Warm | Tier::Hot => None,
    }
}

fn admit_capacity(
    result: &LifecycleResult,
    footprint: Footprint,
    request: LifecycleRequest,
) -> Result<CapacityAdmission, StoreError> {
    if footprint.records > request.max_archive_records
        || footprint.bytes > request.max_archive_bytes
    {
        return Ok(CapacityAdmission::Blocked);
    }
    let totals = Footprint {
        records: result
            .touched_records
            .checked_add(footprint.records)
            .ok_or(StoreError::SchemaMismatch)?,
        bytes: result
            .touched_bytes
            .checked_add(footprint.bytes)
            .ok_or(StoreError::SchemaMismatch)?,
    };
    if totals.records > request.max_archive_records || totals.bytes > request.max_archive_bytes {
        return Ok(CapacityAdmission::Deferred);
    }
    Ok(CapacityAdmission::Admitted(totals))
}

fn trace_footprint(tx: &Transaction<'_>, candidate: &Candidate) -> Result<Footprint, StoreError> {
    let (records, bytes) = match candidate.tier {
        Tier::Hot => typed_footprint(
            tx,
            "SELECT record_count, estimated_bytes FROM hot_trace_index WHERE trace_id=?1 AND indexed_complete=1",
            &candidate.identity,
        )?,
        Tier::Warm => typed_footprint(
            tx,
            "SELECT record_count, estimated_bytes FROM lifecycle_trace_control WHERE trace_key=?1 AND tier='warm'",
            &candidate.identity,
        )?,
        Tier::Cold => typed_footprint(
            tx,
            "SELECT record_count, estimated_bytes FROM lifecycle_trace_control WHERE trace_key=?1 AND tier='cold'",
            &candidate.identity,
        )?,
    };
    if records == 0 {
        return Err(StoreError::SchemaMismatch);
    }
    Ok(Footprint { records, bytes })
}

fn typed_footprint(
    tx: &Transaction<'_>,
    sql: &str,
    identity: &str,
) -> Result<(u32, u64), StoreError> {
    let (records, bytes) = tx.query_row(sql, [identity], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    })?;
    Ok((
        u32::try_from(records).map_err(|_| StoreError::SchemaMismatch)?,
        u64::try_from(bytes).map_err(|_| StoreError::SchemaMismatch)?,
    ))
}

fn move_hot_to_warm(
    tx: &Transaction<'_>,
    candidate: &Candidate,
    now_unix_ms: u64,
) -> Result<(), StoreError> {
    let trace_key = hash_opaque_identifier(&candidate.identity);
    archive_hot_control(tx, candidate, &trace_key, "warm")?;
    tx.execute(
        "INSERT INTO warm_traces(trace_key, latest_observed_at_unix_ms, moved_at_unix_ms) VALUES (?1,?2,?3)",
        params![trace_key, ordered_millis(candidate.latest_observed_at_unix_ms), ordered_millis(now_unix_ms)],
    )?;
    let rows = load_hot_records(tx, &candidate.identity)?;
    insert_span_guards(tx, rows.iter().map(|(_, state, _)| state.as_str()))?;
    for (commit_seq, _, record_json) in rows {
        let record: DurableRecordV1 = serde_json::from_str(&record_json)?;
        record.validate().map_err(|_| StoreError::SchemaMismatch)?;
        tx.execute(
            "INSERT INTO warm_records(span_id, trace_key, original_commit_seq, record_json) VALUES (?1,?2,?3,?4)",
            params![record.span_id, trace_key, commit_seq, record_json],
        )?;
    }
    delete_hot_trace(tx, &candidate.identity)?;
    Ok(())
}

fn move_to_cold(
    tx: &Transaction<'_>,
    candidate: &Candidate,
    now_unix_ms: u64,
) -> Result<(), StoreError> {
    let (trace_key, records) = match candidate.tier {
        Tier::Hot => {
            let trace_key = hash_opaque_identifier(&candidate.identity);
            archive_hot_control(tx, candidate, &trace_key, "cold")?;
            let rows = load_hot_records(tx, &candidate.identity)?;
            insert_span_guards(tx, rows.iter().map(|(_, state, _)| state.as_str()))?;
            let records = decode_records(rows.iter().map(|(_, _, record)| record.as_str()))?;
            delete_hot_trace(tx, &candidate.identity)?;
            (trace_key, records)
        }
        Tier::Warm => {
            let records = load_warm_records(tx, &candidate.identity)?;
            tx.execute(
                "UPDATE lifecycle_trace_control SET tier='cold' WHERE trace_key=?1 AND tier='warm'",
                [&candidate.identity],
            )?;
            tx.execute(
                "DELETE FROM warm_traces WHERE trace_key=?1",
                [&candidate.identity],
            )?;
            (candidate.identity.clone(), records)
        }
        Tier::Cold => return Err(StoreError::SchemaMismatch),
    };
    let blob = serde_json::to_vec(&records)?;
    tx.execute(
        "INSERT INTO cold_traces(trace_key, latest_observed_at_unix_ms, archived_at_unix_ms, record_count, archive_bytes, archive_blob) VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            trace_key,
            ordered_millis(candidate.latest_observed_at_unix_ms),
            ordered_millis(now_unix_ms),
            i64::try_from(records.len()).map_err(|_| StoreError::SchemaMismatch)?,
            i64::try_from(blob.len()).map_err(|_| StoreError::SchemaMismatch)?,
            blob,
        ],
    )?;
    Ok(())
}

fn purge_trace(tx: &Transaction<'_>, candidate: &Candidate) -> Result<(), StoreError> {
    let trace_key = match candidate.tier {
        Tier::Hot => hash_opaque_identifier(&candidate.identity),
        Tier::Warm | Tier::Cold => candidate.identity.clone(),
    };
    match candidate.tier {
        Tier::Hot => {
            let rows = load_hot_records(tx, &candidate.identity)?;
            insert_span_guards(tx, rows.iter().map(|(_, state, _)| state.as_str()))?;
            delete_hot_trace(tx, &candidate.identity)?;
        }
        Tier::Warm => {
            tx.execute(
                "DELETE FROM warm_traces WHERE trace_key=?1",
                [&candidate.identity],
            )?;
            tx.execute(
                "DELETE FROM lifecycle_trace_control WHERE trace_key=?1",
                [&candidate.identity],
            )?;
        }
        Tier::Cold => {
            tx.execute(
                "DELETE FROM cold_traces WHERE trace_key=?1",
                [&candidate.identity],
            )?;
            tx.execute(
                "DELETE FROM lifecycle_trace_control WHERE trace_key=?1",
                [&candidate.identity],
            )?;
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO expired_trace_states(trace_key) VALUES (?1)",
        [&trace_key],
    )?;
    tx.execute(
        "DELETE FROM expired_trace_states WHERE guard_seq NOT IN (SELECT guard_seq FROM expired_trace_states ORDER BY guard_seq DESC LIMIT ?1)",
        [i64::try_from(MAX_EXPIRED_SPAN_GUARDS).map_err(|_| StoreError::SchemaMismatch)?],
    )?;
    Ok(())
}

fn archive_hot_control(
    tx: &Transaction<'_>,
    candidate: &Candidate,
    trace_key: &str,
    tier: &str,
) -> Result<(), StoreError> {
    let footprint = trace_footprint(tx, candidate)?;
    tx.execute(
        "INSERT INTO lifecycle_trace_control(trace_key, tier, latest_observed_at_unix_ms, record_count, estimated_bytes) VALUES (?1,?2,?3,?4,?5)",
        params![
            trace_key,
            tier,
            ordered_millis(candidate.latest_observed_at_unix_ms),
            i64::from(footprint.records),
            i64::try_from(footprint.bytes).map_err(|_| StoreError::SchemaMismatch)?
        ],
    )?;
    let copied = tx.execute(
        "INSERT INTO lifecycle_span_control(span_id, trace_key, original_commit_seq, trace_id, parent_span_id, kind, state_json, record_json) SELECT span_id, ?1, commit_seq, trace_id, parent_span_id, kind, state_json, record_json FROM records WHERE trace_id=?2 ORDER BY commit_seq",
        params![trace_key, candidate.identity],
    )?;
    if copied != usize::try_from(footprint.records).map_err(|_| StoreError::SchemaMismatch)? {
        return Err(StoreError::SchemaMismatch);
    }
    Ok(())
}

fn load_hot_records(
    tx: &Transaction<'_>,
    trace_id: &str,
) -> Result<Vec<(i64, String, String)>, StoreError> {
    let mut statement = tx.prepare(
        "SELECT commit_seq, state_json, record_json FROM records WHERE trace_id=?1 ORDER BY commit_seq",
    )?;
    let rows = statement
        .query_map([trace_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.is_empty() {
        return Err(StoreError::SchemaMismatch);
    }
    Ok(rows)
}

fn load_warm_records(
    tx: &Transaction<'_>,
    trace_key: &str,
) -> Result<Vec<DurableRecordV1>, StoreError> {
    let mut statement = tx.prepare(
        "SELECT record_json FROM warm_records WHERE trace_key=?1 ORDER BY original_commit_seq",
    )?;
    let rows = statement
        .query_map([trace_key], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    decode_records(rows.iter().map(String::as_str))
}

fn decode_records<'a>(
    rows: impl Iterator<Item = &'a str>,
) -> Result<Vec<DurableRecordV1>, StoreError> {
    let mut records = Vec::new();
    for row in rows {
        let record: DurableRecordV1 = serde_json::from_str(row)?;
        record.validate().map_err(|_| StoreError::SchemaMismatch)?;
        records.push(record);
    }
    if records.is_empty() {
        return Err(StoreError::SchemaMismatch);
    }
    Ok(records)
}

fn insert_span_guards<'a>(
    tx: &Transaction<'_>,
    states: impl Iterator<Item = &'a str>,
) -> Result<(), StoreError> {
    for state in states {
        let state = state_from_json(state)?;
        tx.execute(
            "INSERT OR REPLACE INTO expired_span_states(span_id, canonical_state_hash) VALUES (?1,?2)",
            params![state.span_id.as_str(), canonical_state_hash(&state)?],
        )?;
    }
    prune_expired_span_guards(tx)
}

fn delete_hot_trace(tx: &Transaction<'_>, trace_id: &str) -> Result<(), StoreError> {
    tx.execute(
        "DELETE FROM delivery_outcomes WHERE event_id IN (SELECT event_id FROM observations WHERE trace_id=?1)",
        [trace_id],
    )?;
    tx.execute(
        "DELETE FROM source_inputs WHERE event_id IN (SELECT event_id FROM observations WHERE trace_id=?1)",
        [trace_id],
    )?;
    tx.execute("DELETE FROM observations WHERE trace_id=?1", [trace_id])?;
    tx.execute("DELETE FROM topology WHERE trace_id=?1", [trace_id])?;
    tx.execute("DELETE FROM records WHERE trace_id=?1", [trace_id])?;
    tx.execute("DELETE FROM hot_trace_index WHERE trace_id=?1", [trace_id])?;
    Ok(())
}

fn update_scan_cursor(tx: &Transaction<'_>, scan_key: &str) -> Result<(), StoreError> {
    tx.execute(
        "UPDATE metadata SET value=?1 WHERE key=?2",
        params![scan_key, LIFECYCLE_SCAN_CURSOR_KEY],
    )?;
    Ok(())
}

fn invalidate_projection(dir: &std::path::Path) -> Result<(), StoreError> {
    let projection = dir.join(PROJECTION_NAME);
    match fs::symlink_metadata(&projection) {
        Ok(_) => {
            private_file(&projection)?;
            fs::remove_file(&projection)?;
            File::open(dir)?.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn incremental_vacuum_bounded(
    db: &rusqlite::Connection,
    touched_bytes: u64,
) -> Result<u16, StoreError> {
    if touched_bytes == 0 {
        return Ok(0);
    }
    let page_size = db.query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))?;
    let page_size = u64::try_from(page_size).map_err(|_| StoreError::SchemaMismatch)?;
    if page_size == 0 {
        return Err(StoreError::SchemaMismatch);
    }
    let pages = touched_bytes
        .saturating_add(page_size - 1)
        .checked_div(page_size)
        .ok_or(StoreError::SchemaMismatch)?
        .min(MAX_INCREMENTAL_VACUUM_PAGES);
    if pages > 0 {
        let mut statement = db.prepare(&format!("PRAGMA incremental_vacuum({pages})"))?;
        let mut rows = statement.query([])?;
        while rows.next()?.is_some() {}
    }
    u16::try_from(pages).map_err(|_| StoreError::SchemaMismatch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LIFECYCLE_SCAN_CURSOR_INITIAL;
    use rusqlite::Connection;

    fn candidate_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE hot_trace_index (
                trace_id TEXT PRIMARY KEY,
                scan_key TEXT NOT NULL UNIQUE,
                latest_observed_at_unix_ms TEXT NOT NULL,
                unresolved INTEGER NOT NULL,
                record_count INTEGER NOT NULL,
                estimated_bytes INTEGER NOT NULL,
                indexed_complete INTEGER NOT NULL
             );
             CREATE TABLE warm_traces (
                trace_key TEXT PRIMARY KEY,
                latest_observed_at_unix_ms TEXT NOT NULL,
                moved_at_unix_ms TEXT NOT NULL
             );
             CREATE TABLE cold_traces (
                archive_seq INTEGER PRIMARY KEY,
                trace_key TEXT NOT NULL UNIQUE,
                latest_observed_at_unix_ms TEXT NOT NULL,
                archived_at_unix_ms TEXT NOT NULL,
                record_count INTEGER NOT NULL,
                archive_bytes INTEGER NOT NULL,
                archive_blob BLOB NOT NULL
             );",
        )
        .unwrap();
        db
    }

    #[test]
    fn candidate_windows_advance_through_large_recent_population() {
        let db = candidate_db();
        for ordinal in 0..1_000 {
            let trace_id = format!("recent-{ordinal:04}");
            db.execute(
                "INSERT INTO hot_trace_index VALUES (?1,?1,?2,0,1,1,1)",
                params![trace_id, ordered_millis(100)],
            )
            .unwrap();
        }
        db.execute(
            "INSERT INTO hot_trace_index VALUES ('z-old','z-old',?1,0,1,1,1)",
            [ordered_millis(0)],
        )
        .unwrap();

        let mut cursor: LifecycleCursor =
            serde_json::from_str(LIFECYCLE_SCAN_CURSOR_INITIAL).unwrap();
        let mut found_old = false;
        for _ in 0..9 {
            let candidates = lifecycle_candidates(&db, 0, 0, 0, cursor, 128).unwrap();
            assert!(candidates.len() <= 3 * 128);
            assert!(
                candidates
                    .iter()
                    .filter(|candidate| transition_for(candidate, 0, 0, 0).is_some())
                    .count()
                    <= 128
            );
            found_old |= candidates
                .iter()
                .any(|candidate| candidate.identity == "z-old");
            cursor = serde_json::from_str(&candidates.last().unwrap().cursor_after).unwrap();
            if found_old {
                break;
            }
        }
        assert!(
            found_old,
            "bounded raw-key windows did not advance to the old trace"
        );
    }

    #[test]
    fn candidate_tier_rotation_prevents_pinned_hot_starvation() {
        let db = candidate_db();
        for ordinal in 0..2 {
            let trace_id = format!("hot-pinned-{ordinal}");
            db.execute(
                "INSERT INTO hot_trace_index VALUES (?1,?1,?2,1,1,1,1)",
                params![trace_id, ordered_millis(0)],
            )
            .unwrap();
        }
        db.execute(
            "INSERT INTO warm_traces VALUES ('warm-ready',?1,?1)",
            [ordered_millis(0)],
        )
        .unwrap();
        db.execute(
            "INSERT INTO cold_traces VALUES (1,'cold-ready',?1,?1,1,1,X'00')",
            [ordered_millis(0)],
        )
        .unwrap();

        let legacy_cursor: LifecycleCursor =
            serde_json::from_str(r#"{"hot":"","warm":"","cold":""}"#).unwrap();
        assert_eq!(legacy_cursor.next_tier, 0);
        let hot_pass = lifecycle_candidates(&db, 0, 0, 0, legacy_cursor, 2).unwrap();
        assert!(hot_pass.iter().all(|candidate| candidate.tier == Tier::Hot));
        let persisted: LifecycleCursor =
            serde_json::from_str(&hot_pass.last().unwrap().cursor_after).unwrap();
        assert_eq!(persisted.next_tier, 1);

        let next_pass = lifecycle_candidates(&db, 0, 0, 0, persisted, 2).unwrap();
        assert_eq!(
            next_pass
                .iter()
                .map(|candidate| candidate.tier)
                .collect::<Vec<_>>(),
            vec![Tier::Warm, Tier::Cold]
        );
    }
}

#[derive(Clone, Copy, Debug)]
struct StorageStats {
    page_count: u64,
    freelist_pages: u64,
    allocated_bytes: u64,
}

fn storage_stats(
    db: &rusqlite::Connection,
    database_path: &std::path::Path,
) -> Result<StorageStats, StoreError> {
    let page_count = db.query_row("PRAGMA page_count", [], |row| row.get::<_, i64>(0))?;
    let freelist_pages = db.query_row("PRAGMA freelist_count", [], |row| row.get::<_, i64>(0))?;
    let metadata = fs::metadata(database_path)?;
    Ok(StorageStats {
        page_count: u64::try_from(page_count).map_err(|_| StoreError::SchemaMismatch)?,
        freelist_pages: u64::try_from(freelist_pages).map_err(|_| StoreError::SchemaMismatch)?,
        allocated_bytes: allocated_bytes(&metadata),
    })
}

#[cfg(unix)]
fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}
