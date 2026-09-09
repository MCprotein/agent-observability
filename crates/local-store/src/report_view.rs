//! Bounded private staging projection for the paged local report view.

use super::report_view_catalog::{
    managed_report_view_directory, prepare_managed_report_view_directory, source_store_identity,
};
use super::{LocalStore, ReportRenderGuard, StoreError, private_create_new, private_file};
use agent_observability_application::{
    RateTable, ReportProjectionError, project_owned_report_span, resolve_report_repository,
};
use agent_observability_contracts::ReportSpanV2;
use agent_observability_domain::{SpanKind, StatusCode};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const REPORT_VIEW_SCHEMA_VERSION: &str = "agent_observability.report_view_staging.v2";
const STAGING_FILE_PREFIX: &str = ".report-view.sqlite3.staging.";
/// Finite per-generation database plus rollback-journal disk ceiling.
pub const MAX_REPORT_VIEW_BYTES: u64 = 256 * 1024 * 1024;
const SQLITE_PAGE_BYTES: u64 = 4096;
const SQLITE_CACHE_KIB: i64 = 8 * 1024;
const SQLITE_WRITE_HEADROOM_BYTES: u64 = 8 * 1024 * 1024;
const WRITE_BATCH_RECORDS: usize = 128;
const WRITE_BATCH_BYTES: u64 = 2 * 1024 * 1024;
const MAX_STAGING_FILE_COLLISIONS: usize = 64;
pub const MISSING_RATE_FINGERPRINT: &str = "missing_rate_table";
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct CompleteMetadata<'a> {
    generation: u64,
    visibility_epoch: u64,
    rate_fingerprint: &'a str,
    generated_at: &'a str,
    records: usize,
}

struct ProjectionRow {
    index: usize,
    span_json: String,
}

const CREATE_SCHEMA: &str = r"
CREATE TABLE metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;
CREATE TABLE spans (
    source_order INTEGER PRIMARY KEY,
    trace_id TEXT NOT NULL,
    span_id TEXT NOT NULL UNIQUE,
    start_time_unix_ms REAL NOT NULL,
    repo TEXT NOT NULL,
    session_id TEXT,
    turn_id TEXT,
    agent TEXT,
    model TEXT,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    span_json TEXT NOT NULL
);
CREATE INDEX spans_stable_order_idx
    ON spans(start_time_unix_ms, trace_id, span_id);
CREATE INDEX spans_trace_repo_idx
    ON spans(trace_id, repo);
CREATE INDEX spans_trace_order_idx
    ON spans(trace_id, start_time_unix_ms, span_id);
CREATE INDEX spans_repo_order_idx
    ON spans(repo, source_order);
CREATE INDEX spans_session_order_idx
    ON spans(session_id, source_order);
CREATE INDEX spans_turn_order_idx
    ON spans(turn_id, source_order);
CREATE INDEX spans_agent_order_idx
    ON spans(agent, source_order);
CREATE INDEX spans_model_order_idx
    ON spans(model, source_order);
";

/// Failure while constructing a private report-view staging database.
#[derive(Debug)]
pub enum ReportViewBuildError {
    Store(StoreError),
    Projection(ReportProjectionError),
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    Io(io::Error),
    InvalidByteBudget,
    InvalidRateFingerprint,
    CapacityExceeded,
    Busy,
    SnapshotChanged,
    InvalidStagingState,
    CoordinationDenied,
}

impl Display for ReportViewBuildError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Store(_) => "report view authority read failed",
            Self::Projection(_) => "report view privacy projection failed",
            Self::Sqlite(_) => "report view staging database failed",
            Self::Json(_) => "report view staging JSON failed",
            Self::Io(_) => "report view staging filesystem failed",
            Self::InvalidByteBudget => "report view staging byte budget is invalid",
            Self::InvalidRateFingerprint => "report view rate fingerprint is invalid",
            Self::CapacityExceeded => "report view staging byte capacity was exceeded",
            Self::Busy => "another report view staging or publication operation is active",
            Self::SnapshotChanged => "report view source generation changed during construction",
            Self::InvalidStagingState => "report view staging state is invalid",
            Self::CoordinationDenied => "report view storage coordination permit was denied",
        })
    }
}

/// A bounded report-view write phase requiring composition-level coordination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportViewWritePhase {
    Initialize,
    ProjectionBatch,
    RepositoryResolutionBatch,
    FinalMetadata,
    Discard,
}

/// Supplies an owned shared permit for one bounded report-view write phase.
///
/// The store intentionally knows nothing about the runtime barrier. The composition root owns the
/// concrete factory and maps a denied runtime acquisition to [`ReportViewBuildError`]. Each permit
/// is dropped only after the corresponding commit, rollback, close, and rollback-journal cleanup.
pub trait ReportViewPermitFactory {
    type Permit;

    /// Acquires one shared permit. Returning an error must not mutate the staging database.
    ///
    /// # Errors
    /// Returns a build error when the composition root cannot authorize this write phase.
    fn acquire(
        &mut self,
        phase: ReportViewWritePhase,
    ) -> Result<Self::Permit, ReportViewBuildError>;

    /// Revalidates the coordination state after the write phase has fully cleaned up.
    ///
    /// # Errors
    /// Returns a build error when publication or the next write phase is no longer authorized.
    fn revalidate(&mut self, permit: &Self::Permit) -> Result<(), ReportViewBuildError>;
}

struct NoopReportViewPermitFactory;

impl ReportViewPermitFactory for NoopReportViewPermitFactory {
    type Permit = ();

    fn acquire(
        &mut self,
        _phase: ReportViewWritePhase,
    ) -> Result<Self::Permit, ReportViewBuildError> {
        Ok(())
    }

    fn revalidate(&mut self, _permit: &Self::Permit) -> Result<(), ReportViewBuildError> {
        Ok(())
    }
}

impl std::error::Error for ReportViewBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            Self::Projection(error) => Some(error),
            Self::Sqlite(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<StoreError> for ReportViewBuildError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<rusqlite::Error> for ReportViewBuildError {
    fn from(error: rusqlite::Error) -> Self {
        if matches!(
            error.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DiskFull)
        ) {
            Self::CapacityExceeded
        } else {
            Self::Sqlite(error)
        }
    }
}

impl From<serde_json::Error> for ReportViewBuildError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<io::Error> for ReportViewBuildError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// An unpublished report-view database protected by the report publication guard.
///
/// This handle intentionally keeps its connection open only while staging is owned by the
/// publication path. Queries must reopen an immutable published file per request instead of
/// retaining this connection as a lease.
#[derive(Debug)]
pub struct ReportViewStaging {
    path: PathBuf,
    identity_file: fs::File,
    directory: fs::File,
    connection: Option<Connection>,
    publication_guard: Option<ReportRenderGuard>,
    generation: u64,
    visibility_epoch: u64,
    records: usize,
    rate_fingerprint: String,
    source_identity: String,
    generated_at: String,
    cleanup_on_drop: bool,
}

impl ReportViewStaging {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The original create-new descriptor, retained through construction and publication.
    /// Used by the composition root to validate its reservation binding; this is not a lease
    /// on a published snapshot or permission to change the file.
    #[must_use]
    pub fn identity_file(&self) -> &fs::File {
        &self.identity_file
    }

    fn validate_identity(&self) -> Result<(), ReportViewBuildError> {
        private_file(&self.path)?;
        let parent = self
            .path
            .parent()
            .ok_or(ReportViewBuildError::InvalidStagingState)?;
        let named_parent = fs::symlink_metadata(parent)?;
        let directory = self.directory.metadata()?;
        if !directory.is_dir() || !named_parent.is_dir() || named_parent.file_type().is_symlink() {
            return Err(ReportViewBuildError::InvalidStagingState);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let held = self.identity_file.metadata()?;
            let named = fs::symlink_metadata(&self.path)?;
            if !held.is_file()
                || held.mode() & 0o7777 != 0o600
                || held.nlink() != 1
                || (held.dev(), held.ino()) != (named.dev(), named.ino())
                || (directory.dev(), directory.ino()) != (named_parent.dev(), named_parent.ino())
                || named_parent.mode() & 0o7777 != 0o700
            {
                return Err(ReportViewBuildError::InvalidStagingState);
            }
        }
        Ok(())
    }

