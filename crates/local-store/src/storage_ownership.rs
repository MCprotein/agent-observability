//! Exact, callback-scoped ownership observation for local-store authority files.

use crate::{
    DB_NAME, LOCAL_STORE_SCHEMA_VERSION, LocalStore, PROJECTION_NAME, REPORT_RENDER_LOCK_NAME,
    SCHEMA_OBJECTS, STORE_OPEN_LOCK_NAME, StoreError, normalize_schema_sql, private_open_file,
    required_schema_text, required_schema_version, validate_existing_private_dir,
    validate_table_columns,
};
use rusqlite::{Connection, params};
use std::collections::BTreeSet;
use std::fmt::{self, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// Semantic role of one exact local-store authority entry.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StorageOwnedEntryKind {
    AuthorityDirectory,
    Database,
    Projection,
    StoreOpenLock,
    ReportRenderLock,
}

/// One exact path enumerated by a validated local-store ownership observation.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct StorageOwnedEntry<'a> {
    kind: StorageOwnedEntryKind,
    path: &'a Path,
}

impl fmt::Debug for StorageOwnedEntry<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StorageOwnedEntry")
            .field("kind", &self.kind)
            .finish()
    }
}

impl<'a> StorageOwnedEntry<'a> {
    #[must_use]
    pub const fn kind(self) -> StorageOwnedEntryKind {
        self.kind
    }

    #[must_use]
    pub const fn path(self) -> &'a Path {
        self.path
    }
}

#[derive(Debug)]
struct OwnedDescriptor {
    kind: StorageOwnedEntryKind,
    path: PathBuf,
    file: File,
}

/// Callback-scoped observation of exact local-store authority identities.
///
/// The retained descriptors remain private. Path enumeration is explicit, but a matching pathname
/// alone never establishes ownership.
pub struct StorageOwnershipObservation<'a> {
    store: &'a LocalStore,
    owned: Vec<OwnedDescriptor>,
    projection_present: bool,
    report_render_lock_present: bool,
}

/// Failure from an optional, read-only local-store ownership preflight.
pub enum StorageOwnershipPreflightError {
    /// An exact rollback-journal entry exists, so `SQLite` must not be opened by this preflight.
    JournalPresent,
    /// The absent-store boundary or existing store failed validation.
    Store(StoreError),
}

impl fmt::Debug for StorageOwnershipPreflightError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::JournalPresent => "StorageOwnershipPreflightError::JournalPresent",
            Self::Store(_) => "StorageOwnershipPreflightError::Store",
        })
    }
}

impl fmt::Display for StorageOwnershipPreflightError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::JournalPresent => {
                "local store ownership preflight deferred because a rollback journal is present"
            }
            Self::Store(_) => "local store ownership preflight failed",
        })
    }
}

impl std::error::Error for StorageOwnershipPreflightError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::JournalPresent => None,
            Self::Store(error) => Some(error),
        }
    }
}

impl From<StoreError> for StorageOwnershipPreflightError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl fmt::Debug for StorageOwnershipObservation<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let kinds = self
            .owned
            .iter()
            .map(|entry| entry.kind)
            .collect::<BTreeSet<_>>();
        formatter
            .debug_struct("StorageOwnershipObservation")
            .field("entry_count", &self.owned.len())
            .field("kinds", &kinds)
            .finish()
    }
}

