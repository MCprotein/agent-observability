//! Root-scoped writer coordination. Not a write admission decision.
//!
//! Each acquisition opens an independent descriptor: dropping one shared writer
//! must never unlock another. The lock is advisory; coherence requires every
//! production writer to participate. No operational path is enabled by this API.

use crate::MutationGuard;
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

const LOCK_NAME: &str = "storage-accounting.lock";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageCoherenceError {
    Busy,
    Missing,
    WrongMutationRoot,
    InsecurePermissions,
    Symlink,
    InvalidIdentity,
    Inventory(crate::StorageInventoryError),
    Io(std::io::ErrorKind),
    UnsupportedPlatform,
}

impl std::fmt::Display for StorageCoherenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Busy => "storage accounting barrier is busy",
            Self::Missing => "storage accounting barrier is missing",
            Self::WrongMutationRoot => "storage accounting mutation root mismatch",
            Self::InsecurePermissions => "storage accounting path is not private",
            Self::Symlink => "storage accounting refuses symbolic links",
            Self::InvalidIdentity => "storage accounting private identity changed",
            Self::Inventory(_) => "storage accounting inventory validation failed",
            Self::Io(_) => "storage accounting barrier I/O failure",
            Self::UnsupportedPlatform => "storage accounting barrier platform unsupported",
        })
    }
}
impl std::error::Error for StorageCoherenceError {}
impl From<std::io::Error> for StorageCoherenceError {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::WouldBlock => Self::Busy,
            std::io::ErrorKind::NotFound => Self::Missing,
            kind => Self::Io(kind),
        }
    }
}

fn map_mutation_error(error: crate::SingletonError) -> StorageCoherenceError {
    match error {
        crate::SingletonError::Io(error) => error.into(),
        crate::SingletonError::AlreadyRunning => StorageCoherenceError::Busy,
        crate::SingletonError::CorruptMetadata => StorageCoherenceError::InvalidIdentity,
        crate::SingletonError::InsecurePermissions => StorageCoherenceError::InsecurePermissions,
        crate::SingletonError::Symlink => StorageCoherenceError::Symlink,
        crate::SingletonError::UnsupportedPlatform => StorageCoherenceError::UnsupportedPlatform,
        crate::SingletonError::WrongMutationRoot => StorageCoherenceError::WrongMutationRoot,
    }
}

pub struct StorageBarrier {
    root: PathBuf,
    root_directory: File,
    runtime_directory: File,
    identity: File,
}
impl std::fmt::Debug for StorageBarrier {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StorageBarrier")
            .finish_non_exhaustive()
    }
}

#[must_use = "retain the guard until the write and journal cleanup have finished"]
pub struct StorageWriteGuard<'barrier> {
    barrier: &'barrier StorageBarrier,
    file: File,
}
impl std::fmt::Debug for StorageWriteGuard<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StorageWriteGuard")
            .finish_non_exhaustive()
    }
}
#[must_use = "retain the freeze through assessment and the admitted commit or rollback"]
pub struct StorageFreezeGuard<'barrier, 'mutation> {
    held: StorageWriteGuard<'barrier>,
    mutation: &'mutation MutationGuard,
}
impl std::fmt::Debug for StorageFreezeGuard<'_, '_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StorageFreezeGuard")
            .finish_non_exhaustive()
    }
}

/// Owns both locks so a staging-binding callback can release them together.
/// Field order releases accounting exclusion before root mutation ownership.
#[must_use = "retain both locks until the protected operation has completed"]
pub struct OwnedStorageFreezeGuard<'barrier> {
    held: StorageWriteGuard<'barrier>,
    mutation: MutationGuard,
}
impl std::fmt::Debug for OwnedStorageFreezeGuard<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OwnedStorageFreezeGuard")
            .finish_non_exhaustive()
    }
}
impl OwnedStorageFreezeGuard<'_> {
    pub fn mutation(&self) -> &MutationGuard {
        &self.mutation
    }

    pub fn revalidate(&self) -> Result<(), StorageCoherenceError> {
        self.mutation
            .require_root(&self.held.barrier.root)
            .map_err(map_mutation_error)?;
        self.held.revalidate()
    }
}

