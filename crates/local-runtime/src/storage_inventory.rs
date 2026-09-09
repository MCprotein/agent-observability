//! Bounded filesystem classification for separated storage accounting.
//!
//! The classifier callback is a trusted composition boundary: it owns semantic
//! validation for the already-opened entry and returns the resulting bucket.
//! Names alone are not ownership evidence, and this callback does not by itself
//! prove authority or coherence with writers outside the supplied mutation guard.
//!
//! The scan revalidates identities, allocation metadata, and directory entries
//! after callbacks and before returning. This detects observed races; without a
//! freeze shared by every writer it is not an ABA-free filesystem snapshot.

use crate::{MutationGuard, storage::MAX_ACCOUNTING_ENTRIES};
use std::{fs::File, path::Path};

const ALLOCATION_BLOCK_BYTES: u64 = 4096;
const UNIX_ALLOCATION_UNIT_BYTES: u64 = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageAllocationClass {
    Retained,
    Workspace,
    Unknown,
}

/// Observed allocation only, deliberately distinct from policy inputs.
/// No conversion to an admission snapshot is provided: config, reservations,
/// free space and all-writer participation must be established separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageAllocationObservationV1 {
    pub retained_bytes: u64,
    pub workspace_bytes: u64,
    pub unknown_bytes: u64,
    pub unknown_entry_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageInventoryError {
    Io(std::io::ErrorKind),
    WrongMutationRoot,
    OwnershipMismatch,
    InsecurePermissions,
    Symlink,
    Hardlink,
    MountOrDeviceMismatch,
    UnsupportedFileType,
    Replaced,
    Disappeared,
    EntryLimit,
    Overflow,
    UnsupportedPlatform,
}

impl std::fmt::Display for StorageInventoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Io(_) => "storage inventory I/O failure",
            Self::WrongMutationRoot => "storage inventory mutation root mismatch",
            Self::OwnershipMismatch => "storage inventory ownership evidence mismatch",
            Self::InsecurePermissions => "storage inventory path is not private",
            Self::Symlink => "storage inventory refuses symbolic links",
            Self::Hardlink => "storage inventory refuses hard-linked files",
            Self::MountOrDeviceMismatch => "storage inventory filesystem device mismatch",
            Self::UnsupportedFileType => "storage inventory refuses unsupported file types",
            Self::Replaced => "storage inventory entry changed during classification",
            Self::Disappeared => "storage inventory entry disappeared during classification",
            Self::EntryLimit => "storage inventory entry limit exceeded",
            Self::Overflow => "storage inventory arithmetic overflow",
            Self::UnsupportedPlatform => "storage inventory is unsupported on this platform",
        })
    }
}

impl std::error::Error for StorageInventoryError {}

impl From<std::io::Error> for StorageInventoryError {
    fn from(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::NotFound {
            Self::Disappeared
        } else {
            Self::Io(error.kind())
        }
    }
}

#[cfg(unix)]
use std::{
    collections::HashSet,
    ffi::OsString,
    fs::{self, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
};

#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
struct IdentityMetadata {
    device: u64,
    inode: u64,
    mode: u32,
    links: u64,
    size: u64,
    blocks: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

#[cfg(unix)]
impl IdentityMetadata {
    fn capture(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            links: metadata.nlink(),
            size: metadata.size(),
            blocks: metadata.blocks(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        }
    }
}

#[cfg(unix)]
#[derive(Debug)]
struct ObservedEntry {
    relative: PathBuf,
    metadata: IdentityMetadata,
    directory_entries: Option<Vec<OsString>>,
}

#[cfg(unix)]
struct ScannedEntry {
    observed: ObservedEntry,
    class: StorageAllocationClass,
    bytes: u64,
    children: Vec<PathBuf>,
}

#[cfg(unix)]
fn map_guard_error(error: crate::SingletonError) -> StorageInventoryError {
    match error {
        crate::SingletonError::WrongMutationRoot => StorageInventoryError::WrongMutationRoot,
        crate::SingletonError::InsecurePermissions => StorageInventoryError::InsecurePermissions,
        crate::SingletonError::Symlink => StorageInventoryError::Symlink,
        crate::SingletonError::UnsupportedPlatform => StorageInventoryError::UnsupportedPlatform,
        crate::SingletonError::Io(error) => error.into(),
        crate::SingletonError::AlreadyRunning | crate::SingletonError::CorruptMetadata => {
            StorageInventoryError::WrongMutationRoot
        }
    }
}

#[cfg(unix)]
fn read_directory_entries(
    path: &Path,
    maximum_entries: usize,
    limit_error: StorageInventoryError,
) -> Result<Vec<OsString>, StorageInventoryError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        if entries.len() >= maximum_entries {
            return Err(limit_error);
        }
        entries.push(entry?.file_name());
    }
    entries.sort_unstable();
    Ok(entries)
}