impl StorageOwnershipObservation<'_> {
    /// Observes optional published report-view ownership within this authority observation.
    ///
    /// The caller must retain the external all-writer freeze throughout this call. Only the exact
    /// managed-directory absence produces `None`; present entries use the existing catalog checks.
    /// No store connection or write capability is exposed to the callback.
    ///
    /// # Errors
    ///
    /// Returns an error when store authority, report-view authority, or final absence changes.
    pub fn with_report_view_ownership_observation<T>(
        &self,
        use_observation: impl FnOnce(Option<&crate::ReportViewOwnershipObservation<'_>>) -> T,
    ) -> Result<T, crate::ReportViewCatalogError> {
        self.validate_authority()?;
        let result = crate::report_view_catalog::with_optional_report_view_ownership_observation(
            self.store,
            use_observation,
        );
        self.validate_authority()?;
        result
    }

    /// Enumerates only exact entries validated for this observation.
    #[must_use]
    pub fn entries(&self) -> impl ExactSizeIterator<Item = StorageOwnedEntry<'_>> {
        self.owned.iter().map(|entry| StorageOwnedEntry {
            kind: entry.kind,
            path: &entry.path,
        })
    }

    /// Tests whether `descriptor` has the retained identity for the exact known `path`.
    ///
    /// Unknown paths return false. A known path with a wrong identity fails closed.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::InvalidPath`] for a known path whose descriptor type, privacy, link
    /// count, or identity does not match the retained authority entry.
    pub fn recognizes(&self, path: &Path, descriptor: &File) -> Result<bool, StoreError> {
        let Some(owned) = self.owned.iter().find(|entry| entry.path == path) else {
            return Ok(false);
        };
        validate_descriptor(descriptor, owned.kind)?;
        if !same_descriptor_identity(&owned.file, descriptor)? {
            return Err(StoreError::InvalidPath);
        }
        Ok(true)
    }

    fn validate_authority(&self) -> Result<(), StoreError> {
        validate_retained_store_identity(self.store)?;
        validate_structural_schema(&self.store.db)?;
        for owned in &self.owned {
            validate_named_identity(owned)?;
        }
        validate_optional_presence(
            &self.store.dir.join(PROJECTION_NAME),
            self.projection_present,
        )?;
        validate_optional_presence(
            &self.store.dir.join(REPORT_RENDER_LOCK_NAME),
            self.report_render_lock_present,
        )?;
        Ok(())
    }
}

/// Supplies bounded, read-only ownership evidence for an already-open current-schema store.
///
/// The authority directory, database, and store-open lock are required. The projection and report
/// render lock are included only when they already exist. The operation never creates, repairs,
/// cleans, migrates, or scans payload rows, and never approves journals, temporary files, or other
/// descendants by name.
///
/// Pre/post validation detects observed identity and structural-schema changes, but is not an
/// ABA-safe filesystem snapshot. The caller must retain the external all-writer freeze before using
/// this observation for admission.
///
/// # Errors
///
/// Returns [`StoreError`] when retained construction identity, current schema structure, a required
/// entry, or any captured exact identity is invalid.
pub fn with_storage_ownership_observation<T>(
    store: &LocalStore,
    use_observation: impl FnOnce(&StorageOwnershipObservation<'_>) -> T,
) -> Result<T, StoreError> {
    validate_retained_store_identity(store)?;
    validate_structural_schema(&store.db)?;

    let mut owned = Vec::with_capacity(5);
    owned.push(OwnedDescriptor {
        kind: StorageOwnedEntryKind::AuthorityDirectory,
        path: store.dir.clone(),
        file: store.authority_directory.try_clone()?,
    });
    owned.push(OwnedDescriptor {
        kind: StorageOwnedEntryKind::Database,
        path: store.dir.join(DB_NAME),
        file: store.authority_database.try_clone()?,
    });
    owned.push(open_owned_descriptor(
        StorageOwnedEntryKind::StoreOpenLock,
        store.dir.join(STORE_OPEN_LOCK_NAME),
    )?);

    let projection = open_optional_owned_descriptor(
        StorageOwnedEntryKind::Projection,
        store.dir.join(PROJECTION_NAME),
    )?;
    let projection_present = projection.is_some();
    owned.extend(projection);
    let report_render_lock = open_optional_owned_descriptor(
        StorageOwnedEntryKind::ReportRenderLock,
        store.dir.join(REPORT_RENDER_LOCK_NAME),
    )?;
    let report_render_lock_present = report_render_lock.is_some();
    owned.extend(report_render_lock);

    let observation = StorageOwnershipObservation {
        store,
        owned,
        projection_present,
        report_render_lock_present,
    };
    observation.validate_authority()?;
    let result = use_observation(&observation);
    observation.validate_authority()?;
    Ok(result)
}

/// Supplies optional, callback-scoped ownership evidence without creating or recovering a store.
///
/// Only an absent exact store-directory entry is reported as `None`. Its existing private parent
/// is retained and the absence is revalidated after the callback. A present directory must be a
/// complete current-schema store and must have no rollback-journal entry before `SQLite` is opened.
/// Any regular journal, including an empty or malformed one, returns
/// [`StorageOwnershipPreflightError::JournalPresent`] without reading it. The journal absence and
/// retained store identity are checked again after the callback.
///
/// This preflight does not create, recover, repair, migrate, clean, or cache store artifacts. The
/// caller must hold the external all-writer freeze for the entire call. The checks are not an
/// ABA-safe filesystem snapshot against an uncoordinated same-user writer.
///
/// # Errors
///
/// Returns [`StorageOwnershipPreflightError::JournalPresent`] when the exact journal is a valid
/// private regular file, or [`StorageOwnershipPreflightError::Store`] when path identity, privacy,
/// required store entries, current schema, or read-only reader validation fails.
pub fn with_optional_report_reader_storage_ownership<T>(
    store_directory: impl AsRef<Path>,
    use_observation: impl FnOnce(Option<&StorageOwnershipObservation<'_>>) -> T,
) -> Result<T, StorageOwnershipPreflightError> {
    let store_directory = store_directory.as_ref();
    match fs::symlink_metadata(store_directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return with_exact_directory_absence(store_directory, use_observation);
        }
        Err(error) => return Err(StoreError::Io(error).into()),
        Ok(_) => {}
    }

    validate_existing_private_dir(store_directory)?;
    let journal = store_directory
        .join(DB_NAME)
        .with_extension("sqlite3-journal");
    validate_journal_absence(&journal)?;
    let store = LocalStore::open_report_reader(store_directory)?;
    let observed = with_storage_ownership_observation(&store, |observation| {
        let result = use_observation(Some(observation));
        validate_journal_absence(&journal).map(|()| result)
    });
    validate_journal_absence(&journal)?;
    let result = observed.map_err(StorageOwnershipPreflightError::Store)??;
    validate_retained_store_identity(&store)?;
    Ok(result)
}

fn with_exact_directory_absence<T>(
    store_directory: &Path,
    use_observation: impl FnOnce(Option<&StorageOwnershipObservation<'_>>) -> T,
) -> Result<T, StorageOwnershipPreflightError> {
    let parent = store_directory
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent_descriptor = open_directory(parent)?;
    validate_exact_absence(store_directory)?;
    let result = use_observation(None);
    validate_exact_absence(store_directory)?;
    validate_named_descriptor(
        parent,
        &parent_descriptor,
        StorageOwnedEntryKind::AuthorityDirectory,
    )?;
    Ok(result)
}

fn validate_exact_absence(path: &Path) -> Result<(), StorageOwnershipPreflightError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StoreError::Io(error).into()),
        Ok(_) => Err(StoreError::InvalidPath.into()),
    }
}