impl StorageBarrier {
    /// Bootstrap only. Never replaces, truncates, or repairs an existing lock.
    pub fn initialize(
        root: &Path,
        mutation: &MutationGuard,
    ) -> Result<Self, StorageCoherenceError> {
        mutation.require_root(root).map_err(map_mutation_error)?;
        let root_directory = open_private(root, true, false)?;
        let runtime_directory = open_private(&root.join("runtime"), true, false)?;
        let path = root.join("runtime").join(LOCK_NAME);
        match open_private(&path, false, true) {
            Ok(_) | Err(StorageCoherenceError::Io(std::io::ErrorKind::AlreadyExists)) => {}
            Err(error) => return Err(error),
        }
        let barrier = Self::open_existing(root)?;
        // Retry after an interrupted bootstrap must complete durability too.
        barrier.identity.sync_all()?;
        barrier.runtime_directory.sync_all()?;
        validate_named(&root_directory, root, true)?;
        validate_named(&runtime_directory, &root.join("runtime"), true)?;
        mutation.require_root(root).map_err(map_mutation_error)?;
        barrier.revalidate()?;
        Ok(barrier)
    }

    /// Noncreating acquisition of root and stable private lock identities.
    pub fn open_existing(root: &Path) -> Result<Self, StorageCoherenceError> {
        let barrier = Self {
            root: root.to_path_buf(),
            root_directory: open_private(root, true, false)?,
            runtime_directory: open_private(&root.join("runtime"), true, false)?,
            identity: open_private(&root.join("runtime").join(LOCK_NAME), false, false)?,
        };
        barrier.revalidate()?;
        Ok(barrier)
    }

    /// Compatibility discovery for writers before the coordinated policy is enabled.
    /// Only an absent lock in an intact private layout returns `None`. An existing but
    /// invalid or disappearing lock is an error, never permission to bypass coordination.
    /// Separated-mode admission must use `open_existing`, not this optional discovery.
    pub fn open_if_initialized(root: &Path) -> Result<Option<Self>, StorageCoherenceError> {
        let root_directory = open_private(root, true, false)?;
        let runtime_directory = open_private(&root.join("runtime"), true, false)?;
        match fs::symlink_metadata(root.join("runtime").join(LOCK_NAME)) {
            Ok(_) => Self::open_existing(root).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                validate_named(&root_directory, root, true)?;
                validate_named(&runtime_directory, &root.join("runtime"), true)?;
                same_device(&root_directory, &runtime_directory)?;
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn revalidate(&self) -> Result<(), StorageCoherenceError> {
        validate_named(&self.root_directory, &self.root, true)?;
        validate_named(&self.runtime_directory, &self.root.join("runtime"), true)?;
        validate_named(
            &self.identity,
            &self.root.join("runtime").join(LOCK_NAME),
            false,
        )?;
        same_device(&self.root_directory, &self.runtime_directory)?;
        same_device(&self.root_directory, &self.identity)
    }

    fn acquire(&self, exclusive: bool) -> Result<StorageWriteGuard<'_>, StorageCoherenceError> {
        self.revalidate()?;
        let file = open_private(&self.root.join("runtime").join(LOCK_NAME), false, false)?;
        if exclusive {
            FileExt::try_lock_exclusive(&file)?;
        } else {
            FileExt::try_lock_shared(&file)?;
        }
        let held = StorageWriteGuard {
            barrier: self,
            file,
        };
        held.revalidate()?;
        Ok(held)
    }

    pub fn try_begin_write(&self) -> Result<StorageWriteGuard<'_>, StorageCoherenceError> {
        self.acquire(false)
    }

    /// Consumes root mutation ownership, retaining it through the exclusive cut.
    /// On failure the consumed mutation guard is released; no write is authorized.
    pub fn try_freeze_owned(
        &self,
        mutation: MutationGuard,
    ) -> Result<OwnedStorageFreezeGuard<'_>, StorageCoherenceError> {
        mutation
            .require_root(&self.root)
            .map_err(map_mutation_error)?;
        let scope = OwnedStorageFreezeGuard {
            held: self.acquire(true)?,
            mutation,
        };
        scope.revalidate()?;
        Ok(scope)
    }

    /// Hold the returned guard through assessment and the admitted commit/rollback.
    /// Never acquire a long-lived report publication guard from inside this cut.
    pub fn try_freeze<'barrier, 'mutation>(
        &'barrier self,
        mutation: &'mutation MutationGuard,
    ) -> Result<StorageFreezeGuard<'barrier, 'mutation>, StorageCoherenceError> {
        mutation
            .require_root(&self.root)
            .map_err(map_mutation_error)?;
        let freeze = StorageFreezeGuard {
            held: self.acquire(true)?,
            mutation,
        };
        freeze.revalidate()?;
        Ok(freeze)
    }
}