    pub(crate) fn connection(&self) -> &Connection {
        self.connection
            .as_ref()
            .expect("staging connection is present until drop")
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn visibility_epoch(&self) -> u64 {
        self.visibility_epoch
    }

    #[must_use]
    pub const fn records(&self) -> usize {
        self.records
    }

    #[must_use]
    pub fn rate_fingerprint(&self) -> &str {
        &self.rate_fingerprint
    }

    #[must_use]
    pub fn generated_at(&self) -> &str {
        &self.generated_at
    }

    pub(crate) fn source_identity(&self) -> &str {
        &self.source_identity
    }

    /// Closes and synchronizes the complete staging database while retaining its path and guard.
    ///
    /// # Errors
    ///
    /// Returns [`ReportViewBuildError`] if `SQLite` cannot close cleanly or the private staging
    /// file cannot be validated and synchronized for a later guarded publication step.
    pub(crate) fn close_for_publication(&mut self) -> Result<(), ReportViewBuildError> {
        if let Some(connection) = self.connection.take()
            && let Err((connection, error)) = connection.close()
        {
            self.connection = Some(connection);
            return Err(error.into());
        }
        self.validate_identity()?;
        self.identity_file.sync_all()?;
        Ok(())
    }

    /// Closes and removes coordinated staging while holding a composition-supplied permit.
    ///
    /// If permit acquisition fails, the connection, staging file, and publication guard remain
    /// owned by this handle. Coordinated handles never unlink their staging path from [`Drop`].
    ///
    /// # Errors
    ///
    /// Returns an acquisition, close, identity, synchronization, or removal error. A failure does
    /// not authorize releasing any external full-build reservation.
    pub fn discard_with_permit<P: ReportViewPermitFactory>(
        &mut self,
        permits: &mut P,
    ) -> Result<(), ReportViewBuildError> {
        let permit = permits.acquire(ReportViewWritePhase::Discard)?;
        let result = (|| {
            close_staging_connection(self)?;
            self.validate_identity()?;
            fs::remove_file(&self.path)?;
            self.directory.sync_all()?;
            self.cleanup_on_drop = false;
            Ok(())
        })();
        let revalidation = permits.revalidate(&permit);
        match result {
            Ok(()) => revalidation,
            Err(error) => Err(error),
        }
    }
}

impl Drop for ReportViewStaging {
    fn drop(&mut self) {
        // Every coordinated transaction borrows this handle through `ActiveWrite`, whose Drop
        // rolls back (and closes on unwind/failure) before releasing its permit. Therefore a
        // remaining connection is idle: closing it cannot perform transaction or journal cleanup.
        if let Some(connection) = self.connection.take() {
            let _ = connection.close();
        }
        if !self.cleanup_on_drop {
            drop(self.publication_guard.take());
            return;
        }
        // A rejected callback or external replacement must not make Drop delete another file.
        if self.validate_identity().is_ok() {
            let _ = fs::remove_file(&self.path);
        }
        drop(self.publication_guard.take());
    }
}

/// Builds a complete, unpublished report-view projection under the publication guard.
///
/// The caller explicitly reserves no more than 256 MiB for the database plus rollback-journal
/// headroom. The fixed managed directory is derived from the local store, binding the staging file
/// to that authority. The database page ceiling excludes a fixed
/// 8 MiB write reserve, write transactions are bounded, indexes are created while empty, and
/// `SQLite` temporary tables remain in memory only for bounded queries. The returned handle owns
/// both the staging connection and publication guard; dropping it closes `SQLite` before unlinking
/// the file and releasing the guard. This per-generation admission does not reserve the
/// catalog-wide current, retired, and staging total; the caller must reserve that aggregate local
/// storage budget before starting construction.
///
/// # Errors
///
/// Returns [`ReportViewBuildError`] when admission, privacy projection, authority fencing,
/// filesystem safety, or staging database construction fails.
pub fn build_report_view_staging(
    store: &LocalStore,
    rate_fingerprint: &str,
    admitted_bytes: u64,
    rates: Option<&RateTable>,
) -> Result<ReportViewStaging, ReportViewBuildError> {
    build_report_view_staging_observing(store, rate_fingerprint, admitted_bytes, rates, |_| {})
}

/// Builds staging while observing each successfully projected source-record index.
///
/// The callback runs outside the authoritative local-store read transaction and is intended for
/// deterministic contention and generation-fence orchestration. It must not retain record data;
/// the callback receives only the stable source index.
///
/// # Errors
///
/// Returns [`ReportViewBuildError`] under the same bounded admission, privacy, and fencing rules
/// as [`build_report_view_staging`].
pub fn build_report_view_staging_observing(
    store: &LocalStore,
    rate_fingerprint: &str,
    admitted_bytes: u64,
    rates: Option<&RateTable>,
    after_project: impl FnMut(usize),
) -> Result<ReportViewStaging, ReportViewBuildError> {
    build_report_view_staging_bound(
        store,
        rate_fingerprint,
        admitted_bytes,
        rates,
        |_, _| Ok(()),
        after_project,
    )
}

/// Binds a newly created, empty staging file before opening its `SQLite` connection.
///
/// The publication guard is held during `before_write`. The composition root may durably
/// bind its runtime reservation and release its mutation guard there. Returning an error
/// prevents staging database initialization and source projection. Catalog-directory preparation
/// precedes the callback; this API is not a complete filesystem-accounting snapshot.
/// The store retains the original descriptor and rechecks its identity after the callback.
///
/// # Errors
///
/// Returns a callback error or the same validation/build errors as [`build_report_view_staging`].
pub fn build_report_view_staging_bound(
    store: &LocalStore,
    rate_fingerprint: &str,
    admitted_bytes: u64,
    rates: Option<&RateTable>,
    before_write: impl FnOnce(&Path, &fs::File) -> Result<(), ReportViewBuildError>,
    after_project: impl FnMut(usize),
) -> Result<ReportViewStaging, ReportViewBuildError> {
    let mut permits = NoopReportViewPermitFactory;
    build_report_view_staging_bound_impl(
        store,
        rate_fingerprint,
        admitted_bytes,
        rates,
        before_write,
        &mut permits,
        after_project,
        true,
    )
}

/// Builds bound staging with composition-supplied permits around every bounded write phase.
///
/// The existing publication guard remains held across the complete build; the binding callback
/// runs once before initialization and may release the caller's initial mutation scope.
/// A coordinated handle never performs implicit path cleanup in [`Drop`]; callers must use
/// [`ReportViewStaging::discard_with_permit`] when abandoning it. Failed permit acquisition leaves
/// staging and any composition-owned full-build reservation intact for guarded recovery.
///
/// # Errors
///
/// Returns a permit acquisition error or the same validation/build errors as
/// [`build_report_view_staging_bound`].
pub fn build_report_view_staging_bound_coordinated<P: ReportViewPermitFactory>(
    store: &LocalStore,
    rate_fingerprint: &str,
    admitted_bytes: u64,
    rates: Option<&RateTable>,
    before_write: impl FnOnce(&Path, &fs::File) -> Result<(), ReportViewBuildError>,
    permits: &mut P,
    after_project: impl FnMut(usize),
) -> Result<ReportViewStaging, ReportViewBuildError> {
    build_report_view_staging_bound_impl(
        store,
        rate_fingerprint,
        admitted_bytes,
        rates,
        before_write,
        permits,
        after_project,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_report_view_staging_bound_impl<P: ReportViewPermitFactory>(
    store: &LocalStore,
    rate_fingerprint: &str,
    admitted_bytes: u64,
    rates: Option<&RateTable>,
    before_write: impl FnOnce(&Path, &fs::File) -> Result<(), ReportViewBuildError>,
    permits: &mut P,
    mut after_project: impl FnMut(usize),
    cleanup_on_drop: bool,
) -> Result<ReportViewStaging, ReportViewBuildError> {
    let mut staging = create_staging_with_permits(
        store,
        rate_fingerprint,
        admitted_bytes,
        before_write,
        permits,
        cleanup_on_drop,
    )?;
    let visibility_epoch = staging.visibility_epoch;

    let mut pending_error = None;
    let mut batch = Vec::with_capacity(WRITE_BATCH_RECORDS);
    let mut batch_bytes = 0_u64;
    let visit = store.visit_report_snapshot_bounded(|index, record| {
        if pending_error.is_some() {
            return;
        }
        let result = (|| {
            let span = project_owned_report_span(index, record, rates)
                .map_err(ReportViewBuildError::Projection)?;
            span.validate().map_err(|error| {
                ReportViewBuildError::Projection(ReportProjectionError::InvalidReport(error))
            })?;
            let span_json = serde_json::to_string(&span)?;
            let span_bytes = u64::try_from(span_json.len())
                .map_err(|_| ReportViewBuildError::CapacityExceeded)?;
            if span_bytes > WRITE_BATCH_BYTES {
                return Err(ReportViewBuildError::CapacityExceeded);
            }
            if !batch.is_empty()
                && (batch.len() == WRITE_BATCH_RECORDS
                    || batch_bytes
                        .checked_add(span_bytes)
                        .is_none_or(|bytes| bytes > WRITE_BATCH_BYTES))
            {
                write_projection_batch(
                    &mut staging,
                    &batch,
                    admitted_bytes,
                    permits,
                    &mut after_project,
                )?;
                batch.clear();
                batch_bytes = 0;
            }
            batch.push(ProjectionRow { index, span_json });
            batch_bytes = batch_bytes
                .checked_add(span_bytes)
                .ok_or(ReportViewBuildError::CapacityExceeded)?;
            Ok(())
        })();
        if let Err(error) = result {
            pending_error = Some(error);
        }
    });

    if let Some(error) = pending_error {
        return Err(error);
    }
    let visit = visit?;
    if !batch.is_empty() {
        write_projection_batch(
            &mut staging,
            &batch,
            admitted_bytes,
            permits,
            &mut after_project,
        )?;
    }
    enforce_storage_budget(staging.path(), admitted_bytes)?;

    resolve_unknown_repositories(&mut staging, admitted_bytes, permits)?;
    if !source_snapshot_is_current(store, visit.generation, visibility_epoch)? {
        return Err(ReportViewBuildError::SnapshotChanged);
    }
    let generated_at = staging.generated_at().to_owned();
    let complete_metadata = CompleteMetadata {
        generation: visit.generation,
        visibility_epoch,
        rate_fingerprint,
        generated_at: &generated_at,
        records: visit.records,
    };
    write_complete_metadata(&mut staging, &complete_metadata, admitted_bytes, permits)?;
    enforce_storage_budget(staging.path(), admitted_bytes)?;
    if !source_snapshot_is_current(store, visit.generation, visibility_epoch)? {
        return Err(ReportViewBuildError::SnapshotChanged);
    }
    verify_complete_staging(staging.connection(), &complete_metadata)?;

    staging.generation = visit.generation;
    staging.records = visit.records;
    Ok(staging)
}

#[cfg(test)]
fn create_staging(
    store: &LocalStore,
    rate_fingerprint: &str,
    admitted_bytes: u64,
    before_write: impl FnOnce(&Path, &fs::File) -> Result<(), ReportViewBuildError>,
) -> Result<ReportViewStaging, ReportViewBuildError> {
    let mut permits = NoopReportViewPermitFactory;
    create_staging_with_permits(
        store,
        rate_fingerprint,
        admitted_bytes,
        before_write,
        &mut permits,
        true,
    )
}

fn create_staging_with_permits<P: ReportViewPermitFactory>(
    store: &LocalStore,
    rate_fingerprint: &str,
    admitted_bytes: u64,
    before_write: impl FnOnce(&Path, &fs::File) -> Result<(), ReportViewBuildError>,
    permits: &mut P,
    cleanup_on_drop: bool,
) -> Result<ReportViewStaging, ReportViewBuildError> {
    validate_build_inputs(rate_fingerprint, admitted_bytes)?;
    let publication_guard = store
        .try_acquire_report_render_guard()?
        .ok_or(ReportViewBuildError::Busy)?;
    let visibility_epoch = store.report_visibility_epoch()?;
    let directory = managed_report_view_directory(store)?;
    prepare_managed_report_view_directory(store, &directory, visibility_epoch)?;
    let source_identity = source_store_identity(store)?;
    let generated_at = trusted_generated_at()?;
    let directory_handle = fs::File::open(&directory)?;
    let (path, identity_file) = create_staging_file(&directory)?;
    let mut staging = ReportViewStaging {
        path,
        identity_file,
        directory: directory_handle,
        connection: None,
        publication_guard: Some(publication_guard),
        generation: 0,
        visibility_epoch,
        records: 0,
        rate_fingerprint: rate_fingerprint.to_owned(),
        source_identity,
        generated_at,
        cleanup_on_drop,
    };
    staging.validate_identity()?;
    before_write(staging.path(), staging.identity_file())?;
    staging.validate_identity()?;
    if staging.identity_file.metadata()?.len() != 0 {
        return Err(ReportViewBuildError::InvalidStagingState);
    }
    let permit = permits.acquire(ReportViewWritePhase::Initialize)?;
    let result = (|| {
        let connection = match open_staging_connection(staging.path(), admitted_bytes) {
            Ok(connection) => connection,
            Err(error) => {
                ensure_rollback_journal_clean(staging.path())?;
                return Err(error);
            }
        };
        if let Err(error) = connection.execute_batch(CREATE_SCHEMA) {
            close_connection(connection, staging.path())?;
            return Err(error.into());
        }
        if let Err(error) = enforce_storage_budget(staging.path(), admitted_bytes) {
            close_connection(connection, staging.path())?;
            return Err(error);
        }
        ensure_rollback_journal_clean(staging.path())?;
        staging.connection = Some(connection);
        Ok(())
    })();
    let revalidation = permits.revalidate(&permit);
    match result {
        Ok(()) => revalidation?,
        Err(error) => return Err(error),
    }
    Ok(staging)
}

fn source_snapshot_is_current(
    store: &LocalStore,
    generation: u64,
    visibility_epoch: u64,
) -> Result<bool, ReportViewBuildError> {
    Ok(store.report_status()?.generation == generation
        && store.report_visibility_epoch()? == visibility_epoch)
}

fn validate_build_inputs(
    rate_fingerprint: &str,
    admitted_bytes: u64,
) -> Result<(), ReportViewBuildError> {
    let admitted_range =
        SQLITE_WRITE_HEADROOM_BYTES + (4 * SQLITE_PAGE_BYTES)..=MAX_REPORT_VIEW_BYTES;
    if !admitted_range.contains(&admitted_bytes) {
        return Err(ReportViewBuildError::InvalidByteBudget);
    }
    let digest = rate_fingerprint.len() == 64
        && rate_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if rate_fingerprint != MISSING_RATE_FINGERPRINT && !digest {
        return Err(ReportViewBuildError::InvalidRateFingerprint);
    }
    Ok(())
}

fn create_staging_file(directory: &Path) -> Result<(PathBuf, fs::File), ReportViewBuildError> {
    for _ in 0..MAX_STAGING_FILE_COLLISIONS {
        let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            "{STAGING_FILE_PREFIX}{}.{sequence}",
            std::process::id()
        ));
        match private_create_new(&path) {
            Ok(file) => {
                file.sync_all()?;
                return Ok((path, file));
            }
            Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(ReportViewBuildError::InvalidStagingState)
}

fn open_staging_connection(
    path: &Path,
    admitted_bytes: u64,
) -> Result<Connection, ReportViewBuildError> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    let page_bytes =
        i64::try_from(SQLITE_PAGE_BYTES).map_err(|_| ReportViewBuildError::InvalidByteBudget)?;
    connection.pragma_update(None, "page_size", page_bytes)?;
    let database_bytes = admitted_bytes
        .checked_sub(SQLITE_WRITE_HEADROOM_BYTES)
        .ok_or(ReportViewBuildError::InvalidByteBudget)?;
    let max_pages = i64::try_from(database_bytes / SQLITE_PAGE_BYTES)
        .map_err(|_| ReportViewBuildError::InvalidByteBudget)?;
    connection.pragma_update(None, "max_page_count", max_pages)?;
    connection.pragma_update(None, "cache_size", -SQLITE_CACHE_KIB)?;
    connection.pragma_update(None, "journal_mode", "DELETE")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    let journal_size_limit = i64::try_from(SQLITE_WRITE_HEADROOM_BYTES)
        .map_err(|_| ReportViewBuildError::InvalidByteBudget)?;
    connection.pragma_update(None, "journal_size_limit", journal_size_limit)?;
    connection.pragma_update(None, "temp_store", "MEMORY")?;
    connection.pragma_update(None, "foreign_keys", true)?;
    let actual_max_pages: i64 =
        connection.pragma_query_value(None, "max_page_count", |row| row.get(0))?;
    if actual_max_pages != max_pages {
        return Err(ReportViewBuildError::InvalidStagingState);
    }
    Ok(connection)
}

fn insert_span(
    connection: &Connection,
    index: usize,
    span: &ReportSpanV2,
    span_json: &str,
) -> Result<(), ReportViewBuildError> {
    let source_order = i64::try_from(index).map_err(|_| ReportViewBuildError::CapacityExceeded)?;
    connection.prepare_cached(
        "INSERT INTO spans(source_order, trace_id, span_id, start_time_unix_ms, repo, session_id, turn_id, agent, model, kind, status, span_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
    )?.execute(params![
            source_order,
            span.trace_id,
            span.span_id,
            span.start_time_unix_ms,
            span.repo,
            span.session_id,
            span.turn_id,
            span.agent.name.as_deref().unwrap_or("unknown"),
            span.agent.model.as_deref().unwrap_or("unknown"),
            kind_name(span.kind),
            status_name(span.status),
            span_json
        ])?;
    Ok(())
}

fn write_projection_batch<P: ReportViewPermitFactory>(
    staging: &mut ReportViewStaging,
    batch: &[ProjectionRow],
    admitted_bytes: u64,
    permits: &mut P,
    after_project: &mut impl FnMut(usize),
) -> Result<(), ReportViewBuildError> {
    let path = staging.path().to_owned();
    with_write_transaction(
        staging,
        permits,
        ReportViewWritePhase::ProjectionBatch,
        |connection| {
            for row in batch {
                let span: ReportSpanV2 = serde_json::from_str(&row.span_json)?;
                insert_span(connection, row.index, &span, &row.span_json)?;
                after_project(row.index);
            }
            enforce_storage_budget(&path, admitted_bytes)
        },
    )
}

fn resolve_unknown_repositories<P: ReportViewPermitFactory>(
    staging: &mut ReportViewStaging,
    admitted_bytes: u64,
    permits: &mut P,
) -> Result<(), ReportViewBuildError> {
    let mut last_source_order = -1_i64;
    loop {
        let batch = {
            let mut statement = staging.connection().prepare(
                "SELECT source_order, trace_id, repo, span_json FROM spans WHERE source_order>?1 ORDER BY source_order LIMIT ?2",
            )?;
            let batch_limit = i64::try_from(WRITE_BATCH_RECORDS)
                .map_err(|_| ReportViewBuildError::InvalidStagingState)?;
            let mut rows = statement.query(params![last_source_order, batch_limit])?;
            let mut batch = Vec::new();
            let mut decoded_bytes = 0_u64;
            while let Some(row) = rows.next()? {
                let span_json = row.get::<_, String>(3)?;
                let row_bytes = u64::try_from(span_json.len())
                    .map_err(|_| ReportViewBuildError::CapacityExceeded)?;
                if row_bytes > WRITE_BATCH_BYTES {
                    return Err(ReportViewBuildError::CapacityExceeded);
                }
                let next_bytes = decoded_bytes
                    .checked_add(row_bytes)
                    .ok_or(ReportViewBuildError::CapacityExceeded)?;
                if !batch.is_empty() && next_bytes > WRITE_BATCH_BYTES {
                    break;
                }
                decoded_bytes = next_bytes;
                batch.push((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    span_json,
                ));
            }
            batch
        };
        if batch.is_empty() {
            break;
        }

        let path = staging.path().to_owned();
        with_write_transaction(
            staging,
            permits,
            ReportViewWritePhase::RepositoryResolutionBatch,
            |connection| {
                let mut repositories = connection.prepare(
                    "SELECT DISTINCT repo FROM spans WHERE trace_id=?1 AND repo!='unknown' LIMIT 2",
                )?;
                let mut update = connection
                    .prepare("UPDATE spans SET repo=?1, span_json=?2 WHERE source_order=?3")?;
                for (source_order, trace_id, repo, span_json) in &batch {
                    if repo != "unknown" {
                        continue;
                    }
                    let known = repositories
                        .query_map([trace_id], |row| row.get::<_, String>(0))?
                        .collect::<Result<Vec<_>, _>>()?;
                    let mut span: ReportSpanV2 = serde_json::from_str(span_json)?;
                    resolve_report_repository(&mut span, known.iter().map(String::as_str));
                    span.validate().map_err(|error| {
                        ReportViewBuildError::Projection(ReportProjectionError::InvalidReport(
                            error,
                        ))
                    })?;
                    let resolved_json = serde_json::to_string(&span)?;
                    update.execute(params![span.repo, resolved_json, source_order])?;
                }
                enforce_storage_budget(&path, admitted_bytes)
            },
        )?;
        last_source_order = batch
            .last()
            .map(|row| row.0)
            .ok_or(ReportViewBuildError::InvalidStagingState)?;
        enforce_storage_budget(staging.path(), admitted_bytes)?;
    }
    Ok(())
}

fn write_complete_metadata<P: ReportViewPermitFactory>(
    staging: &mut ReportViewStaging,
    metadata: &CompleteMetadata<'_>,
    admitted_bytes: u64,
    permits: &mut P,
) -> Result<(), ReportViewBuildError> {
    let records =
        u64::try_from(metadata.records).map_err(|_| ReportViewBuildError::CapacityExceeded)?;
    let path = staging.path().to_owned();
    with_write_transaction(
        staging,
        permits,
        ReportViewWritePhase::FinalMetadata,
        |connection| {
            let mut insert =
                connection.prepare("INSERT INTO metadata(key, value) VALUES (?1, ?2)")?;
            for (key, value) in [
                ("schema_version", REPORT_VIEW_SCHEMA_VERSION.to_owned()),
                ("source_generation", metadata.generation.to_string()),
                ("visibility_epoch", metadata.visibility_epoch.to_string()),
                ("rate_fingerprint", metadata.rate_fingerprint.to_owned()),
                ("generated_at", metadata.generated_at.to_owned()),
                ("records", records.to_string()),
                ("complete", "1".to_owned()),
            ] {
                insert.execute(params![key, value])?;
            }
            enforce_storage_budget(&path, admitted_bytes)
        },
    )
}

struct ActiveWrite<'staging, 'permit, Permit> {
    staging: &'staging mut ReportViewStaging,
    _permit: &'permit Permit,
    transaction_active: bool,
}

impl<'staging, 'permit, Permit> ActiveWrite<'staging, 'permit, Permit> {
    fn begin(
        staging: &'staging mut ReportViewStaging,
        permit: &'permit Permit,
    ) -> Result<Self, ReportViewBuildError> {
        let mut write = Self {
            staging,
            _permit: permit,
            transaction_active: false,
        };
        if let Err(error) = write.staging.connection().execute_batch("BEGIN IMMEDIATE") {
            write.close_connection()?;
            return Err(error.into());
        }
        write.transaction_active = true;
        Ok(write)
    }

    fn connection(&self) -> &Connection {
        self.staging.connection()
    }

    fn commit(mut self) -> Result<(), ReportViewBuildError> {
        if let Err(error) = self.staging.connection().execute_batch("COMMIT") {
            self.close_connection()?;
            return Err(error.into());
        }
        self.transaction_active = false;
        if let Err(error) = ensure_rollback_journal_clean(self.staging.path()) {
            self.close_connection()?;
            return Err(error);
        }
        Ok(())
    }

    fn rollback(mut self) -> Result<(), ReportViewBuildError> {
        if let Err(error) = self.staging.connection().execute_batch("ROLLBACK") {
            self.close_connection()?;
            return Err(error.into());
        }
        self.transaction_active = false;
        if let Err(error) = ensure_rollback_journal_clean(self.staging.path()) {
            self.close_connection()?;
            return Err(error);
        }
        Ok(())
    }

    fn close_connection(&mut self) -> Result<(), ReportViewBuildError> {
        self.transaction_active = false;
        close_staging_connection(self.staging)
    }
}

impl<Permit> Drop for ActiveWrite<'_, '_, Permit> {
    fn drop(&mut self) {
        if self.transaction_active {
            let rolled_back = self.staging.connection().execute_batch("ROLLBACK").is_ok()
                && ensure_rollback_journal_clean(self.staging.path()).is_ok();
            self.transaction_active = false;
            if !rolled_back || std::thread::panicking() {
                let _ = close_staging_connection(self.staging);
            }
        }
    }
}

fn with_write_transaction<P, T>(
    staging: &mut ReportViewStaging,
    permits: &mut P,
    phase: ReportViewWritePhase,
    operation: impl FnOnce(&Connection) -> Result<T, ReportViewBuildError>,
) -> Result<T, ReportViewBuildError>
where
    P: ReportViewPermitFactory,
{
    let permit = permits.acquire(phase)?;
    let result = (|| {
        let write = ActiveWrite::begin(staging, &permit)?;
        match operation(write.connection()) {
            Ok(value) => {
                write.commit()?;
                Ok(value)
            }
            Err(error) => {
                write.rollback()?;
                Err(error)
            }
        }
    })();
    let revalidation = permits.revalidate(&permit);
    match result {
        Ok(value) => {
            revalidation?;
            Ok(value)
        }
        Err(error) => Err(error),
    }
}

fn close_staging_connection(staging: &mut ReportViewStaging) -> Result<(), ReportViewBuildError> {
    if let Some(connection) = staging.connection.take() {
        close_connection(connection, staging.path())?;
    }
    ensure_rollback_journal_clean(staging.path())
}

fn close_connection(connection: Connection, path: &Path) -> Result<(), ReportViewBuildError> {
    let close_error = match connection.close() {
        Ok(()) => None,
        Err((connection, error)) => {
            drop(connection);
            Some(error)
        }
    };
    ensure_rollback_journal_clean(path)?;
    match close_error {
        Some(error) => Err(error.into()),
        None => Ok(()),
    }
}

fn ensure_rollback_journal_clean(path: &Path) -> Result<(), ReportViewBuildError> {
    let mut journal_name = path.as_os_str().to_owned();
    journal_name.push("-journal");
    match fs::symlink_metadata(PathBuf::from(journal_name)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => Err(ReportViewBuildError::InvalidStagingState),
    }
}

fn trusted_generated_at() -> Result<String, ReportViewBuildError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ReportViewBuildError::InvalidStagingState)?;
    let seconds = duration.as_secs();
    let days =
        i64::try_from(seconds / 86_400).map_err(|_| ReportViewBuildError::InvalidStagingState)?;
    let seconds_of_day = seconds % 86_400;
    let (year, month, day) = civil_date_from_unix_days(days)?;
    let hour = seconds_of_day / 3_600;
    let minute = seconds_of_day % 3_600 / 60;
    let second = seconds_of_day % 60;
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        duration.subsec_millis()
    ))
}

