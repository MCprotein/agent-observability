use crate::{CollectorError, InstalledLayout};
use agent_observability_local_runtime::{
    install,
    storage_coherence::{StorageBarrier, StorageCoherenceError, StorageMutationWriter},
};
use std::path::Path;

#[cfg(test)]
thread_local! {
    static FAIL_PUBLISH_SYNC: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
    static REPLACE_PUBLISHED_TARGET: std::cell::RefCell<Option<(std::path::PathBuf, bool)>> = const { std::cell::RefCell::new(None) };
    static RECREATE_PUBLISHED_TEMP: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn replace_published_target(target: &Path, after_sync: bool) {
    if REPLACE_PUBLISHED_TARGET.with(|fault| {
        fault
            .borrow()
            .as_ref()
            .is_some_and(|(path, after)| path == target && *after == after_sync)
    }) {
        std::fs::rename(target, target.with_extension("retained")).unwrap();
        std::fs::write(target, b"foreign replacement").unwrap();
    }
}

pub(super) struct SettingsDirectory {
    path: std::path::PathBuf,
    file: std::fs::File,
}

impl SettingsDirectory {
    pub(super) fn open(path: &Path) -> Result<Self, CollectorError> {
        crate::validate_private_directory(path)?;
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_DIRECTORY);
        }
        let directory = Self {
            path: path.to_path_buf(),
            file: options.open(path)?,
        };
        directory.revalidate()?;
        Ok(directory)
    }

    pub(super) fn revalidate(&self) -> Result<(), CollectorError> {
        crate::validate_private_directory(&self.path)?;
        require_named_identity(&self.path, &self.file)
    }

    pub(super) fn sync(&self) -> Result<(), CollectorError> {
        self.revalidate()?;
        self.file.sync_all()?;
        self.revalidate()
    }

    pub(super) fn publish(
        &self,
        temporary: &Path,
        held: &std::fs::File,
        target: &Path,
    ) -> Result<(), CollectorError> {
        self.revalidate()?;
        require_named_identity(temporary, held)?;
        std::fs::rename(temporary, target)?;
        let verification = self.sync_after_publish(target, held);
        #[cfg(test)]
        if verification.is_ok()
            && RECREATE_PUBLISHED_TEMP.with(|fault| fault.borrow().as_deref() == Some(target))
        {
            std::fs::write(temporary, b"foreign temporary replacement").unwrap();
        }
        verification.map_err(|verification| CollectorError::StorageWriteUnverified {
            operation_completed: true,
            primary: None,
            verification: Box::new(verification),
        })
    }

    fn sync_after_publish(
        &self,
        target: &Path,
        held: &std::fs::File,
    ) -> Result<(), CollectorError> {
        #[cfg(test)]
        if FAIL_PUBLISH_SYNC.with(|fault| fault.borrow().as_deref() == Some(target)) {
            return Err(CollectorError::Runtime(
                "injected publication sync failure".into(),
            ));
        }
        #[cfg(test)]
        replace_published_target(target, false);
        require_named_identity(target, held)?;
        self.sync()?;
        #[cfg(test)]
        replace_published_target(target, true);
        require_named_identity(target, held)
    }
}

pub(super) fn require_named_identity(
    path: &Path,
    file: &std::fs::File,
) -> Result<(), CollectorError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let named = std::fs::symlink_metadata(path)?;
        let held = file.metadata()?;
        if !named.file_type().is_symlink() && (named.dev(), named.ino()) == (held.dev(), held.ino())
        {
            return Ok(());
        }
    }
    #[cfg(not(unix))]
    let _ = (path, file);
    Err(CollectorError::Runtime(
        "collector settings file identity changed".into(),
    ))
}

pub(crate) fn with_settings_writer<T>(
    root: &Path,
    operation: impl FnOnce(&InstalledLayout) -> Result<T, CollectorError>,
) -> Result<T, CollectorError> {
    with_settings_writer_observing(root, operation, || {})
}

pub(super) fn with_settings_writer_waiting_for_root<T>(
    root: &Path,
    operation: impl FnOnce(&InstalledLayout) -> Result<T, CollectorError>,
) -> Result<T, CollectorError> {
    with_settings_writer_waiting_observing(root, operation, || {}, || {})
}