impl StorageWriteGuard<'_> {
    pub fn revalidate(&self) -> Result<(), StorageCoherenceError> {
        self.barrier.revalidate()?;
        same_lock_identity(&self.file, &self.barrier.identity)?;
        validate_named(
            &self.file,
            &self.barrier.root.join("runtime").join(LOCK_NAME),
            false,
        )?;
        self.barrier.revalidate()
    }
}

impl Drop for StorageWriteGuard<'_> {
    fn drop(&mut self) {
        // Release this acquisition even if a process-creation window has duplicated
        // its descriptor. Independently opened writer permits keep their own locks.
        let _ = FileExt::unlock(&self.file);
    }
}

/// Root-serialized writer participation; this scope makes no admission decision.
#[derive(Debug)]
pub struct StorageMutationWriter<'barrier> {
    permit: Option<StorageWriteGuard<'barrier>>,
    mutation: MutationGuard,
    root: PathBuf,
}

impl<'barrier> StorageMutationWriter<'barrier> {
    pub fn acquire(
        root: &Path,
        barrier: Option<&'barrier StorageBarrier>,
    ) -> Result<Self, StorageCoherenceError> {
        Self::acquire_with(root, barrier, false, false, || {})
    }

    /// Excludes other accounting participants without making an admission decision.
    pub fn acquire_exclusive(
        root: &Path,
        barrier: Option<&'barrier StorageBarrier>,
    ) -> Result<Self, StorageCoherenceError> {
        Self::acquire_with(root, barrier, true, false, || {})
    }

    /// Waits for root mutation ownership, then tries shared accounting once.
    /// Calls the fast, non-reentrant callback once only after actual root contention.
    pub fn acquire_waiting_for_root(
        root: &Path,
        barrier: Option<&'barrier StorageBarrier>,
        on_contention: impl FnOnce(),
    ) -> Result<Self, StorageCoherenceError> {
        Self::acquire_with(root, barrier, false, true, on_contention)
    }

    /// Waits for root mutation ownership, then tries accounting exclusion once.
    /// Intended for foreground/manual operations whose caller owns the wait policy.
    /// Calls the fast, non-reentrant callback once only after actual root contention.
    pub fn acquire_exclusive_waiting_for_root(
        root: &Path,
        barrier: Option<&'barrier StorageBarrier>,
        on_contention: impl FnOnce(),
    ) -> Result<Self, StorageCoherenceError> {
        Self::acquire_with(root, barrier, true, true, on_contention)
    }

    fn acquire_with(
        root: &Path,
        barrier: Option<&'barrier StorageBarrier>,
        exclusive: bool,
        wait_for_root: bool,
        on_contention: impl FnOnce(),
    ) -> Result<Self, StorageCoherenceError> {
        match barrier {
            Some(barrier) if barrier.root != root => {
                return Err(StorageCoherenceError::WrongMutationRoot);
            }
            None if StorageBarrier::open_if_initialized(root)?.is_some() => {
                return Err(StorageCoherenceError::InvalidIdentity);
            }
            _ => {}
        }
        let mutation = if barrier.is_some() && wait_for_root {
            MutationGuard::acquire_existing(&root.join("runtime"), on_contention)
        } else if barrier.is_some() {
            MutationGuard::try_acquire_existing(&root.join("runtime"))
        } else if wait_for_root {
            MutationGuard::acquire_waiting(&root.join("runtime"), on_contention)
        } else {
            MutationGuard::try_acquire(&root.join("runtime"))
        }
        .map_err(map_mutation_error)?;
        mutation.require_root(root).map_err(map_mutation_error)?;
        let permit = barrier
            .map(|barrier| barrier.acquire(exclusive))
            .transpose()?;
        let scope = Self {
            permit,
            mutation,
            root: root.to_path_buf(),
        };
        scope.revalidate()?;
        Ok(scope)
    }

    pub fn mutation(&self) -> &MutationGuard {
        &self.mutation
    }

