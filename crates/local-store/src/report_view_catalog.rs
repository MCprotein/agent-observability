//! Private immutable report-view publication and bounded catalog management.

use super::report_view::{ReportViewBuildError, ReportViewStaging};
use super::{
    LocalStore, REPORT_RENDER_LOCK_NAME, ReportRenderGuard, StoreError, private_create_new,
    private_dir, private_file, validate_existing_private_dir,
};
use agent_observability_contracts::hash_opaque_identifier;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt::{self, Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const MANAGED_DIRECTORY_NAME: &str = "report-views.v1";
const CATALOG_FILE_NAME: &str = "catalog.json";
const CATALOG_SCHEMA_VERSION: &str = "agent_observability.report_view_catalog.v1";
const CATALOG_TEMP_PREFIX: &str = ".catalog.json.tmp.";
const STAGING_FILE_PREFIX: &str = ".report-view.sqlite3.staging.";
const IMMUTABLE_FILE_PREFIX: &str = "report-view-";
const IMMUTABLE_FILE_SUFFIX: &str = ".sqlite3";
const MAX_CATALOG_BYTES: u64 = 16 * 1024;
const MAX_MANAGED_DIRECTORY_ENTRIES: usize = 16;
const MAX_TEMP_COLLISIONS: usize = 64;
const SQLITE_CACHE_KIB: i64 = 8 * 1024;
static CATALOG_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Private sidecar query layout; never part of the HTTP contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReportViewKernel {
    V1,
    V2,
}

/// Whether a validated current sidecar needs rebuilding with the current kernel.
/// An absent view returns false; callers separately schedule initial publication.
///
/// # Errors
/// Returns an error for unsafe, unknown or incompatible metadata/index layouts.
pub fn current_report_view_needs_kernel_upgrade(
    store: &LocalStore,
) -> Result<bool, ReportViewCatalogError> {
    let scope = ReportViewReadScope::acquire(store)?;
    let Some(snapshot) = scope.current()? else {
        return Ok(false);
    };
    scope.with_snapshot_kernel(snapshot.view_id(), |_, _, kernel| {
        Ok(kernel == ReportViewKernel::V1)
    })
}

/// Failure while publishing, reopening, recovering, or retiring report-view snapshots.
#[derive(Debug)]
pub enum ReportViewCatalogError {
    Store(StoreError),
    Build(ReportViewBuildError),
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    Io(io::Error),
    Busy,
    SnapshotChanged,
    SourceMismatch,
    SnapshotExpired,
    RefreshPending,
    InvalidCatalog,
    CatalogCapacityExceeded,
    InvalidVisibilityAdvance,
}

impl Display for ReportViewCatalogError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Store(_) => "report view authority read failed",
            Self::Build(_) => "report view staging finalization failed",
            Self::Sqlite(_) => "report view snapshot database failed",
            Self::Json(_) => "report view catalog JSON failed",
            Self::Io(_) => "report view catalog filesystem failed",
            Self::Busy => "another report view or destructive operation is active",
            Self::SnapshotChanged => "report view source generation changed before publication",
            Self::SourceMismatch => "report view belongs to a different local store",
            Self::SnapshotExpired => "report view snapshot is no longer retained",
            Self::RefreshPending => "report view visibility epoch is no longer current",
            Self::InvalidCatalog => "report view catalog is invalid",
            Self::CatalogCapacityExceeded => "report view catalog byte capacity was exceeded",
            Self::InvalidVisibilityAdvance => "report visibility epoch advance is invalid",
        })
    }
}

impl std::error::Error for ReportViewCatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            Self::Build(error) => Some(error),
            Self::Sqlite(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<StoreError> for ReportViewCatalogError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<ReportViewBuildError> for ReportViewCatalogError {
    fn from(error: ReportViewBuildError) -> Self {
        Self::Build(error)
    }
}

impl From<rusqlite::Error> for ReportViewCatalogError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl From<serde_json::Error> for ReportViewCatalogError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<io::Error> for ReportViewCatalogError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Metadata for one immutable, content-free report-view database.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportViewSnapshot {
    view_id: String,
    generation: u64,
    visibility_epoch: u64,
    records: usize,
    rate_fingerprint: String,
    generated_at: String,
    file_name: String,
}

impl ReportViewSnapshot {
    #[must_use]
    pub fn view_id(&self) -> &str {
        &self.view_id
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
}

/// Result of installing one immutable snapshot as catalog current.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportViewPublication {
    current: ReportViewSnapshot,
    retired: Option<ReportViewSnapshot>,
    cleanup_pending: bool,
}

impl ReportViewPublication {
    #[must_use]
    pub const fn current(&self) -> &ReportViewSnapshot {
        &self.current
    }

    #[must_use]
    pub const fn retired(&self) -> Option<&ReportViewSnapshot> {
        self.retired.as_ref()
    }

    #[must_use]
    pub const fn cleanup_pending(&self) -> bool {
        self.cleanup_pending
    }
}

/// Guard token that keeps report publication and queries fenced during destructive work.
#[derive(Debug)]
pub struct ReportViewRetirement {
    _publication_guard: ReportRenderGuard,
    previous_visibility_epoch: u64,
    next_visibility_epoch: u64,
}

impl ReportViewRetirement {
    #[must_use]
    pub const fn previous_visibility_epoch(&self) -> u64 {
        self.previous_visibility_epoch
    }

    #[must_use]
    pub const fn next_visibility_epoch(&self) -> u64 {
        self.next_visibility_epoch
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportViewCatalog {
    schema_version: String,
    source_identity: String,
    visibility_epoch: u64,
    current: Option<ReportViewSnapshot>,
    retired: Option<ReportViewSnapshot>,
}

/// Semantic role of one exact path retained by report-view ownership evidence.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ReportViewOwnedEntryKind {
    ManagedDirectory,
    Catalog,
    CurrentSnapshot,
    RetiredSnapshot,
}

/// One exact path enumerated by validated report-view ownership evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReportViewOwnedEntry<'a> {
    kind: ReportViewOwnedEntryKind,
    path: &'a Path,
}

impl<'a> ReportViewOwnedEntry<'a> {
    #[must_use]
    pub const fn kind(self) -> ReportViewOwnedEntryKind {
        self.kind
    }

    #[must_use]
    pub const fn path(self) -> &'a Path {
        self.path
    }
}

#[derive(Debug)]
struct OwnedDescriptor {
    kind: ReportViewOwnedEntryKind,
    path: PathBuf,
    file: File,
}

/// Callback-scoped observation of the exact report-view directory, catalog, and retained snapshots.
///
/// File descriptors, including the existing render-lock identity, remain private and live until
/// this value is dropped. Callers can enumerate paths and test an already-open descriptor, but
/// cannot turn a matching filename into ownership evidence.
pub struct ReportViewOwnershipObservation<'a> {
    store: &'a LocalStore,
    render_lock_path: PathBuf,
    render_lock: File,
    source_identity: String,
    visibility_epoch: u64,
    catalog: Option<ReportViewCatalog>,
    owned: Vec<OwnedDescriptor>,
}

impl fmt::Debug for ReportViewOwnershipObservation<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let kinds = self
            .owned
            .iter()
            .map(|entry| entry.kind)
            .collect::<BTreeSet<_>>();
        formatter
            .debug_struct("ReportViewOwnershipObservation")
            .field("entry_count", &self.owned.len())
            .field("kinds", &kinds)
            .finish()
    }
}

impl ReportViewOwnershipObservation<'_> {
    /// Enumerates the exact paths and roles validated at acquisition.
    #[must_use]
    pub fn entries(&self) -> impl ExactSizeIterator<Item = ReportViewOwnedEntry<'_>> {
        self.owned.iter().map(|entry| ReportViewOwnedEntry {
            kind: entry.kind,
            path: &entry.path,
        })
    }

    /// Tests whether `descriptor` is the retained private identity for the exact known `path`.
    ///
    /// Unknown paths return false. A known path with a replaced, aliased, or otherwise mismatched
    /// descriptor fails closed with [`ReportViewCatalogError::InvalidCatalog`].
    ///
    /// # Errors
    ///
    /// Returns [`ReportViewCatalogError::InvalidCatalog`] when a known path does not match its
    /// retained private descriptor, or an I/O error when descriptor metadata cannot be read.
    pub fn recognizes(
        &self,
        path: &Path,
        descriptor: &File,
    ) -> Result<bool, ReportViewCatalogError> {
        let Some(owned) = self.owned.iter().find(|owned| owned.path == path) else {
            return Ok(false);
        };
        validate_descriptor_for_kind(descriptor, owned.kind)?;
        if !same_descriptor_identity(&owned.file, descriptor)? {
            return Err(ReportViewCatalogError::InvalidCatalog);
        }
        Ok(true)
    }

    fn validate_authority(&self) -> Result<(), ReportViewCatalogError> {
        validate_render_lock_identity(&self.render_lock_path, &self.render_lock)?;
        let source_identity =
            source_store_identity(self.store).map_err(ReportViewCatalogError::Build)?;
        if source_identity != self.source_identity {
            return Err(ReportViewCatalogError::SourceMismatch);
        }
        let visibility_epoch = self.store.report_visibility_epoch()?;
        if visibility_epoch != self.visibility_epoch {
            return Err(ReportViewCatalogError::RefreshPending);
        }
        let directory = existing_managed_report_view_directory(self.store)
            .map_err(ReportViewCatalogError::Build)?
            .ok_or(ReportViewCatalogError::InvalidCatalog)?;
        if self
            .owned
            .first()
            .is_none_or(|owned| owned.path != directory)
        {
            return Err(ReportViewCatalogError::InvalidCatalog);
        }
        for owned in &self.owned {
            validate_named_identity(owned)?;
        }
        let catalog = read_catalog(&directory)?;
        validate_catalog_authority(catalog.as_ref(), &source_identity, visibility_epoch)?;
        if catalog != self.catalog {
            return Err(ReportViewCatalogError::InvalidCatalog);
        }
        if let Some(catalog) = catalog.as_ref() {
            validate_catalog_snapshot_databases(&directory, catalog)?;
        }
        Ok(())
    }
}