fn validate_journal_absence(path: &Path) -> Result<(), StorageOwnershipPreflightError> {
    match crate::migration_admission::open_private_journal(path)? {
        None => Ok(()),
        Some(_) => Err(StorageOwnershipPreflightError::JournalPresent),
    }
}

pub(super) fn capture_store_identity(
    directory: &Path,
    database: &Path,
) -> Result<(File, File), StoreError> {
    let authority_directory = open_directory(directory)?;
    let authority_database = open_file(database)?;
    validate_named_descriptor(
        directory,
        &authority_directory,
        StorageOwnedEntryKind::AuthorityDirectory,
    )?;
    validate_named_descriptor(
        database,
        &authority_database,
        StorageOwnedEntryKind::Database,
    )?;
    Ok((authority_directory, authority_database))
}

pub(super) fn validate_captured_store_identity(
    directory: &Path,
    database: &Path,
    authority_directory: &File,
    authority_database: &File,
) -> Result<(), StoreError> {
    validate_named_descriptor(
        directory,
        authority_directory,
        StorageOwnedEntryKind::AuthorityDirectory,
    )?;
    validate_named_descriptor(
        database,
        authority_database,
        StorageOwnedEntryKind::Database,
    )
}

pub(super) fn validate_connection_path(
    connection: &Connection,
    database: &Path,
) -> Result<(), StoreError> {
    if connection.path().map(Path::new) != Some(database) {
        return Err(StoreError::InvalidPath);
    }
    Ok(())
}

pub(super) fn validate_private_file_identity(
    path: &Path,
    retained: &File,
) -> Result<(), StoreError> {
    validate_named_descriptor(path, retained, StorageOwnedEntryKind::Database)
}

fn validate_retained_store_identity(store: &LocalStore) -> Result<(), StoreError> {
    validate_captured_store_identity(
        &store.dir,
        &store.dir.join(DB_NAME),
        &store.authority_directory,
        &store.authority_database,
    )?;
    validate_connection_path(&store.db, &store.dir.join(DB_NAME))
}