fn with_settings_writer_waiting_observing<T>(
    root: &Path,
    operation: impl FnOnce(&InstalledLayout) -> Result<T, CollectorError>,
    before_postcheck: impl FnOnce(),
    on_contention: impl FnOnce(),
) -> Result<T, CollectorError> {
    with_settings_writer_acquiring(root, operation, before_postcheck, |root, barrier| {
        StorageMutationWriter::acquire_waiting_for_root(root, barrier, on_contention)
    })
}

pub(super) fn published_finalization_error(error: CollectorError) -> CollectorError {
    if matches!(
        error,
        CollectorError::StorageWriteUnverified {
            operation_completed: true,
            ..
        }
    ) {
        return error;
    }
    CollectorError::StorageWriteUnverified {
        operation_completed: true,
        primary: Some(Box::new(error)),
        verification: Box::new(CollectorError::Runtime(
            "collector settings finalization failed".into(),
        )),
    }
}

fn with_settings_writer_observing<T>(
    root: &Path,
    operation: impl FnOnce(&InstalledLayout) -> Result<T, CollectorError>,
    before_postcheck: impl FnOnce(),
) -> Result<T, CollectorError> {
    with_settings_writer_acquiring(root, operation, before_postcheck, |root, barrier| {
        StorageMutationWriter::acquire(root, barrier)
    })
}

fn with_settings_writer_acquiring<T>(
    root: &Path,
    operation: impl FnOnce(&InstalledLayout) -> Result<T, CollectorError>,
    before_postcheck: impl FnOnce(),
    acquire: impl for<'a> FnOnce(
        &Path,
        Option<&'a StorageBarrier>,
    ) -> Result<StorageMutationWriter<'a>, StorageCoherenceError>,
) -> Result<T, CollectorError> {
    let layout = install(root).map_err(crate::runtime_error)?;
    let barrier =
        agent_observability_local_runtime::storage_coherence::StorageBarrier::open_if_initialized(
            &layout.root,
        )
        .map_err(crate::runtime_error)?;
    let scope = acquire(&layout.root, barrier.as_ref()).map_err(crate::runtime_error)?;
    let result = operation(&layout);
    before_postcheck();
    let outcome = match (result, scope.revalidate()) {
        (result, Ok(())) => result,
        (Ok(_), Err(verification)) => Err(CollectorError::StorageWriteUnverified {
            operation_completed: true,
            primary: None,
            verification: Box::new(crate::runtime_error(verification)),
        }),
        (Err(primary), Err(verification)) => {
            let operation_completed = matches!(
                &primary,
                CollectorError::StorageWriteUnverified {
                    operation_completed: true,
                    ..
                }
            );
            Err(CollectorError::StorageWriteUnverified {
                operation_completed,
                primary: Some(Box::new(primary)),
                verification: Box::new(crate::runtime_error(verification)),
            })
        }
    };
    drop(scope);
    drop(barrier);
    outcome
}

