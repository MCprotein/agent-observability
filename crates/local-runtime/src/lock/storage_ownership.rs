//! Read-only singleton artifact evidence, separate from process liveness and writer ownership.

use super::{
    CoordinatedSingletonScope, MutationGuard, SingletonError, parse_nonce, same_file,
    same_identity, same_private_directory, validate_private_empty_lock, validate_private_metadata,
};
#[cfg(unix)]
use super::{no_follow_flag, nonblocking_flag};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek},
    path::{Path, PathBuf},
};

const SCOPES: [CoordinatedSingletonScope; 4] = [
    CoordinatedSingletonScope::Runtime,
    CoordinatedSingletonScope::Collector,
    CoordinatedSingletonScope::SettingsUi,
    CoordinatedSingletonScope::DashboardUi,
];

/// Evidence for four exact singleton scopes under a retained root mutation guard.
///
/// Callers must also retain their accounting freeze through capture, matching and final
/// revalidation. Valid metadata does not prove that its PID is alive or authorize cleanup.
pub struct SingletonStorageOwnershipEvidence<'guard> {
    root: PathBuf,
    root_directory: File,
    mutation: &'guard MutationGuard,
    scopes: Vec<ScopeEvidence>,
}

struct ScopeEvidence {
    relative: PathBuf,
    directory: Option<File>,
    lock: Option<File>,
    metadata: Option<(File, Vec<u8>)>,
}

impl std::fmt::Debug for SingletonStorageOwnershipEvidence<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SingletonStorageOwnershipEvidence")
            .finish_non_exhaustive()
    }
}

impl<'guard> SingletonStorageOwnershipEvidence<'guard> {
    /// Noncreating capture of exact private directories, empty locks and bounded metadata.
    pub fn capture(root: &Path, mutation: &'guard MutationGuard) -> Result<Self, SingletonError> {
        mutation.require_root(root)?;
        let root_directory = open_directory(root)?;
        let mut scopes = Vec::with_capacity(SCOPES.len());
        for scope in SCOPES {
            let directory_path = scope.directory(root);
            let relative = directory_path
                .strip_prefix(root)
                .map_err(|_| SingletonError::WrongMutationRoot)?
                .to_path_buf();
            let directory = optional_directory(&directory_path)?;
            let lock = open_optional_file(&directory_path.join("runtime.lock"))?;
            if let Some(lock) = &lock {
                validate_private_empty_lock(lock)?;
            }
            let metadata = open_optional_file(&directory_path.join("runtime.meta"))?
                .map(|file| read_metadata(&file).map(|bytes| (file, bytes)))
                .transpose()?;
            if metadata.is_some() && lock.is_none() {
                return Err(SingletonError::CorruptMetadata);
            }
            scopes.push(ScopeEvidence {
                relative,
                directory,
                lock,
                metadata,
            });
        }
        let evidence = Self {
            root: root.to_path_buf(),
            root_directory,
            mutation,
            scopes,
        };
        evidence.revalidate()?;
        Ok(evidence)
    }