fn validate_structural_schema(db: &Connection) -> Result<(), StoreError> {
    if required_schema_version(db)? != LOCAL_STORE_SCHEMA_VERSION {
        return Err(StoreError::SchemaMismatch);
    }
    let object_count: i64 = db.query_row(
        "SELECT COUNT(*) FROM main.sqlite_schema WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    if usize::try_from(object_count).ok() != Some(SCHEMA_OBJECTS.len()) {
        return Err(StoreError::SchemaMismatch);
    }
    for (kind, name, expected_sql) in SCHEMA_OBJECTS {
        let actual_sql = required_schema_text(db.query_row(
            "SELECT sql FROM main.sqlite_schema WHERE type=?1 AND name=?2",
            params![kind, name],
            |row| row.get(0),
        ))?;
        if normalize_schema_sql(&actual_sql) != normalize_schema_sql(expected_sql) {
            return Err(StoreError::SchemaMismatch);
        }
    }
    validate_table_columns(db)?;
    let temporary: i64 = db.query_row("SELECT count(*) FROM temp.sqlite_schema", [], |row| {
        row.get(0)
    })?;
    let attached: i64 = db.query_row(
        "SELECT count(*) FROM pragma_database_list WHERE name NOT IN ('main','temp')",
        [],
        |row| row.get(0),
    )?;
    if temporary != 0 || attached != 0 {
        return Err(StoreError::SchemaMismatch);
    }
    Ok(())
}

fn open_owned_descriptor(
    kind: StorageOwnedEntryKind,
    path: PathBuf,
) -> Result<OwnedDescriptor, StoreError> {
    let file = open_descriptor(&path, kind)?;
    let owned = OwnedDescriptor { kind, path, file };
    validate_named_identity(&owned)?;
    Ok(owned)
}

fn open_optional_owned_descriptor(
    kind: StorageOwnedEntryKind,
    path: PathBuf,
) -> Result<Option<OwnedDescriptor>, StoreError> {
    match fs::symlink_metadata(&path) {
        Ok(_) => open_owned_descriptor(kind, path).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn validate_optional_presence(path: &Path, expected: bool) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) if expected => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound && !expected => Ok(()),
        Ok(_) | Err(_) => Err(StoreError::InvalidPath),
    }
}

fn validate_named_identity(owned: &OwnedDescriptor) -> Result<(), StoreError> {
    validate_named_descriptor(&owned.path, &owned.file, owned.kind)
}

fn validate_named_descriptor(
    path: &Path,
    retained: &File,
    kind: StorageOwnedEntryKind,
) -> Result<(), StoreError> {
    validate_descriptor(retained, kind)?;
    let named = open_descriptor(path, kind)?;
    if !same_descriptor_identity(retained, &named)? {
        return Err(StoreError::InvalidPath);
    }
    Ok(())
}

fn open_descriptor(path: &Path, kind: StorageOwnedEntryKind) -> Result<File, StoreError> {
    if kind == StorageOwnedEntryKind::AuthorityDirectory {
        open_directory(path)
    } else {
        open_file(path)
    }
}

fn open_directory(path: &Path) -> Result<File, StoreError> {
    open_directory_observing(path, || {})
}

fn open_directory_observing(path: &Path, before_open: impl FnOnce()) -> Result<File, StoreError> {
    validate_existing_private_dir(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(crate::no_follow_flag() | crate::nonblocking_open_flag());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0220_0000);
    }
    before_open();
    let file = options.open(path)?;
    validate_descriptor(&file, StorageOwnedEntryKind::AuthorityDirectory)?;
    Ok(file)
}

fn open_file(path: &Path) -> Result<File, StoreError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(crate::no_follow_flag() | crate::nonblocking_open_flag());
    }
    let file = options.open(path)?;
    private_open_file(&file)?;
    validate_descriptor(&file, StorageOwnedEntryKind::Database)?;
    Ok(file)
}