    pub fn revalidate(&self) -> Result<(), StorageCoherenceError> {
        self.mutation
            .require_root(&self.root)
            .map_err(map_mutation_error)?;
        if let Some(permit) = &self.permit {
            if permit.barrier.root != self.root {
                return Err(StorageCoherenceError::WrongMutationRoot);
            }
            permit.revalidate()?;
        } else if StorageBarrier::open_if_initialized(&self.root)?.is_some() {
            return Err(StorageCoherenceError::InvalidIdentity);
        }
        Ok(())
    }
}
impl StorageFreezeGuard<'_, '_> {
    pub fn revalidate(&self) -> Result<(), StorageCoherenceError> {
        self.mutation
            .require_root(&self.held.barrier.root)
            .map_err(map_mutation_error)?;
        self.held.revalidate()
    }

    /// Recognizes only this freeze's exact stable accounting lock, not descendants.
    pub fn matches_accounting_lock(
        &self,
        relative: &Path,
        candidate: &File,
    ) -> Result<bool, StorageCoherenceError> {
        if relative != Path::new("runtime").join(LOCK_NAME) {
            return Ok(false);
        }
        self.revalidate()?;
        same_lock_identity(candidate, &self.held.barrier.identity)?;
        validate_named(candidate, &self.held.barrier.root.join(relative), false)?;
        self.revalidate()?;
        Ok(true)
    }

    /// Classifies under this exact root's exclusive cut, without releasing it.
    /// All production writers must participate before this is used for admission.
    pub fn classify(
        &self,
        classifier: impl FnMut(
            &Path,
            &File,
        )
            -> Result<crate::StorageAllocationClass, crate::StorageInventoryError>,
    ) -> Result<crate::storage_inventory::StorageAllocationObservationV1, StorageCoherenceError>
    {
        self.revalidate()?;
        let inventory = crate::storage_inventory::classify_storage(
            &self.held.barrier.root,
            self.mutation,
            classifier,
        )
        .map_err(StorageCoherenceError::Inventory)?;
        self.revalidate()?;
        Ok(inventory)
    }
}

#[cfg(unix)]
fn open_private(path: &Path, directory: bool, create: bool) -> Result<File, StorageCoherenceError> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(!directory)
        .create_new(create)
        .mode(0o600)
        .custom_flags(crate::lock::no_follow_flag() | crate::lock::nonblocking_flag());
    let file = options.open(path)?;
    validate_named(&file, path, directory)?;
    Ok(file)
}
#[cfg(unix)]
fn validate_named(file: &File, path: &Path, directory: bool) -> Result<(), StorageCoherenceError> {
    use std::os::unix::fs::MetadataExt;
    let held = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    let valid = |metadata: &fs::Metadata| {
        if directory {
            metadata.is_dir() && metadata.mode() & 0o7777 == 0o700
        } else {
            metadata.is_file()
                && metadata.mode() & 0o7777 == 0o600
                && metadata.nlink() == 1
                && metadata.len() == 0
        }
    };
    if !valid(&held) || !valid(&named) || (held.dev(), held.ino()) != (named.dev(), named.ino()) {
        return Err(StorageCoherenceError::InvalidIdentity);
    }
    Ok(())
}
#[cfg(unix)]
fn same_lock_identity(first: &File, second: &File) -> Result<(), StorageCoherenceError> {
    use std::os::unix::fs::MetadataExt;
    let first = first.metadata()?;
    let second = second.metadata()?;
    for metadata in [&first, &second] {
        if !metadata.is_file()
            || metadata.mode() & 0o7777 != 0o600
            || metadata.nlink() != 1
            || metadata.len() != 0
        {
            return Err(StorageCoherenceError::InvalidIdentity);
        }
    }
    if (first.dev(), first.ino()) != (second.dev(), second.ino()) {
        return Err(StorageCoherenceError::InvalidIdentity);
    }
    Ok(())
}
#[cfg(not(unix))]
fn same_lock_identity(_: &File, _: &File) -> Result<(), StorageCoherenceError> {
    Err(StorageCoherenceError::UnsupportedPlatform)
}

