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
    use std::collections::BTreeSet;
    use std::fs::{self, File};
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