fn civil_date_from_unix_days(days: i64) -> Result<(i64, i64, i64), ReportViewBuildError> {
    let shifted = days
        .checked_add(719_468)
        .ok_or(ReportViewBuildError::InvalidStagingState)?;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    if !(0..=9_999).contains(&year) {
        return Err(ReportViewBuildError::InvalidStagingState);
    }
    Ok((year, month, day))
}

fn verify_complete_staging(
    connection: &Connection,
    metadata: &CompleteMetadata<'_>,
) -> Result<(), ReportViewBuildError> {
    for (key, expected) in [
        ("schema_version", REPORT_VIEW_SCHEMA_VERSION.to_owned()),
        ("source_generation", metadata.generation.to_string()),
        ("visibility_epoch", metadata.visibility_epoch.to_string()),
        ("rate_fingerprint", metadata.rate_fingerprint.to_owned()),
        ("generated_at", metadata.generated_at.to_owned()),
        ("records", metadata.records.to_string()),
        ("complete", "1".to_owned()),
    ] {
        let actual: Option<String> = connection
            .query_row("SELECT value FROM metadata WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        if actual.as_deref() != Some(expected.as_str()) {
            return Err(ReportViewBuildError::InvalidStagingState);
        }
    }
    let projected_records: i64 =
        connection.query_row("SELECT COUNT(*) FROM spans", [], |row| row.get(0))?;
    if usize::try_from(projected_records).ok() != Some(metadata.records) {
        return Err(ReportViewBuildError::InvalidStagingState);
    }
    Ok(())
}

