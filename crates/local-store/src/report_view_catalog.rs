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
    use_snapshot: impl FnOnce(&Connection, &ReportViewSnapshot) -> Result<T, ReportViewCatalogError>,
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
    use_snapshot(&connection, snapshot)
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
    let _guard = try_guard(store)?;
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

#[cfg(unix)]
#[expect(
    clippy::used_underscore_binding,
    reason = "the leader-owned guard field is intentionally named for RAII but its file identity binds retirement authority"
)]
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
    let actual = guard._file.metadata()?;
    Ok(expected.dev() == actual.dev() && expected.ino() == actual.ino())
}

#[cfg(windows)]
#[expect(
    clippy::used_underscore_binding,
    reason = "the leader-owned guard field is intentionally named for RAII but its file identity binds retirement authority"
)]
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
    let actual = guard._file.metadata()?;
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

fn validate_snapshot_database(
    connection: &Connection,
    snapshot: &ReportViewSnapshot,
) -> Result<(), ReportViewCatalogError> {
    for (key, expected) in [
        (
            "schema_version",
            "agent_observability.report_view_staging.v1".to_owned(),
        ),
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
    Ok(())
}

fn open_private_read(path: &Path) -> Result<File, ReportViewCatalogError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(no_follow_flag());
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
}