/// Supplies a bounded, read-only observation of report-view ownership.
///
/// This operation requires an existing managed report-view directory and existing render lock.
/// It never creates, repairs, cleans up, publishes, or migrates an artifact. Current and retired
/// snapshots are validated only through their bounded metadata and index contracts. It retains and
/// revalidates the render-lock identity without acquiring that lock, so an immutable published view
/// can be observed while a new report is building.
///
/// This before/after observation is not an ABA-safe or coherent all-writer filesystem snapshot.
/// A caller must hold the external all-writer freeze before using it as storage-admission evidence;
/// no marker or value supplied by the caller is accepted as authority here.
///
/// # Errors
///
/// Returns [`ReportViewCatalogError`] if the existing render lock is absent, catalog authority is
/// invalid, a retained snapshot is invalid, or any exact path identity changes.
pub fn with_report_view_ownership_observation<T>(
    store: &LocalStore,
    use_observation: impl FnOnce(&ReportViewOwnershipObservation<'_>) -> T,
) -> Result<T, ReportViewCatalogError> {
    let directory = existing_managed_report_view_directory(store)
        .map_err(ReportViewCatalogError::Build)?
        .ok_or(ReportViewCatalogError::RefreshPending)?;
    let (render_lock_path, render_lock) = open_existing_render_lock_identity(store)?;
    let source_identity = source_store_identity(store).map_err(ReportViewCatalogError::Build)?;
    let visibility_epoch = store.report_visibility_epoch()?;
    let catalog = read_catalog(&directory)?;
    validate_catalog_authority(catalog.as_ref(), &source_identity, visibility_epoch)?;

    let mut owned = Vec::with_capacity(4);
    owned.push(open_owned_descriptor(
        ReportViewOwnedEntryKind::ManagedDirectory,
        directory.clone(),
    )?);
    if let Some(catalog) = catalog.as_ref() {
        owned.push(open_owned_descriptor(
            ReportViewOwnedEntryKind::Catalog,
            directory.join(CATALOG_FILE_NAME),
        )?);
        for (kind, snapshot) in [
            (
                ReportViewOwnedEntryKind::CurrentSnapshot,
                catalog.current.as_ref(),
            ),
            (
                ReportViewOwnedEntryKind::RetiredSnapshot,
                catalog.retired.as_ref(),
            ),
        ] {
            if let Some(snapshot) = snapshot {
                owned.push(open_owned_descriptor(
                    kind,
                    directory.join(&snapshot.file_name),
                )?);
            }
        }
    }
    let observation = ReportViewOwnershipObservation {
        store,
        render_lock_path,
        render_lock,
        source_identity,
        visibility_epoch,
        catalog,
        owned,
    };
    observation.validate_authority()?;
    let result = use_observation(&observation);
    observation.validate_authority()?;
    Ok(result)
}

// The enclosing storage ownership observation retains and revalidates the store directory.
pub(crate) fn with_optional_report_view_ownership_observation<T>(
    store: &LocalStore,
    use_observation: impl FnOnce(Option<&ReportViewOwnershipObservation<'_>>) -> T,
) -> Result<T, ReportViewCatalogError> {
    let directory = managed_report_view_path(store).map_err(ReportViewCatalogError::Build)?;
    match fs::symlink_metadata(&directory) {
        Ok(_) => with_report_view_ownership_observation(store, |view| use_observation(Some(view))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let result = use_observation(None);
            match fs::symlink_metadata(&directory) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(result),
                Err(error) => Err(error.into()),
                Ok(_) => Err(ReportViewCatalogError::InvalidCatalog),
            }
        }
        Err(error) => Err(error.into()),
    }
}

/// Publishes a complete staging database as an immutable current snapshot.
///
/// The staging handle retains the nonblocking publication guard through the immutable rename,
/// catalog replacement, and cleanup attempt. The previous current snapshot becomes the sole
/// retired snapshot when it belongs to the same visibility epoch.
///
/// # Errors
///
/// Returns [`ReportViewCatalogError`] if authority fences changed, source identity differs, or
/// private bounded publication cannot be completed.
pub fn publish_report_view(
    store: &LocalStore,
    mut staging: ReportViewStaging,
) -> Result<ReportViewPublication, ReportViewCatalogError> {
    let source_identity = source_store_identity(store).map_err(ReportViewCatalogError::Build)?;
    if staging.source_identity() != source_identity {
        return Err(ReportViewCatalogError::SourceMismatch);
    }
    let directory = existing_managed_report_view_directory(store)
        .map_err(ReportViewCatalogError::Build)?
        .ok_or(ReportViewCatalogError::InvalidCatalog)?;
    if staging.path().parent() != Some(directory.as_path()) {
        return Err(ReportViewCatalogError::SourceMismatch);
    }
    staging.close_for_publication()?;
    let status = store.report_status()?;
    let visibility_epoch = store.report_visibility_epoch()?;
    if status.generation != staging.generation() || visibility_epoch != staging.visibility_epoch() {
        return Err(ReportViewCatalogError::SnapshotChanged);
    }
    let previous = read_catalog(&directory)?;
    validate_catalog_authority(previous.as_ref(), &source_identity, visibility_epoch)?;
    cleanup_orphans(&directory, previous.as_ref(), Some(staging.path()))?;
    let snapshot = snapshot_from_staging(&staging)?;
    let immutable_path = directory.join(&snapshot.file_name);
    ensure_path_absent(&immutable_path)?;
    fs::rename(staging.path(), &immutable_path)?;
    private_file(&immutable_path)?;
    sync_directory(&directory)?;

    let retired = previous.and_then(|catalog| catalog.current);
    let catalog = ReportViewCatalog {
        schema_version: CATALOG_SCHEMA_VERSION.to_owned(),
        source_identity,
        visibility_epoch,
        current: Some(snapshot.clone()),
        retired,
    };
    write_catalog(&directory, &catalog)?;
    let cleanup_pending = cleanup_orphans(&directory, Some(&catalog), None).is_err();
    Ok(ReportViewPublication {
        current: snapshot,
        retired: catalog.retired,
        cleanup_pending,
    })
}

/// Returns current published metadata without retaining a snapshot file descriptor.
///
/// # Errors
///
/// Returns [`ReportViewCatalogError`] when the guard is busy or catalog authority is invalid.
pub fn current_report_view(
    store: &LocalStore,
) -> Result<Option<ReportViewSnapshot>, ReportViewCatalogError> {
    ReportViewReadScope::acquire(store)?.current()
}

/// Owns one publication boundary through query reduction, cursor updates and response validation.
/// Connections remain callback-local; only this scope retains the publication lock.
pub(crate) struct ReportViewReadScope<'a> {
    store: &'a LocalStore,
    _guard: ReportRenderGuard,
}

impl<'a> ReportViewReadScope<'a> {
    pub(crate) fn acquire(store: &'a LocalStore) -> Result<Self, ReportViewCatalogError> {
        Ok(Self {
            store,
            _guard: try_guard(store)?,
        })
    }

    pub(crate) fn current(&self) -> Result<Option<ReportViewSnapshot>, ReportViewCatalogError> {
        current_report_view_guarded(self.store)
    }

    pub(crate) fn source_generation(&self) -> Result<u64, ReportViewCatalogError> {
        Ok(self.store.report_status()?.generation)
    }

    pub(crate) fn with_snapshot<T>(
        &self,
        view_id: &str,
        use_snapshot: impl FnOnce(&Connection, &ReportViewSnapshot) -> Result<T, ReportViewCatalogError>,
    ) -> Result<T, ReportViewCatalogError> {
        with_report_view_snapshot_guarded(self.store, view_id, |connection, snapshot, _| {
            use_snapshot(connection, snapshot)
        })
    }
    pub(crate) fn with_snapshot_kernel<T>(
        &self,
        view_id: &str,
        use_snapshot: impl FnOnce(
            &Connection,
            &ReportViewSnapshot,
            ReportViewKernel,
        ) -> Result<T, ReportViewCatalogError>,
    ) -> Result<T, ReportViewCatalogError> {
        with_report_view_snapshot_guarded(self.store, view_id, use_snapshot)
    }
}

fn current_report_view_guarded(
    store: &LocalStore,
) -> Result<Option<ReportViewSnapshot>, ReportViewCatalogError> {
    let Some(directory) =
        existing_managed_report_view_directory(store).map_err(ReportViewCatalogError::Build)?
    else {
        return Ok(None);
    };
    let source_identity = source_store_identity(store).map_err(ReportViewCatalogError::Build)?;
    let visibility_epoch = store.report_visibility_epoch()?;
    let catalog = read_catalog(&directory)?;
    validate_catalog_authority(catalog.as_ref(), &source_identity, visibility_epoch)?;
    if let Some(catalog) = catalog.as_ref() {
        validate_catalog_files(&directory, catalog)?;
    }
    Ok(catalog.and_then(|value| value.current))
}

/// Opens one retained immutable snapshot read-only for the duration of a single request callback.
///
/// The connection and publication guard are both dropped before this function returns. Callers
/// may retain only the returned value and copied snapshot metadata, never a database lease.
///
/// # Errors
///
/// Returns [`ReportViewCatalogError`] for busy, revoked, expired, corrupt, or unsafe snapshots.
pub fn with_report_view_snapshot<T>(
    store: &LocalStore,
    view_id: &str,
    use_snapshot: impl FnOnce(&Connection, &ReportViewSnapshot) -> Result<T, ReportViewCatalogError>,
) -> Result<T, ReportViewCatalogError> {
    ReportViewReadScope::acquire(store)?.with_snapshot(view_id, use_snapshot)
}

fn with_report_view_snapshot_guarded<T>(
    store: &LocalStore,
    view_id: &str,
    use_snapshot: impl FnOnce(
        &Connection,
        &ReportViewSnapshot,
        ReportViewKernel,
    ) -> Result<T, ReportViewCatalogError>,
) -> Result<T, ReportViewCatalogError> {
    validate_view_id(view_id)?;
    let directory = existing_managed_report_view_directory(store)
        .map_err(ReportViewCatalogError::Build)?
        .ok_or(ReportViewCatalogError::RefreshPending)?;
    let source_identity = source_store_identity(store).map_err(ReportViewCatalogError::Build)?;
    let visibility_epoch = store.report_visibility_epoch()?;
    let catalog = read_catalog(&directory)?.ok_or(ReportViewCatalogError::RefreshPending)?;
    validate_catalog_authority(Some(&catalog), &source_identity, visibility_epoch)?;
    validate_catalog_files(&directory, &catalog)?;
    let snapshot = [catalog.current.as_ref(), catalog.retired.as_ref()]
        .into_iter()
        .flatten()
        .find(|snapshot| snapshot.view_id == view_id)
        .ok_or(ReportViewCatalogError::SnapshotExpired)?;
    let path = directory.join(&snapshot.file_name);
    private_file(&path)?;
    validate_snapshot_journal_absence(&path)?;
    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    connection.busy_timeout(Duration::ZERO)?;
    connection.pragma_update(None, "query_only", true)?;
    connection.pragma_update(None, "cache_size", -SQLITE_CACHE_KIB)?;
    let kernel = validate_snapshot_database(&connection, snapshot)?;
    let result = use_snapshot(&connection, snapshot, kernel);
    validate_snapshot_journal_absence(&path)?;
    result
}

/// Removes all report views before a caller advances visibility in a destructive transaction.
///
/// The caller transfers its already-owned publication guard into this operation. The returned
/// token must remain alive until the destructive transaction commits or aborts. Cleanup failure
/// returns an error and prevents destructive work from beginning.
///
/// # Errors
///
/// Returns [`ReportViewCatalogError`] if the guard belongs to another store, the epoch is not
/// exactly the next value, or any managed file cannot be safely retired and synchronized.
pub fn retire_report_views_for_visibility_change(
    store: &LocalStore,
    publication_guard: ReportRenderGuard,
    next_visibility_epoch: u64,
) -> Result<ReportViewRetirement, ReportViewCatalogError> {
    if !report_render_guard_matches(store, &publication_guard)? {
        return Err(ReportViewCatalogError::SourceMismatch);
    }
    let previous_visibility_epoch = store.report_visibility_epoch()?;
    if previous_visibility_epoch.checked_add(1) != Some(next_visibility_epoch) {
        return Err(ReportViewCatalogError::InvalidVisibilityAdvance);
    }
    if let Some(directory) =
        existing_managed_report_view_directory(store).map_err(ReportViewCatalogError::Build)?
    {
        remove_catalog_first(&directory)?;
        cleanup_orphans(&directory, None, None)?;
        sync_directory(&directory)?;
    }
    Ok(ReportViewRetirement {
        _publication_guard: publication_guard,
        previous_visibility_epoch,
        next_visibility_epoch,
    })
}

/// Reconciles interrupted publication files without retaining any database connection.
///
/// # Errors
///
/// Returns [`ReportViewCatalogError`] when the guard is busy or managed state is unsafe.
pub fn recover_report_view_catalog(store: &LocalStore) -> Result<(), ReportViewCatalogError> {
    let guard = try_guard(store)?;
    recover_report_view_catalog_locked(store, &guard)
}

/// Existing-only render exclusion retained from assessment through catalog cleanup.
/// This permit neither creates a lock nor authorizes policy admission or proves E=0.
/// Callers must separately hold the required root/accounting ownership throughout.
pub struct ExistingReportRenderGuard {
    root: PathBuf,
    directory: File,
    database: File,
    guard: ReportRenderGuard,
}

impl fmt::Debug for ExistingReportRenderGuard {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExistingReportRenderGuard")
            .finish_non_exhaustive()
    }
}