fn enforce_storage_budget(path: &Path, admitted_bytes: u64) -> Result<(), ReportViewBuildError> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(ReportViewBuildError::Store(StoreError::Symlink));
    }
    let database_bytes = fs::metadata(path)?.len();
    let mut journal_name = path.as_os_str().to_owned();
    journal_name.push("-journal");
    let journal_path = PathBuf::from(journal_name);
    let journal_bytes = match fs::symlink_metadata(&journal_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(ReportViewBuildError::Store(StoreError::Symlink));
        }
        Ok(metadata) if metadata.is_file() => metadata.len(),
        Ok(_) => return Err(ReportViewBuildError::Store(StoreError::InvalidPath)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
    };
    if database_bytes
        .checked_add(journal_bytes)
        .is_none_or(|bytes| bytes > admitted_bytes)
    {
        return Err(ReportViewBuildError::CapacityExceeded);
    }
    Ok(())
}

const fn kind_name(kind: SpanKind) -> &'static str {
    match kind {
        SpanKind::Workstream => "workstream",
        SpanKind::AgentSession => "agent_session",
        SpanKind::Turn => "turn",
        SpanKind::LlmRequest => "llm_request",
        SpanKind::ToolExecution => "tool_execution",
        SpanKind::Permission => "permission",
        SpanKind::Compaction => "compaction",
    }
}