#[cfg(unix)]
fn validate_kind_permissions_and_device(
    metadata: &fs::Metadata,
    root_device: u64,
) -> Result<bool, StorageInventoryError> {
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return Err(StorageInventoryError::Symlink);
    }
    if metadata.dev() != root_device {
        return Err(StorageInventoryError::MountOrDeviceMismatch);
    }
    if metadata.is_dir() {
        if metadata.mode() & 0o7777 != 0o700 {
            return Err(StorageInventoryError::InsecurePermissions);
        }
        return Ok(true);
    }
    if metadata.is_file() {
        if metadata.mode() & 0o7777 != 0o600 {
            return Err(StorageInventoryError::InsecurePermissions);
        }
        if metadata.nlink() != 1 {
            return Err(StorageInventoryError::Hardlink);
        }
        return Ok(false);
    }
    Err(StorageInventoryError::UnsupportedFileType)
}

#[cfg(unix)]
fn allocated_bytes(metadata: &IdentityMetadata) -> Result<u64, StorageInventoryError> {
    let bytes = metadata
        .blocks
        .checked_mul(UNIX_ALLOCATION_UNIT_BYTES)
        .ok_or(StorageInventoryError::Overflow)?;
    bytes
        .checked_add(ALLOCATION_BLOCK_BYTES - 1)
        .map(|rounded| rounded / ALLOCATION_BLOCK_BYTES * ALLOCATION_BLOCK_BYTES)
        .ok_or(StorageInventoryError::Overflow)
}

#[cfg(unix)]
fn named_metadata(path: &Path) -> Result<fs::Metadata, StorageInventoryError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(StorageInventoryError::Symlink),
        Ok(metadata) => Ok(metadata),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn revalidate_named(path: &Path, expected: &IdentityMetadata) -> Result<(), StorageInventoryError> {
    if IdentityMetadata::capture(&named_metadata(path)?) != *expected {
        return Err(StorageInventoryError::Replaced);
    }
    Ok(())
}

#[cfg(unix)]
fn scan_entry(
    root: &Path,
    relative: &Path,
    root_device: u64,
    maximum_children: usize,
    identities: &mut HashSet<(u64, u64)>,
    classifier: &mut impl FnMut(&Path, &File) -> Result<StorageAllocationClass, StorageInventoryError>,
) -> Result<ScannedEntry, StorageInventoryError> {
    let path = root.join(relative);
    let before_open = named_metadata(&path)?;
    let is_directory = validate_kind_permissions_and_device(&before_open, root_device)?;

    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(crate::lock::no_follow_flag());
    let file = options.open(&path)?;
    let held_metadata = file.metadata()?;
    validate_kind_permissions_and_device(&held_metadata, root_device)?;
    let metadata = IdentityMetadata::capture(&held_metadata);
    if metadata != IdentityMetadata::capture(&before_open) {
        return Err(StorageInventoryError::Replaced);
    }
    if !identities.insert((metadata.device, metadata.inode)) {
        return Err(StorageInventoryError::Hardlink);
    }

    let directory_entries = is_directory
        .then(|| read_directory_entries(&path, maximum_children, StorageInventoryError::EntryLimit))
        .transpose()?;
    revalidate_named(&path, &metadata)?;
    let class = classifier(relative, &file)?;
    revalidate_named(&path, &metadata)?;
    if IdentityMetadata::capture(&file.metadata()?) != metadata {
        return Err(StorageInventoryError::Replaced);
    }
    if let Some(expected_entries) = &directory_entries
        && read_directory_entries(
            &path,
            expected_entries.len(),
            StorageInventoryError::Replaced,
        )? != *expected_entries
    {
        return Err(StorageInventoryError::Replaced);
    }

    let children = directory_entries
        .iter()
        .flatten()
        .rev()
        .map(|name| relative.join(name))
        .collect();
    Ok(ScannedEntry {
        bytes: allocated_bytes(&metadata)?,
        observed: ObservedEntry {
            relative: relative.to_path_buf(),
            metadata,
            directory_entries,
        },
        class,
        children,
    })
}