impl ExistingReportRenderGuard {
    /// Acquires the existing private render lock once, without creating or waiting.
    ///
    /// # Errors
    /// Returns Busy on contention; missing, unsafe, or replaced paths fail closed.
    pub fn try_acquire(store: &LocalStore) -> Result<Self, ReportViewCatalogError> {
        validate_render_store_authority(store)?;
        let directory = store.authority_directory.try_clone()?;
        let database = store.authority_database.try_clone()?;
        let file = open_existing_render_lock(&store.dir.join(REPORT_RENDER_LOCK_NAME))?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|error| {
            if error.kind() == io::ErrorKind::WouldBlock {
                ReportViewCatalogError::Busy
            } else {
                ReportViewCatalogError::Io(error)
            }
        })?;
        let guard = Self {
            root: store.dir.clone(),
            directory,
            database,
            guard: ReportRenderGuard { file },
        };
        guard.revalidate(store)?;
        Ok(guard)
    }

    /// Revalidates the original store directory/database and locked descriptor against
    /// their paths and the supplied store's retained authority, not just its pathname.
    /// No pathname-based lock acquisition or creation is performed.
    ///
    /// # Errors
    /// Rejects a different root, replacement, absence, or unsafe descriptor.
    pub fn revalidate(&self, store: &LocalStore) -> Result<(), ReportViewCatalogError> {
        if self.root != store.dir {
            return Err(ReportViewCatalogError::SourceMismatch);
        }
        validate_render_store_authority(store)?;
        super::storage_ownership::validate_captured_store_identity(
            &self.root,
            &store.database_path(),
            &self.directory,
            &self.database,
        )?;
        let file = open_existing_render_lock(&self.root.join(REPORT_RENDER_LOCK_NAME))?;
        validate_descriptor_for_kind(&self.guard.file, ReportViewOwnedEntryKind::Catalog)?;
        if !same_descriptor_identity(&self.guard.file, &file)? {
            return Err(ReportViewCatalogError::InvalidCatalog);
        }
        Ok(())
    }
}

fn validate_render_store_authority(store: &LocalStore) -> Result<(), ReportViewCatalogError> {
    super::storage_ownership::validate_captured_store_identity(
        &store.dir,
        &store.database_path(),
        &store.authority_directory,
        &store.authority_database,
    )?;
    super::storage_ownership::validate_connection_path(&store.db, &store.database_path())?;
    Ok(())
}

fn open_existing_render_lock(path: &Path) -> Result<File, ReportViewCatalogError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(super::no_follow_flag() | super::nonblocking_open_flag());
    }
    let file = options.open(path)?;
    validate_descriptor_for_kind(&file, ReportViewOwnedEntryKind::Catalog)?;
    Ok(file)
}

/// Reuses the retained existing-only permit for catalog cleanup, without reacquiring a lock.
/// The caller must retain its separate root/accounting ownership across assessment and cleanup.
/// This does not change legacy recovery or establish policy admission or a zero-write estimate.
///
/// # Errors
/// Rejects wrong-root or replaced permits before cleanup and revalidates after cleanup.
/// Cleanup failures remain primary when the final identity check also fails.
pub fn recover_report_view_catalog_with_existing_guard(
    store: &LocalStore,
    guard: &ExistingReportRenderGuard,
) -> Result<(), ReportViewCatalogError> {
    guard.revalidate(store)?;
    let result = recover_report_view_catalog_locked(store, &guard.guard);
    let postcheck = guard.revalidate(store);
    result.and(postcheck)
}

fn recover_report_view_catalog_locked(
    store: &LocalStore,
    _guard: &ReportRenderGuard,
) -> Result<(), ReportViewCatalogError> {
    let Some(directory) =
        existing_managed_report_view_directory(store).map_err(ReportViewCatalogError::Build)?
    else {
        return Ok(());
    };
    let source_identity = source_store_identity(store).map_err(ReportViewCatalogError::Build)?;
    let visibility_epoch = store.report_visibility_epoch()?;
    let catalog = read_catalog(&directory)?;
    match catalog {
        Some(catalog)
            if catalog.source_identity == source_identity
                && catalog.visibility_epoch == visibility_epoch =>
        {
            validate_catalog(&catalog)?;
            cleanup_orphans(&directory, Some(&catalog), None)?;
        }
        Some(catalog) if catalog.source_identity != source_identity => {
            return Err(ReportViewCatalogError::SourceMismatch);
        }
        Some(_) => {
            remove_catalog_first(&directory)?;
            cleanup_orphans(&directory, None, None)?;
        }
        None => cleanup_orphans(&directory, None, None)?,
    }
    sync_directory(&directory)?;
    Ok(())
}

pub(crate) fn managed_report_view_directory(
    store: &LocalStore,
) -> Result<PathBuf, ReportViewBuildError> {
    let directory = managed_report_view_path(store)?;
    private_dir(&directory)?;
    validate_managed_directory(&directory)
}

fn existing_managed_report_view_directory(
    store: &LocalStore,
) -> Result<Option<PathBuf>, ReportViewBuildError> {
    let directory = managed_report_view_path(store)?;
    match validate_existing_private_dir(&directory) {
        Ok(()) => {}
        Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    validate_managed_directory(&directory).map(Some)
}

fn managed_report_view_path(store: &LocalStore) -> Result<PathBuf, ReportViewBuildError> {
    let database = fs::canonicalize(store.database_path())?;
    private_file(&database)?;
    let root = database
        .parent()
        .ok_or(ReportViewBuildError::InvalidStagingState)?;
    Ok(root.join(MANAGED_DIRECTORY_NAME))
}

fn validate_managed_directory(directory: &Path) -> Result<PathBuf, ReportViewBuildError> {
    let expected_parent = directory
        .parent()
        .ok_or(ReportViewBuildError::InvalidStagingState)?;
    let directory = fs::canonicalize(directory)?;
    if directory.parent() != Some(expected_parent) {
        return Err(ReportViewBuildError::InvalidStagingState);
    }
    Ok(directory)
}

pub(crate) fn source_store_identity(store: &LocalStore) -> Result<String, ReportViewBuildError> {
    let database = fs::canonicalize(store.database_path())?;
    private_file(&database)?;
    let mut digest = Sha256::new();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::MetadataExt;
        digest.update(database.as_os_str().as_bytes());
        let metadata = fs::metadata(&database)?;
        digest.update(metadata.dev().to_le_bytes());
        digest.update(metadata.ino().to_le_bytes());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        digest.update(database.to_string_lossy().as_bytes());
        let metadata = fs::metadata(&database)?;
        digest.update(
            metadata
                .volume_serial_number()
                .unwrap_or_default()
                .to_le_bytes(),
        );
        digest.update(metadata.file_index().unwrap_or_default().to_le_bytes());
    }
    #[cfg(not(any(unix, windows)))]
    digest.update(database.to_string_lossy().as_bytes());
    Ok(format!(
        "store:sha256:{}",
        lowercase_hex(&digest.finalize())
    ))
}

pub(crate) fn prepare_managed_report_view_directory(
    store: &LocalStore,
    directory: &Path,
    visibility_epoch: u64,
) -> Result<(), ReportViewBuildError> {
    let source_identity = source_store_identity(store)?;
    let catalog = read_catalog(directory).map_err(|_| ReportViewBuildError::InvalidStagingState)?;
    match catalog {
        Some(catalog)
            if catalog.source_identity == source_identity
                && catalog.visibility_epoch == visibility_epoch =>
        {
            validate_catalog(&catalog).map_err(|_| ReportViewBuildError::InvalidStagingState)?;
            cleanup_orphans(directory, Some(&catalog), None)
                .map_err(|_| ReportViewBuildError::InvalidStagingState)?;
        }
        Some(catalog) if catalog.source_identity != source_identity => {
            return Err(ReportViewBuildError::InvalidStagingState);
        }
        Some(_) => {
            remove_catalog_first(directory)
                .map_err(|_| ReportViewBuildError::InvalidStagingState)?;
            cleanup_orphans(directory, None, None)
                .map_err(|_| ReportViewBuildError::InvalidStagingState)?;
        }
        None => cleanup_orphans(directory, None, None)
            .map_err(|_| ReportViewBuildError::InvalidStagingState)?,
    }
    Ok(())
}

fn try_guard(store: &LocalStore) -> Result<ReportRenderGuard, ReportViewCatalogError> {
    store
        .try_acquire_report_render_guard()?
        .ok_or(ReportViewCatalogError::Busy)
}

fn open_existing_render_lock_identity(
    store: &LocalStore,
) -> Result<(PathBuf, File), ReportViewCatalogError> {
    let path = store.dir.join(REPORT_RENDER_LOCK_NAME);
    private_file(&path)?;
    let file = open_private_read(&path)?;
    validate_descriptor_for_kind(&file, ReportViewOwnedEntryKind::Catalog)?;
    validate_render_lock_identity(&path, &file)?;
    Ok((path, file))
}

fn validate_render_lock_identity(
    path: &Path,
    retained: &File,
) -> Result<(), ReportViewCatalogError> {
    let named = open_private_read(path)?;
    validate_descriptor_for_kind(&named, ReportViewOwnedEntryKind::Catalog)?;
    if !same_descriptor_identity(retained, &named)? {
        return Err(ReportViewCatalogError::InvalidCatalog);
    }
    Ok(())
}

fn open_owned_descriptor(
    kind: ReportViewOwnedEntryKind,
    path: PathBuf,
) -> Result<OwnedDescriptor, ReportViewCatalogError> {
    let file = open_owned_descriptor_for_validation(kind, &path)?;
    let owned = OwnedDescriptor { kind, path, file };
    validate_named_identity(&owned)?;
    Ok(owned)
}

fn validate_named_identity(owned: &OwnedDescriptor) -> Result<(), ReportViewCatalogError> {
    let named = open_owned_descriptor_for_validation(owned.kind, &owned.path)?;
    if !same_descriptor_identity(&owned.file, &named)? {
        return Err(ReportViewCatalogError::InvalidCatalog);
    }
    Ok(())
}

fn open_owned_descriptor_for_validation(
    kind: ReportViewOwnedEntryKind,
    path: &Path,
) -> Result<File, ReportViewCatalogError> {
    let file = if kind == ReportViewOwnedEntryKind::ManagedDirectory {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(no_follow_flag());
        }
        options.open(path)?
    } else {
        open_private_read(path)?
    };
    validate_descriptor_for_kind(&file, kind)?;
    Ok(file)
}

