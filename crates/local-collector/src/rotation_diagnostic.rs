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

        fn cleanup(self) {
            fs::remove_dir_all(&self.0).expect("owned private diagnostic cleanup succeeded");
            assert!(!self.0.exists(), "owned diagnostic directory removed");
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

    fn sample(root: &Path) -> Result<Sample, ()> {
        let mut result = Sample {
            allocated: StorageBudget::allocated_tree_bytes(root).map_err(|_| ())?,
            ..Sample::default()
        };
        let entries = match fs::read_dir(root.join("state/store/report-views.v1")) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(result),
            Err(_) => return Err(()),
        };
        for entry in entries {
            let entry = entry.map_err(|_| ())?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let bytes = match entry.metadata() {
                Ok(metadata) => metadata.len(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => return Err(()),
            };
            if name.ends_with("-journal") {
                result.journal += bytes;
            } else if name.ends_with(".sqlite3")
                || name.starts_with(".report-view.sqlite3.staging.")
            {
                result.database += bytes;
            }
        }
        Ok(result)
    }

    struct StopSampler<'a>(&'a AtomicBool);

    impl Drop for StopSampler<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }

    fn measured_refresh(root: &Path) -> Sample {
        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let monitor = scope.spawn(|| {
                let mut peak = Sample::default();
                loop {
                    let observed = sample(root)?;
                    peak.allocated = peak.allocated.max(observed.allocated);
                    peak.database = peak.database.max(observed.database);
                    peak.journal = peak.journal.max(observed.journal);
                    if done.load(Ordering::Acquire) {
                        return Ok::<_, ()>(peak);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            });
            let stop = StopSampler(&done);
            let result = refresh_dashboard_snapshot(root);
            drop(stop);
            let peak = monitor
                .join()
                .expect("sampler completed")
                .expect("allocated accounting succeeded");
            assert!(result.unwrap_or_else(|_| panic!("bounded snapshot publication failed")));
            peak
        })
    }

    fn complete_summary(
        store: &LocalStore,
        snapshot: &str,
    ) -> agent_observability_contracts::dashboard::DashboardKpisV1 {
        use agent_observability_contracts::dashboard::{
            DASHBOARD_QUERY_VERSION, DashboardQueryRequestV1, DashboardQueryResponseV1,
            DashboardSummaryKindV1, DashboardSummaryRequestV1, DashboardWorkV1,
        };
        let mut service = agent_observability_local_store::DashboardQueryService::new([7; 32]);
        let deadline = Instant::now() + Duration::from_mins(5);
        let mut cursor = None;
        loop {
            assert!(Instant::now() < deadline, "summary diagnostic deadline");
            let response = service.query(
                store,
                DashboardQueryRequestV1::Summary(DashboardSummaryRequestV1 {
                    schema_version: DASHBOARD_QUERY_VERSION.into(),
                    kind: DashboardSummaryKindV1::Summary,
                    filters: None,
                    snapshot_id: snapshot.into(),
                    cursor,
                }),
            );
            let DashboardQueryResponseV1::Summary(response) = response else {
                panic!("summary failed with bounded status");
            };
            if let DashboardWorkV1::Complete(work) = response.work {
                assert!(response.pagination.next_cursor.is_none());
                return work.kpis;
            }
            cursor = Some(
                response
                    .pagination
                    .next_cursor
                    .expect("pending summary has continuation"),
            );
        }
    }

    fn three_generations(root: &Path) {
        let store = LocalStore::open_current(root.join("state/store"))
            .unwrap_or_else(|_| panic!("current authority open failed"));
        let before = canonical_identity(&store);
        let budget = StorageBudget::calculate(1024 * 1024 * 1024, false).unwrap();
        let mut previous_generation = None;
        let mut first_files = Vec::new();
        let mut first_summary = None;
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
            let summary = complete_summary(&store, snapshot.view_id());
            if let Some(expected) = &first_summary {
                assert!(
                    expected == &summary,
                    "complete summary remains identical across rotations"
                );
            } else {
                first_summary = Some(summary);
            }
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

    fn new_ingest_three_generations(root: &Path) {
        let mut state = collector_state(root);
        let baseline = state.store.record_count().unwrap();
        let baseline_observations = state.store.observation_count().unwrap();
        let budget = StorageBudget::calculate(1024 * 1024 * 1024, false).unwrap();
        measured_refresh(root);
        state.store.invalidate_report().unwrap();
        measured_refresh(root);
        for rotation in 1..=3_u64 {
            assert_eq!(catalog_files(root).len(), 2);
            let config = load(&state.layout.config).unwrap();
            let admission = crate::RuntimeControl::new(&config)
                .unwrap()
                .collector_admission_diagnostic(root, u64::from(config.collection.max_batch_bytes))
                .unwrap();
            // This deliberately optimistic comparison is diagnostic only, not a
            // safe journal allowance: framing, growth and materialization are omitted.
            let authority_logical_bytes = fs::metadata(state.store.database_path()).unwrap().len();
            let retained_view_allocated_bytes = catalog_files(root)
                .iter()
                .map(|path| StorageBudget::allocated_bytes(path).unwrap())
                .sum::<u64>();
            println!(
                "new_ingest_rotation={rotation} store_allocated_bytes={} reservation_bytes={} report_reserved_bytes={} writable_headroom_bytes={} deficit_bytes={}",
                admission.existing_store_allocated_bytes,
                admission.collector_reservation_bytes,
                admission.current_report_reserved_bytes,
                admission.writable_headroom_bytes,
                admission.deficit_bytes,
            );
            println!(
                "authority_logical_bytes={authority_logical_bytes} retained_view_allocated_bytes={retained_view_allocated_bytes} optimistic_authority_only_deficit_bytes={}",
                authority_logical_bytes.saturating_sub(admission.writable_headroom_bytes),
            );
            let payload = projected_notify(
                "private-capacity-probe-thread",
                &format!("private-capacity-probe-turn-{rotation}"),
            );
            let before = canonical_identity(&state.store);
            let before_status = state.store.report_status().unwrap();
            let before_cursor = state.store.cursor("codex", &state.source_generation).unwrap();
            let before_view = current_report_view(&state.store).unwrap().unwrap();
            let outcome = ingest_notify_locked(&mut state, &payload);
            if outcome.is_err() {
                assert_eq!(canonical_identity(&state.store), before);
                assert_eq!(state.store.report_status().unwrap(), before_status);
                assert_eq!(state.store.cursor("codex", &state.source_generation).unwrap(), before_cursor);
                assert_eq!(current_report_view(&state.store).unwrap().unwrap().view_id(), before_view.view_id());
                println!("new_ingest_refusal_preserves_authority_cursor_generation_and_view=true");
            }
            assert!(matches!(
                outcome.unwrap(),
                super::super::IngestOutcome::Committed
            ));
            assert_eq!(state.store.record_count().unwrap(), baseline + rotation);
            assert_eq!(
                state.store.observation_count().unwrap(),
                baseline_observations + rotation,
            );
            let identity = canonical_identity(&state.store);
            let peak = measured_refresh(root);
            assert!(peak.allocated <= budget.writable_limit());
            let snapshot = current_report_view(&state.store).unwrap().unwrap();
            assert_eq!(u64::try_from(snapshot.records()).unwrap(), baseline + rotation);
            assert!(!state.store.report_status().unwrap().pending());
            assert_eq!(canonical_identity(&state.store), identity);
            assert_eq!(catalog_files(root).len(), 2);
            println!(
                "new_ingest_rotation={rotation} records={} allocated_tree_bytes={} sampled_peak_tree_bytes={} status=ok",
                baseline + rotation,
                StorageBudget::allocated_tree_bytes(root).unwrap(),
                peak.allocated,
            );
        }
    }

    #[test]
    fn synthetic_new_ingest_rotates_with_current_and_retired_views() {
        let runtime = PrivateRuntime::new();
        new_ingest_three_generations(&runtime.0);
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
        let config_guard = ConfigMutationGuard::acquire(&state.layout).unwrap();
        save(&config_guard, &config).unwrap();
        drop(config_guard);
        let budget =
            StorageBudget::calculate(config.collection.local_storage_budget_bytes, false).unwrap();
        fill_to_remaining(&runtime.0, budget, 4 * 1024 * 1024);
        state
            .store
            .invalidate_report()
            .unwrap_or_else(|_| panic!("temporary invalidation failed"));
        assert!(matches!(
            crate::refresh_report_observing(&state.layout, &state.store, |_| {}),
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
        let config_guard = ConfigMutationGuard::acquire(&state.layout).unwrap();
        save(&config_guard, &config).unwrap();
        drop(config_guard);
        let budget =
            StorageBudget::calculate(config.collection.local_storage_budget_bytes, false).unwrap();
        state
            .store
            .invalidate_report()
            .unwrap_or_else(|_| panic!("temporary invalidation failed"));
        let mut injected = false;
        let result = crate::refresh_report_observing(&state.layout, &state.store, |_| {
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

    struct BackupProcess(std::process::Child);

    impl Drop for BackupProcess {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn backup_into(source: &Path, runtime: &PrivateRuntime) {
        let layout = install(&runtime.0).unwrap();
        let store_dir = layout.state.join("store");
        drop(LocalStore::open(&store_dir).unwrap());
        let target = store_dir.join("local-store.sqlite3");
        // Backup command contains only the freshly owned destination; source is a separate argv.
        let destination = target
            .to_str()
            .expect("temporary destination is UTF-8")
            .replace('\'', "''");
        let mut child = BackupProcess(
            Command::new("sqlite3")
                .arg("-readonly")
                .args(["-cmd", ".timeout 5000"])
                .arg(source)
                .arg(format!(".backup '{destination}'"))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("SQLite backup tool available"),
        );
        let deadline = Instant::now() + Duration::from_mins(2);
        loop {
            if let Some(status) = child.0.try_wait().expect("backup status available") {
                assert!(status.success(), "read-only coherent backup succeeded");
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.0.kill();
                let _ = child.0.wait();
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
        // Legacy migration is admitted against the copied runtime only; never open the source.
        let config = load(&layout.config).unwrap();
        let headroom = crate::RuntimeControl::new(&config)
            .unwrap()
            .migration_headroom(&runtime.0)
            .unwrap();
        drop(
            LocalStore::open_with_migration_headroom_deferred_projection(&store_dir, headroom)
                .unwrap_or_else(|error| {
                    let code = match &error {
                        agent_observability_local_store::StoreError::Sqlite(error) => error.sqlite_error_code(),
                        _ => None,
                    };
                    panic!("temporary copy migration failed: {error}; sqlite_code={code:?}; admitted_bytes={headroom}")
                }),
        );
    }

    #[test]
    fn synthetic_read_only_backup_rotates_without_changing_source() {
        let source_runtime = PrivateRuntime::new();
        let mut state = collector_state(&source_runtime.0);
        ingest_notify_locked(
            &mut state,
            &projected_notify("backup-thread", "backup-turn"),
        )
        .unwrap();
        drop(state);
        let source = source_runtime.0.join("state/store/local-store.sqlite3");
        let before = fs::read(&source).unwrap();
        let runtime = PrivateRuntime::new();
        backup_into(&source, &runtime);
        three_generations(&runtime.0);
        assert_eq!(fs::read(source).unwrap(), before);
    }

    /// Opt-in only: source uses the read-only `SQLite` backup API, never `LocalStore`.
    /// Run with `AO_ROTATION_BACKUP_SOURCE=/path/to/coherent.sqlite3` and `--ignored --nocapture`.
    #[test]
    #[ignore = "explicit private-backup diagnostic; leader-run only"]
    fn private_backup_three_generation_rotation() {
        let source =
            std::env::var_os("AO_ROTATION_BACKUP_SOURCE").expect("explicit backup source required");
        let source = fs::canonicalize(source).expect("backup source exists");
        assert!(source.is_file());
        let runtime = PrivateRuntime::new();
        backup_into(&source, &runtime);
        three_generations(&runtime.0);
        runtime.cleanup();
    }

    /// New writes with two retained views; never opens source as a store.
    #[test]
    #[ignore = "explicit private-backup new-ingest diagnostic; leader-run only"]
    fn private_backup_new_ingest_three_generations() {
        let source = std::env::var_os("AO_ROTATION_BACKUP_SOURCE")
            .expect("explicit backup source required");
        let source = fs::canonicalize(source).expect("backup source exists");
        assert!(source.is_file());
        let runtime = PrivateRuntime::new();
        backup_into(&source, &runtime);
        new_ingest_three_generations(&runtime.0);
        runtime.cleanup();
    }

    /// Isolates backup/migration RSS from report construction; use a process-level RSS probe.
    #[test]
    #[ignore = "explicit private-backup migration measurement; leader-run only"]
    fn private_backup_migration_only() {
        let source = std::env::var_os("AO_ROTATION_BACKUP_SOURCE")
            .expect("explicit backup source required");
        let source = fs::canonicalize(source).expect("backup source exists");
        assert!(source.is_file());
        let runtime = PrivateRuntime::new();
        backup_into(&source, &runtime);
        let store = LocalStore::open_current(runtime.0.join("state/store")).unwrap();
        eprintln!("migration_records={} status=ok", store.record_count().unwrap());
        drop(store);
        runtime.cleanup();
    }

    /// Measurement only: rewrites indexes of an isolated copied sidecar, never a usable view.
    #[test]
    #[ignore = "explicit private-backup index measurement; not query parity evidence"]
    fn private_backup_narrow_index_measurement() {
        let source =
            std::env::var_os("AO_ROTATION_BACKUP_SOURCE").expect("explicit backup source required");
        let source = fs::canonicalize(source).expect("backup source exists");
        assert!(source.is_file());
        let runtime = PrivateRuntime::new();
        backup_into(&source, &runtime);
        measured_refresh(&runtime.0);
        let files = catalog_files(&runtime.0);
        assert_eq!(files.len(), 1);
        let measurement = runtime.0.join("index-measurement.sqlite3");
        fs::copy(&files[0], &measurement).expect("copy owned sidecar for isolated measurement");
        let unvacuumed = fs::metadata(&measurement).unwrap().len();
        run_measurement_sql(&measurement, "PRAGMA cache_size=-8192; VACUUM;");
        let before = fs::metadata(&measurement).unwrap().len();
        let sql = "PRAGMA cache_size=-8192; PRAGMA temp_store=FILE; \
            DROP INDEX spans_repo_order_idx; DROP INDEX spans_session_order_idx; \
            DROP INDEX spans_turn_order_idx; DROP INDEX spans_agent_order_idx; \
            DROP INDEX spans_model_order_idx; \
            CREATE INDEX spans_repo_order_idx ON spans(repo,source_order); \
            CREATE INDEX spans_session_order_idx ON spans(session_id,source_order); \
            CREATE INDEX spans_turn_order_idx ON spans(turn_id,source_order); \
            CREATE INDEX spans_agent_order_idx ON spans(agent,source_order); \
            CREATE INDEX spans_model_order_idx ON spans(model,source_order); VACUUM;";
        run_measurement_sql(&measurement, sql);
        let after = fs::metadata(&measurement).unwrap().len();
        println!(
            "index_measurement_unvacuumed_bytes={unvacuumed} index_measurement_baseline_bytes={before} index_measurement_narrow_bytes={after} query_parity=unverified"
        );
        assert!(after < before, "narrow indexes must demonstrate savings");
        runtime.cleanup();
    }

    fn run_measurement_sql(measurement: &Path, sql: &str) {
        let private_directory = measurement.parent().expect("private measurement parent");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(private_directory)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        let mut child = BackupProcess(
            Command::new("sqlite3")
                .env("SQLITE_TMPDIR", private_directory)
                .env("TMPDIR", private_directory)
                .current_dir(private_directory)
                .arg(measurement)
                .arg(sql)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("SQLite measurement tool available"),
        );
        let deadline = Instant::now() + Duration::from_mins(2);
        loop {
            if let Some(status) = child.0.try_wait().expect("measurement status available") {
                assert!(status.success(), "isolated index measurement succeeded");
                break;
            }
            assert!(Instant::now() < deadline, "index measurement deadline");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