#[cfg(unix)]
fn revalidate_entry(root: &Path, entry: &ObservedEntry) -> Result<(), StorageInventoryError> {
    let path = root.join(&entry.relative);
    revalidate_named(&path, &entry.metadata)?;
    if let Some(expected_entries) = &entry.directory_entries
        && read_directory_entries(
            &path,
            expected_entries.len(),
            StorageInventoryError::Replaced,
        )? != *expected_entries
    {
        return Err(StorageInventoryError::Replaced);
    }
    Ok(())
}

fn add_classified_bytes(
    snapshot: &mut StorageAllocationObservationV1,
    class: StorageAllocationClass,
    bytes: u64,
) -> Result<(), StorageInventoryError> {
    let bucket = match class {
        StorageAllocationClass::Retained => &mut snapshot.retained_bytes,
        StorageAllocationClass::Workspace => &mut snapshot.workspace_bytes,
        StorageAllocationClass::Unknown => {
            snapshot.unknown_entry_count = snapshot
                .unknown_entry_count
                .checked_add(1)
                .ok_or(StorageInventoryError::Overflow)?;
            &mut snapshot.unknown_bytes
        }
    };
    *bucket = bucket
        .checked_add(bytes)
        .ok_or(StorageInventoryError::Overflow)?;
    Ok(())
}

/// Classify one private runtime tree without reading entry payloads.
///
/// The callback receives a path relative to `root` and a descriptor retained
/// across its semantic validation. It must validate ownership using the owning
/// module's authority; recognizing a filename is insufficient. Evidence
/// failures must return [`StorageInventoryError::OwnershipMismatch`] rather
/// than being downgraded to [`StorageAllocationClass::Unknown`].
pub(crate) fn classify_storage(
    root: &Path,
    guard: &MutationGuard,
    classifier: impl FnMut(&Path, &File) -> Result<StorageAllocationClass, StorageInventoryError>,
) -> Result<StorageAllocationObservationV1, StorageInventoryError> {
    classify_storage_impl(root, guard, classifier)
}

#[cfg(unix)]
fn classify_storage_impl(
    root: &Path,
    guard: &MutationGuard,
    mut classifier: impl FnMut(&Path, &File) -> Result<StorageAllocationClass, StorageInventoryError>,
) -> Result<StorageAllocationObservationV1, StorageInventoryError> {
    guard.require_root(root).map_err(map_guard_error)?;
    let root_named = named_metadata(root)?;
    if !root_named.is_dir() {
        return Err(StorageInventoryError::UnsupportedFileType);
    }
    let root_device = root_named.dev();

    let mut pending = vec![PathBuf::new()];
    let mut discovered_entries = 1_usize;
    let mut observed = Vec::new();
    let mut identities = HashSet::new();
    let mut snapshot = StorageAllocationObservationV1 {
        retained_bytes: 0,
        workspace_bytes: 0,
        unknown_bytes: 0,
        unknown_entry_count: 0,
    };

    while let Some(relative) = pending.pop() {
        if observed.len() >= MAX_ACCOUNTING_ENTRIES {
            return Err(StorageInventoryError::EntryLimit);
        }
        let entry = scan_entry(
            root,
            &relative,
            root_device,
            MAX_ACCOUNTING_ENTRIES - discovered_entries,
            &mut identities,
            &mut classifier,
        )?;
        discovered_entries = discovered_entries
            .checked_add(entry.children.len())
            .ok_or(StorageInventoryError::Overflow)?;
        pending.extend(entry.children);
        add_classified_bytes(&mut snapshot, entry.class, entry.bytes)?;
        observed.push(entry.observed);
    }

    for entry in &observed {
        revalidate_entry(root, entry)?;
    }
    guard.require_root(root).map_err(map_guard_error)?;
    Ok(snapshot)
}