fn validate_descriptor(file: &File, kind: StorageOwnedEntryKind) -> Result<(), StoreError> {
    let metadata = file.metadata()?;
    let valid_kind = if kind == StorageOwnedEntryKind::AuthorityDirectory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    };
    if !valid_kind {
        return Err(StoreError::InvalidPath);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let expected_mode = if metadata.is_dir() { 0o700 } else { 0o600 };
        if metadata.permissions().mode() & 0o777 != expected_mode
            || (metadata.is_file() && metadata.nlink() != 1)
        {
            return Err(StoreError::InvalidPath);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn same_descriptor_identity(left: &File, right: &File) -> Result<bool, StoreError> {
    use std::os::unix::fs::MetadataExt;
    let left = left.metadata()?;
    let right = right.metadata()?;
    Ok(left.dev() == right.dev() && left.ino() == right.ino())
}

#[cfg(windows)]
fn same_descriptor_identity(left: &File, right: &File) -> Result<bool, StoreError> {
    use std::os::windows::fs::MetadataExt;
    let left = left.metadata()?;
    let right = right.metadata()?;
    Ok(left.volume_serial_number() == right.volume_serial_number()
        && left.file_index() == right.file_index())
}

#[cfg(not(any(unix, windows)))]
fn same_descriptor_identity(_left: &File, _right: &File) -> Result<bool, StoreError> {
    Err(StoreError::InvalidPath)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LocalStore, PROJECTION_NAME, REPORT_RENDER_LOCK_NAME, STORE_OPEN_LOCK_NAME, StoreError,
        private_create_new,
    };
    use std::collections::{BTreeMap, BTreeSet};
    use std::ffi::OsString;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn test_directory(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agent-observability-storage-ownership-{label}-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn private_test_directory(label: &str) -> PathBuf {
        let directory = test_directory(label);
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        }
        directory
    }

    fn snapshot_directory_bytes(directory: &Path) -> BTreeMap<OsString, Vec<u8>> {
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (entry.file_name(), fs::read(entry.path()).unwrap())
            })
            .collect()
    }

    fn write_private_file(path: &Path, bytes: &[u8]) {
        let mut file = private_create_new(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    #[test]
    fn optional_preflight_reports_exact_absence_without_creating_store() {
        let parent = private_test_directory("optional-absent-parent");
        let directory = parent.join("store");
        let result = with_optional_report_reader_storage_ownership(&directory, |observation| {
            assert!(observation.is_none());
            "absent"
        })
        .unwrap();

        assert_eq!(result, "absent");
        assert!(!directory.exists());
        assert_eq!(fs::read_dir(&parent).unwrap().count(), 0);
        fs::remove_dir(parent).unwrap();
    }

    #[test]
    fn optional_preflight_classifies_current_store_without_changing_bytes() {
        let directory = test_directory("optional-current");
        let _ = fs::remove_dir_all(&directory);
        drop(LocalStore::open(&directory).unwrap());
        let before = snapshot_directory_bytes(&directory);

        let entry_count =
            with_optional_report_reader_storage_ownership(&directory, |observation| {
                observation.unwrap().entries().len()
            })
            .unwrap();

        assert_eq!(entry_count, 4);
        assert_eq!(snapshot_directory_bytes(&directory), before);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn optional_preflight_does_not_flatten_incomplete_store_to_absence() {
        let parent = private_test_directory("optional-missing-parent");
        let missing_parent_store = parent.join("missing-parent").join("store");
        let mut missing_parent_callback_called = false;
        let missing_parent =
            with_optional_report_reader_storage_ownership(&missing_parent_store, |_| {
                missing_parent_callback_called = true;
            });
        assert!(matches!(
            missing_parent,
            Err(StorageOwnershipPreflightError::Store(StoreError::Io(_)))
        ));
        assert!(!missing_parent_callback_called);
        fs::remove_dir(parent).unwrap();

        for missing in ["database", "lock"] {
            let directory = test_directory(&format!("optional-missing-{missing}"));
            let _ = fs::remove_dir_all(&directory);
            drop(LocalStore::open(&directory).unwrap());
            let missing_path = if missing == "database" {
                directory.join(DB_NAME)
            } else {
                directory.join(STORE_OPEN_LOCK_NAME)
            };
            fs::remove_file(&missing_path).unwrap();
            let mut callback_called = false;

            let result = with_optional_report_reader_storage_ownership(&directory, |_| {
                callback_called = true;
            });

            assert!(matches!(
                result,
                Err(StorageOwnershipPreflightError::Store(StoreError::Io(_)))
            ));
            assert!(!callback_called);
            assert!(!missing_path.exists());
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn optional_preflight_defers_all_private_regular_journals_before_sqlite_open() {
        for (label, bytes) in [
            ("empty", &[][..]),
            ("zeroed", &[0_u8; 28][..]),
            (
                "valid-looking",
                &[
                    0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0,
                    0, 1, 0, 0, 2, 0, 0, 0x10, 0, 0,
                ][..],
            ),
            ("malformed", b"not-a-rollback-journal".as_slice()),
        ] {
            let directory = private_test_directory(&format!("optional-journal-{label}"));
            let journal = directory.join("local-store.sqlite3-journal");
            write_private_file(&journal, bytes);
            let mut callback_called = false;

            let result = with_optional_report_reader_storage_ownership(&directory, |_| {
                callback_called = true;
            });

            assert!(matches!(
                result,
                Err(StorageOwnershipPreflightError::JournalPresent)
            ));
            assert!(!callback_called);
            assert_eq!(fs::read(&journal).unwrap(), bytes);
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn optional_preflight_rejects_callback_created_journal_and_preserves_it() {
        let directory = test_directory("optional-created-journal");
        let _ = fs::remove_dir_all(&directory);
        drop(LocalStore::open(&directory).unwrap());
        let journal = directory.join("local-store.sqlite3-journal");
        let bytes = b"callback-created-journal";

        let result = with_optional_report_reader_storage_ownership(&directory, |observation| {
            assert!(observation.is_some());
            write_private_file(&journal, bytes);
        });

        assert!(matches!(
            result,
            Err(StorageOwnershipPreflightError::JournalPresent)
        ));
        assert_eq!(fs::read(&journal).unwrap(), bytes);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn optional_preflight_rejects_store_appearing_after_absent_callback() {
        let parent = private_test_directory("optional-appearing-parent");
        let directory = parent.join("store");

        let result = with_optional_report_reader_storage_ownership(&directory, |observation| {
            assert!(observation.is_none());
            fs::create_dir(&directory).unwrap();
        });

        assert!(matches!(
            result,
            Err(StorageOwnershipPreflightError::Store(
                StoreError::InvalidPath
            ))
        ));
        assert!(directory.is_dir());
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn optional_preflight_rejects_directory_replacement_and_symlink() {
        use std::os::unix::fs::PermissionsExt;

        let directory = test_directory("optional-directory-replacement");
        let _ = fs::remove_dir_all(&directory);
        drop(LocalStore::open(&directory).unwrap());
        let displaced = directory.with_extension("displaced");
        let _ = fs::remove_dir_all(&displaced);

        let replacement =
            with_optional_report_reader_storage_ownership(&directory, |observation| {
                assert!(observation.is_some());
                fs::rename(&directory, &displaced).unwrap();
                fs::create_dir(&directory).unwrap();
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            });
        assert!(matches!(
            replacement,
            Err(StorageOwnershipPreflightError::Store(
                StoreError::InvalidPath | StoreError::Io(_)
            ))
        ));

        fs::remove_dir(&directory).unwrap();
        std::os::unix::fs::symlink(&displaced, &directory).unwrap();
        let mut callback_called = false;
        let symlink = with_optional_report_reader_storage_ownership(&directory, |_| {
            callback_called = true;
        });
        assert!(matches!(
            symlink,
            Err(StorageOwnershipPreflightError::Store(StoreError::Symlink))
        ));
        assert!(!callback_called);
        fs::remove_file(directory).unwrap();
        fs::remove_dir_all(displaced).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn optional_preflight_rejects_directory_disappearance_and_special_journals() {
        let directory = test_directory("optional-directory-disappearance");
        let _ = fs::remove_dir_all(&directory);
        drop(LocalStore::open(&directory).unwrap());
        let displaced = directory.with_extension("disappeared");
        let _ = fs::remove_dir_all(&displaced);

        let disappeared =
            with_optional_report_reader_storage_ownership(&directory, |observation| {
                assert!(observation.is_some());
                fs::rename(&directory, &displaced).unwrap();
            });
        assert!(matches!(
            disappeared,
            Err(StorageOwnershipPreflightError::Store(
                StoreError::InvalidPath | StoreError::Io(_)
            ))
        ));
        assert!(!directory.exists());
        fs::remove_dir_all(displaced).unwrap();

        for special in ["symlink", "fifo"] {
            let directory = private_test_directory(&format!("optional-journal-{special}"));
            let journal = directory.join("local-store.sqlite3-journal");
            if special == "symlink" {
                let target = directory.join("journal-target");
                write_private_file(&target, b"target");
                std::os::unix::fs::symlink(&target, &journal).unwrap();
            } else {
                assert!(
                    std::process::Command::new("mkfifo")
                        .args(["-m", "600"])
                        .arg(&journal)
                        .status()
                        .unwrap()
                        .success()
                );
            }
            let mut callback_called = false;

            let result = with_optional_report_reader_storage_ownership(&directory, |_| {
                callback_called = true;
            });

            assert!(matches!(
                result,
                Err(StorageOwnershipPreflightError::Store(
                    StoreError::Symlink | StoreError::InvalidPath
                ))
            ));
            assert!(!callback_called);
            assert!(fs::symlink_metadata(&journal).is_ok());
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn optional_preflight_error_debug_and_display_are_sanitized() {
        let secret = "/private/store/secret-payload";
        let error = StorageOwnershipPreflightError::Store(StoreError::Io(io::Error::other(secret)));

        for rendered in [format!("{error:?}"), error.to_string()] {
            assert!(!rendered.contains(secret));
            assert!(!rendered.contains("secret"));
        }
    }

    #[test]
    #[cfg(unix)]
    fn ownership_opens_refuse_fifo_without_waiting() {
        use std::os::unix::fs::PermissionsExt;
        const PROBE: &str = "AGENTOBS_STORE_OWNERSHIP_FIFO_PROBE";
        if std::env::var_os(PROBE).is_none() {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "storage_ownership::tests::ownership_opens_refuse_fifo_without_waiting",
                ])
                .env(PROBE, "1")
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success());
                    return;
                }
                if std::time::Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("store ownership open blocked on FIFO");
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        let directory = test_directory("fifo-opens");
        let store = LocalStore::open(&directory).unwrap();
        let candidate = directory.join("candidate");
        assert!(
            std::process::Command::new("mkfifo")
                .args(["-m", "600"])
                .arg(&candidate)
                .status()
                .unwrap()
                .success()
        );
        assert!(open_file(&candidate).is_err());
        fs::remove_file(&candidate).unwrap();
        fs::create_dir(&candidate).unwrap();
        fs::set_permissions(&candidate, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            open_directory_observing(&candidate, || {
                fs::rename(&candidate, directory.join("original-directory")).unwrap();
                assert!(
                    std::process::Command::new("mkfifo")
                        .args(["-m", "600"])
                        .arg(&candidate)
                        .status()
                        .unwrap()
                        .success()
                );
            })
            .is_err()
        );
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn observation_enumerates_exact_store_entries_without_private_debug_content() {
        let directory = test_directory("entries");
        let _ = fs::remove_dir_all(&directory);
        let store = LocalStore::open(&directory).unwrap();
        let render_guard = store.acquire_report_render_guard().unwrap();

        with_storage_ownership_observation(&store, |observation| {
            let entries = observation.entries().collect::<Vec<_>>();
            assert_eq!(entries.len(), 5);
            assert_eq!(
                entries
                    .iter()
                    .map(|entry| entry.kind())
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from([
                    StorageOwnedEntryKind::AuthorityDirectory,
                    StorageOwnedEntryKind::Database,
                    StorageOwnedEntryKind::Projection,
                    StorageOwnedEntryKind::StoreOpenLock,
                    StorageOwnedEntryKind::ReportRenderLock,
                ])
            );
            for entry in entries {
                assert!(!format!("{entry:?}").contains(directory.to_string_lossy().as_ref()));
                let descriptor = File::open(entry.path()).unwrap();
                assert!(observation.recognizes(entry.path(), &descriptor).unwrap());
            }
            let debug = format!("{observation:?}");
            assert!(debug.contains("entry_count: 5"));
            assert!(!debug.contains(directory.to_string_lossy().as_ref()));
            assert!(!debug.contains("LocalStore"));
            assert!(!debug.contains("File"));
        })
        .unwrap();

        drop(render_guard);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn observation_does_not_create_optional_files_and_requires_existing_store_lock() {
        let directory = test_directory("missing");
        let _ = fs::remove_dir_all(&directory);
        let store =
            LocalStore::open_with_migration_headroom_deferred_projection(&directory, 0).unwrap();
        let projection = directory.join(PROJECTION_NAME);
        let render_lock = directory.join(REPORT_RENDER_LOCK_NAME);
        assert!(!projection.exists());
        assert!(!render_lock.exists());

        with_storage_ownership_observation(&store, |observation| {
            assert_eq!(observation.entries().len(), 3);
        })
        .unwrap();
        assert!(!projection.exists());
        assert!(!render_lock.exists());

        let store_lock = directory.join(STORE_OPEN_LOCK_NAME);
        fs::remove_file(&store_lock).unwrap();
        assert!(matches!(
            with_storage_ownership_observation(&store, |_| ()),
            Err(StoreError::InvalidPath | StoreError::Io(_))
        ));
        assert!(!store_lock.exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn observation_rejects_unrecognized_zero_length_and_temp_names() {
        let directory = test_directory("unknown");
        let _ = fs::remove_dir_all(&directory);
        let store = LocalStore::open(&directory).unwrap();
        let zero = directory.join("unknown-zero");
        let temp = directory.join(format!(".{PROJECTION_NAME}.tmp.0.0"));
        drop(private_create_new(&zero).unwrap());
        drop(private_create_new(&temp).unwrap());

        with_storage_ownership_observation(&store, |observation| {
            assert!(
                !observation
                    .recognizes(&zero, &File::open(&zero).unwrap())
                    .unwrap()
            );
            assert!(
                !observation
                    .recognizes(&temp, &File::open(&temp).unwrap())
                    .unwrap()
            );
        })
        .unwrap();
        let _ = fs::remove_dir_all(directory);
    }

    #[cfg(unix)]
    #[test]
    fn observation_rejects_replacement_hardlink_alias_and_store_lock_replacement() {
        let replacement_directory = test_directory("replacement");
        let _ = fs::remove_dir_all(&replacement_directory);
        let replacement_store = LocalStore::open(&replacement_directory).unwrap();
        let projection = replacement_store.projection_path();
        let replacement = with_storage_ownership_observation(&replacement_store, |observation| {
            let displaced = replacement_directory.join("displaced-projection");
            fs::rename(&projection, &displaced).unwrap();
            drop(private_create_new(&projection).unwrap());
            observation.recognizes(&projection, &File::open(&projection).unwrap())
        });
        assert!(matches!(
            replacement,
            Ok(Err(StoreError::InvalidPath)) | Err(StoreError::InvalidPath)
        ));

        let alias_directory = test_directory("alias");
        let _ = fs::remove_dir_all(&alias_directory);
        let alias_store = LocalStore::open(&alias_directory).unwrap();
        let projection = alias_store.projection_path();
        let alias = alias_directory.join("projection-alias");
        let alias_result = with_storage_ownership_observation(&alias_store, |observation| {
            fs::hard_link(&projection, &alias).unwrap();
            observation.recognizes(&projection, &File::open(&alias).unwrap())
        });
        assert!(matches!(
            alias_result,
            Ok(Err(StoreError::InvalidPath)) | Err(StoreError::InvalidPath)
        ));

        let lock_directory = test_directory("lock-replacement");
        let _ = fs::remove_dir_all(&lock_directory);
        let lock_store = LocalStore::open(&lock_directory).unwrap();
        let lock = lock_directory.join(STORE_OPEN_LOCK_NAME);
        let lock_result = with_storage_ownership_observation(&lock_store, |_| {
            fs::rename(&lock, lock_directory.join("displaced-store-open.lock")).unwrap();
            drop(private_create_new(&lock).unwrap());
        });
        assert!(matches!(lock_result, Err(StoreError::InvalidPath)));
        let _ = fs::remove_dir_all(replacement_directory);
        let _ = fs::remove_dir_all(alias_directory);
        let _ = fs::remove_dir_all(lock_directory);
    }

    #[test]
    fn retained_database_identity_rejects_valid_named_replacement() {
        let directory = test_directory("database-replacement");
        let _ = fs::remove_dir_all(&directory);
        let store = LocalStore::open(&directory).unwrap();
        let database = store.database_path();
        let displaced = directory.join("displaced.sqlite3");
        fs::rename(&database, &displaced).unwrap();
        fs::copy(&displaced, &database).unwrap();

        assert_eq!(
            crate::required_schema_version(&store.db).unwrap(),
            crate::LOCAL_STORE_SCHEMA_VERSION
        );
        assert!(matches!(
            with_storage_ownership_observation(&store, |_| ()),
            Err(StoreError::InvalidPath)
        ));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn observation_rejects_structural_schema_mutation_during_callback() {
        let directory = test_directory("schema-mutation");
        let _ = fs::remove_dir_all(&directory);
        let store = LocalStore::open(&directory).unwrap();

        let result = with_storage_ownership_observation(&store, |_| {
            store
                .db
                .execute("CREATE TEMP TABLE ownership_unknown(value TEXT)", [])
                .unwrap();
        });
        assert!(matches!(result, Err(StoreError::SchemaMismatch)));
        let _ = fs::remove_dir_all(directory);
    }
}