#[cfg(unix)]
fn same_device(first: &File, second: &File) -> Result<(), StorageCoherenceError> {
    use std::os::unix::fs::MetadataExt;
    if first.metadata()?.dev() != second.metadata()?.dev() {
        return Err(StorageCoherenceError::InvalidIdentity);
    }
    Ok(())
}
#[cfg(not(unix))]
fn open_private(_: &Path, _: bool, _: bool) -> Result<File, StorageCoherenceError> {
    Err(StorageCoherenceError::UnsupportedPlatform)
}
#[cfg(not(unix))]
fn validate_named(_: &File, _: &Path, _: bool) -> Result<(), StorageCoherenceError> {
    Err(StorageCoherenceError::UnsupportedPlatform)
}
#[cfg(not(unix))]
fn same_device(_: &File, _: &File) -> Result<(), StorageCoherenceError> {
    Err(StorageCoherenceError::UnsupportedPlatform)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink};
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    // Keep lock-release assertions outside the harness's concurrent process
    // creation: a fork-before-exec may temporarily inherit another test's locks.
    // The child creates its own fixture after exec; no timing retries are used.
    fn run_isolated(case: &str) -> bool {
        if std::env::var("AGENTOBS_TEST_STORAGE_COHERENCE_CASE").as_deref() == Ok(case) {
            return false;
        }
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    &format!("storage_coherence::tests::{case}"),
                    "--test-threads=1"
                ])
                .env("AGENTOBS_TEST_STORAGE_COHERENCE_CASE", case)
                .status()
                .unwrap()
                .success()
        );
        true
    }

    fn fixture() -> (std::path::PathBuf, crate::MutationGuard) {
        let root = std::env::temp_dir().join(format!(
            "storage-coherence-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mutation = crate::MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        (root, mutation)
    }

    #[test]
    fn writer_release_is_not_delayed_by_a_duplicate_descriptor() {
        let (root, mutation) = fixture();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let writer = barrier.try_begin_write().unwrap();
        let duplicate = writer.file.try_clone().unwrap();
        let other_writer = barrier.try_begin_write().unwrap();
        drop(writer);
        assert!(matches!(
            barrier.try_freeze(&mutation),
            Err(StorageCoherenceError::Busy)
        ));
        drop(other_writer);
        let freeze = barrier.try_freeze(&mutation).unwrap();
        freeze.revalidate().unwrap();
        drop(freeze);
        drop(duplicate);
        drop(barrier);
        drop(mutation);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn foreground_writer_waits_only_for_root_mutation() {
        let (root, mutation) = fixture();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        assert!(matches!(
            StorageMutationWriter::acquire_exclusive(&root, Some(&barrier)),
            Err(StorageCoherenceError::Busy)
        ));
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (result_tx, result_rx) = std::sync::mpsc::channel();

        std::thread::scope(|scope| {
            let root = &root;
            let barrier = &barrier;
            scope.spawn(move || {
                let result = StorageMutationWriter::acquire_exclusive_waiting_for_root(
                    root,
                    Some(barrier),
                    || {
                        started_tx.send(()).unwrap();
                    },
                )
                .and_then(|writer| writer.revalidate());
                result_tx.send(result).unwrap();
            });
            let contention = started_rx.recv_timeout(std::time::Duration::from_secs(5));
            drop(mutation);
            contention.unwrap();
            result_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .unwrap();
        });

        let accounting_writer = barrier.try_begin_write().unwrap();
        let started = std::time::Instant::now();
        assert!(matches!(
            StorageMutationWriter::acquire_exclusive_waiting_for_root(
                &root,
                Some(&barrier),
                || {
                    panic!("uncontended root must not notify for accounting contention");
                }
            ),
            Err(StorageCoherenceError::Busy)
        ));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        drop(accounting_writer);
        let mutation = crate::MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        drop(mutation);
        drop(barrier);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shared_foreground_writer_waits_for_actual_root_contention() {
        for initialized in [false, true] {
            let (root, mutation) = fixture();
            let barrier =
                initialized.then(|| StorageBarrier::initialize(&root, &mutation).unwrap());
            let (started_tx, started_rx) = std::sync::mpsc::channel();
            let (result_tx, result_rx) = std::sync::mpsc::channel();

            std::thread::scope(|scope| {
                let root = &root;
                let barrier = barrier.as_ref();
                scope.spawn(move || {
                    let result =
                        StorageMutationWriter::acquire_waiting_for_root(root, barrier, || {
                            started_tx.send(()).unwrap();
                        })
                        .and_then(|writer| writer.revalidate());
                    result_tx.send(result).unwrap();
                });
                let contention = started_rx.recv_timeout(std::time::Duration::from_secs(5));
                drop(mutation);
                contention.unwrap();
                result_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap()
                    .unwrap();
            });

            drop(barrier);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn shared_foreground_writer_coexists_but_does_not_retry_accounting_freeze() {
        let (root, mutation) = fixture();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        drop(mutation);

        let participant = barrier.try_begin_write().unwrap();
        let writer = StorageMutationWriter::acquire_waiting_for_root(&root, Some(&barrier), || {
            panic!("uncontended root must not notify")
        })
        .unwrap();
        participant.revalidate().unwrap();
        writer.revalidate().unwrap();
        drop(writer);
        drop(participant);

        let accounting_freeze = barrier.acquire(true).unwrap();
        assert!(matches!(
            StorageMutationWriter::acquire_waiting_for_root(&root, Some(&barrier), || {
                panic!("uncontended root must not notify for accounting contention");
            }),
            Err(StorageCoherenceError::Busy)
        ));
        crate::MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        drop(accounting_freeze);
        drop(barrier);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shared_foreground_writer_fails_closed_for_missing_or_replaced_identity() {
        let (root, mutation) = fixture();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        drop(mutation);
        std::fs::remove_file(root.join("runtime/mutation.lock")).unwrap();
        assert!(matches!(
            StorageMutationWriter::acquire_waiting_for_root(&root, Some(&barrier), || {
                panic!("missing identity is not contention");
            }),
            Err(StorageCoherenceError::Missing)
        ));
        drop(barrier);
        std::fs::remove_dir_all(root).unwrap();

        let (root, mutation) = fixture();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        drop(mutation);
        let path = root.join("runtime/storage-accounting.lock");
        std::fs::rename(&path, root.join("runtime/original-accounting.lock")).unwrap();
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        assert!(matches!(
            StorageMutationWriter::acquire_waiting_for_root(&root, Some(&barrier), || {
                panic!("replaced identity is not contention");
            }),
            Err(StorageCoherenceError::InvalidIdentity)
        ));
        assert!(matches!(
            StorageMutationWriter::acquire_waiting_for_root(&root, None, || {
                panic!("initialized accounting is not root contention");
            }),
            Err(StorageCoherenceError::InvalidIdentity)
        ));
        crate::MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        drop(barrier);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn subprocess_writer_probe() {
        let Some(root) = std::env::var_os("AGENTOBS_TEST_STORAGE_BARRIER_ROOT") else {
            return;
        };
        let barrier = StorageBarrier::open_existing(std::path::Path::new(&root)).unwrap();
        let result = barrier.try_begin_write();
        if std::env::var_os("AGENTOBS_TEST_STORAGE_BARRIER_BUSY").is_some() {
            assert!(matches!(result, Err(StorageCoherenceError::Busy)));
        } else {
            result.unwrap().revalidate().unwrap();
        }
    }

    #[test]
    fn freeze_excludes_an_independent_process_only_for_its_lifetime() {
        if run_isolated("freeze_excludes_an_independent_process_only_for_its_lifetime") {
            return;
        }
        let (root, mutation) = fixture();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let probe = |busy: bool| {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "storage_coherence::tests::subprocess_writer_probe",
                ])
                .env("AGENTOBS_TEST_STORAGE_BARRIER_ROOT", &root)
                .env_remove("AGENTOBS_TEST_STORAGE_BARRIER_BUSY");
            if busy {
                command.env("AGENTOBS_TEST_STORAGE_BARRIER_BUSY", "1");
            }
            assert!(command.status().unwrap().success());
        };
        let writer = barrier.try_begin_write().unwrap();
        probe(false);
        drop(writer);
        let freeze = barrier.try_freeze(&mutation).unwrap();
        probe(true);
        freeze.revalidate().unwrap();
        drop(freeze);
        probe(false);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn owned_freeze_releases_mutation_and_writer_exclusion_together() {
        if run_isolated("owned_freeze_releases_mutation_and_writer_exclusion_together") {
            return;
        }
        let (root, mutation) = fixture();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let scope = barrier.try_freeze_owned(mutation).unwrap();
        scope.revalidate().unwrap();
        scope.mutation().require_root(&root).unwrap();
        assert!(matches!(
            crate::MutationGuard::try_acquire(&root.join("runtime")),
            Err(crate::SingletonError::AlreadyRunning)
        ));
        assert!(matches!(
            barrier.try_begin_write(),
            Err(StorageCoherenceError::Busy)
        ));
        assert!(!format!("{scope:?}").contains(root.to_str().unwrap()));
        drop(scope);
        let mutation = crate::MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let writer = barrier.try_begin_write().unwrap();
        // A denied freeze must release its consumed mutation guard too.
        assert!(matches!(
            barrier.try_freeze_owned(mutation),
            Err(StorageCoherenceError::Busy)
        ));
        let mutation = crate::MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        drop(writer);
        let scope = barrier.try_freeze_owned(mutation).unwrap();
        std::fs::rename(
            root.join("runtime/storage-accounting.lock"),
            root.join("runtime/replaced.lock"),
        )
        .unwrap();
        assert!(scope.revalidate().is_err());
        drop(scope);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_open_never_creates_and_initialization_preserves_identity() {
        let (root, mutation) = fixture();
        assert!(StorageBarrier::open_existing(&root).is_err());
        assert!(
            StorageBarrier::open_if_initialized(&root)
                .unwrap()
                .is_none()
        );
        assert!(!root.join("runtime/storage-accounting.lock").exists());
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        assert!(
            StorageBarrier::open_if_initialized(&root)
                .unwrap()
                .is_some()
        );
        let second = StorageBarrier::initialize(&root, &mutation).unwrap();
        barrier.revalidate().unwrap();
        second.revalidate().unwrap();
        assert!(!format!("{barrier:?}").contains(root.to_str().unwrap()));
        std::fs::remove_file(root.join("runtime/storage-accounting.lock")).unwrap();
        std::os::unix::fs::symlink("absent", root.join("runtime/storage-accounting.lock")).unwrap();
        assert!(StorageBarrier::open_if_initialized(&root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn frozen_inventory_retains_exclusion_after_success_and_failure() {
        if run_isolated("frozen_inventory_retains_exclusion_after_success_and_failure") {
            return;
        }
        let (root, mutation) = fixture();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let freeze = barrier.try_freeze(&mutation).unwrap();
        let relative = std::path::Path::new("runtime/storage-accounting.lock");
        let lock = std::fs::File::open(root.join(relative)).unwrap();
        let different = std::fs::File::open(root.join("runtime/mutation.lock")).unwrap();
        assert!(freeze.matches_accounting_lock(relative, &lock).unwrap());
        assert!(
            freeze
                .matches_accounting_lock(relative, &different)
                .is_err()
        );
        assert!(
            !freeze
                .matches_accounting_lock(std::path::Path::new("unowned"), &lock)
                .unwrap()
        );
        let inventory = freeze
            .classify(|_, _| {
                assert!(matches!(
                    barrier.try_begin_write(),
                    Err(StorageCoherenceError::Busy)
                ));
                Ok(crate::StorageAllocationClass::Unknown)
            })
            .unwrap();
        assert_eq!(inventory.unknown_entry_count, 4);
        assert!(matches!(
            barrier.try_begin_write(),
            Err(StorageCoherenceError::Busy)
        ));
        assert_eq!(
            freeze.classify(|_, _| Err(crate::StorageInventoryError::OwnershipMismatch)),
            Err(StorageCoherenceError::Inventory(
                crate::StorageInventoryError::OwnershipMismatch
            ))
        );
        assert!(matches!(
            barrier.try_begin_write(),
            Err(StorageCoherenceError::Busy)
        ));
        drop(freeze);
        barrier.try_begin_write().unwrap().revalidate().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shared_writers_and_exclusive_freeze_have_independent_lock_lifetimes() {
        if run_isolated("shared_writers_and_exclusive_freeze_have_independent_lock_lifetimes") {
            return;
        }
        let (root, mutation) = fixture();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let writer = barrier.try_begin_write().unwrap();
        let another = barrier.try_begin_write().unwrap();
        assert!(matches!(
            barrier.try_freeze(&mutation),
            Err(StorageCoherenceError::Busy)
        ));
        drop(writer);
        assert!(matches!(
            barrier.try_freeze(&mutation),
            Err(StorageCoherenceError::Busy)
        ));
        drop(another);
        let freeze = barrier.try_freeze(&mutation).unwrap();
        freeze.revalidate().unwrap();
        assert!(matches!(
            barrier.try_begin_write(),
            Err(StorageCoherenceError::Busy)
        ));
        assert!(matches!(
            barrier.try_freeze(&mutation),
            Err(StorageCoherenceError::Busy)
        ));
        drop(freeze);
        barrier.try_begin_write().unwrap().revalidate().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replacement_alias_symlink_and_nonempty_lock_fail_closed() {
        let (root, mutation) = fixture();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let path = root.join("runtime/storage-accounting.lock");
        let writer = barrier.try_begin_write().unwrap();
        std::fs::rename(&path, root.join("runtime/original-lock")).unwrap();
        let replacement = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        assert!(barrier.try_begin_write().is_err());
        assert!(writer.revalidate().is_err());
        std::io::Write::write_all(&mut &replacement, b"x").unwrap();
        assert!(StorageBarrier::open_existing(&root).is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::hard_link(root.join("runtime/original-lock"), &path).unwrap();
        assert!(StorageBarrier::open_existing(&root).is_err());
        std::fs::remove_file(&path).unwrap();
        symlink(root.join("runtime/original-lock"), &path).unwrap();
        assert!(StorageBarrier::open_existing(&root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn direct_binding_rejects_aba_even_when_sequential_named_checks_pass() {
        let (root, mutation) = fixture();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let path = root.join("runtime/storage-accounting.lock");
        let original = root.join("runtime/original");
        let alternate = root.join("runtime/alternate");
        let replacement = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&alternate)
            .unwrap();
        // Each old pathname check can succeed against a different inode.
        barrier.revalidate().unwrap();
        std::fs::rename(&path, &original).unwrap();
        std::fs::rename(&alternate, &path).unwrap();
        validate_named(&replacement, &path, false).unwrap();
        std::fs::rename(&path, &alternate).unwrap();
        std::fs::rename(&original, &path).unwrap();
        barrier.revalidate().unwrap();
        assert_eq!(
            same_lock_identity(&barrier.identity, &replacement),
            Err(StorageCoherenceError::InvalidIdentity)
        );
        let misbound = StorageWriteGuard {
            barrier: &barrier,
            file: replacement,
        };
        assert_eq!(
            misbound.revalidate(),
            Err(StorageCoherenceError::InvalidIdentity)
        );
        let freeze = barrier.try_freeze(&mutation).unwrap();
        assert_eq!(
            freeze.matches_accounting_lock(
                std::path::Path::new("runtime/storage-accounting.lock"),
                &misbound.file
            ),
            Err(StorageCoherenceError::InvalidIdentity)
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mutation_errors_preserve_typed_causes() {
        use crate::SingletonError;
        assert_eq!(
            map_mutation_error(SingletonError::Io(std::io::ErrorKind::NotFound.into())),
            StorageCoherenceError::Missing
        );
        assert_eq!(
            map_mutation_error(SingletonError::Io(
                std::io::ErrorKind::PermissionDenied.into()
            )),
            StorageCoherenceError::Io(std::io::ErrorKind::PermissionDenied)
        );
        assert_eq!(
            map_mutation_error(SingletonError::InsecurePermissions),
            StorageCoherenceError::InsecurePermissions
        );
        assert_eq!(
            map_mutation_error(SingletonError::Symlink),
            StorageCoherenceError::Symlink
        );
        assert_eq!(
            map_mutation_error(SingletonError::UnsupportedPlatform),
            StorageCoherenceError::UnsupportedPlatform
        );
        assert_eq!(
            map_mutation_error(SingletonError::WrongMutationRoot),
            StorageCoherenceError::WrongMutationRoot
        );
        assert_eq!(
            map_mutation_error(SingletonError::AlreadyRunning),
            StorageCoherenceError::Busy
        );
        assert_eq!(
            map_mutation_error(SingletonError::CorruptMetadata),
            StorageCoherenceError::InvalidIdentity
        );
    }

    #[test]
    fn wrong_mutation_root_and_replaced_root_are_rejected() {
        let (root, mutation) = fixture();
        let (other, other_mutation) = fixture();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        assert!(barrier.try_freeze(&other_mutation).is_err());
        let original = root.with_extension("original");
        std::fs::rename(&root, &original).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::rename(original.join("runtime"), root.join("runtime")).unwrap();
        assert!(barrier.try_begin_write().is_err());
        assert!(barrier.try_freeze(&mutation).is_err());
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(original).unwrap();
        std::fs::remove_dir_all(other).unwrap();
    }
}