#[cfg(all(test, unix))]
mod tests {
    use super::with_settings_writer_observing;
    use agent_observability_local_runtime::{MutationGuard, install};
    use std::{
        collections::BTreeMap,
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
    };

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agent-observability-settings-coordination-{name}-{}",
            std::process::id()
        ))
    }

    fn initialized_barrier(
        root: &Path,
    ) -> agent_observability_local_runtime::storage_coherence::StorageBarrier {
        let layout = install(root).unwrap();
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        agent_observability_local_runtime::storage_coherence::StorageBarrier::initialize(
            &layout.root,
            &mutation,
        )
        .unwrap()
    }

    fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn visit(root: &Path, path: &Path, snapshot: &mut BTreeMap<PathBuf, Vec<u8>>) {
            let mut entries = fs::read_dir(path)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>();
            entries.sort();
            for entry in entries {
                let relative = entry.strip_prefix(root).unwrap().to_path_buf();
                let metadata = fs::symlink_metadata(&entry).unwrap();
                if metadata.is_dir() {
                    snapshot.insert(relative, b"directory".to_vec());
                    visit(root, &entry, snapshot);
                } else {
                    snapshot.insert(relative, fs::read(&entry).unwrap());
                }
            }
        }

        let mut snapshot = BTreeMap::new();
        visit(root, root, &mut snapshot);
        snapshot
    }

    fn write_legacy_settings(root: &Path) {
        let layout = install(root).unwrap();
        let path = layout.runtime.join("collector.json");
        fs::write(
            &path,
            br#"{"schema_version":"local_collector.v1","port":4318,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","source_generation":"codex-otel-v1"}"#,
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn settings_waits_for_actual_root_contention_then_executes_once() {
        use std::{sync::mpsc, thread, time::Duration};

        for coordinated in [false, true] {
            let root = root(&format!("settings-root-wait-{coordinated}"));
            let settings = crate::install_settings(&root).unwrap();
            let barrier = coordinated.then(|| initialized_barrier(&root));
            let before = snapshot_tree(&root);
            let mutation = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
            let (contended_tx, contended_rx) = mpsc::channel::<()>();
            let (executed_tx, executed_rx) = mpsc::channel();
            let (result_tx, result_rx) = mpsc::channel();
            let worker_root = root.clone();
            let worker = thread::spawn(move || {
                let result = super::with_settings_writer_waiting_observing(
                    &worker_root,
                    |layout| {
                        executed_tx.send(()).unwrap();
                        crate::install_settings_locked(layout)
                    },
                    || {},
                    || contended_tx.send(()).unwrap(),
                );
                result_tx.send(result).unwrap();
            });
            let contention = contended_rx.recv_timeout(Duration::from_secs(5));
            let before_release = executed_rx.try_recv();
            let default_result = crate::install_settings(&root);
            drop(mutation);
            let result = result_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            worker.join().unwrap();
            assert!(contention.is_ok(), "root contention must be observed");
            assert!(matches!(before_release, Err(mpsc::TryRecvError::Empty)));
            assert!(
                matches!(default_result, Err(crate::CollectorError::Runtime(message))
                if message == "storage accounting barrier is busy")
            );
            assert_eq!(result.unwrap(), settings);
            assert_eq!(executed_rx.try_iter().count(), 1);
            assert_eq!(
                crate::install_settings_waiting_for_root(&root).unwrap(),
                settings
            );
            assert!(snapshot_tree(&root) == before, "settings/TLS bytes changed");
            assert_eq!(
                root.join("runtime/storage-accounting.lock").exists(),
                coordinated
            );
            drop(barrier);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn waiting_settings_writer_rejects_accounting_contention_without_running_operation() {
        use std::{sync::mpsc, thread, time::Duration};

        let root = root("waiting-accounting-busy");
        crate::install_settings(&root).unwrap();
        let barrier = initialized_barrier(&root);
        let before = snapshot_tree(&root);
        let accounting = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join("runtime/storage-accounting.lock"))
            .unwrap();
        accounting.try_lock().unwrap();
        let (result_tx, result_rx) = mpsc::channel();
        let worker_root = root.clone();
        let worker = thread::spawn(move || {
            let result = super::with_settings_writer_waiting_observing(
                &worker_root,
                |_| -> Result<(), crate::CollectorError> { panic!("operation must not run") },
                || panic!("postcheck must not run before acquisition"),
                || panic!("accounting contention must not notify root contention"),
            );
            result_tx.send(result).unwrap();
        });
        let result = result_rx.recv_timeout(Duration::from_secs(5));
        // Completion must occur while accounting remains held, without retaining root.
        let mutation = MutationGuard::try_acquire(&root.join("runtime"));
        drop(accounting);
        worker.join().unwrap();
        assert!(
            matches!(result.unwrap(), Err(crate::CollectorError::Runtime(message))
            if message == "storage accounting barrier is busy")
        );
        drop(mutation.unwrap());
        assert!(snapshot_tree(&root) == before, "settings/TLS bytes changed");
        drop(barrier);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn waiting_settings_writer_preserves_postcheck_and_primary_errors() {
        for coordinated in [false, true] {
            for completed in [false, true] {
                let root = root(&format!("waiting-postcheck-{coordinated}-{completed}"));
                let layout = install(&root).unwrap();
                let barrier = coordinated.then(|| initialized_barrier(&root));
                let lock = layout.runtime.join("mutation.lock");
                let retained = layout.runtime.join("retained-mutation.lock");
                let error = super::with_settings_writer_waiting_observing(
                    &root,
                    |_| {
                        if completed {
                            Ok(())
                        } else {
                            Err(crate::CollectorError::Runtime(
                                "primary operation error".into(),
                            ))
                        }
                    },
                    || {
                        fs::rename(&lock, &retained).unwrap();
                        fs::write(&lock, []).unwrap();
                        fs::set_permissions(&lock, fs::Permissions::from_mode(0o600)).unwrap();
                    },
                    || panic!("uncontended root must not notify"),
                )
                .unwrap_err();
                assert!(
                    matches!(&error, crate::CollectorError::StorageWriteUnverified {
                    operation_completed, primary, ..
                } if *operation_completed == completed && primary.is_none() == completed)
                );
                if !completed {
                    assert!(
                        matches!(&error, crate::CollectorError::StorageWriteUnverified {
                        primary: Some(primary), ..
                    } if matches!(primary.as_ref(), crate::CollectorError::Runtime(message)
                        if message == "primary operation error"))
                    );
                }
                assert!(retained.exists());
                assert!(fs::read(&lock).unwrap().is_empty());
                assert_eq!(
                    layout.runtime.join("storage-accounting.lock").exists(),
                    coordinated
                );
                drop(barrier);
                fs::remove_dir_all(root).unwrap();
            }
        }
    }

    #[test]
    fn shared_permit_excludes_freeze_with_the_already_held_mutation() {
        use agent_observability_local_runtime::storage_coherence::{
            StorageCoherenceError, StorageMutationWriter,
        };
        let root = root("shared-permit");
        let _ = fs::remove_dir_all(&root);
        let barrier = initialized_barrier(&root);
        let root = install(&root).unwrap().root;
        let scope = StorageMutationWriter::acquire(&root, Some(&barrier)).unwrap();
        assert!(matches!(
            barrier.try_freeze(scope.mutation()),
            Err(StorageCoherenceError::Busy)
        ));
        scope.revalidate().unwrap();
        drop(scope);
        let mutation = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        drop(barrier.try_freeze(&mutation).unwrap());
        drop(mutation);
        drop(barrier);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retained_directory_rejects_fifo_replacement_without_blocking() {
        const CHILD: &str = "AGENTOBS_SETTINGS_FIFO_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "settings_coordination::tests::retained_directory_rejects_fifo_replacement_without_blocking"])
                .env(CHILD, "1").spawn().unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success());
                    return;
                }
                if std::time::Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("settings directory operation blocked on FIFO");
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        let root = root("directory-fifo");
        let layout = install(&root).unwrap();
        let directory = super::SettingsDirectory::open(&layout.runtime).unwrap();
        let path = layout.runtime.join("temporary");
        let file = crate::create_settings_temporary(&path).unwrap();
        fs::rename(&layout.runtime, layout.root.join("retained-runtime")).unwrap();
        assert!(
            std::process::Command::new("mkfifo")
                .args(["-m", "600"])
                .arg(&layout.runtime)
                .status()
                .unwrap()
                .success()
        );
        assert!(directory.sync().is_err());
        assert!(super::SettingsDirectory::open(&layout.runtime).is_err());
        assert!(
            directory
                .publish(&path, &file, &layout.runtime.join("target"))
                .is_err()
        );
        assert!(crate::finish_settings_temporary(&directory, &path, &file, Ok(())).is_err());
        assert!(layout.root.join("retained-runtime/temporary").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn publication_rejects_replaced_temporary_and_preserves_target() {
        let root = root("publication-temp");
        let layout = install(&root).unwrap();
        let directory = super::SettingsDirectory::open(&layout.runtime).unwrap();
        let path = layout.runtime.join("temporary");
        let target = layout.runtime.join("target");
        fs::write(&target, b"original").unwrap();
        let file = crate::create_settings_temporary(&path).unwrap();
        fs::rename(&path, layout.runtime.join("retained-temp")).unwrap();
        fs::write(&path, b"foreign").unwrap();
        assert!(directory.publish(&path, &file, &target).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(fs::read(&path).unwrap(), b"foreign");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_writer_revalidates_mutation_without_creating_accounting_barrier() {
        let root = root("legacy-mutation");
        let layout = install(&root).unwrap();
        let path = layout.runtime.join("mutation.lock");
        let result = with_settings_writer_observing(
            &root,
            |_| Ok(()),
            || {
                fs::rename(&path, layout.runtime.join("retained-mutation.lock")).unwrap();
                fs::write(&path, []).unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            },
        );
        assert!(result.is_err());
        assert!(!layout.runtime.join("storage-accounting.lock").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn temporary_cleanup_preserves_foreign_file_and_primary_error() {
        let root = root("foreign-temp");
        let layout = install(&root).unwrap();
        let path = layout.runtime.join("test-owned-temp");
        let file = crate::create_settings_temporary(&path).unwrap();
        fs::rename(&path, layout.runtime.join("retained-temp")).unwrap();
        fs::write(&path, b"foreign").unwrap();
        let result = crate::finish_settings_temporary(
            &super::SettingsDirectory::open(&layout.runtime).unwrap(),
            &path,
            &file,
            Err(crate::CollectorError::Runtime("primary".into())),
        );
        assert!(
            matches!(result, Err(crate::CollectorError::Runtime(ref text))
            if text == "primary; collector temporary cleanup failed")
        );
        assert_eq!(fs::read(path).unwrap(), b"foreign");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn temporary_cleanup_removes_only_owned_file_and_accepts_missing() {
        let root = root("owned-temp");
        let layout = install(&root).unwrap();
        let path = layout.runtime.join("test-owned-temp");
        let file = crate::create_settings_temporary(&path).unwrap();
        let directory = super::SettingsDirectory::open(&layout.runtime).unwrap();
        crate::finish_settings_temporary(&directory, &path, &file, Ok(())).unwrap();
        assert!(!path.exists());
        crate::finish_settings_temporary(&directory, &path, &file, Ok(())).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn initialized_missing_mutation_lock_is_not_recreated() {
        let root = root("missing-mutation");
        let _ = fs::remove_dir_all(&root);
        let barrier = initialized_barrier(&root);
        let path = root.join("runtime/mutation.lock");
        fs::remove_file(&path).unwrap();
        let result = with_settings_writer_observing(&root, |_| Ok(()), || {});
        assert!(result.is_err());
        assert!(!path.exists());
        drop(barrier);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mutation_replacement_is_rejected_by_postcheck() {
        let root = root("mutation-replacement");
        let _ = fs::remove_dir_all(&root);
        let barrier = initialized_barrier(&root);
        let path = root.join("runtime/mutation.lock");
        let result = with_settings_writer_observing(
            &root,
            |_| Ok(()),
            || {
                fs::rename(&path, root.join("runtime/retained-mutation.lock")).unwrap();
                fs::write(&path, []).unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            },
        );
        assert!(result.is_err());
        drop(barrier);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn busy_settings_install_returns_error_without_writing_settings_or_tls() {
        let root = root("install-busy");
        let _ = fs::remove_dir_all(&root);
        let barrier = initialized_barrier(&root);
        let layout = install(&root).unwrap();
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let freeze = barrier.try_freeze(&mutation).unwrap();
        let before = snapshot_tree(&root);

        assert!(crate::install_settings(&root).is_err());
        assert_eq!(snapshot_tree(&root), before);

        drop(freeze);
        drop(mutation);
        drop(barrier);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn busy_migration_commit_returns_error_without_changing_journal_or_tls() {
        let root = root("commit-busy");
        let _ = fs::remove_dir_all(&root);
        write_legacy_settings(&root);
        crate::install_settings(&root).unwrap();
        let barrier = initialized_barrier(&root);
        let layout = install(&root).unwrap();
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let freeze = barrier.try_freeze(&mutation).unwrap();
        let before = snapshot_tree(&root);

        assert!(crate::commit_settings_migration(&root).is_err());
        assert_eq!(snapshot_tree(&root), before);

        drop(freeze);
        drop(mutation);
        drop(barrier);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn busy_migration_rollback_returns_error_without_changing_settings_or_tls() {
        let root = root("rollback-busy");
        let _ = fs::remove_dir_all(&root);
        write_legacy_settings(&root);
        crate::install_settings(&root).unwrap();
        let barrier = initialized_barrier(&root);
        let layout = install(&root).unwrap();
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let freeze = barrier.try_freeze(&mutation).unwrap();
        let before = snapshot_tree(&root);

        assert!(crate::rollback_settings_migration(&root).is_err());
        assert_eq!(snapshot_tree(&root), before);

        drop(freeze);
        drop(mutation);
        drop(barrier);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn busy_port_recovery_returns_error_without_changing_settings() {
        let root = root("port-busy");
        let _ = fs::remove_dir_all(&root);
        let settings = crate::install_settings(&root).unwrap();
        let occupied = crate::tests::occupy_loopback_port(settings.port);
        let barrier = initialized_barrier(&root);
        let layout = install(&root).unwrap();
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let freeze = barrier.try_freeze(&mutation).unwrap();
        let before = snapshot_tree(&root);

        assert!(crate::recover_occupied_persisted_port(&root, &settings).is_err());
        assert_eq!(snapshot_tree(&root), before);

        drop(freeze);
        drop(mutation);
        drop(barrier);
        drop(occupied);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exact_postcheck_rejects_replaced_accounting_lock_after_operation() {
        let root = root("postcheck-replaced");
        let _ = fs::remove_dir_all(&root);
        let barrier = initialized_barrier(&root);
        drop(barrier);
        let marker = root.join("runtime/settings-operation-complete");
        let lock = root.join("runtime/storage-accounting.lock");
        let retained = root.join("runtime/retained-storage-accounting.lock");

        let result = with_settings_writer_observing(
            &root,
            |_| {
                fs::write(&marker, b"complete")?;
                Ok(())
            },
            || {
                fs::rename(&lock, &retained).unwrap();
                fs::write(&lock, []).unwrap();
                fs::set_permissions(&lock, fs::Permissions::from_mode(0o600)).unwrap();
            },
        );

        assert!(result.is_err());
        assert_eq!(fs::read(marker).unwrap(), b"complete");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn operation_error_remains_primary_when_exact_postcheck_also_fails() {
        let root = root("primary-error");
        let _ = fs::remove_dir_all(&root);
        let barrier = initialized_barrier(&root);
        drop(barrier);
        let lock = root.join("runtime/storage-accounting.lock");
        let retained = root.join("runtime/retained-storage-accounting.lock");

        let result = with_settings_writer_observing(
            &root,
            |_| {
                Err::<(), _>(crate::CollectorError::Runtime(
                    "primary operation error".into(),
                ))
            },
            || {
                fs::rename(&lock, &retained).unwrap();
                fs::write(&lock, []).unwrap();
                fs::set_permissions(&lock, fs::Permissions::from_mode(0o600)).unwrap();
            },
        );

        let error = result.unwrap_err();
        assert!(
            matches!(&error, crate::CollectorError::StorageWriteUnverified {
            operation_completed: false, primary: Some(primary), ..
        } if matches!(primary.as_ref(), crate::CollectorError::Runtime(message) if message == "primary operation error"))
        );
        assert_eq!(
            std::error::Error::source(&error).unwrap().to_string(),
            "primary operation error"
        );
        assert!(!error.to_string().contains("primary operation error"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn omitted_initialized_barrier_never_recreates_missing_mutation() {
        use agent_observability_local_runtime::storage_coherence::StorageMutationWriter;
        let root = root("omitted-barrier");
        let layout = install(&root).unwrap();
        let barrier = initialized_barrier(&root);
        assert!(StorageMutationWriter::acquire(&layout.root, None).is_err());
        fs::remove_file(layout.runtime.join("mutation.lock")).unwrap();
        assert!(StorageMutationWriter::acquire(&layout.root, None).is_err());
        assert!(StorageMutationWriter::acquire_exclusive(&layout.root, None).is_err());
        assert!(!layout.runtime.join("mutation.lock").exists());
        drop(barrier);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exclusive_writer_excludes_shared_permit_and_revalidates_legacy_root() {
        use agent_observability_local_runtime::storage_coherence::{
            StorageCoherenceError, StorageMutationWriter,
        };
        let root = root("exclusive-writer");
        let layout = install(&root).unwrap();
        let barrier = initialized_barrier(&root);
        let scope = StorageMutationWriter::acquire_exclusive(&layout.root, Some(&barrier)).unwrap();
        assert!(matches!(
            barrier.try_begin_write(),
            Err(StorageCoherenceError::Busy)
        ));
        scope.revalidate().unwrap();
        drop(scope);
        drop(barrier);
        fs::remove_dir_all(&root).unwrap();
        let layout = install(&root).unwrap();
        let scope = StorageMutationWriter::acquire_exclusive(&layout.root, None).unwrap();
        fs::remove_file(layout.runtime.join("mutation.lock")).unwrap();
        assert!(scope.revalidate().is_err());
        assert!(!layout.runtime.join("storage-accounting.lock").exists());
        drop(scope);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loss_postcheck_preserves_exact_operation_outcome() {
        for lock_name in ["mutation.lock", "storage-accounting.lock"] {
            for completed in [false, true] {
                let root = root(&format!("loss-{lock_name}-{completed}"));
                let barrier = initialized_barrier(&root);
                drop(barrier);
                let error = with_settings_writer_observing(
                    &root,
                    |_| {
                        if completed {
                            Ok(())
                        } else {
                            Err(crate::CollectorError::Runtime("private-primary".into()))
                        }
                    },
                    || fs::remove_file(root.join("runtime").join(lock_name)).unwrap(),
                )
                .unwrap_err();
                assert!(
                    matches!(&error, crate::CollectorError::StorageWriteUnverified {
                    operation_completed, primary, verification
                } if *operation_completed == completed && primary.is_none() == completed
                    && matches!(verification.as_ref(), crate::CollectorError::Runtime(message)
                        if message == "storage accounting barrier is missing"))
                );
                assert!(!error.to_string().contains("private-primary"));
                assert!(!error.to_string().contains(root.to_str().unwrap()));
                fs::remove_dir_all(root).unwrap();
            }
        }
    }

    #[test]
    fn successful_postcheck_preserves_results_and_failed_postcheck_preserves_publication() {
        let root = root("outcome-preservation");
        let barrier = initialized_barrier(&root);
        assert_eq!(
            with_settings_writer_observing(&root, |_| Ok(42), || {}).unwrap(),
            42
        );
        assert!(
            matches!(with_settings_writer_observing(&root, |_| Err::<(), _>(crate::CollectorError::Runtime("unchanged".into())), || {}),
            Err(crate::CollectorError::Runtime(message)) if message == "unchanged")
        );
        let error = with_settings_writer_observing(
            &root,
            |_| {
                Err::<(), _>(crate::CollectorError::StorageWriteUnverified {
                    operation_completed: true,
                    primary: Some(Box::new(crate::CollectorError::Runtime(
                        "original-private-error".into(),
                    ))),
                    verification: Box::new(crate::CollectorError::Runtime(
                        "original-private-verification".into(),
                    )),
                })
            },
            || fs::remove_file(root.join("runtime/mutation.lock")).unwrap(),
        )
        .unwrap_err();
        assert!(
            matches!(&error, crate::CollectorError::StorageWriteUnverified { operation_completed: true, primary: Some(primary), .. }
            if matches!(primary.as_ref(), crate::CollectorError::StorageWriteUnverified { operation_completed: true, .. }))
        );
        assert!(!error.to_string().contains("original-private"));
        drop(barrier);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn publication_rejects_target_replacement_before_and_after_directory_sync() {
        for after_sync in [false, true] {
            let root = root(&format!("published-identity-{after_sync}"));
            let layout = install(&root).unwrap();
            let directory = super::SettingsDirectory::open(&layout.runtime).unwrap();
            let target = layout.runtime.join("published-target");
            let temporary = layout.runtime.join("owned-temporary");
            let mut held = crate::create_settings_temporary(&temporary).unwrap();
            std::io::Write::write_all(&mut held, b"owned publication").unwrap();
            held.sync_all().unwrap();
            super::REPLACE_PUBLISHED_TARGET
                .with(|fault| *fault.borrow_mut() = Some((target.clone(), after_sync)));
            let result = directory.publish(&temporary, &held, &target);
            super::REPLACE_PUBLISHED_TARGET.with(|fault| *fault.borrow_mut() = None);
            assert!(
                matches!(&result, Err(crate::CollectorError::StorageWriteUnverified {
                operation_completed: true, primary: None, verification
            }) if matches!(verification.as_ref(), crate::CollectorError::Runtime(message)
                if message == "collector settings file identity changed"))
            );
            let result = crate::finish_settings_temporary(&directory, &temporary, &held, result);
            assert!(matches!(
                result,
                Err(crate::CollectorError::StorageWriteUnverified {
                    operation_completed: true,
                    ..
                })
            ));
            assert_eq!(fs::read(&target).unwrap(), b"foreign replacement");
            assert_eq!(
                fs::read(target.with_extension("retained")).unwrap(),
                b"owned publication"
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn published_settings_cleanup_failure_preserves_credentials_and_migration() {
        for migration in [false, true] {
            let root = root(&format!("published-cleanup-{migration}"));
            assert!(!root.exists());
            let layout = install(&root).unwrap();
            let path = crate::settings_path(&layout);
            let previous = if migration {
                fs::write(&path, b"legacy settings").unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
                Some(crate::read_private_snapshot(&path, crate::MAX_SETTINGS_BYTES).unwrap())
            } else {
                None
            };
            super::RECREATE_PUBLISHED_TEMP.with(|fault| *fault.borrow_mut() = Some(path.clone()));
            let result = match previous.as_ref() {
                Some(previous) => crate::begin_settings_migration(&layout, previous, None),
                None => crate::replace_settings(&layout, None),
            };
            super::RECREATE_PUBLISHED_TEMP.with(|fault| *fault.borrow_mut() = None);
            assert!(matches!(
                result,
                Err(crate::CollectorError::StorageWriteUnverified {
                    operation_completed: true,
                    ..
                })
            ));
            let settings = crate::load_settings_from_layout(&layout).unwrap();
            for credential in [
                &settings.credentials.ca_certificate,
                &settings.credentials.server_certificate,
                &settings.credentials.server_private_key,
            ] {
                assert!(
                    !fs::read(layout.runtime.join(credential))
                        .unwrap()
                        .is_empty()
                );
            }
            assert_eq!(crate::settings_migration_path(&layout).exists(), migration);
            let foreign = fs::read_dir(path.parent().unwrap())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".collector.json.tmp.")
                })
                .map(|entry| fs::read(entry.path()).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(foreign, vec![b"foreign temporary replacement".to_vec()]);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn published_settings_sync_failure_preserves_credentials_and_migration() {
        for migration in [false, true] {
            let root = root(&format!("published-sync-{migration}"));
            let layout = install(&root).unwrap();
            let path = crate::settings_path(&layout);
            let previous = if migration {
                fs::write(&path, b"legacy settings").unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
                Some(crate::read_private_snapshot(&path, crate::MAX_SETTINGS_BYTES).unwrap())
            } else {
                None
            };
            super::FAIL_PUBLISH_SYNC.with(|fault| *fault.borrow_mut() = Some(path.clone()));
            let result = match previous.as_ref() {
                Some(previous) => crate::begin_settings_migration(&layout, previous, None),
                None => crate::replace_settings(&layout, None),
            };
            super::FAIL_PUBLISH_SYNC.with(|fault| *fault.borrow_mut() = None);
            assert!(matches!(
                result,
                Err(crate::CollectorError::StorageWriteUnverified {
                    operation_completed: true,
                    ..
                })
            ));
            let settings = crate::load_settings_from_layout(&layout).unwrap();
            assert!(!settings.generation.is_empty());
            assert_eq!(crate::settings_migration_path(&layout).exists(), migration);
            fs::remove_dir_all(root).unwrap();
        }
    }
}