    /// Matches only captured directory, lock and metadata identities; siblings remain unknown.
    pub fn matches_entry(&self, relative: &Path, candidate: &File) -> Result<bool, SingletonError> {
        self.mutation.require_root(&self.root)?;
        for scope in &self.scopes {
            if relative == scope.relative {
                let expected = scope
                    .directory
                    .as_ref()
                    .ok_or(SingletonError::WrongMutationRoot)?;
                validate_directory(candidate, &self.root.join(relative))?;
                same_identity(expected, candidate)?;
                return Ok(true);
            }
            let expected = if relative == scope.relative.join("runtime.lock") {
                Some(scope.lock.as_ref())
            } else if relative == scope.relative.join("runtime.meta") {
                Some(scope.metadata.as_ref().map(|(file, _)| file))
            } else {
                None
            };
            if let Some(expected) = expected {
                let expected = expected.ok_or(SingletonError::WrongMutationRoot)?;
                validate_private_metadata(candidate)?;
                same_identity(expected, candidate)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Rejects changed presence, identities, permissions or bounded metadata content.
    pub fn revalidate(&self) -> Result<(), SingletonError> {
        self.mutation.require_root(&self.root)?;
        validate_directory(&self.root_directory, &self.root)?;
        for scope in &self.scopes {
            let directory_path = self.root.join(&scope.relative);
            match (&scope.directory, optional_directory(&directory_path)?) {
                (Some(held), Some(named)) => same_identity(held, &named)?,
                (None, None) => {}
                _ => return Err(SingletonError::WrongMutationRoot),
            }
            revalidate_file(&directory_path.join("runtime.lock"), scope.lock.as_ref())?;
            if let Some(lock) = &scope.lock {
                validate_private_empty_lock(lock)?;
            }
            revalidate_file(
                &directory_path.join("runtime.meta"),
                scope.metadata.as_ref().map(|(file, _)| file),
            )?;
            if let Some((file, bytes)) = &scope.metadata
                && read_metadata(file)? != *bytes
            {
                return Err(SingletonError::CorruptMetadata);
            }
        }
        validate_directory(&self.root_directory, &self.root)?;
        self.mutation.require_root(&self.root)
    }
}

#[cfg(unix)]
fn validate_directory(file: &File, path: &Path) -> Result<(), SingletonError> {
    use std::os::unix::fs::PermissionsExt;
    same_private_directory(file, path)?;
    if file.metadata()?.permissions().mode() & 0o7777 != 0o700 {
        return Err(SingletonError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_directory(_file: &File, _path: &Path) -> Result<(), SingletonError> {
    Err(SingletonError::UnsupportedPlatform)
}

#[cfg(unix)]
fn open_nofollow(path: &Path) -> Result<File, SingletonError> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag() | nonblocking_flag())
        .open(path)?)
}

#[cfg(not(unix))]
fn open_nofollow(_path: &Path) -> Result<File, SingletonError> {
    Err(SingletonError::UnsupportedPlatform)
}

fn open_directory(path: &Path) -> Result<File, SingletonError> {
    let file = open_nofollow(path)?;
    validate_directory(&file, path)?;
    Ok(file)
}

fn optional_directory(path: &Path) -> Result<Option<File>, SingletonError> {
    match fs::symlink_metadata(path) {
        Ok(_) => open_directory(path).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn open_optional_file(path: &Path) -> Result<Option<File>, SingletonError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => return Err(SingletonError::Symlink),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let file = open_nofollow(path)?;
    validate_private_metadata(&file)?;
    same_file(&file, path)?;
    Ok(Some(file))
}

fn revalidate_file(path: &Path, held: Option<&File>) -> Result<(), SingletonError> {
    match (held, open_optional_file(path)?) {
        (Some(held), Some(named)) => same_identity(held, &named),
        (None, None) => Ok(()),
        _ => Err(SingletonError::WrongMutationRoot),
    }
}

fn read_metadata(file: &File) -> Result<Vec<u8>, SingletonError> {
    validate_private_metadata(file)?;
    let mut file = file.try_clone()?;
    file.rewind()?;
    let mut bytes = Vec::new();
    file.take(super::RUNTIME_METADATA_MAX_BYTES_U64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > super::RUNTIME_METADATA_MAX_BYTES {
        return Err(SingletonError::CorruptMetadata);
    }
    parse_nonce(std::str::from_utf8(&bytes).map_err(|_| SingletonError::CorruptMetadata)?)?;
    Ok(bytes)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> (crate::InstalledLayout, MutationGuard) {
        let path = std::env::temp_dir().join(format!(
            "singleton-ownership-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(!path.exists());
        let layout = crate::install(&path).unwrap();
        let mutation = MutationGuard::acquire(&layout.runtime).unwrap();
        (layout, mutation)
    }

    fn private_write(path: &Path, bytes: &[u8]) {
        use std::io::Write;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(bytes).unwrap();
    }

    fn metadata() -> Vec<u8> {
        format!(
            "runtime_metadata.v1\npid=7\nboot_nonce={}\n",
            "a".repeat(64)
        )
        .into_bytes()
    }

    #[test]
    fn missing_scopes_are_noncreating_and_unknown_names_are_not_approved() {
        let (layout, mutation) = fixture();
        let evidence = SingletonStorageOwnershipEvidence::capture(&layout.root, &mutation).unwrap();
        assert!(!layout.runtime.join("collector").exists());
        assert!(!layout.runtime.join("runtime.lock").exists());
        let candidate = File::open(&layout.config).unwrap();
        for path in [
            "runtime/other/runtime.meta",
            "runtime/.runtime.meta.tmp.7",
            "runtime/collector/child",
        ] {
            assert!(!evidence.matches_entry(Path::new(path), &candidate).unwrap());
        }
        evidence.revalidate().unwrap();
        drop(evidence);
        drop(mutation);
        fs::remove_dir_all(layout.root).unwrap();
    }

    #[test]
    fn valid_unpaired_lock_and_all_four_metadata_scopes_are_owned_without_process_probe() {
        let (layout, mutation) = fixture();
        for scope in SCOPES {
            let directory = scope.directory(&layout.root);
            if !directory.exists() {
                fs::create_dir(&directory).unwrap();
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            }
            private_write(&directory.join("runtime.lock"), &[]);
            private_write(&directory.join("runtime.meta"), &metadata());
        }
        let evidence = SingletonStorageOwnershipEvidence::capture(&layout.root, &mutation).unwrap();
        for scope in SCOPES {
            let directory = scope.directory(&layout.root);
            for path in [
                directory.clone(),
                directory.join("runtime.lock"),
                directory.join("runtime.meta"),
            ] {
                assert!(
                    evidence
                        .matches_entry(
                            path.strip_prefix(&layout.root).unwrap(),
                            &File::open(&path).unwrap()
                        )
                        .unwrap()
                );
            }
        }
        let debug = format!("{evidence:?}");
        assert!(!debug.contains("boot_nonce"));
        assert!(!debug.contains(&"a".repeat(64)));
        assert!(!debug.contains(layout.root.to_str().unwrap()));
        evidence.revalidate().unwrap();
        drop(evidence);
        fs::remove_file(layout.runtime.join("runtime.meta")).unwrap();
        SingletonStorageOwnershipEvidence::capture(&layout.root, &mutation).unwrap();
        drop(mutation);
        fs::remove_dir_all(layout.root).unwrap();
    }

    #[test]
    fn invalid_or_oversized_metadata_and_metadata_without_lock_fail_closed() {
        for bytes in [b"invalid".to_vec(), vec![b'a'; 257], metadata()] {
            let (layout, mutation) = fixture();
            private_write(&layout.runtime.join("runtime.meta"), &bytes);
            assert!(SingletonStorageOwnershipEvidence::capture(&layout.root, &mutation).is_err());
            if bytes != metadata() {
                private_write(&layout.runtime.join("runtime.lock"), &[]);
                assert!(
                    SingletonStorageOwnershipEvidence::capture(&layout.root, &mutation).is_err()
                );
            }
            drop(mutation);
            fs::remove_dir_all(layout.root).unwrap();
        }
    }

    #[test]
    fn appearance_replacement_and_in_place_metadata_change_are_rejected() {
        for change in 0..3 {
            let (layout, mutation) = fixture();
            let path = layout.runtime.join("runtime.meta");
            private_write(&layout.runtime.join("runtime.lock"), &[]);
            if change > 0 {
                private_write(&path, &metadata());
            }
            let evidence =
                SingletonStorageOwnershipEvidence::capture(&layout.root, &mutation).unwrap();
            match change {
                0 => private_write(&path, &metadata()),
                1 => {
                    fs::rename(&path, layout.runtime.join("retained.meta")).unwrap();
                    private_write(&path, &metadata());
                    assert!(
                        evidence
                            .matches_entry(
                                Path::new("runtime/runtime.meta"),
                                &File::open(&path).unwrap()
                            )
                            .is_err()
                    );
                }
                _ => fs::write(
                    &path,
                    String::from_utf8(metadata())
                        .unwrap()
                        .replace(&"a".repeat(64), &"b".repeat(64)),
                )
                .unwrap(),
            }
            assert!(evidence.revalidate().is_err());
            drop(evidence);
            drop(mutation);
            fs::remove_dir_all(layout.root).unwrap();
        }
    }

    #[test]
    fn broad_modes_hardlinks_nonempty_locks_and_wrong_roots_are_rejected() {
        for change in 0..4 {
            let (layout, mutation) = fixture();
            let path = layout.runtime.join("runtime.lock");
            private_write(&path, &[]);
            match change {
                0 => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
                1 => fs::hard_link(&path, layout.runtime.join("alias")).unwrap(),
                2 => fs::write(&path, b"not empty").unwrap(),
                _ => {
                    fs::set_permissions(&layout.runtime, fs::Permissions::from_mode(0o750))
                        .unwrap();
                }
            }
            assert!(SingletonStorageOwnershipEvidence::capture(&layout.root, &mutation).is_err());
            fs::set_permissions(&layout.runtime, fs::Permissions::from_mode(0o700)).unwrap();
            assert!(SingletonStorageOwnershipEvidence::capture(&layout.state, &mutation).is_err());
            drop(mutation);
            fs::remove_dir_all(layout.root).unwrap();
        }
    }

    #[test]
    fn scope_replacement_disappearance_and_wrong_candidate_are_rejected() {
        for change in 0..3 {
            let (layout, mutation) = fixture();
            let directory = layout.runtime.join("collector");
            fs::create_dir(&directory).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            let lock = directory.join("runtime.lock");
            private_write(&lock, &[]);
            let evidence =
                SingletonStorageOwnershipEvidence::capture(&layout.root, &mutation).unwrap();
            assert!(
                evidence
                    .matches_entry(
                        Path::new("runtime/collector/runtime.lock"),
                        &File::open(&layout.config).unwrap()
                    )
                    .is_err()
            );
            match change {
                0 => {
                    fs::rename(&directory, layout.runtime.join("retained-collector")).unwrap();
                    fs::create_dir(&directory).unwrap();
                    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
                    private_write(&directory.join("runtime.lock"), &[]);
                }
                1 => fs::remove_file(&lock).unwrap(),
                _ => fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap(),
            }
            assert!(evidence.revalidate().is_err());
            drop(evidence);
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            drop(mutation);
            fs::remove_dir_all(layout.root).unwrap();
        }
    }

    #[test]
    fn symlinks_are_never_followed() {
        let (layout, mutation) = fixture();
        std::os::unix::fs::symlink(&layout.config, layout.runtime.join("runtime.meta")).unwrap();
        assert!(SingletonStorageOwnershipEvidence::capture(&layout.root, &mutation).is_err());
        fs::remove_file(layout.runtime.join("runtime.meta")).unwrap();
        std::os::unix::fs::symlink(&layout.state, layout.runtime.join("collector")).unwrap();
        assert!(SingletonStorageOwnershipEvidence::capture(&layout.root, &mutation).is_err());
        drop(mutation);
        fs::remove_dir_all(layout.root).unwrap();
    }

    #[test]
    fn fifo_capture_is_rejected_without_blocking() {
        const PROBE: &str = "AGENTOBS_TEST_SINGLETON_OWNERSHIP_FIFO";
        if std::env::var_os(PROBE).is_some() {
            let (layout, mutation) = fixture();
            assert!(
                std::process::Command::new("mkfifo")
                    .args(["-m", "600"])
                    .arg(layout.runtime.join("runtime.lock"))
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(SingletonStorageOwnershipEvidence::capture(&layout.root, &mutation).is_err());
            drop(mutation);
            fs::remove_dir_all(layout.root).unwrap();
            return;
        }
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "lock::storage_ownership::tests::fifo_capture_is_rejected_without_blocking",
            ])
            .env(PROBE, "1")
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("singleton ownership capture blocked on a FIFO");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