fn validate_descriptor_for_kind(
    file: &File,
    kind: ReportViewOwnedEntryKind,
) -> Result<(), ReportViewCatalogError> {
    let metadata = file.metadata()?;
    let valid_kind = if kind == ReportViewOwnedEntryKind::ManagedDirectory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    };
    if !valid_kind {
        return Err(ReportViewCatalogError::InvalidCatalog);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let expected_mode = if metadata.is_dir() { 0o700 } else { 0o600 };
        if metadata.permissions().mode() & 0o777 != expected_mode
            || (metadata.is_file() && metadata.nlink() != 1)
        {
            return Err(ReportViewCatalogError::InvalidCatalog);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn same_descriptor_identity(left: &File, right: &File) -> Result<bool, ReportViewCatalogError> {
    use std::os::unix::fs::MetadataExt;
    let left = left.metadata()?;
    let right = right.metadata()?;
    Ok(left.dev() == right.dev() && left.ino() == right.ino())
}

#[cfg(windows)]
fn same_descriptor_identity(left: &File, right: &File) -> Result<bool, ReportViewCatalogError> {
    use std::os::windows::fs::MetadataExt;
    let left = left.metadata()?;
    let right = right.metadata()?;
    Ok(left.volume_serial_number() == right.volume_serial_number()
        && left.file_index() == right.file_index())
}

#[cfg(not(any(unix, windows)))]
fn same_descriptor_identity(_left: &File, _right: &File) -> Result<bool, ReportViewCatalogError> {
    Ok(false)
}

#[cfg(unix)]
fn report_render_guard_matches(
    store: &LocalStore,
    guard: &ReportRenderGuard,
) -> Result<bool, ReportViewCatalogError> {
    use std::os::unix::fs::MetadataExt;

    let lock_path = store.dir.join(REPORT_RENDER_LOCK_NAME);
    match private_file(&lock_path) {
        Ok(()) => {}
        Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    }
    let expected = fs::metadata(lock_path)?;
    let actual = guard.file.metadata()?;
    Ok(expected.dev() == actual.dev() && expected.ino() == actual.ino())
}

#[cfg(windows)]
fn report_render_guard_matches(
    store: &LocalStore,
    guard: &ReportRenderGuard,
) -> Result<bool, ReportViewCatalogError> {
    use std::os::windows::fs::MetadataExt;

    let lock_path = store.dir.join(REPORT_RENDER_LOCK_NAME);
    match private_file(&lock_path) {
        Ok(()) => {}
        Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    }
    let expected = fs::metadata(lock_path)?;
    let actual = guard.file.metadata()?;
    Ok(
        expected.volume_serial_number() == actual.volume_serial_number()
            && expected.file_index() == actual.file_index(),
    )
}

#[cfg(not(any(unix, windows)))]
fn report_render_guard_matches(
    _store: &LocalStore,
    _guard: &ReportRenderGuard,
) -> Result<bool, ReportViewCatalogError> {
    Ok(false)
}

fn snapshot_from_staging(
    staging: &ReportViewStaging,
) -> Result<ReportViewSnapshot, ReportViewCatalogError> {
    let name = staging
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(ReportViewCatalogError::InvalidCatalog)?;
    let identity_material = format!(
        "{}\0{}\0{}\0{}\0{name}",
        staging.source_identity(),
        staging.generation(),
        staging.visibility_epoch(),
        staging.rate_fingerprint()
    );
    let view_id = hash_opaque_identifier(&identity_material)
        .strip_prefix("id:sha256:")
        .ok_or(ReportViewCatalogError::InvalidCatalog)?
        .to_owned();
    validate_view_id(&view_id)?;
    Ok(ReportViewSnapshot {
        file_name: format!("{IMMUTABLE_FILE_PREFIX}{view_id}{IMMUTABLE_FILE_SUFFIX}"),
        view_id,
        generation: staging.generation(),
        visibility_epoch: staging.visibility_epoch(),
        records: staging.records(),
        rate_fingerprint: staging.rate_fingerprint().to_owned(),
        generated_at: staging.generated_at().to_owned(),
    })
}

fn validate_catalog_authority(
    catalog: Option<&ReportViewCatalog>,
    source_identity: &str,
    visibility_epoch: u64,
) -> Result<(), ReportViewCatalogError> {
    let Some(catalog) = catalog else {
        return Ok(());
    };
    validate_catalog(catalog)?;
    if catalog.source_identity != source_identity {
        return Err(ReportViewCatalogError::SourceMismatch);
    }
    if catalog.visibility_epoch != visibility_epoch {
        return Err(ReportViewCatalogError::RefreshPending);
    }
    Ok(())
}

fn validate_catalog(catalog: &ReportViewCatalog) -> Result<(), ReportViewCatalogError> {
    let source_digest = catalog.source_identity.strip_prefix("store:sha256:");
    if catalog.schema_version != CATALOG_SCHEMA_VERSION
        || source_digest.is_none_or(|digest| {
            digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
    {
        return Err(ReportViewCatalogError::InvalidCatalog);
    }
    let mut ids = BTreeSet::new();
    for snapshot in [catalog.current.as_ref(), catalog.retired.as_ref()]
        .into_iter()
        .flatten()
    {
        validate_snapshot(snapshot, catalog.visibility_epoch)?;
        if !ids.insert(snapshot.view_id.as_str()) {
            return Err(ReportViewCatalogError::InvalidCatalog);
        }
    }
    if catalog.current.is_none() && catalog.retired.is_some() {
        return Err(ReportViewCatalogError::InvalidCatalog);
    }
    Ok(())
}

fn validate_snapshot(
    snapshot: &ReportViewSnapshot,
    visibility_epoch: u64,
) -> Result<(), ReportViewCatalogError> {
    validate_view_id(&snapshot.view_id)?;
    let expected_name = format!(
        "{IMMUTABLE_FILE_PREFIX}{}{IMMUTABLE_FILE_SUFFIX}",
        snapshot.view_id
    );
    if snapshot.file_name != expected_name
        || snapshot.visibility_epoch != visibility_epoch
        || !valid_rate_fingerprint(&snapshot.rate_fingerprint)
        || snapshot.generated_at.is_empty()
        || snapshot.generated_at.len() > 64
        || !snapshot.generated_at.is_ascii()
    {
        return Err(ReportViewCatalogError::InvalidCatalog);
    }
    Ok(())
}

fn validate_view_id(view_id: &str) -> Result<(), ReportViewCatalogError> {
    if view_id.len() == 64
        && view_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(ReportViewCatalogError::InvalidCatalog)
    }
}

fn valid_rate_fingerprint(value: &str) -> bool {
    value == super::report_view::MISSING_RATE_FINGERPRINT
        || (value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
}

fn read_catalog(directory: &Path) -> Result<Option<ReportViewCatalog>, ReportViewCatalogError> {
    let path = directory.join(CATALOG_FILE_NAME);
    let file = match open_private_read(&path) {
        Ok(file) => file,
        Err(ReportViewCatalogError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let bytes = file.metadata()?.len();
    if bytes > MAX_CATALOG_BYTES {
        return Err(ReportViewCatalogError::CatalogCapacityExceeded);
    }
    let mut body = Vec::with_capacity(
        usize::try_from(bytes).map_err(|_| ReportViewCatalogError::CatalogCapacityExceeded)?,
    );
    file.take(MAX_CATALOG_BYTES + 1).read_to_end(&mut body)?;
    let body_bytes =
        u64::try_from(body.len()).map_err(|_| ReportViewCatalogError::CatalogCapacityExceeded)?;
    if body_bytes > MAX_CATALOG_BYTES {
        return Err(ReportViewCatalogError::CatalogCapacityExceeded);
    }
    let catalog = serde_json::from_slice(&body)?;
    validate_catalog(&catalog)?;
    Ok(Some(catalog))
}

fn write_catalog(
    directory: &Path,
    catalog: &ReportViewCatalog,
) -> Result<(), ReportViewCatalogError> {
    validate_catalog(catalog)?;
    let body = serde_json::to_vec(catalog)?;
    let body_bytes =
        u64::try_from(body.len()).map_err(|_| ReportViewCatalogError::CatalogCapacityExceeded)?;
    if body_bytes > MAX_CATALOG_BYTES {
        return Err(ReportViewCatalogError::CatalogCapacityExceeded);
    }
    let final_path = directory.join(CATALOG_FILE_NAME);
    match fs::symlink_metadata(&final_path) {
        Ok(_) => private_file(&final_path)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let (temp_path, mut temp) = create_catalog_temp(directory)?;
    let result = (|| {
        temp.write_all(&body)?;
        temp.sync_all()?;
        drop(temp);
        fs::rename(&temp_path, &final_path)?;
        private_file(&final_path)?;
        sync_directory(directory)?;
        Ok::<(), ReportViewCatalogError>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn create_catalog_temp(directory: &Path) -> Result<(PathBuf, File), ReportViewCatalogError> {
    for _ in 0..MAX_TEMP_COLLISIONS {
        let sequence = CATALOG_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            "{CATALOG_TEMP_PREFIX}{}.{sequence}",
            std::process::id()
        ));
        match private_create_new(&path) {
            Ok(file) => return Ok((path, file)),
            Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(ReportViewCatalogError::InvalidCatalog)
}

fn remove_catalog_first(directory: &Path) -> Result<(), ReportViewCatalogError> {
    let path = directory.join(CATALOG_FILE_NAME);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(ReportViewCatalogError::Store(StoreError::Symlink))
        }
        Ok(metadata) if !metadata.is_file() => {
            Err(ReportViewCatalogError::Store(StoreError::InvalidPath))
        }
        Ok(_) => {
            private_file(&path)?;
            fs::remove_file(path)?;
            sync_directory(directory)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn ensure_path_absent(path: &Path) -> Result<(), ReportViewCatalogError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(ReportViewCatalogError::InvalidCatalog),
        Err(error) => Err(error.into()),
    }
}

fn cleanup_orphans(
    directory: &Path,
    catalog: Option<&ReportViewCatalog>,
    preserved_staging: Option<&Path>,
) -> Result<(), ReportViewCatalogError> {
    if let Some(catalog) = catalog {
        validate_catalog_files(directory, catalog)?;
    }
    let preserved_name = match preserved_staging {
        Some(path)
            if path.parent() == Some(directory)
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(STAGING_FILE_PREFIX)) =>
        {
            path.file_name().and_then(|name| name.to_str())
        }
        Some(_) => return Err(ReportViewCatalogError::InvalidCatalog),
        None => None,
    };
    let retained: BTreeSet<&str> = catalog
        .into_iter()
        .flat_map(|catalog| [catalog.current.as_ref(), catalog.retired.as_ref()])
        .flatten()
        .map(|snapshot| snapshot.file_name.as_str())
        .collect();
    let mut removals = Vec::new();
    for (index, entry) in fs::read_dir(directory)?.enumerate() {
        if index >= MAX_MANAGED_DIRECTORY_ENTRIES {
            return Err(ReportViewCatalogError::InvalidCatalog);
        }
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() || !file_type.is_file() {
            return Err(ReportViewCatalogError::InvalidCatalog);
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| ReportViewCatalogError::InvalidCatalog)?;
        if name == CATALOG_FILE_NAME
            || retained.contains(name.as_str())
            || preserved_name == Some(name.as_str())
        {
            continue;
        }
        let managed = name.starts_with(STAGING_FILE_PREFIX)
            || name.starts_with(CATALOG_TEMP_PREFIX)
            || (name.starts_with(IMMUTABLE_FILE_PREFIX)
                && (name.ends_with(IMMUTABLE_FILE_SUFFIX) || name.ends_with(".sqlite3-journal")));
        if !managed {
            return Err(ReportViewCatalogError::InvalidCatalog);
        }
        removals.push(entry.path());
    }
    for path in removals {
        private_file(&path)?;
        fs::remove_file(path)?;
    }
    Ok(())
}

fn validate_catalog_files(
    directory: &Path,
    catalog: &ReportViewCatalog,
) -> Result<(), ReportViewCatalogError> {
    for snapshot in [catalog.current.as_ref(), catalog.retired.as_ref()]
        .into_iter()
        .flatten()
    {
        private_file(&directory.join(&snapshot.file_name))?;
    }
    Ok(())
}

fn validate_catalog_snapshot_databases(
    directory: &Path,
    catalog: &ReportViewCatalog,
) -> Result<(), ReportViewCatalogError> {
    for snapshot in [catalog.current.as_ref(), catalog.retired.as_ref()]
        .into_iter()
        .flatten()
    {
        let path = directory.join(&snapshot.file_name);
        private_file(&path)?;
        validate_snapshot_journal_absence(&path)?;
        let connection = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        connection.busy_timeout(Duration::ZERO)?;
        connection.pragma_update(None, "query_only", true)?;
        connection.pragma_update(None, "cache_size", -SQLITE_CACHE_KIB)?;
        validate_snapshot_database(&connection, snapshot)?;
        validate_snapshot_journal_absence(&path)?;
    }
    Ok(())
}

fn validate_snapshot_journal_absence(path: &Path) -> Result<(), ReportViewCatalogError> {
    // A missing snapshot or parent must not be mistaken for an absent sibling journal.
    private_file(path)?;
    match crate::migration_admission::open_private_journal(&path.with_extension("sqlite3-journal"))?
    {
        None => Ok(()),
        Some(_) => Err(ReportViewCatalogError::InvalidCatalog),
    }
}

fn validate_snapshot_database(
    connection: &Connection,
    snapshot: &ReportViewSnapshot,
) -> Result<ReportViewKernel, ReportViewCatalogError> {
    let version: String = connection.query_row(
        "SELECT value FROM metadata WHERE key='schema_version'",
        [],
        |row| row.get(0),
    )?;
    let kernel = match version.as_str() {
        "agent_observability.report_view_staging.v1" => ReportViewKernel::V1,
        "agent_observability.report_view_staging.v2" => ReportViewKernel::V2,
        _ => return Err(ReportViewCatalogError::InvalidCatalog),
    };
    let metadata_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM (SELECT 1 FROM metadata LIMIT 8)",
        [],
        |row| row.get(0),
    )?;
    if metadata_count != 7 {
        return Err(ReportViewCatalogError::InvalidCatalog);
    }
    for (key, expected) in [
        ("source_generation", snapshot.generation.to_string()),
        ("visibility_epoch", snapshot.visibility_epoch.to_string()),
        ("rate_fingerprint", snapshot.rate_fingerprint.clone()),
        ("generated_at", snapshot.generated_at.clone()),
        ("records", snapshot.records.to_string()),
        ("complete", "1".to_owned()),
    ] {
        let actual: Option<String> = connection
            .query_row("SELECT value FROM metadata WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        if actual.as_deref() != Some(expected.as_str()) {
            return Err(ReportViewCatalogError::InvalidCatalog);
        }
    }
    validate_kernel_indexes(connection, kernel)?;
    Ok(kernel)
}

fn validate_kernel_indexes(
    connection: &Connection,
    kernel: ReportViewKernel,
) -> Result<(), ReportViewCatalogError> {
    let mut keys = connection
        .prepare("SELECT name,type,pk FROM pragma_table_info('spans') WHERE pk<>0 LIMIT 2")?;
    let keys = keys
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if keys != vec![("source_order".into(), "INTEGER".into(), 1)] {
        return Err(ReportViewCatalogError::InvalidCatalog);
    }

    for (name, dimension) in [
        ("spans_repo_order_idx", "repo"),
        ("spans_session_order_idx", "session_id"),
        ("spans_turn_order_idx", "turn_id"),
        ("spans_agent_order_idx", "agent"),
        ("spans_model_order_idx", "model"),
        ("spans_stable_order_idx", ""),
        ("spans_trace_repo_idx", ""),
        ("spans_trace_order_idx", ""),
    ] {
        let expected = match name {
            "spans_stable_order_idx" => vec!["start_time_unix_ms", "trace_id", "span_id"],
            "spans_trace_repo_idx" => vec!["trace_id", "repo"],
            "spans_trace_order_idx" => vec!["trace_id", "start_time_unix_ms", "span_id"],
            _ => match kernel {
                ReportViewKernel::V1 => {
                    vec![dimension, "start_time_unix_ms", "trace_id", "span_id"]
                }
                ReportViewKernel::V2 => vec![dimension, "source_order"],
            },
        };
        let definition: Option<(i64, i64, String)> = connection
            .query_row(
                "SELECT [unique],partial,origin FROM pragma_index_list('spans') WHERE name=?1",
                [name],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if definition != Some((0, 0, "c".into())) {
            return Err(ReportViewCatalogError::InvalidCatalog);
        }
        let mut statement = connection.prepare(
            "SELECT name,desc,coll FROM pragma_index_xinfo(?1) WHERE key=1 ORDER BY seqno LIMIT 6",
        )?;
        let columns = statement
            .query_map([name], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if columns.len() != expected.len()
            || columns
                .iter()
                .zip(expected)
                .any(|((actual, descending, collation), expected)| {
                    actual.as_deref() != Some(expected) || *descending != 0 || collation != "BINARY"
                })
        {
            return Err(ReportViewCatalogError::InvalidCatalog);
        }
    }
    Ok(())
}

fn open_private_read(path: &Path) -> Result<File, ReportViewCatalogError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(no_follow_flag() | super::nonblocking_open_flag());
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(ReportViewCatalogError::Store(StoreError::InvalidPath));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o777 != 0o600 {
            return Err(ReportViewCatalogError::Store(
                StoreError::InsecurePermissions,
            ));
        }
    }
    Ok(file)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
const fn no_follow_flag() -> i32 {
    0x20_000
}

#[cfg(target_os = "macos")]
const fn no_follow_flag() -> i32 {
    0x100
}

fn sync_directory(directory: &Path) -> Result<(), ReportViewCatalogError> {
    File::open(directory)?.sync_all()?;
    Ok(())
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report_view::MISSING_RATE_FINGERPRINT;

    const TEST_ADMISSION: u64 = 128 * 1024 * 1024;

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agent-observability-report-view-catalog-{label}-{}",
            std::process::id()
        ))
    }

    fn open_store(label: &str) -> (PathBuf, LocalStore) {
        let directory = temp_dir(label);
        let _ = fs::remove_dir_all(&directory);
        let store = LocalStore::open(&directory).unwrap();
        (directory, store)
    }

    fn build(store: &LocalStore) -> ReportViewStaging {
        crate::build_report_view_staging(store, MISSING_RATE_FINGERPRINT, TEST_ADMISSION, None)
            .unwrap()
    }

    #[test]
    fn snapshot_journals_rejected_before_open_for_current_and_retired() {
        let (directory, store) = open_store("snapshot-journal-preopen");
        let first = publish_report_view(&store, build(&store)).unwrap();
        store.invalidate_report().unwrap();
        let second = publish_report_view(&store, build(&store)).unwrap();
        for snapshot in [first.current(), second.current()] {
            let path = directory
                .join(MANAGED_DIRECTORY_NAME)
                .join(&snapshot.file_name);
            let original = fs::read(&path).unwrap();
            let journal = path.with_extension("sqlite3-journal");
            for bytes in [&[][..], &[0_u8; 28][..], b"malformed".as_slice()] {
                use std::io::Write;
                private_create_new(&journal)
                    .unwrap()
                    .write_all(bytes)
                    .unwrap();
                for invalid_database in [false, true] {
                    if invalid_database {
                        fs::write(&path, b"invalid database").unwrap();
                    }
                    let result =
                        with_report_view_snapshot::<()>(&store, snapshot.view_id(), |_, _| {
                            panic!("journal must prevent callback")
                        });
                    assert!(matches!(
                        result,
                        Err(ReportViewCatalogError::InvalidCatalog)
                    ));
                    assert!(matches!(
                        with_report_view_ownership_observation(&store, |_| {
                            panic!("journal must prevent ownership callback")
                        }),
                        Err(ReportViewCatalogError::InvalidCatalog)
                    ));
                    assert_eq!(fs::read(&journal).unwrap(), bytes);
                    assert_eq!(
                        fs::read(&path).unwrap(),
                        if invalid_database {
                            b"invalid database".to_vec()
                        } else {
                            original.clone()
                        }
                    );
                }
                fs::write(&path, &original).unwrap();
                fs::remove_file(&journal).unwrap();
            }
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn snapshot_callback_created_journals_are_rejected_and_preserved() {
        let (directory, store) = open_store("snapshot-journal-callback");
        let first = publish_report_view(&store, build(&store)).unwrap();
        store.invalidate_report().unwrap();
        let second = publish_report_view(&store, build(&store)).unwrap();
        for snapshot in [first.current(), second.current()] {
            let path = directory
                .join(MANAGED_DIRECTORY_NAME)
                .join(&snapshot.file_name);
            let original = fs::read(&path).unwrap();
            let journal = path.with_extension("sqlite3-journal");
            let result = with_report_view_snapshot(&store, snapshot.view_id(), |_, _| {
                drop(private_create_new(&journal).unwrap());
                Ok(())
            });
            assert!(matches!(
                result,
                Err(ReportViewCatalogError::InvalidCatalog)
            ));
            assert_eq!(fs::read(&journal).unwrap(), b"");
            fs::remove_file(&journal).unwrap();
            let result = with_report_view_ownership_observation(&store, |_| {
                drop(private_create_new(&journal).unwrap());
            });
            assert!(matches!(
                result,
                Err(ReportViewCatalogError::InvalidCatalog)
            ));
            assert_eq!(fs::read(&journal).unwrap(), b"");
            assert_eq!(fs::read(&path).unwrap(), original);
            fs::remove_file(&journal).unwrap();
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_journal_unsafe_entries_preserve_store_errors() {
        let (directory, store) = open_store("snapshot-journal-unsafe");
        let published = publish_report_view(&store, build(&store)).unwrap();
        let path = directory
            .join(MANAGED_DIRECTORY_NAME)
            .join(&published.current().file_name);
        let journal = path.with_extension("sqlite3-journal");
        let target = directory.join("journal-target");
        drop(private_create_new(&target).unwrap());
        for kind in ["symlink", "hardlink", "directory"] {
            match kind {
                "symlink" => std::os::unix::fs::symlink(&target, &journal).unwrap(),
                "hardlink" => fs::hard_link(&target, &journal).unwrap(),
                _ => fs::create_dir(&journal).unwrap(),
            }
            let query =
                with_report_view_snapshot::<()>(&store, published.current().view_id(), |_, _| {
                    panic!("unsafe journal accepted")
                });
            let owner = with_report_view_ownership_observation(&store, |_| {
                panic!("unsafe journal accepted")
            });
            for result in [query, owner] {
                assert!(matches!(result, Err(ReportViewCatalogError::Store(_))));
            }
            assert!(fs::symlink_metadata(&journal).is_ok());
            if kind == "directory" {
                fs::remove_dir(&journal).unwrap();
            } else {
                fs::remove_file(&journal).unwrap();
            }
        }
        assert_eq!(fs::read(&target).unwrap(), b"");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn snapshot_journal_postcheck_rejects_missing_parent() {
        let (directory, store) = open_store("snapshot-journal-missing-parent");
        let published = publish_report_view(&store, build(&store)).unwrap();
        let managed = directory.join(MANAGED_DIRECTORY_NAME);
        let displaced = directory.join("displaced-views");
        let result = with_report_view_snapshot(&store, published.current().view_id(), |_, _| {
            fs::rename(&managed, &displaced).unwrap();
            Ok(())
        });
        assert!(matches!(
            result,
            Err(ReportViewCatalogError::Store(StoreError::Io(_)))
        ));
        assert!(displaced.is_dir());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn nested_optional_ownership_preserves_absence_and_rejects_appearance() {
        let (directory, store) = open_store("nested-absent");
        let managed = directory.join(MANAGED_DIRECTORY_NAME);
        crate::storage_ownership::with_optional_report_reader_storage_ownership(
            &directory,
            |owner| {
                owner
                    .unwrap()
                    .with_report_view_ownership_observation(|view| {
                        assert!(view.is_none());
                    })
            },
        )
        .unwrap()
        .unwrap();
        assert!(!managed.exists());
        assert!(!directory.join(REPORT_RENDER_LOCK_NAME).exists());
        let result =
            crate::storage_ownership::with_storage_ownership_observation(&store, |owner| {
                owner.with_report_view_ownership_observation(|view| {
                    assert!(view.is_none());
                    fs::create_dir(&managed).unwrap();
                })
            })
            .unwrap();
        assert!(matches!(
            result,
            Err(ReportViewCatalogError::InvalidCatalog)
        ));
        assert!(managed.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn nested_optional_ownership_preserves_published_membership() {
        let (directory, store) = open_store("nested-membership");
        publish_report_view(&store, build(&store)).unwrap();
        store.invalidate_report().unwrap();
        publish_report_view(&store, build(&store)).unwrap();
        crate::storage_ownership::with_optional_report_reader_storage_ownership(
            &directory,
            |owner| {
                owner
                    .unwrap()
                    .with_report_view_ownership_observation(|view| {
                        let view = view.unwrap();
                        assert_eq!(
                            view.entries()
                                .map(ReportViewOwnedEntry::kind)
                                .collect::<BTreeSet<_>>(),
                            BTreeSet::from([
                                ReportViewOwnedEntryKind::ManagedDirectory,
                                ReportViewOwnedEntryKind::Catalog,
                                ReportViewOwnedEntryKind::CurrentSnapshot,
                                ReportViewOwnedEntryKind::RetiredSnapshot
                            ])
                        );
                        for entry in view.entries() {
                            assert!(
                                view.recognizes(entry.path(), &File::open(entry.path()).unwrap())
                                    .unwrap()
                            );
                        }
                    })
            },
        )
        .unwrap()
        .unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn nested_optional_ownership_keeps_present_store_errors() {
        let (directory, store) = open_store("nested-errors");
        publish_report_view(&store, build(&store)).unwrap();
        store
            .db
            .execute(
                "UPDATE metadata SET value='1' WHERE key='report_visibility_epoch'",
                [],
            )
            .unwrap();
        let result =
            crate::storage_ownership::with_storage_ownership_observation(&store, |owner| {
                owner.with_report_view_ownership_observation(|_| panic!("stale view accepted"))
            })
            .unwrap();
        assert!(matches!(
            result,
            Err(ReportViewCatalogError::RefreshPending)
        ));
        store
            .db
            .execute(
                "UPDATE metadata SET value='0' WHERE key='report_visibility_epoch'",
                [],
            )
            .unwrap();
        fs::remove_file(directory.join(REPORT_RENDER_LOCK_NAME)).unwrap();
        let result =
            crate::storage_ownership::with_storage_ownership_observation(&store, |owner| {
                owner.with_report_view_ownership_observation(|_| panic!("missing lock accepted"))
            })
            .unwrap();
        assert!(result.is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn nested_optional_ownership_rejects_replacement_symlink_and_invalid_catalog() {
        let (directory, store) = open_store("nested-invalid");
        publish_report_view(&store, build(&store)).unwrap();
        let managed = directory.join(MANAGED_DIRECTORY_NAME);
        let displaced = directory.join("displaced-view");
        let result =
            crate::storage_ownership::with_storage_ownership_observation(&store, |owner| {
                owner.with_report_view_ownership_observation(|view| {
                    assert!(view.is_some());
                    fs::rename(&managed, &displaced).unwrap();
                    fs::create_dir(&managed).unwrap();
                })
            })
            .unwrap();
        assert!(result.is_err());
        fs::remove_dir(&managed).unwrap();
        std::os::unix::fs::symlink(&displaced, &managed).unwrap();
        let result =
            crate::storage_ownership::with_storage_ownership_observation(&store, |owner| {
                owner.with_report_view_ownership_observation(|_| panic!("invalid path accepted"))
            })
            .unwrap();
        assert!(result.is_err());
        fs::remove_file(&managed).unwrap();
        fs::rename(&displaced, &managed).unwrap();
        fs::write(managed.join(CATALOG_FILE_NAME), b"invalid catalog").unwrap();
        let result =
            crate::storage_ownership::with_storage_ownership_observation(&store, |owner| {
                owner.with_report_view_ownership_observation(|_| panic!("invalid catalog accepted"))
            })
            .unwrap();
        assert!(result.is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn metadata_reads_and_recovery_do_not_create_a_missing_managed_directory() {
        let (directory, store) = open_store("readonly-missing");
        let managed = directory.join(MANAGED_DIRECTORY_NAME);
        assert!(!managed.exists());
        assert!(current_report_view(&store).unwrap().is_none());
        assert!(matches!(
            with_report_view_snapshot(&store, &"0".repeat(64), |_, _| Ok(())),
            Err(ReportViewCatalogError::RefreshPending)
        ));
        recover_report_view_catalog(&store).unwrap();
        assert!(!managed.exists());

        private_dir(&managed).unwrap();
        fs::remove_dir(&managed).unwrap();
        assert!(current_report_view(&store).unwrap().is_none());
        assert!(!managed.exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[cfg(unix)]
    #[test]
    fn metadata_reads_reject_unsafe_managed_directories_without_repairing_them() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let (directory, store) = open_store("readonly-unsafe-directory");
        let managed = directory.join(MANAGED_DIRECTORY_NAME);
        fs::create_dir(&managed).unwrap();
        fs::set_permissions(&managed, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            current_report_view(&store),
            Err(ReportViewCatalogError::Build(ReportViewBuildError::Store(
                StoreError::InsecurePermissions
            )))
        ));
        assert_eq!(
            fs::metadata(&managed).unwrap().permissions().mode() & 0o777,
            0o755
        );

        fs::remove_dir(&managed).unwrap();
        let target = directory.join("managed-target");
        private_dir(&target).unwrap();
        symlink(&target, &managed).unwrap();
        assert!(matches!(
            current_report_view(&store),
            Err(ReportViewCatalogError::Build(ReportViewBuildError::Store(
                StoreError::Symlink
            )))
        ));
        assert!(
            fs::symlink_metadata(&managed)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn publication_reopens_readonly_per_request_and_releases_guard() {
        let (directory, store) = open_store("publish-readonly");
        let peer = LocalStore::open_current(&directory).unwrap();
        let publication = publish_report_view(&store, build(&store)).unwrap();
        assert_eq!(publication.current().generation(), 0);
        assert_eq!(publication.current().visibility_epoch(), 0);
        assert_eq!(publication.current().records(), 0);
        assert_eq!(
            publication.current().rate_fingerprint(),
            MISSING_RATE_FINGERPRINT
        );
        assert!(publication.current().generated_at().ends_with('Z'));
        assert!(publication.current().generated_at().len() <= 64);
        assert!(publication.retired().is_none());
        assert!(!publication.cleanup_pending());
        let catalog_body = fs::read_to_string(
            managed_report_view_directory(&store)
                .unwrap()
                .join(CATALOG_FILE_NAME),
        )
        .unwrap();
        assert!(!catalog_body.contains(directory.to_string_lossy().as_ref()));
        assert!(!catalog_body.contains("local-store.sqlite3"));
        let current = current_report_view(&store).unwrap().unwrap();
        assert_eq!(current, *publication.current());

        let records =
            with_report_view_snapshot(&store, current.view_id(), |connection, metadata| {
                assert_eq!(metadata, &current);
                let query_only: i64 =
                    connection.pragma_query_value(None, "query_only", |row| row.get(0))?;
                assert_eq!(query_only, 1);
                let busy_timeout: i64 =
                    connection.pragma_query_value(None, "busy_timeout", |row| row.get(0))?;
                assert_eq!(busy_timeout, 0);
                let count: i64 =
                    connection.query_row("SELECT COUNT(*) FROM spans", [], |row| row.get(0))?;
                usize::try_from(count).map_err(|_| ReportViewCatalogError::InvalidCatalog)
            })
            .unwrap();
        assert_eq!(records, 0);
        assert!(peer.try_acquire_report_render_guard().unwrap().is_some());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn catalog_retains_only_current_and_one_retired_snapshot() {
        let (directory, store) = open_store("bounded-retention");
        let first = publish_report_view(&store, build(&store)).unwrap();
        store.invalidate_report().unwrap();
        let second = publish_report_view(&store, build(&store)).unwrap();
        assert_eq!(second.retired(), Some(first.current()));
        store.invalidate_report().unwrap();
        let third = publish_report_view(&store, build(&store)).unwrap();
        assert_eq!(third.retired(), Some(second.current()));
        assert!(matches!(
            with_report_view_snapshot(&store, first.current().view_id(), |_, _| Ok(())),
            Err(ReportViewCatalogError::SnapshotExpired)
        ));
        let managed = managed_report_view_directory(&store).unwrap();
        let immutable_files = fs::read_dir(managed)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.ends_with(IMMUTABLE_FILE_SUFFIX))
            })
            .count();
        assert_eq!(immutable_files, 2);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn ownership_observation_enumerates_current_retired_catalog_and_directory() {
        let (directory, store) = open_store("ownership-membership");
        let peer = LocalStore::open_current(&directory).unwrap();
        let first = publish_report_view(&store, build(&store)).unwrap();
        store.invalidate_report().unwrap();
        let second = publish_report_view(&store, build(&store)).unwrap();

        with_report_view_ownership_observation(&store, |observation| {
            let entries = observation.entries().collect::<Vec<_>>();
            assert_eq!(entries.len(), 4);
            let debug = format!("{observation:?}");
            assert!(debug.contains("entry_count: 4"));
            assert!(debug.contains("ManagedDirectory"));
            assert!(debug.contains("Catalog"));
            assert!(debug.contains("CurrentSnapshot"));
            assert!(debug.contains("RetiredSnapshot"));
            assert!(!debug.contains(directory.to_string_lossy().as_ref()));
            assert!(!debug.contains("store:sha256:"));
            assert!(!debug.contains("LocalStore"));
            assert!(!debug.contains("File"));
            assert_eq!(
                entries
                    .iter()
                    .map(|entry| entry.kind())
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from([
                    ReportViewOwnedEntryKind::ManagedDirectory,
                    ReportViewOwnedEntryKind::Catalog,
                    ReportViewOwnedEntryKind::CurrentSnapshot,
                    ReportViewOwnedEntryKind::RetiredSnapshot,
                ])
            );
            assert!(peer.try_acquire_report_render_guard().unwrap().is_some());
            for entry in entries {
                let descriptor = File::open(entry.path()).unwrap();
                assert!(observation.recognizes(entry.path(), &descriptor).unwrap());
            }
            let paths = observation
                .entries()
                .map(|entry| entry.path().to_path_buf())
                .collect::<BTreeSet<_>>();
            assert!(paths.iter().any(|path| {
                path.file_name().and_then(|name| name.to_str())
                    == Some(first.current().file_name.as_str())
            }));
            assert!(paths.iter().any(|path| {
                path.file_name().and_then(|name| name.to_str())
                    == Some(second.current().file_name.as_str())
            }));
        })
        .unwrap();
        assert!(peer.try_acquire_report_render_guard().unwrap().is_some());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn ownership_observation_rejects_matching_orphan_and_zero_length_bogus_names() {
        let (directory, store) = open_store("ownership-orphans");
        publish_report_view(&store, build(&store)).unwrap();
        let managed = managed_report_view_directory(&store).unwrap();
        let orphan = managed.join(format!(
            "{IMMUTABLE_FILE_PREFIX}{}{IMMUTABLE_FILE_SUFFIX}",
            "0".repeat(64)
        ));
        drop(private_create_new(&orphan).unwrap());
        let bogus = managed.join(format!("{STAGING_FILE_PREFIX}0.0"));
        drop(private_create_new(&bogus).unwrap());

        with_report_view_ownership_observation(&store, |observation| {
            assert!(
                !observation
                    .recognizes(&orphan, &File::open(&orphan).unwrap())
                    .unwrap()
            );
            assert!(
                !observation
                    .recognizes(&bogus, &File::open(&bogus).unwrap())
                    .unwrap()
            );
        })
        .unwrap();
        let _ = fs::remove_dir_all(directory);
    }

    #[cfg(unix)]
    #[test]
    fn ownership_observation_refuses_replacement_alias_and_lock_replacement() {
        let (directory, store) = open_store("ownership-identity");
        let published = publish_report_view(&store, build(&store)).unwrap();
        let managed = managed_report_view_directory(&store).unwrap();
        let snapshot = managed.join(&published.current().file_name);

        let replacement_result = with_report_view_ownership_observation(&store, |observation| {
            let displaced = managed.join("displaced.sqlite3");
            fs::rename(&snapshot, &displaced).unwrap();
            drop(private_create_new(&snapshot).unwrap());
            observation.recognizes(&snapshot, &File::open(&snapshot).unwrap())
        });
        assert!(matches!(
            replacement_result,
            Ok(Err(ReportViewCatalogError::InvalidCatalog))
                | Err(ReportViewCatalogError::InvalidCatalog)
        ));

        fs::remove_file(&snapshot).unwrap();
        fs::rename(managed.join("displaced.sqlite3"), &snapshot).unwrap();
        let alias_result = with_report_view_ownership_observation(&store, |observation| {
            let alias = managed.join("snapshot-alias");
            fs::hard_link(&snapshot, &alias).unwrap();
            observation.recognizes(&snapshot, &File::open(&alias).unwrap())
        });
        assert!(matches!(
            alias_result,
            Ok(Err(ReportViewCatalogError::InvalidCatalog))
                | Err(ReportViewCatalogError::InvalidCatalog)
        ));
        fs::remove_file(managed.join("snapshot-alias")).unwrap();

        let (lock_directory, lock_store) = open_store("ownership-lock-replacement");
        publish_report_view(&lock_store, build(&lock_store)).unwrap();
        let lock = lock_directory.join(REPORT_RENDER_LOCK_NAME);
        let lock_result = with_report_view_ownership_observation(&lock_store, |_observation| {
            let displaced = lock_directory.join("displaced-report-render.lock");
            fs::rename(&lock, &displaced).unwrap();
            drop(private_create_new(&lock).unwrap());
        });
        assert!(matches!(
            lock_result,
            Err(ReportViewCatalogError::InvalidCatalog)
        ));
        let _ = fs::remove_dir_all(directory);
        let _ = fs::remove_dir_all(lock_directory);
    }

    #[test]
    fn ownership_observation_is_invalidated_by_visibility_or_catalog_mutation() {
        let (visibility_directory, visibility_store) = open_store("ownership-visibility");
        publish_report_view(&visibility_store, build(&visibility_store)).unwrap();
        let visibility_result =
            with_report_view_ownership_observation(&visibility_store, |_observation| {
                visibility_store
                    .db
                    .execute(
                        "UPDATE metadata SET value='1' WHERE key='report_visibility_epoch'",
                        [],
                    )
                    .unwrap();
            });
        assert!(matches!(
            visibility_result,
            Err(ReportViewCatalogError::RefreshPending)
        ));

        let (catalog_directory, catalog_store) = open_store("ownership-catalog-mutation");
        publish_report_view(&catalog_store, build(&catalog_store)).unwrap();
        let catalog_path = managed_report_view_directory(&catalog_store)
            .unwrap()
            .join(CATALOG_FILE_NAME);
        let catalog_result =
            with_report_view_ownership_observation(&catalog_store, |_observation| {
                let catalog = ReportViewCatalog {
                    schema_version: CATALOG_SCHEMA_VERSION.to_owned(),
                    source_identity: source_store_identity(&catalog_store).unwrap(),
                    visibility_epoch: 0,
                    current: None,
                    retired: None,
                };
                fs::write(&catalog_path, serde_json::to_vec(&catalog).unwrap()).unwrap();
            });
        assert!(matches!(
            catalog_result,
            Err(ReportViewCatalogError::InvalidCatalog)
        ));
        let _ = fs::remove_dir_all(visibility_directory);
        let _ = fs::remove_dir_all(catalog_directory);
    }

    #[test]
    fn ownership_observation_never_creates_lock_and_reads_current_during_build() {
        let (missing_directory, missing_store) = open_store("ownership-missing-guard");
        let managed = missing_directory.join(MANAGED_DIRECTORY_NAME);
        private_dir(&managed).unwrap();
        let lock = missing_directory.join(REPORT_RENDER_LOCK_NAME);
        assert!(!lock.exists());
        assert!(with_report_view_ownership_observation(&missing_store, |_| ()).is_err());
        assert!(!lock.exists());

        let (busy_directory, busy_store) = open_store("ownership-during-build");
        let published = publish_report_view(&busy_store, build(&busy_store)).unwrap();
        busy_store.invalidate_report().unwrap();
        let staging = build(&busy_store);
        with_report_view_ownership_observation(&busy_store, |observation| {
            let current = observation
                .entries()
                .find(|entry| entry.kind() == ReportViewOwnedEntryKind::CurrentSnapshot)
                .unwrap();
            assert_eq!(
                current.path().file_name().and_then(|name| name.to_str()),
                Some(published.current().file_name.as_str())
            );
            assert!(
                observation
                    .recognizes(current.path(), &File::open(current.path()).unwrap())
                    .unwrap()
            );
            assert!(
                !observation
                    .recognizes(staging.path(), &File::open(staging.path()).unwrap())
                    .unwrap()
            );
        })
        .unwrap();
        drop(staging);
        let _ = fs::remove_dir_all(missing_directory);
        let _ = fs::remove_dir_all(busy_directory);
    }

    #[test]
    fn publication_rejects_generation_drift_and_cross_store_staging() {
        let (first_directory, first) = open_store("source-first");
        let first_peer = LocalStore::open_current(&first_directory).unwrap();
        let staging = build(&first);
        first_peer.invalidate_report().unwrap();
        assert!(matches!(
            publish_report_view(&first, staging),
            Err(ReportViewCatalogError::SnapshotChanged)
        ));
        assert!(current_report_view(&first).unwrap().is_none());

        let staging = build(&first);
        let (second_directory, second) = open_store("source-second");
        assert!(matches!(
            publish_report_view(&second, staging),
            Err(ReportViewCatalogError::SourceMismatch)
        ));
        assert!(current_report_view(&second).unwrap().is_none());
        let _ = fs::remove_dir_all(first_directory);
        let _ = fs::remove_dir_all(second_directory);
    }

    #[test]
    fn retirement_removes_views_and_holds_guard_for_destructive_transaction() {
        let (directory, store) = open_store("retirement");
        let peer = LocalStore::open_current(&directory).unwrap();
        let published = publish_report_view(&store, build(&store)).unwrap();
        let guard = store.try_acquire_report_render_guard().unwrap().unwrap();
        let retirement = retire_report_views_for_visibility_change(&store, guard, 1).unwrap();
        assert_eq!(retirement.previous_visibility_epoch(), 0);
        assert_eq!(retirement.next_visibility_epoch(), 1);
        assert!(peer.try_acquire_report_render_guard().unwrap().is_none());
        let managed = managed_report_view_directory(&store).unwrap();
        assert_eq!(fs::read_dir(managed).unwrap().count(), 0);
        drop(retirement);
        assert!(peer.try_acquire_report_render_guard().unwrap().is_some());
        assert!(matches!(
            with_report_view_snapshot(&store, published.current().view_id(), |_, _| Ok(())),
            Err(ReportViewCatalogError::RefreshPending)
        ));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn retirement_rejects_a_guard_owned_by_another_store() {
        let (first_directory, first) = open_store("retirement-guard-first");
        let (second_directory, second) = open_store("retirement-guard-second");
        let foreign_guard = first.try_acquire_report_render_guard().unwrap().unwrap();
        assert!(matches!(
            retire_report_views_for_visibility_change(&second, foreign_guard, 1),
            Err(ReportViewCatalogError::SourceMismatch)
        ));
        let _ = fs::remove_dir_all(first_directory);
        let _ = fs::remove_dir_all(second_directory);
    }

    #[test]
    fn recovery_removes_interrupted_managed_files() {
        let (directory, store) = open_store("orphan-recovery");
        let managed = managed_report_view_directory(&store).unwrap();
        drop(private_create_new(&managed.join(format!("{STAGING_FILE_PREFIX}orphan"))).unwrap());
        drop(
            private_create_new(&managed.join(format!(
                "{IMMUTABLE_FILE_PREFIX}{}{IMMUTABLE_FILE_SUFFIX}",
                "0".repeat(64)
            )))
            .unwrap(),
        );
        recover_report_view_catalog(&store).unwrap();
        assert_eq!(fs::read_dir(managed).unwrap().count(), 0);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn existing_render_guard_missing_lock_is_noncreating_and_legacy_recovery_still_creates() {
        let (directory, store) = open_store("existing-render-missing");
        let lock = directory.join(REPORT_RENDER_LOCK_NAME);
        if lock.exists() {
            fs::remove_file(&lock).unwrap();
        }
        assert!(matches!(ExistingReportRenderGuard::try_acquire(&store),
            Err(ReportViewCatalogError::Io(error)) if error.kind() == io::ErrorKind::NotFound));
        assert!(!lock.exists());
        recover_report_view_catalog(&store).unwrap();
        assert!(lock.exists());
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn existing_render_guard_retains_exclusion_through_cleanup_and_releases() {
        let (directory, store) = open_store("existing-render-held");
        drop(try_guard(&store).unwrap());
        let managed = managed_report_view_directory(&store).unwrap();
        let orphan = managed.join(format!("{STAGING_FILE_PREFIX}orphan"));
        drop(private_create_new(&orphan).unwrap());
        let guard = ExistingReportRenderGuard::try_acquire(&store).unwrap();
        assert!(matches!(
            ExistingReportRenderGuard::try_acquire(&store),
            Err(ReportViewCatalogError::Busy)
        ));
        guard.revalidate(&store).unwrap();
        recover_report_view_catalog_with_existing_guard(&store, &guard).unwrap();
        assert!(!orphan.exists());
        assert!(matches!(
            ExistingReportRenderGuard::try_acquire(&store),
            Err(ReportViewCatalogError::Busy)
        ));
        drop(guard);
        ExistingReportRenderGuard::try_acquire(&store)
            .unwrap()
            .revalidate(&store)
            .unwrap();
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn existing_render_guard_rejects_foreign_root_and_replaced_lock_without_cleanup() {
        let (directory, store) = open_store("existing-render-original");
        let (other_directory, other) = open_store("existing-render-foreign");
        drop(try_guard(&store).unwrap());
        let guard = ExistingReportRenderGuard::try_acquire(&store).unwrap();
        assert!(matches!(
            recover_report_view_catalog_with_existing_guard(&other, &guard),
            Err(ReportViewCatalogError::SourceMismatch)
        ));
        let managed = managed_report_view_directory(&store).unwrap();
        let orphan = managed.join(format!("{STAGING_FILE_PREFIX}orphan"));
        drop(private_create_new(&orphan).unwrap());
        let lock = directory.join(REPORT_RENDER_LOCK_NAME);
        let retained = directory.join("retained-render.lock");
        fs::rename(&lock, &retained).unwrap();
        drop(private_create_new(&lock).unwrap());
        assert!(guard.revalidate(&store).is_err());
        assert!(recover_report_view_catalog_with_existing_guard(&store, &guard).is_err());
        assert!(orphan.exists());
        assert!(retained.exists());
        assert_eq!(fs::read(lock).unwrap(), b"");
        drop(guard);
        drop(store);
        drop(other);
        fs::remove_dir_all(directory).unwrap();
        fs::remove_dir_all(other_directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn existing_render_guard_rejects_unsafe_locks_and_replaced_directory() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let (directory, store) = open_store("existing-render-unsafe");
        let lock = directory.join(REPORT_RENDER_LOCK_NAME);
        drop(try_guard(&store).unwrap());
        let retained = directory.join("retained-lock");
        fs::rename(&lock, &retained).unwrap();
        symlink(&retained, &lock).unwrap();
        assert!(ExistingReportRenderGuard::try_acquire(&store).is_err());
        fs::remove_file(&lock).unwrap();
        fs::hard_link(&retained, &lock).unwrap();
        assert!(ExistingReportRenderGuard::try_acquire(&store).is_err());
        fs::remove_file(&lock).unwrap();
        fs::rename(&retained, &lock).unwrap();
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(ExistingReportRenderGuard::try_acquire(&store).is_err());
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o600)).unwrap();
        let guard = ExistingReportRenderGuard::try_acquire(&store).unwrap();
        let original = directory.with_extension("retained-root");
        fs::rename(&directory, &original).unwrap();
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        drop(private_create_new(&lock).unwrap());
        assert!(guard.revalidate(&store).is_err());
        assert!(recover_report_view_catalog_with_existing_guard(&store, &guard).is_err());
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        drop(guard);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
        fs::remove_dir_all(original).unwrap();
    }

    fn replace_render_test_authority(
        directory: &Path,
        store: &LocalStore,
        database_only: bool,
    ) -> (PathBuf, LocalStore) {
        let retained = directory.with_extension("retained-authority");
        if database_only {
            fs::rename(store.database_path(), &retained).unwrap();
            fs::copy(&retained, store.database_path()).unwrap();
        } else {
            fs::rename(directory, &retained).unwrap();
        }
        let replacement = LocalStore::open(directory).unwrap();
        drop(try_guard(&replacement).unwrap());
        (retained, replacement)
    }

    #[test]
    fn existing_render_guard_rejects_authority_replaced_before_acquisition() {
        for database_only in [false, true] {
            let (directory, store) = open_store(&format!("existing-render-before-{database_only}"));
            drop(try_guard(&store).unwrap());
            let (retained, replacement) =
                replace_render_test_authority(&directory, &store, database_only);
            assert!(ExistingReportRenderGuard::try_acquire(&store).is_err());
            ExistingReportRenderGuard::try_acquire(&replacement)
                .unwrap()
                .revalidate(&replacement)
                .unwrap();
            drop(store);
            drop(replacement);
            fs::remove_dir_all(directory).unwrap();
            if database_only {
                fs::remove_file(retained).unwrap();
            } else {
                fs::remove_dir_all(retained).unwrap();
            }
        }
    }

    #[test]
    fn existing_render_guard_rejects_replaced_database_and_new_store_at_same_path() {
        let (directory, store) = open_store("existing-render-after-database");
        drop(try_guard(&store).unwrap());
        let guard = ExistingReportRenderGuard::try_acquire(&store).unwrap();
        let retained = directory.join("retained-database");
        fs::rename(store.database_path(), &retained).unwrap();
        fs::copy(&retained, store.database_path()).unwrap();
        let replacement = LocalStore::open_current(&directory).unwrap();
        let managed = managed_report_view_directory(&replacement).unwrap();
        let orphan = managed.join(format!("{STAGING_FILE_PREFIX}orphan"));
        drop(private_create_new(&orphan).unwrap());
        assert!(guard.revalidate(&store).is_err());
        assert!(guard.revalidate(&replacement).is_err());
        assert!(recover_report_view_catalog_with_existing_guard(&store, &guard).is_err());
        assert!(recover_report_view_catalog_with_existing_guard(&replacement, &guard).is_err());
        assert!(orphan.exists());
        assert!(retained.exists());
        drop(guard);
        ExistingReportRenderGuard::try_acquire(&replacement)
            .unwrap()
            .revalidate(&replacement)
            .unwrap();
        drop(store);
        drop(replacement);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn existing_render_guard_fifo_opens_are_nonblocking() {
        use std::os::unix::fs::FileTypeExt;
        // The same finite subprocess pattern as storage_ownership FIFO coverage keeps a
        // blocking-open regression from hanging the test runner. No global env mutation.
        const PROBE: &str = "AGENTOBS_EXISTING_RENDER_FIFO_PROBE";
        if std::env::var_os(PROBE).is_none() {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "report_view_catalog::tests::existing_render_guard_fifo_opens_are_nonblocking",
                ])
                .env(PROBE, "1")
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success());
                    return;
                }
                if std::time::Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("existing render guard blocked on FIFO");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let (directory, store) = open_store("existing-render-fifo");
        drop(try_guard(&store).unwrap());
        let guard = ExistingReportRenderGuard::try_acquire(&store).unwrap();
        let lock = directory.join(REPORT_RENDER_LOCK_NAME);
        fs::rename(&lock, directory.join("retained-lock")).unwrap();
        assert!(
            std::process::Command::new("mkfifo")
                .args(["-m", "600"])
                .arg(&lock)
                .status()
                .unwrap()
                .success()
        );
        assert!(ExistingReportRenderGuard::try_acquire(&store).is_err());
        assert!(guard.revalidate(&store).is_err());
        assert!(recover_report_view_catalog_with_existing_guard(&store, &guard).is_err());
        assert!(fs::symlink_metadata(&lock).unwrap().file_type().is_fifo());
        let original = directory.with_extension("retained-root");
        fs::rename(&directory, &original).unwrap();
        assert!(
            std::process::Command::new("mkfifo")
                .args(["-m", "600"])
                .arg(&directory)
                .status()
                .unwrap()
                .success()
        );
        assert!(ExistingReportRenderGuard::try_acquire(&store).is_err());
        assert!(guard.revalidate(&store).is_err());
        drop(guard);
        drop(store);
        fs::remove_file(directory).unwrap();
        fs::remove_dir_all(original).unwrap();
    }

    #[test]
    fn oversized_catalog_is_rejected_before_unbounded_read() {
        let (directory, store) = open_store("catalog-capacity");
        let managed = managed_report_view_directory(&store).unwrap();
        let mut catalog = private_create_new(&managed.join(CATALOG_FILE_NAME)).unwrap();
        catalog
            .write_all(&vec![b'x'; usize::try_from(MAX_CATALOG_BYTES).unwrap() + 1])
            .unwrap();
        catalog.sync_all().unwrap();
        drop(catalog);
        assert!(matches!(
            current_report_view(&store),
            Err(ReportViewCatalogError::CatalogCapacityExceeded)
        ));
        let _ = fs::remove_dir_all(directory);
    }

    #[cfg(unix)]
    #[test]
    fn catalog_fifo_recovery_returns_without_waiting_and_preserves_normal_catalog() {
        use std::os::unix::fs::FileTypeExt;
        const PROBE: &str = "AGENTOBS_CATALOG_RECOVERY_FIFO_PROBE";
        if std::env::var_os(PROBE).is_none() {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "report_view_catalog::tests::catalog_fifo_recovery_returns_without_waiting_and_preserves_normal_catalog", "--nocapture"])
                .env(PROBE, "1").spawn().unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success());
                    return;
                }
                if std::time::Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("catalog recovery blocked on FIFO; child killed and reaped");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let (directory, store) = open_store("catalog-recovery-fifo");
        let current = publish_report_view(&store, build(&store))
            .unwrap()
            .current()
            .clone();
        let catalog = directory
            .join(MANAGED_DIRECTORY_NAME)
            .join(CATALOG_FILE_NAME);
        let original = fs::read(&catalog).unwrap();
        recover_report_view_catalog(&store).unwrap();
        assert_eq!(fs::read(&catalog).unwrap(), original);
        let guard = ExistingReportRenderGuard::try_acquire(&store).unwrap();
        let retained = directory.join("retained-catalog.json");
        fs::rename(&catalog, &retained).unwrap();
        assert!(
            std::process::Command::new("mkfifo")
                .args(["-m", "600"])
                .arg(&catalog)
                .status()
                .unwrap()
                .success()
        );
        eprintln!("entering public catalog recovery with FIFO and retained existing guard");
        assert!(matches!(
            recover_report_view_catalog_with_existing_guard(&store, &guard),
            Err(ReportViewCatalogError::Store(StoreError::InvalidPath))
        ));
        assert!(
            fs::symlink_metadata(&catalog)
                .unwrap()
                .file_type()
                .is_fifo()
        );
        assert_eq!(fs::read(&retained).unwrap(), original);
        guard.revalidate(&store).unwrap();
        drop(guard);
        assert!(matches!(
            current_report_view(&store),
            Err(ReportViewCatalogError::Store(StoreError::InvalidPath))
        ));
        fs::remove_file(&catalog).unwrap();
        fs::rename(&retained, &catalog).unwrap();
        recover_report_view_catalog(&store).unwrap();
        assert_eq!(current_report_view(&store).unwrap(), Some(current));
        assert_eq!(fs::read(&catalog).unwrap(), original);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_revokes_and_removes_every_old_epoch_snapshot() {
        let (directory, store) = open_store("old-epoch-recovery");
        let published = publish_report_view(&store, build(&store)).unwrap();
        store
            .db
            .execute(
                "UPDATE metadata SET value='1' WHERE key='report_visibility_epoch'",
                [],
            )
            .unwrap();
        assert!(matches!(
            with_report_view_snapshot(&store, published.current().view_id(), |_, _| Ok(())),
            Err(ReportViewCatalogError::RefreshPending)
        ));
        recover_report_view_catalog(&store).unwrap();
        assert!(current_report_view(&store).unwrap().is_none());
        let managed = managed_report_view_directory(&store).unwrap();
        assert_eq!(fs::read_dir(managed).unwrap().count(), 0);
        let _ = fs::remove_dir_all(directory);
    }

    #[cfg(unix)]
    #[test]
    fn recovery_fails_closed_on_symlinked_managed_entry() {
        use std::os::unix::fs::symlink;

        let (directory, store) = open_store("orphan-symlink");
        let managed = managed_report_view_directory(&store).unwrap();
        let link = managed.join(format!("{STAGING_FILE_PREFIX}link"));
        symlink(store.database_path(), &link).unwrap();
        assert!(matches!(
            recover_report_view_catalog(&store),
            Err(ReportViewCatalogError::InvalidCatalog)
        ));
        fs::remove_file(link).unwrap();
        let _ = fs::remove_dir_all(directory);
    }
    fn rewrite_kernel(store: &LocalStore, snapshot: &ReportViewSnapshot, version: &str) {
        let path = managed_report_view_directory(store)
            .unwrap()
            .join(&snapshot.file_name);
        let connection = Connection::open(path).unwrap();
        for (name, dimension) in [
            ("repo", "repo"),
            ("session", "session_id"),
            ("turn", "turn_id"),
            ("agent", "agent"),
            ("model", "model"),
        ] {
            let tail = if version == "v1" {
                "start_time_unix_ms,trace_id,span_id"
            } else {
                "source_order"
            };
            connection.execute_batch(&format!("DROP INDEX spans_{name}_order_idx; CREATE INDEX spans_{name}_order_idx ON spans({dimension},{tail});")).unwrap();
        }
        connection
            .execute(
                "UPDATE metadata SET value=?1 WHERE key='schema_version'",
                [format!("agent_observability.report_view_staging.{version}")],
            )
            .unwrap();
    }

    #[test]
    fn dual_kernel_upgrade_preserves_v1_on_failed_build_and_retires_after_v2() {
        let (directory, store) = open_store("dual-kernel");
        assert!(!current_report_view_needs_kernel_upgrade(&store).unwrap());
        let old = publish_report_view(&store, build(&store))
            .unwrap()
            .current()
            .clone();
        rewrite_kernel(&store, &old, "v1");
        assert!(current_report_view_needs_kernel_upgrade(&store).unwrap());
        assert!(
            crate::build_report_view_staging(&store, MISSING_RATE_FINGERPRINT, 1, None).is_err()
        );
        with_report_view_snapshot(&store, old.view_id(), |_, _| Ok(())).unwrap();
        assert!(current_report_view_needs_kernel_upgrade(&store).unwrap());
        let new = publish_report_view(&store, build(&store))
            .unwrap()
            .current()
            .clone();
        assert!(!current_report_view_needs_kernel_upgrade(&store).unwrap());
        let scope = ReportViewReadScope::acquire(&store).unwrap();
        assert_eq!(
            scope
                .with_snapshot_kernel(old.view_id(), |_, _, kernel| Ok(kernel))
                .unwrap(),
            ReportViewKernel::V1
        );
        assert_eq!(
            scope
                .with_snapshot_kernel(new.view_id(), |_, _, kernel| Ok(kernel))
                .unwrap(),
            ReportViewKernel::V2
        );
        drop(scope);
        publish_report_view(&store, build(&store)).unwrap();
        assert!(matches!(
            with_report_view_snapshot(&store, old.view_id(), |_, _| Ok(())),
            Err(ReportViewCatalogError::SnapshotExpired)
        ));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn unknown_metadata_and_mismatched_index_shapes_fail_closed() {
        let (directory, store) = open_store("kernel-corrupt");
        let view = publish_report_view(&store, build(&store))
            .unwrap()
            .current()
            .clone();
        let path = directory.join(MANAGED_DIRECTORY_NAME).join(&view.file_name);
        let connection = Connection::open(&path).unwrap();
        for sql in [
            "UPDATE metadata SET value='agent_observability.report_view_staging.v9' WHERE key='schema_version'",
            "UPDATE metadata SET value='agent_observability.report_view_staging.v1' WHERE key='schema_version'",
            "UPDATE metadata SET value='agent_observability.report_view_staging.v2' WHERE key='schema_version'; INSERT INTO metadata VALUES('unknown','x')",
            "DELETE FROM metadata WHERE key='unknown'; DROP INDEX spans_repo_order_idx; CREATE INDEX spans_repo_order_idx ON spans(repo,source_order DESC)",
            "DROP INDEX spans_repo_order_idx; CREATE INDEX spans_repo_order_idx ON spans(repo COLLATE NOCASE,source_order)",
            "DROP INDEX spans_repo_order_idx; CREATE INDEX spans_repo_order_idx ON spans(repo,source_order) WHERE repo<>''",
        ] {
            connection.execute_batch(sql).unwrap();
            assert!(matches!(
                current_report_view_needs_kernel_upgrade(&store),
                Err(ReportViewCatalogError::InvalidCatalog)
            ));
            assert!(with_report_view_snapshot(&store, view.view_id(), |_, _| Ok(())).is_err());
        }
        drop(connection);
        let _ = fs::remove_dir_all(directory);
    }
}