const fn status_name(status: StatusCode) -> &'static str {
    match status {
        StatusCode::Unset => "unset",
        StatusCode::Ok => "ok",
        StatusCode::Error => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_observability_contracts::{
        AgentSource, JsonValue, ObservationEvent, SourceObservation, hash_opaque_identifier,
    };
    use agent_observability_domain::{
        CorrelationIds, LifecycleState, ObservationId, SourceCursor, SourceGeneration, SpanId,
        Timing, TokenUsage, TraceId,
    };
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    const TEST_ADMISSION: u64 = MAX_REPORT_VIEW_BYTES;
    const TEST_RATE_FINGERPRINT: &str =
        "0000000000000000000000000000000000000000000000000000000000000000";
    const TEST_RAW_SENTINEL: &str = "RAW_PRIVATE_CONTENT_SENTINEL";

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum PermitEvent {
        Acquired(ReportViewWritePhase),
        Revalidated(ReportViewWritePhase),
        Released(ReportViewWritePhase),
    }

    struct RecordingPermit {
        phase: ReportViewWritePhase,
        events: Rc<RefCell<Vec<PermitEvent>>>,
        active: Rc<Cell<bool>>,
        staging_path: Rc<RefCell<Option<PathBuf>>>,
        assert_journal_clean: bool,
    }

    impl Drop for RecordingPermit {
        fn drop(&mut self) {
            if self.assert_journal_clean {
                let path = self.staging_path.borrow().clone().unwrap();
                assert!(!PathBuf::from(format!("{}-journal", path.display())).exists());
            }
            assert!(self.active.replace(false));
            self.events
                .borrow_mut()
                .push(PermitEvent::Released(self.phase));
        }
    }

    struct RecordingPermitFactory {
        events: Rc<RefCell<Vec<PermitEvent>>>,
        active: Rc<Cell<bool>>,
        staging_path: Rc<RefCell<Option<PathBuf>>>,
        denied: Option<ReportViewWritePhase>,
        revalidation_denied: Option<ReportViewWritePhase>,
        assert_projection_journal_clean: bool,
        projection_acquires: usize,
        lifecycle_between_batches: Rc<Cell<bool>>,
    }

    impl RecordingPermitFactory {
        fn new(staging_path: Rc<RefCell<Option<PathBuf>>>) -> Self {
            Self {
                events: Rc::new(RefCell::new(Vec::new())),
                active: Rc::new(Cell::new(false)),
                staging_path,
                denied: None,
                revalidation_denied: None,
                assert_projection_journal_clean: false,
                projection_acquires: 0,
                lifecycle_between_batches: Rc::new(Cell::new(false)),
            }
        }
    }

    impl ReportViewPermitFactory for RecordingPermitFactory {
        type Permit = RecordingPermit;

        fn acquire(
            &mut self,
            phase: ReportViewWritePhase,
        ) -> Result<Self::Permit, ReportViewBuildError> {
            if self.denied == Some(phase) {
                return Err(ReportViewBuildError::CoordinationDenied);
            }
            assert!(!self.active.replace(true));
            if phase == ReportViewWritePhase::ProjectionBatch {
                self.projection_acquires += 1;
                if self.projection_acquires == 2 {
                    self.lifecycle_between_batches.set(true);
                }
            }
            self.events.borrow_mut().push(PermitEvent::Acquired(phase));
            Ok(RecordingPermit {
                phase,
                events: Rc::clone(&self.events),
                active: Rc::clone(&self.active),
                staging_path: Rc::clone(&self.staging_path),
                assert_journal_clean: self.assert_projection_journal_clean
                    && phase == ReportViewWritePhase::ProjectionBatch,
            })
        }

        fn revalidate(&mut self, permit: &Self::Permit) -> Result<(), ReportViewBuildError> {
            assert!(self.active.get());
            if let Some(path) = self.staging_path.borrow().clone() {
                assert!(!PathBuf::from(format!("{}-journal", path.display())).exists());
            }
            self.events
                .borrow_mut()
                .push(PermitEvent::Revalidated(permit.phase));
            if self.revalidation_denied == Some(permit.phase) {
                return Err(ReportViewBuildError::CoordinationDenied);
            }
            Ok(())
        }
    }

    #[test]
    fn staging_cap_and_sqlite_bounds_preserve_non_disk_limits() {
        assert_eq!(MAX_REPORT_VIEW_BYTES, 256 * 1024 * 1024);
        assert!(validate_build_inputs(TEST_RATE_FINGERPRINT, MAX_REPORT_VIEW_BYTES).is_ok());
        assert!(validate_build_inputs(TEST_RATE_FINGERPRINT, MAX_REPORT_VIEW_BYTES + 1).is_err());
        assert!(validate_build_inputs(TEST_RATE_FINGERPRINT, SQLITE_WRITE_HEADROOM_BYTES).is_err());
        let root = temp_dir("sqlite-bounds");
        let _ = fs::remove_dir_all(&root);
        let store = LocalStore::open(&root).unwrap();
        let admitted = MAX_REPORT_VIEW_BYTES - 1;
        let staging =
            create_staging(&store, TEST_RATE_FINGERPRINT, admitted, |_, _| Ok(())).unwrap();
        let value = |name| {
            staging
                .connection()
                .pragma_query_value(None, name, |row| row.get::<_, i64>(0))
                .unwrap()
        };
        assert_eq!(
            value("max_page_count"),
            i64::try_from((admitted - SQLITE_WRITE_HEADROOM_BYTES) / SQLITE_PAGE_BYTES).unwrap()
        );
        assert_eq!(value("cache_size"), -8192);
        assert_eq!(value("journal_size_limit"), 8 * 1024 * 1024);
        assert_eq!(WRITE_BATCH_RECORDS, 128);
        assert_eq!(WRITE_BATCH_BYTES, 2 * 1024 * 1024);
        drop(staging);
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agent-observability-report-view-{label}-{}",
            std::process::id()
        ))
    }

    fn observation(
        ordinal: u64,
        span: &str,
        parent: Option<&str>,
        event: ObservationEvent,
    ) -> SourceObservation {
        SourceObservation {
            source: AgentSource::Codex,
            source_generation: SourceGeneration::parse("generation").unwrap(),
            previous_source_cursor: (ordinal > 1)
                .then(|| SourceCursor::parse((ordinal - 1).to_string()).unwrap()),
            source_cursor: SourceCursor::parse(ordinal.to_string()).unwrap(),
            observation_id: ObservationId::parse(format!("observation-{ordinal}")).unwrap(),
            trace_id: TraceId::parse("raw-trace").unwrap(),
            span_id: SpanId::parse(span).unwrap(),
            parent_span_id: parent.map(|value| SpanId::parse(value).unwrap()),
            correlation: CorrelationIds::default(),
            event,
            lifecycle: LifecycleState::Completed,
            timing: Timing::new(ordinal, Some(ordinal + 1)).unwrap(),
            token_usage: TokenUsage::default(),
        }
    }

    fn open_seeded_store(label: &str) -> (PathBuf, LocalStore) {
        let directory = temp_dir(label);
        let _ = fs::remove_dir_all(&directory);
        let mut store = LocalStore::open(&directory).unwrap();
        store
            .ingest(&observation(
                1,
                "raw-session",
                None,
                ObservationEvent::Session {
                    model: Some("model-a".into()),
                    project: Some("known-repo".into()),
                },
            ))
            .unwrap();
        (directory, store)
    }

    fn staging_paths(directory: &Path) -> Vec<PathBuf> {
        let directory = directory.join("report-views.v1");
        if !directory.is_dir() {
            return Vec::new();
        }
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(STAGING_FILE_PREFIX))
            })
            .collect()
    }

    #[test]
    fn binding_precedes_sqlite_writes_and_rejection_keeps_the_file_empty() {
        let (directory, store) = open_seeded_store("binding-rejection");
        let mut retained_file = None;
        let error = build_report_view_staging_bound(
            &store,
            MISSING_RATE_FINGERPRINT,
            MAX_REPORT_VIEW_BYTES,
            None,
            |path, file| {
                assert_eq!(file.metadata()?.len(), 0);
                assert_eq!(fs::metadata(path)?.len(), 0);
                assert!(!PathBuf::from(format!("{}-journal", path.display())).exists());
                retained_file = Some(file.try_clone()?);
                Err(ReportViewBuildError::InvalidStagingState)
            },
            |_| panic!("projection must not start after binding rejection"),
        )
        .unwrap_err();
        assert!(matches!(error, ReportViewBuildError::InvalidStagingState));
        assert_eq!(retained_file.unwrap().metadata().unwrap().len(), 0);
        assert!(staging_paths(&directory).is_empty());
        assert!(store.try_acquire_report_render_guard().unwrap().is_some());
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn binding_retains_created_identity_through_projection() {
        use std::cell::Cell;
        use std::os::unix::fs::MetadataExt;
        let (directory, store) = open_seeded_store("binding-identity");
        let created = Cell::new(None);
        let bound = Cell::new(false);
        let staging = build_report_view_staging_bound(
            &store,
            MISSING_RATE_FINGERPRINT,
            MAX_REPORT_VIEW_BYTES,
            None,
            |_, file| {
                let metadata = file.metadata()?;
                assert_eq!(metadata.len(), 0);
                created.set(Some((metadata.dev(), metadata.ino())));
                bound.set(true);
                Ok(())
            },
            |_| assert!(bound.get()),
        )
        .unwrap();
        let held = staging.identity_file().metadata().unwrap();
        let named = fs::metadata(staging.path()).unwrap();
        assert_eq!(created.get(), Some((held.dev(), held.ino())));
        assert_eq!((held.dev(), held.ino()), (named.dev(), named.ino()));
        assert!(held.len() > 0);
        drop(staging);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn binding_path_replacement_is_rejected_without_writing_or_removing_replacement() {
        let (directory, store) = open_seeded_store("binding-replacement");
        let mut replaced_path = None;
        let mut original = None;
        let error = build_report_view_staging_bound(
            &store,
            MISSING_RATE_FINGERPRINT,
            MAX_REPORT_VIEW_BYTES,
            None,
            |path, file| {
                original = Some(file.try_clone()?);
                fs::remove_file(path)?;
                let replacement = private_create_new(path)?;
                replacement.sync_all()?;
                replaced_path = Some(path.to_path_buf());
                Ok(())
            },
            |_| panic!("replaced staging must never be projected"),
        )
        .unwrap_err();
        assert!(matches!(error, ReportViewBuildError::InvalidStagingState));
        assert_eq!(original.unwrap().metadata().unwrap().len(), 0);
        assert_eq!(fs::metadata(replaced_path.unwrap()).unwrap().len(), 0);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn binding_rejects_aliases_and_unexpected_writes_before_sqlite_initialization() {
        use std::io::Write;
        for alias in [false, true] {
            let (directory, store) = open_seeded_store(&format!("binding-alias-or-write-{alias}"));
            let mut held = None;
            let result = build_report_view_staging_bound(
                &store,
                MISSING_RATE_FINGERPRINT,
                MAX_REPORT_VIEW_BYTES,
                None,
                |path, file| {
                    held = Some(file.try_clone()?);
                    if alias {
                        fs::hard_link(path, directory.join("alias"))?;
                    } else {
                        (&*file).write_all(b"not-sqlite")?;
                    }
                    Ok(())
                },
                |_| panic!("invalid staging must not be projected"),
            );
            assert!(matches!(
                result,
                Err(ReportViewBuildError::InvalidStagingState)
            ));
            assert_eq!(
                held.unwrap().metadata().unwrap().len(),
                if alias { 0 } else { 10 }
            );
            drop(store);
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn coordinated_build_acquires_and_releases_one_permit_per_write_phase() {
        let (directory, mut store) = open_seeded_store("coordinated-sequence");
        for ordinal in 2..=129 {
            store
                .ingest(&observation(
                    ordinal,
                    &format!("span-{ordinal}"),
                    Some("raw-session"),
                    ObservationEvent::Turn,
                ))
                .unwrap();
        }
        let path = Rc::new(RefCell::new(None));
        let mut permits = RecordingPermitFactory::new(Rc::clone(&path));
        let lifecycle_between_batches = Rc::clone(&permits.lifecycle_between_batches);
        let mut staging = build_report_view_staging_bound_coordinated(
            &store,
            MISSING_RATE_FINGERPRINT,
            MAX_REPORT_VIEW_BYTES,
            None,
            |staging_path, _| {
                *path.borrow_mut() = Some(staging_path.to_owned());
                Ok(())
            },
            &mut permits,
            |index| {
                if index >= WRITE_BATCH_RECORDS {
                    assert!(lifecycle_between_batches.get());
                }
            },
        )
        .unwrap();
        assert!(!permits.active.get());
        assert_eq!(
            permits.events.borrow().as_slice(),
            &[
                PermitEvent::Acquired(ReportViewWritePhase::Initialize),
                PermitEvent::Revalidated(ReportViewWritePhase::Initialize),
                PermitEvent::Released(ReportViewWritePhase::Initialize),
                PermitEvent::Acquired(ReportViewWritePhase::ProjectionBatch),
                PermitEvent::Revalidated(ReportViewWritePhase::ProjectionBatch),
                PermitEvent::Released(ReportViewWritePhase::ProjectionBatch),
                PermitEvent::Acquired(ReportViewWritePhase::ProjectionBatch),
                PermitEvent::Revalidated(ReportViewWritePhase::ProjectionBatch),
                PermitEvent::Released(ReportViewWritePhase::ProjectionBatch),
                PermitEvent::Acquired(ReportViewWritePhase::RepositoryResolutionBatch),
                PermitEvent::Revalidated(ReportViewWritePhase::RepositoryResolutionBatch),
                PermitEvent::Released(ReportViewWritePhase::RepositoryResolutionBatch),
                PermitEvent::Acquired(ReportViewWritePhase::RepositoryResolutionBatch),
                PermitEvent::Revalidated(ReportViewWritePhase::RepositoryResolutionBatch),
                PermitEvent::Released(ReportViewWritePhase::RepositoryResolutionBatch),
                PermitEvent::Acquired(ReportViewWritePhase::FinalMetadata),
                PermitEvent::Revalidated(ReportViewWritePhase::FinalMetadata),
                PermitEvent::Released(ReportViewWritePhase::FinalMetadata),
            ]
        );
        staging.discard_with_permit(&mut permits).unwrap();
        assert_eq!(
            &permits.events.borrow()[permits.events.borrow().len() - 3..],
            &[
                PermitEvent::Acquired(ReportViewWritePhase::Discard),
                PermitEvent::Revalidated(ReportViewWritePhase::Discard),
                PermitEvent::Released(ReportViewWritePhase::Discard),
            ]
        );
        assert!(staging_paths(&directory).is_empty());
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn coordinated_initialization_denial_leaves_empty_staging_untouched() {
        let (directory, store) = open_seeded_store("coordinated-init-denied");
        let path = Rc::new(RefCell::new(None));
        let mut permits = RecordingPermitFactory::new(Rc::clone(&path));
        permits.denied = Some(ReportViewWritePhase::Initialize);
        let error = build_report_view_staging_bound_coordinated(
            &store,
            MISSING_RATE_FINGERPRINT,
            MAX_REPORT_VIEW_BYTES,
            None,
            |staging_path, _| {
                *path.borrow_mut() = Some(staging_path.to_owned());
                Ok(())
            },
            &mut permits,
            |_| panic!("projection must not start without an initialization permit"),
        )
        .unwrap_err();
        assert!(matches!(error, ReportViewBuildError::CoordinationDenied));
        let path = path.borrow().clone().unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        assert!(!PathBuf::from(format!("{}-journal", path.display())).exists());
        assert!(path.is_file());
        assert_eq!(staging_paths(&directory).len(), 1);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn initialization_postcheck_denial_stops_projection_and_preserves_staging() {
        let (directory, store) = open_seeded_store("coordinated-init-postcheck-denied");
        let path = Rc::new(RefCell::new(None));
        let mut permits = RecordingPermitFactory::new(Rc::clone(&path));
        permits.revalidation_denied = Some(ReportViewWritePhase::Initialize);
        let error = build_report_view_staging_bound_coordinated(
            &store,
            MISSING_RATE_FINGERPRINT,
            MAX_REPORT_VIEW_BYTES,
            None,
            |staging_path, _| {
                *path.borrow_mut() = Some(staging_path.to_owned());
                Ok(())
            },
            &mut permits,
            |_| panic!("projection must not start after initialization postcheck denial"),
        )
        .unwrap_err();
        assert!(matches!(error, ReportViewBuildError::CoordinationDenied));
        assert_eq!(
            permits.events.borrow().as_slice(),
            &[
                PermitEvent::Acquired(ReportViewWritePhase::Initialize),
                PermitEvent::Revalidated(ReportViewWritePhase::Initialize),
                PermitEvent::Released(ReportViewWritePhase::Initialize),
            ]
        );
        let path = path.borrow().clone().unwrap();
        assert!(path.is_file());
        let connection =
            Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let records: i64 = connection
            .query_row("SELECT COUNT(*) FROM spans", [], |row| row.get(0))
            .unwrap();
        assert_eq!(records, 0);
        drop(connection);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn coordinated_projection_denial_does_not_start_a_write_transaction() {
        let (directory, store) = open_seeded_store("coordinated-projection-denied");
        let path = Rc::new(RefCell::new(None));
        let mut permits = RecordingPermitFactory::new(Rc::clone(&path));
        permits.denied = Some(ReportViewWritePhase::ProjectionBatch);
        let error = build_report_view_staging_bound_coordinated(
            &store,
            MISSING_RATE_FINGERPRINT,
            MAX_REPORT_VIEW_BYTES,
            None,
            |staging_path, _| {
                *path.borrow_mut() = Some(staging_path.to_owned());
                Ok(())
            },
            &mut permits,
            |_| panic!("projection callback must not run after permit denial"),
        )
        .unwrap_err();
        assert!(matches!(error, ReportViewBuildError::CoordinationDenied));
        let path = path.borrow().clone().unwrap();
        let connection =
            Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let records: i64 = connection
            .query_row("SELECT COUNT(*) FROM spans", [], |row| row.get(0))
            .unwrap();
        assert_eq!(records, 0);
        assert!(!PathBuf::from(format!("{}-journal", path.display())).exists());
        drop(connection);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn projection_postcheck_denial_stops_before_the_next_batch_and_preserves_staging() {
        let (directory, mut store) = open_seeded_store("coordinated-postcheck-batch-denied");
        for ordinal in 2..=129 {
            store
                .ingest(&observation(
                    ordinal,
                    &format!("span-{ordinal}"),
                    Some("raw-session"),
                    ObservationEvent::Turn,
                ))
                .unwrap();
        }
        let path = Rc::new(RefCell::new(None));
        let mut permits = RecordingPermitFactory::new(Rc::clone(&path));
        permits.revalidation_denied = Some(ReportViewWritePhase::ProjectionBatch);
        let error = build_report_view_staging_bound_coordinated(
            &store,
            MISSING_RATE_FINGERPRINT,
            MAX_REPORT_VIEW_BYTES,
            None,
            |staging_path, _| {
                *path.borrow_mut() = Some(staging_path.to_owned());
                Ok(())
            },
            &mut permits,
            |_| {},
        )
        .unwrap_err();
        assert!(matches!(error, ReportViewBuildError::CoordinationDenied));
        assert_eq!(
            permits
                .events
                .borrow()
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        PermitEvent::Acquired(ReportViewWritePhase::ProjectionBatch)
                    )
                })
                .count(),
            1
        );
        let path = path.borrow().clone().unwrap();
        let connection =
            Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let records: i64 = connection
            .query_row("SELECT COUNT(*) FROM spans", [], |row| row.get(0))
            .unwrap();
        assert_eq!(records, i64::try_from(WRITE_BATCH_RECORDS).unwrap());
        let complete: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM metadata WHERE key='complete'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(complete, 0);
        assert!(path.is_file());
        drop(connection);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn final_metadata_postcheck_denial_prevents_publication_and_preserves_staging() {
        let (directory, store) = open_seeded_store("coordinated-postcheck-publish-denied");
        let path = Rc::new(RefCell::new(None));
        let mut permits = RecordingPermitFactory::new(Rc::clone(&path));
        permits.revalidation_denied = Some(ReportViewWritePhase::FinalMetadata);
        let error = build_report_view_staging_bound_coordinated(
            &store,
            MISSING_RATE_FINGERPRINT,
            MAX_REPORT_VIEW_BYTES,
            None,
            |staging_path, _| {
                *path.borrow_mut() = Some(staging_path.to_owned());
                Ok(())
            },
            &mut permits,
            |_| {},
        )
        .unwrap_err();
        assert!(matches!(error, ReportViewBuildError::CoordinationDenied));
        let path = path.borrow().clone().unwrap();
        assert!(path.is_file());
        let connection =
            Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let complete: String = connection
            .query_row(
                "SELECT value FROM metadata WHERE key='complete'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(complete, "1");
        drop(connection);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn begin_error_is_revalidated_after_cleanup_without_losing_primary_error() {
        let (directory, store) = open_seeded_store("coordinated-begin-error-ordering");
        let path = Rc::new(RefCell::new(None));
        let mut staging = create_staging(
            &store,
            MISSING_RATE_FINGERPRINT,
            MAX_REPORT_VIEW_BYTES,
            |staging_path, _| {
                *path.borrow_mut() = Some(staging_path.to_owned());
                Ok(())
            },
        )
        .unwrap();
        staging
            .connection()
            .execute_batch("BEGIN IMMEDIATE")
            .unwrap();
        let mut permits = RecordingPermitFactory::new(Rc::clone(&path));
        permits.revalidation_denied = Some(ReportViewWritePhase::ProjectionBatch);
        let error = with_write_transaction(
            &mut staging,
            &mut permits,
            ReportViewWritePhase::ProjectionBatch,
            |_| -> Result<(), ReportViewBuildError> {
                panic!("operation must not run after BEGIN failure")
            },
        )
        .unwrap_err();
        assert!(matches!(error, ReportViewBuildError::Sqlite(_)));
        assert_eq!(
            permits.events.borrow().as_slice(),
            &[
                PermitEvent::Acquired(ReportViewWritePhase::ProjectionBatch),
                PermitEvent::Revalidated(ReportViewWritePhase::ProjectionBatch),
                PermitEvent::Released(ReportViewWritePhase::ProjectionBatch),
            ]
        );
        assert!(staging.connection.is_none());
        drop(staging);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn operation_error_remains_primary_when_postcheck_is_also_denied() {
        let (directory, store) = open_seeded_store("coordinated-operation-error-ordering");
        let path = Rc::new(RefCell::new(None));
        let mut staging = create_staging(
            &store,
            MISSING_RATE_FINGERPRINT,
            MAX_REPORT_VIEW_BYTES,
            |staging_path, _| {
                *path.borrow_mut() = Some(staging_path.to_owned());
                Ok(())
            },
        )
        .unwrap();
        let mut permits = RecordingPermitFactory::new(Rc::clone(&path));
        permits.revalidation_denied = Some(ReportViewWritePhase::ProjectionBatch);
        let error = with_write_transaction(
            &mut staging,
            &mut permits,
            ReportViewWritePhase::ProjectionBatch,
            |_| Err::<(), _>(ReportViewBuildError::InvalidStagingState),
        )
        .unwrap_err();
        assert!(matches!(error, ReportViewBuildError::InvalidStagingState));
        assert_eq!(
            permits.events.borrow().as_slice(),
            &[
                PermitEvent::Acquired(ReportViewWritePhase::ProjectionBatch),
                PermitEvent::Revalidated(ReportViewWritePhase::ProjectionBatch),
                PermitEvent::Released(ReportViewWritePhase::ProjectionBatch),
            ]
        );
        drop(staging);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn coordinated_final_batch_capacity_error_rolls_back_before_permit_release() {
        let (directory, store) = open_seeded_store("coordinated-rollback");
        let mut durable: agent_observability_contracts::DurableRecordV1 = store
            .db
            .query_row("SELECT record_json FROM records LIMIT 1", [], |row| {
                let json: String = row.get(0)?;
                Ok(serde_json::from_str(&json).unwrap())
            })
            .unwrap();
        durable.agent.version = Some("v".repeat(1024 * 1024));
        store
            .db
            .execute(
                "UPDATE records SET record_json=?1",
                [serde_json::to_string(&durable).unwrap()],
            )
            .unwrap();
        let path = Rc::new(RefCell::new(None));
        let mut permits = RecordingPermitFactory::new(Rc::clone(&path));
        permits.assert_projection_journal_clean = true;
        let error = build_report_view_staging_bound_coordinated(
            &store,
            MISSING_RATE_FINGERPRINT,
            SQLITE_WRITE_HEADROOM_BYTES + (32 * SQLITE_PAGE_BYTES),
            None,
            |staging_path, _| {
                *path.borrow_mut() = Some(staging_path.to_owned());
                Ok(())
            },
            &mut permits,
            |_| {},
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ReportViewBuildError::CapacityExceeded | ReportViewBuildError::Sqlite(_)
        ));
        assert!(!permits.active.get());
        assert!(permits.events.borrow().contains(&PermitEvent::Acquired(
            ReportViewWritePhase::ProjectionBatch
        )));
        let path = path.borrow().clone().unwrap();
        let connection =
            Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let records: i64 = connection
            .query_row("SELECT COUNT(*) FROM spans", [], |row| row.get(0))
            .unwrap();
        assert_eq!(records, 0);
        assert!(!PathBuf::from(format!("{}-journal", path.display())).exists());
        drop(connection);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn coordinated_panic_closes_active_transaction_before_permit_release() {
        let (directory, store) = open_seeded_store("coordinated-panic");
        let path = Rc::new(RefCell::new(None));
        let mut permits = RecordingPermitFactory::new(Rc::clone(&path));
        permits.assert_projection_journal_clean = true;
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = build_report_view_staging_bound_coordinated(
                &store,
                MISSING_RATE_FINGERPRINT,
                MAX_REPORT_VIEW_BYTES,
                None,
                |staging_path, _| {
                    *path.borrow_mut() = Some(staging_path.to_owned());
                    Ok(())
                },
                &mut permits,
                |_| panic!("projection callback panic"),
            );
        }));
        assert!(unwind.is_err());
        assert!(!permits.active.get());
        let path = path.borrow().clone().unwrap();
        assert!(path.is_file());
        assert!(!PathBuf::from(format!("{}-journal", path.display())).exists());
        let connection =
            Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let records: i64 = connection
            .query_row("SELECT COUNT(*) FROM spans", [], |row| row.get(0))
            .unwrap();
        assert_eq!(records, 0);
        drop(connection);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn coordinated_discard_denial_preserves_staging_and_render_guard() {
        let (directory, store) = open_seeded_store("coordinated-discard-denied");
        let path = Rc::new(RefCell::new(None));
        let mut permits = RecordingPermitFactory::new(Rc::clone(&path));
        let mut staging = build_report_view_staging_bound_coordinated(
            &store,
            MISSING_RATE_FINGERPRINT,
            MAX_REPORT_VIEW_BYTES,
            None,
            |staging_path, _| {
                *path.borrow_mut() = Some(staging_path.to_owned());
                Ok(())
            },
            &mut permits,
            |_| {},
        )
        .unwrap();
        permits.denied = Some(ReportViewWritePhase::Discard);
        assert!(matches!(
            staging.discard_with_permit(&mut permits),
            Err(ReportViewBuildError::CoordinationDenied)
        ));
        let staging_path = staging.path().to_owned();
        assert!(staging_path.is_file());
        assert!(store.try_acquire_report_render_guard().unwrap().is_none());
        drop(staging);
        assert!(staging_path.is_file());
        assert!(store.try_acquire_report_render_guard().unwrap().is_some());
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn discard_postcheck_runs_after_removal_and_returns_denial_on_success() {
        let (directory, store) = open_seeded_store("coordinated-discard-postcheck-denied");
        let path = Rc::new(RefCell::new(None));
        let mut permits = RecordingPermitFactory::new(Rc::clone(&path));
        let mut staging = build_report_view_staging_bound_coordinated(
            &store,
            MISSING_RATE_FINGERPRINT,
            MAX_REPORT_VIEW_BYTES,
            None,
            |staging_path, _| {
                *path.borrow_mut() = Some(staging_path.to_owned());
                Ok(())
            },
            &mut permits,
            |_| {},
        )
        .unwrap();
        permits.events.borrow_mut().clear();
        permits.revalidation_denied = Some(ReportViewWritePhase::Discard);
        let error = staging.discard_with_permit(&mut permits).unwrap_err();
        assert!(matches!(error, ReportViewBuildError::CoordinationDenied));
        assert_eq!(
            permits.events.borrow().as_slice(),
            &[
                PermitEvent::Acquired(ReportViewWritePhase::Discard),
                PermitEvent::Revalidated(ReportViewWritePhase::Discard),
                PermitEvent::Released(ReportViewWritePhase::Discard),
            ]
        );
        assert!(!path.borrow().as_ref().unwrap().exists());
        drop(staging);
        assert!(store.try_acquire_report_render_guard().unwrap().is_some());
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    fn assert_trace_order_index(connection: &Connection) {
        let columns = connection
            .prepare("SELECT name FROM pragma_index_info('spans_trace_order_idx') ORDER BY seqno")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(columns, ["trace_id", "start_time_unix_ms", "span_id"]);
        let unused_indexes: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN ('spans_kind_order_idx','spans_status_order_idx')",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(
            unused_indexes, 0,
            "unqueried indexes must not consume the bounded snapshot budget"
        );
    }

    fn projected_turn(connection: &Connection) -> (String, ReportSpanV2) {
        let (trace_id, span_id, repo, agent, model, json): (
            String,
            String,
            String,
            String,
            String,
            String,
        ) = connection
            .query_row(
                "SELECT trace_id, span_id, repo, agent, model, span_json FROM spans WHERE source_order=1",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(trace_id, hash_opaque_identifier("raw-trace"));
        assert_eq!(span_id, hash_opaque_identifier("raw-turn"));
        assert_eq!(repo, "known-repo");
        assert_eq!(agent, "unknown");
        assert_eq!(model, "unknown");
        let span: ReportSpanV2 = serde_json::from_str(&json).unwrap();
        assert_eq!(span.agent.name, None);
        assert_eq!(span.agent.model, None);
        (json, span)
    }

    #[test]
    fn staging_is_complete_private_content_free_and_trace_repo_resolved() {
        let (directory, mut store) = open_seeded_store("privacy");
        store
            .ingest(&observation(
                2,
                "raw-turn",
                Some("raw-session"),
                ObservationEvent::Turn,
            ))
            .unwrap();
        {
            let mut durable: agent_observability_contracts::DurableRecordV1 = store
                .db
                .query_row(
                    "SELECT record_json FROM records ORDER BY commit_seq DESC LIMIT 1",
                    [],
                    |row| {
                        let json: String = row.get(0)?;
                        Ok(serde_json::from_str(&json).unwrap())
                    },
                )
                .unwrap();
            durable.content.prompt = Some(JsonValue::String(TEST_RAW_SENTINEL.into()));
            durable.agent.name = None;
            durable.agent.model = None;
            store
                .db
                .execute(
                    "UPDATE records SET record_json=?1 WHERE commit_seq=(SELECT MAX(commit_seq) FROM records)",
                    [serde_json::to_string(&durable).unwrap()],
                )
                .unwrap();
        }

        let mut staging =
            build_report_view_staging(&store, TEST_RATE_FINGERPRINT, TEST_ADMISSION, None).unwrap();
        assert_eq!(staging.generation(), 2);
        assert_eq!(staging.visibility_epoch(), 0);
        assert_eq!(staging.records(), 2);
        assert_eq!(staging.rate_fingerprint(), TEST_RATE_FINGERPRINT);
        assert_eq!(
            staging.path().parent(),
            Some(
                fs::canonicalize(&directory)
                    .unwrap()
                    .join("report-views.v1")
                    .as_path()
            )
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(staging.path().parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(staging.path()).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let complete: String = staging
            .connection()
            .query_row(
                "SELECT value FROM metadata WHERE key='complete'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(complete, "1");
        assert_trace_order_index(staging.connection());
        let (json, record) = projected_turn(staging.connection());
        assert!(!json.contains("raw-trace"));
        assert!(!json.contains("raw-turn"));
        assert!(!json.contains(TEST_RAW_SENTINEL));
        assert_eq!(
            record.availability.repository.reason,
            "derived_from_trace_context"
        );

        let path = staging.path().to_owned();
        let peer = LocalStore::open_current(&directory).unwrap();
        assert!(peer.try_acquire_report_render_guard().unwrap().is_none());
        staging.close_for_publication().unwrap();
        assert!(path.is_file());
        assert!(peer.try_acquire_report_render_guard().unwrap().is_none());
        drop(staging);
        assert!(!path.exists());
        assert!(peer.try_acquire_report_render_guard().unwrap().is_some());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn capacity_failure_removes_staging_without_complete_marker() {
        let (directory, store) = open_seeded_store("capacity");
        let error = build_report_view_staging(
            &store,
            TEST_RATE_FINGERPRINT,
            SQLITE_WRITE_HEADROOM_BYTES + (4 * SQLITE_PAGE_BYTES),
            None,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ReportViewBuildError::CapacityExceeded | ReportViewBuildError::Sqlite(_)
        ));
        assert!(staging_paths(&directory).is_empty());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn generation_change_cannot_produce_a_complete_staging_file() {
        let (directory, reader) = open_seeded_store("generation-change");
        let mut writer = LocalStore::open_current(&directory).unwrap();
        let error = build_report_view_staging_observing(
            &reader,
            TEST_RATE_FINGERPRINT,
            TEST_ADMISSION,
            None,
            |index| {
                if index == 0 {
                    writer
                        .ingest(&observation(
                            2,
                            "raw-turn",
                            Some("raw-session"),
                            ObservationEvent::Turn,
                        ))
                        .unwrap();
                }
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ReportViewBuildError::Store(StoreError::ReportSnapshotChanged)
                | ReportViewBuildError::SnapshotChanged
        ));
        assert!(staging_paths(&directory).is_empty());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn active_publication_guard_returns_busy_without_creating_staging() {
        let (directory, store) = open_seeded_store("busy");
        let guard = store.acquire_report_render_guard().unwrap();
        let error = build_report_view_staging(&store, TEST_RATE_FINGERPRINT, TEST_ADMISSION, None)
            .unwrap_err();
        assert!(matches!(error, ReportViewBuildError::Busy));
        assert!(staging_paths(&directory).is_empty());
        drop(guard);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn rate_fingerprint_is_a_lowercase_digest_or_fixed_missing_marker() {
        let (directory, store) = open_seeded_store("rate-fingerprint");
        let invalid_fingerprints = [
            String::new(),
            "arbitrary-rate-label".to_owned(),
            "A".repeat(64),
        ];
        for invalid in &invalid_fingerprints {
            assert!(matches!(
                build_report_view_staging(&store, invalid, TEST_ADMISSION, None,),
                Err(ReportViewBuildError::InvalidRateFingerprint)
            ));
        }
        let staging =
            build_report_view_staging(&store, MISSING_RATE_FINGERPRINT, TEST_ADMISSION, None)
                .unwrap();
        let stored: String = staging
            .connection()
            .query_row(
                "SELECT value FROM metadata WHERE key='rate_fingerprint'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, MISSING_RATE_FINGERPRINT);
        drop(staging);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn content_free_projection_can_exceed_the_legacy_html_limit_within_admission() {
        const RECORDS: usize = 20_000;
        const LEGACY_HTML_LIMIT: u64 = 32 * 1024 * 1024;

        let (directory, mut store) = open_seeded_store("beyond-html-limit");
        let mut template: agent_observability_contracts::DurableRecordV1 = store
            .db
            .query_row("SELECT record_json FROM records LIMIT 1", [], |row| {
                let json: String = row.get(0)?;
                Ok(serde_json::from_str(&json).unwrap())
            })
            .unwrap();
        template.agent.version = Some("v".repeat(1_200));
        let transaction = store.db.transaction().unwrap();
        {
            let mut insert = transaction
                .prepare(
                    "INSERT INTO records(span_id, trace_id, parent_span_id, kind, state_json, record_json) VALUES (?1, ?2, NULL, 'turn', '{}', ?3)",
                )
                .unwrap();
            for ordinal in 1..RECORDS {
                template.trace_id = format!("trace-{ordinal}");
                template.span_id = format!("span-{ordinal}");
                template.parent_span_id = None;
                template.span_kind = SpanKind::Turn;
                insert
                    .execute(params![
                        template.span_id,
                        template.trace_id,
                        serde_json::to_string(&template).unwrap()
                    ])
                    .unwrap();
            }
        }
        transaction.commit().unwrap();
        store
            .db
            .execute(
                "UPDATE metadata SET value='2' WHERE key='report_generation'",
                [],
            )
            .unwrap();

        let staging =
            build_report_view_staging(&store, TEST_RATE_FINGERPRINT, TEST_ADMISSION, None).unwrap();
        assert_eq!(staging.records(), RECORDS);
        let bytes = fs::metadata(staging.path()).unwrap().len();
        assert!(bytes > LEGACY_HTML_LIMIT, "staging bytes: {bytes}");
        assert!(bytes <= TEST_ADMISSION);
        drop(staging);
        let _ = fs::remove_dir_all(directory);
    }
}