#[cfg(not(unix))]
fn classify_storage_impl(
    _root: &Path,
    _guard: &MutationGuard,
    _classifier: impl FnMut(&Path, &File) -> Result<StorageAllocationClass, StorageInventoryError>,
) -> Result<StorageAllocationObservationV1, StorageInventoryError> {
    Err(StorageInventoryError::UnsupportedPlatform)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{StorageBudget, storage::MAX_ACCOUNTING_ENTRIES};
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt, symlink},
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        root: PathBuf,
        guard: Option<MutationGuard>,
    }

    impl Fixture {
        fn new(label: &str) -> Self {
            let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "storage-inventory-{label}-{}-{sequence}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&root);
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(root.join("runtime"))
                .unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
            Self {
                root,
                guard: Some(guard),
            }
        }

        fn classify(
            &self,
            classifier: impl FnMut(
                &Path,
                &File,
            ) -> Result<StorageAllocationClass, StorageInventoryError>,
        ) -> Result<StorageAllocationObservationV1, StorageInventoryError> {
            classify_storage(&self.root, self.guard.as_ref().unwrap(), classifier)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            drop(self.guard.take());
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn private_file(path: &Path, bytes: usize) {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(&vec![0_u8; bytes]).unwrap();
    }

    #[test]
    fn zero_allocated_unknown_entry_is_counted() {
        let fixture = Fixture::new("unknown-zero");
        private_file(&fixture.root.join("unknown"), 0);
        let snapshot = fixture
            .classify(|path, _| {
                Ok(if path == Path::new("unknown") {
                    StorageAllocationClass::Unknown
                } else {
                    StorageAllocationClass::Retained
                })
            })
            .unwrap();
        assert_eq!(snapshot.unknown_bytes, 0);
        assert_eq!(snapshot.unknown_entry_count, 1);
    }

    #[test]
    fn semantic_ownership_mismatch_fails_closed() {
        let fixture = Fixture::new("ownership-mismatch");
        private_file(&fixture.root.join("candidate"), 0);
        assert_eq!(
            fixture.classify(|path, _| {
                if path == Path::new("candidate") {
                    Err(StorageInventoryError::OwnershipMismatch)
                } else {
                    Ok(StorageAllocationClass::Retained)
                }
            }),
            Err(StorageInventoryError::OwnershipMismatch)
        );
    }

    #[test]
    fn symlink_and_hardlink_alias_are_rejected() {
        let symlink_fixture = Fixture::new("symlink");
        private_file(&symlink_fixture.root.join("target"), 0);
        symlink("target", symlink_fixture.root.join("link")).unwrap();
        assert_eq!(
            symlink_fixture.classify(|_, _| Ok(StorageAllocationClass::Retained)),
            Err(StorageInventoryError::Symlink)
        );

        let hardlink_fixture = Fixture::new("hardlink");
        let target = hardlink_fixture.root.join("target");
        private_file(&target, 0);
        fs::hard_link(&target, hardlink_fixture.root.join("alias")).unwrap();
        assert_eq!(
            hardlink_fixture.classify(|_, _| Ok(StorageAllocationClass::Retained)),
            Err(StorageInventoryError::Hardlink)
        );
    }

    #[test]
    fn callback_replacement_disappearance_and_growth_are_rejected() {
        let replacement = Fixture::new("replacement");
        private_file(&replacement.root.join("victim"), 0);
        assert_eq!(
            replacement.classify(|path, _| {
                if path == Path::new("victim") {
                    fs::rename(
                        replacement.root.join("victim"),
                        replacement.root.join("saved"),
                    )
                    .unwrap();
                    private_file(&replacement.root.join("victim"), 0);
                }
                Ok(StorageAllocationClass::Retained)
            }),
            Err(StorageInventoryError::Replaced)
        );

        let disappearance = Fixture::new("disappearance");
        private_file(&disappearance.root.join("victim"), 0);
        assert_eq!(
            disappearance.classify(|path, _| {
                if path == Path::new("victim") {
                    fs::remove_file(disappearance.root.join("victim")).unwrap();
                }
                Ok(StorageAllocationClass::Retained)
            }),
            Err(StorageInventoryError::Disappeared)
        );

        let growth = Fixture::new("growth");
        private_file(&growth.root.join("victim"), 0);
        assert_eq!(
            growth.classify(|path, _| {
                if path == Path::new("victim") {
                    OpenOptions::new()
                        .write(true)
                        .open(growth.root.join("victim"))
                        .unwrap()
                        .set_len(8192)
                        .unwrap();
                }
                Ok(StorageAllocationClass::Retained)
            }),
            Err(StorageInventoryError::Replaced)
        );

        let directory_growth = Fixture::new("directory-growth");
        assert_eq!(
            directory_growth.classify(|path, _| {
                if path.as_os_str().is_empty() {
                    private_file(&directory_growth.root.join("new-entry"), 0);
                }
                Ok(StorageAllocationClass::Retained)
            }),
            Err(StorageInventoryError::Replaced)
        );
    }

    #[test]
    fn special_files_are_rejected() {
        use std::os::unix::net::UnixListener;

        let fixture = Fixture::new("special-file");
        let _listener = UnixListener::bind(fixture.root.join("socket")).unwrap();
        assert_eq!(
            fixture.classify(|_, _| Ok(StorageAllocationClass::Retained)),
            Err(StorageInventoryError::UnsupportedFileType)
        );
    }

    #[test]
    fn broad_file_and_directory_permissions_are_rejected() {
        let file_fixture = Fixture::new("file-permission");
        let file = file_fixture.root.join("broad");
        private_file(&file, 0);
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        assert_eq!(
            file_fixture.classify(|_, _| Ok(StorageAllocationClass::Retained)),
            Err(StorageInventoryError::InsecurePermissions)
        );

        let directory_fixture = Fixture::new("directory-permission");
        let directory = directory_fixture.root.join("broad");
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o750)).unwrap();
        assert_eq!(
            directory_fixture.classify(|_, _| Ok(StorageAllocationClass::Retained)),
            Err(StorageInventoryError::InsecurePermissions)
        );

        let setuid_fixture = Fixture::new("setuid-permission");
        let setuid_file = setuid_fixture.root.join("setuid");
        private_file(&setuid_file, 0);
        fs::set_permissions(&setuid_file, fs::Permissions::from_mode(0o4600)).unwrap();
        assert_eq!(
            setuid_fixture.classify(|_, _| Ok(StorageAllocationClass::Retained)),
            Err(StorageInventoryError::InsecurePermissions)
        );

        let sticky_fixture = Fixture::new("sticky-permission");
        let sticky_directory = sticky_fixture.root.join("sticky");
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&sticky_directory)
            .unwrap();
        fs::set_permissions(&sticky_directory, fs::Permissions::from_mode(0o1700)).unwrap();
        assert_eq!(
            sticky_fixture.classify(|_, _| Ok(StorageAllocationClass::Retained)),
            Err(StorageInventoryError::InsecurePermissions)
        );
    }

    #[test]
    fn scan_rejects_more_than_existing_entry_bound() {
        let fixture = Fixture::new("entry-limit");
        for index in 0..MAX_ACCOUNTING_ENTRIES {
            private_file(&fixture.root.join(format!("entry-{index}")), 0);
        }
        let mut callback_count = 0;
        assert_eq!(
            fixture.classify(|_, _| {
                callback_count += 1;
                Ok(StorageAllocationClass::Retained)
            }),
            Err(StorageInventoryError::EntryLimit)
        );
        assert_eq!(callback_count, 0, "wide root is bounded before callback");
    }

    #[test]
    fn all_three_buckets_sum_to_the_strict_tree_allocation() {
        let fixture = Fixture::new("bucket-sum");
        private_file(&fixture.root.join("retained"), 4096);
        private_file(&fixture.root.join("workspace"), 4096);
        private_file(&fixture.root.join("unknown"), 4096);

        let snapshot = fixture
            .classify(|path, _| {
                Ok(match path.to_str() {
                    Some("workspace") => StorageAllocationClass::Workspace,
                    Some("unknown") => StorageAllocationClass::Unknown,
                    _ => StorageAllocationClass::Retained,
                })
            })
            .unwrap();
        let classified_total = snapshot
            .retained_bytes
            .checked_add(snapshot.workspace_bytes)
            .and_then(|bytes| bytes.checked_add(snapshot.unknown_bytes))
            .unwrap();
        assert_eq!(
            classified_total,
            StorageBudget::allocated_tree_bytes_strict(&fixture.root).unwrap()
        );
        assert_eq!(snapshot.unknown_entry_count, 1);
        assert!(snapshot.retained_bytes > 0);
        assert!(snapshot.workspace_bytes > 0);
        assert!(snapshot.unknown_bytes > 0);
    }
}
