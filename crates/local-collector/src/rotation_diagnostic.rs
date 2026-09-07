// Compiled only inside the collector's test module; never a shipped mutation command.
mod rotation_diagnostic {
    use super::*;
    use std::hash::{DefaultHasher, Hasher};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct PrivateRuntime(PathBuf);

    impl PrivateRuntime {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "agent-observability-rotation-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&root).expect("fresh private runtime");
            Self(root)
        }
    }

    impl Drop for PrivateRuntime {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // Streaming canonical serialization identity: retain no whole-record DTO or raw output.
    fn canonical_identity(store: &LocalStore) -> (u64, u64, u64) {
        let mut hash = DefaultHasher::new();
        let mut bytes = 0_u64;
        let mut count = 0_u64;
        store
            .visit_report_snapshot_bounded(|_, record| {
                let encoded = serde_json::to_vec(&record).expect("canonical record serialization");
                hash.write_usize(encoded.len());
                hash.write(&encoded);
                bytes += u64::try_from(encoded.len()).unwrap();
                count += 1;
            })
            .unwrap_or_else(|_| panic!("bounded canonical record read failed"));
        (count, bytes, hash.finish())
    }

    fn catalog_files(root: &Path) -> Vec<PathBuf> {
        let directory = root.join("state/store/report-views.v1");
        if !directory.exists() {
            return Vec::new();
        }
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "sqlite3")
            })
            .collect()
    }

    #[derive(Clone, Copy, Default)]
    struct Sample {
        allocated: u64,
        database: u64,
        journal: u64,
    }

    fn sample(root: &Path) -> Sample {
        let mut result = Sample {
            allocated: StorageBudget::allocated_tree_bytes(root).unwrap_or(0),
            ..Sample::default()
        };
        if let Ok(entries) = fs::read_dir(root.join("state/store/report-views.v1")) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let bytes = entry.metadata().map_or(0, |metadata| metadata.len());
                if name.ends_with("-journal") {
                    result.journal += bytes;
                } else if name.ends_with(".sqlite3")
                    || name.starts_with(".report-view.sqlite3.staging.")
                {
                    result.database += bytes;
                }
            }
        }
        result
    }

    fn measured_refresh(root: &Path) -> Sample {
        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let monitor = scope.spawn(|| {
                let mut peak = Sample::default();
                loop {
                    let observed = sample(root);
                    peak.allocated = peak.allocated.max(observed.allocated);
                    peak.database = peak.database.max(observed.database);
                    peak.journal = peak.journal.max(observed.journal);
                    if done.load(Ordering::Acquire) {
                        return peak;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            });
            let result = refresh_dashboard_snapshot(root);
            done.store(true, Ordering::Release);
            let peak = monitor.join().expect("sampler completed");
            assert!(result.unwrap_or_else(|_| panic!("bounded snapshot publication failed")));
            peak
        })
    }

    fn three_generations(root: &Path) {
        let store = LocalStore::open_current(root.join("state/store"))
            .unwrap_or_else(|_| panic!("current authority open failed"));
        let before = canonical_identity(&store);
        let budget = StorageBudget::calculate(1024 * 1024 * 1024, false).unwrap();
        let mut previous_generation = None;
        let mut first_files = Vec::new();
        for rotation in 0..3 {
            if rotation > 0 {
                store
                    .invalidate_report()
                    .unwrap_or_else(|_| panic!("temporary invalidation failed"));
            }
            if rotation == 2 {
                assert_eq!(
                    catalog_files(root).len(),
                    2,
                    "current plus retired before third build"
                );
            }
            let peak = measured_refresh(root);
            assert!(peak.allocated <= budget.writable_limit());
            let snapshot = current_report_view(&store)
                .unwrap_or_else(|_| panic!("current publication read failed"))
                .expect("current publication exists");
            assert!(
                previous_generation.is_none_or(|generation| snapshot.generation() > generation)
            );
            previous_generation = Some(snapshot.generation());
            assert_eq!(u64::try_from(snapshot.records()).unwrap(), before.0);
            assert_eq!(
                store
                    .record_count()
                    .unwrap_or_else(|_| panic!("authority count failed")),
                before.0
            );
            assert_eq!(canonical_identity(&store), before);
            let files = catalog_files(root);
            assert_eq!(files.len(), if rotation == 0 { 1 } else { 2 });
            if rotation == 0 {
                first_files = files.clone();
            }
            if rotation == 2 {
                assert!(
                    first_files.iter().all(|path| !path.exists()),
                    "old retired cleaned"
                );
            }
            let allocated = StorageBudget::allocated_tree_bytes(root).unwrap();
            assert!(allocated <= budget.writable_limit());
            let logical_bytes: u64 = files
                .iter()
                .map(|path| fs::metadata(path).unwrap().len())
                .sum();
            println!(
                "rotation={} records={} canonical_bytes={} logical_db_bytes={} allocated_tree_bytes={} sampled_peak_tree_bytes={} sampled_peak_db_bytes={} sampled_peak_journal_bytes={} status=ok",
                rotation + 1,
                before.0,
                before.1,
                logical_bytes,
                allocated,
                peak.allocated,
                peak.database,
                peak.journal
            );
        }
    }

    #[test]
    fn synthetic_three_generation_rotation_preserves_authority_and_retires_old_view() {
        let runtime = PrivateRuntime::new();
        let mut state = collector_state(&runtime.0);
        ingest_notify_locked(
            &mut state,
            &projected_notify("rotation-thread", "rotation-turn"),
        )
        .unwrap();
        drop(state);
        three_generations(&runtime.0);
    }

    fn fill_to_remaining(root: &Path, budget: StorageBudget, remaining: u64) {
        let file_path = root.join("admission-fixture.bin");
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(file_path)
            .unwrap();
        let chunk = vec![0x5a; 1024 * 1024];
        loop {
            let allocated = StorageBudget::allocated_tree_bytes(root).unwrap();
            let needed = budget
                .writable_limit()
                .saturating_sub(remaining)
                .saturating_sub(allocated);
            if needed == 0 {
                break;
            }
            file.write_all(&chunk[..usize::try_from(needed.min(1024 * 1024)).unwrap()])
                .unwrap();
            file.sync_all().unwrap();
        }
    }

    #[test]
    fn low_headroom_refuses_rotation_without_losing_current_or_authority() {
        let runtime = PrivateRuntime::new();
        let mut state = collector_state(&runtime.0);
        ingest_notify_locked(
            &mut state,
            &projected_notify("capacity-thread", "capacity-turn"),
        )
        .unwrap();
        assert!(refresh_dashboard_snapshot(&runtime.0).unwrap());
        let current = current_report_view(&state.store).unwrap().unwrap();
        let before = canonical_identity(&state.store);
        let mut config = load(&state.layout.config).unwrap();
        config.collection.local_storage_budget_bytes = 256 * 1024 * 1024;
        let budget =
            StorageBudget::calculate(config.collection.local_storage_budget_bytes, false).unwrap();
        fill_to_remaining(&runtime.0, budget, 4 * 1024 * 1024);
        state
            .store
            .invalidate_report()
            .unwrap_or_else(|_| panic!("temporary invalidation failed"));
        assert!(matches!(
            crate::refresh_report_observing(&state.layout, &state.store, &config, |_| {}),
            Err(ReportFailure::Capacity)
        ));
        assert_eq!(current_report_view(&state.store).unwrap(), Some(current));
        assert_eq!(canonical_identity(&state.store), before);
        assert_eq!(catalog_files(&runtime.0).len(), 1);
    }

    #[test]
    fn concurrent_growth_is_rechecked_before_catalog_publication() {
        let runtime = PrivateRuntime::new();
        let mut state = collector_state(&runtime.0);
        ingest_notify_locked(
            &mut state,
            &projected_notify("growth-thread", "growth-turn"),
        )
        .unwrap();
        assert!(refresh_dashboard_snapshot(&runtime.0).unwrap());
        let current = current_report_view(&state.store).unwrap().unwrap();
        let before = canonical_identity(&state.store);
        let mut config = load(&state.layout.config).unwrap();
        config.collection.local_storage_budget_bytes = 256 * 1024 * 1024;
        let budget =
            StorageBudget::calculate(config.collection.local_storage_budget_bytes, false).unwrap();
        state
            .store
            .invalidate_report()
            .unwrap_or_else(|_| panic!("temporary invalidation failed"));
        let mut injected = false;
        let result = crate::refresh_report_observing(&state.layout, &state.store, &config, |_| {
            if !injected {
                injected = true;
                fill_to_remaining(&runtime.0, budget, 16 * 1024);
            }
        });
        assert!(injected);
        assert!(matches!(result, Err(ReportFailure::Capacity)));
        assert_eq!(current_report_view(&state.store).unwrap(), Some(current));
        assert_eq!(canonical_identity(&state.store), before);
        assert_eq!(catalog_files(&runtime.0).len(), 1);
        assert!(
            !fs::read_dir(runtime.0.join("state/store/report-views.v1"))
                .unwrap()
                .any(|entry| {
                    entry
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".report-view.sqlite3.staging.")
                })
        );
    }

    /// Opt-in only: source is read through SQLite's read-only backup API, never opened by LocalStore.
    /// Run with AO_ROTATION_BACKUP_SOURCE=/path/to/coherent.sqlite3 and --ignored --nocapture.
    #[test]
    #[ignore = "explicit private-backup diagnostic; leader-run only"]
    fn private_backup_three_generation_rotation() {
        let source =
            std::env::var_os("AO_ROTATION_BACKUP_SOURCE").expect("explicit backup source required");
        let source = fs::canonicalize(source).expect("backup source exists");
        assert!(source.is_file());
        let runtime = PrivateRuntime::new();
        let layout = install(&runtime.0).unwrap();
        let store_dir = layout.state.join("store");
        drop(LocalStore::open(&store_dir).unwrap());
        let target = store_dir.join("local-store.sqlite3");
        // stdin command contains only the freshly owned destination; source is a separate argv.
        let destination = target
            .to_str()
            .expect("temporary destination is UTF-8")
            .replace('\'', "''");
        let mut child = Command::new("sqlite3")
            .arg("-readonly")
            .arg(&source)
            .arg(format!(".backup '{destination}'"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("SQLite backup tool available");
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            if let Some(status) = child.try_wait().expect("backup status available") {
                assert!(status.success(), "read-only coherent backup succeeded");
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("backup deadline exceeded");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(
                fs::metadata(&runtime.0).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        three_generations(&runtime.0);
    }
}
