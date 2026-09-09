//! Private, bounded composition of read-only storage ownership evidence.
//!
//! The caller owns the all-writer freeze and captures integration evidence inside
//! that same freeze. This module does not create, recover, clean, cache, or expose
//! a write-capable store, and its result is observation only, never admission.

#[cfg(target_os = "macos")]
use agent_observability_codex_integration::LaunchAgentStorageOwnershipEvidence;
use agent_observability_codex_integration::storage_accounting::CodexConfigSnapshotOwnershipEvidence;
use agent_observability_local_collector::storage_ownership::{
    CollectorPrivateStorageObservation, CollectorStorageOwnershipEvidence,
    CollectorTlsOwnershipEvidence,
};
use agent_observability_local_runtime::{
    InstalledLayout, StorageAllocationClass, StorageInventoryError,
    config::ConfigAccountingEvidence, lock::storage_ownership::SingletonStorageOwnershipEvidence,
    reservation::ReportReservationEvidence, storage_coherence::OwnedStorageFreezeGuard,
    storage_inventory::StorageAllocationObservationV1,
};
use agent_observability_local_store::ReportViewOwnershipObservation;
use agent_observability_local_store::storage_ownership::{
    StorageOwnershipObservation, with_optional_report_reader_storage_ownership,
};
use agent_observability_local_ui::DashboardCapabilityStorageOwnershipEvidence;
use agent_observability_static_report::StaticReportStorageOwnershipEvidence;
use std::{fs::File, path::Path};

#[cfg(not(target_os = "macos"))]
pub(crate) type LaunchAgentStorageOwnershipEvidence = ();

pub(crate) struct AllOwnerStorageObservation {
    pub(crate) allocation: StorageAllocationObservationV1,
    pub(crate) report_reserved_bytes: u64,
    pub(crate) config_revision: String,
}

/// Composes all currently supported ownership evidence under one caller-owned freeze.
///
/// `codex` and, on macOS, `launch` must have been captured by the caller after
/// acquiring `freeze`. Unsupported-platform `LaunchAgent` artifacts deliberately remain
/// unknown. The callback receives raw allocation and reservation observations only.
pub(crate) fn with_all_owner_storage_observation<T>(
    layout: &InstalledLayout,
    freeze: &OwnedStorageFreezeGuard<'_>,
    codex: &CodexConfigSnapshotOwnershipEvidence,
    launch: Option<&LaunchAgentStorageOwnershipEvidence>,
    consume: impl FnOnce(&AllOwnerStorageObservation) -> Result<T, String>,
) -> Result<T, String> {
    validate_layout(layout)?;
    #[cfg(target_os = "macos")]
    let launch = launch.ok_or_else(|| failure("LaunchAgent evidence is required"))?;
    #[cfg(not(target_os = "macos"))]
    let _ = launch;

    let root = layout.root.as_path();
    let mutation = freeze.mutation();
    let config = ConfigAccountingEvidence::capture(root, mutation)
        .map_err(|_| failure("config evidence capture failed"))?;
    let reservation = ReportReservationEvidence::capture(root, mutation)
        .map_err(|_| failure("reservation evidence capture failed"))?;
    let singleton = SingletonStorageOwnershipEvidence::capture(root, mutation)
        .map_err(|_| failure("singleton evidence capture failed"))?;
    let collector = CollectorStorageOwnershipEvidence::capture(layout)
        .map_err(|_| failure("collector evidence capture failed"))?;
    let tls = CollectorTlsOwnershipEvidence::capture(layout)
        .map_err(|_| failure("collector TLS evidence capture failed"))?;
    let mut private = CollectorPrivateStorageObservation::capture(layout)
        .map_err(|_| failure("private artifact evidence capture failed"))?;
    let dashboard = DashboardCapabilityStorageOwnershipEvidence::capture(root)
        .map_err(|_| failure("dashboard evidence capture failed"))?;
    let report = StaticReportStorageOwnershipEvidence::capture(root)
        .map_err(|_| failure("static report evidence capture failed"))?;

    revalidate_all(
        freeze,
        &config,
        &reservation,
        &singleton,
        &collector,
        tls.as_ref(),
        &private,
        &dashboard,
        &report,
        codex,
        #[cfg(target_os = "macos")]
        launch,
    )?;

    let mut callback_error = None;
    let store_directory = layout.state.join("store");
    let observed = with_optional_report_reader_storage_ownership(&store_directory, |store| {
        with_optional_report_view(store, |published| {
            let allocation = freeze
                .classify(|relative, candidate| {
                    classify_entry(
                        root,
                        relative,
                        candidate,
                        freeze,
                        &config,
                        &reservation,
                        &singleton,
                        &collector,
                        tls.as_ref(),
                        &mut private,
                        &dashboard,
                        &report,
                        codex,
                        #[cfg(target_os = "macos")]
                        launch,
                        store,
                        published,
                    )
                })
                .map_err(|_| failure("storage inventory classification failed"))?;
            let observation = AllOwnerStorageObservation {
                allocation,
                report_reserved_bytes: reservation.captured_reserved_bytes(),
                config_revision: config.expected_revision().to_owned(),
            };
            match consume(&observation) {
                Ok(value) => Ok(value),
                Err(error) => {
                    callback_error = Some(error.clone());
                    Err(error)
                }
            }
        })
    });

    let postcheck = revalidate_all(
        freeze,
        &config,
        &reservation,
        &singleton,
        &collector,
        tls.as_ref(),
        &private,
        &dashboard,
        &report,
        codex,
        #[cfg(target_os = "macos")]
        launch,
    );

    if let Some(primary) = callback_error {
        return Err(primary);
    }
    postcheck?;
    observed
        .map_err(|_| failure("store ownership observation failed"))?
        .map_err(|_| failure("report-view ownership observation failed"))?
}

#[allow(clippy::too_many_arguments)]
fn classify_entry(
    root: &Path,
    relative: &Path,
    candidate: &File,
    freeze: &OwnedStorageFreezeGuard<'_>,
    config: &ConfigAccountingEvidence<'_>,
    reservation: &ReportReservationEvidence<'_>,
    singleton: &SingletonStorageOwnershipEvidence<'_>,
    collector: &CollectorStorageOwnershipEvidence,
    tls: Option<&CollectorTlsOwnershipEvidence>,
    private: &mut CollectorPrivateStorageObservation,
    dashboard: &DashboardCapabilityStorageOwnershipEvidence,
    report: &StaticReportStorageOwnershipEvidence,
    codex: &CodexConfigSnapshotOwnershipEvidence,
    #[cfg(target_os = "macos")] launch: &LaunchAgentStorageOwnershipEvidence,
    store: Option<&StorageOwnershipObservation<'_>>,
    published: Option<&ReportViewOwnershipObservation<'_>>,
) -> Result<StorageAllocationClass, StorageInventoryError> {
    let absolute = root.join(relative);
    let mut retained = false;
    retain_match(&mut retained, config.matches_entry(relative, candidate))?;
    retain_match(
        &mut retained,
        freeze
            .mutation()
            .matches_accounting_lock(root, relative, candidate),
    )?;
    retain_match(
        &mut retained,
        freeze.matches_accounting_lock(relative, candidate),
    )?;
    retain_match(
        &mut retained,
        reservation.matches_control_entry(relative, candidate),
    )?;
    retain_match(&mut retained, singleton.matches_entry(relative, candidate))?;
    retain_match(&mut retained, collector.matches_entry(relative, candidate))?;
    retain_match(
        &mut retained,
        match tls {
            Some(tls) => tls.matches_entry(relative, candidate),
            None => Ok(false),
        },
    )?;
    retain_match(&mut retained, private.matches_entry(relative, candidate))?;
    retain_match(&mut retained, dashboard.matches_entry(relative, candidate))?;
    retain_match(&mut retained, report.matches_entry(relative, candidate))?;
    retain_match(&mut retained, codex.matches_entry(relative, candidate))?;
    #[cfg(target_os = "macos")]
    retain_match(&mut retained, launch.matches_entry(relative, candidate))?;
    retain_match(
        &mut retained,
        match store {
            Some(store) => store.recognizes(&absolute, candidate),
            None => Ok(false),
        },
    )?;
    retain_match(
        &mut retained,
        match published {
            Some(published) => published.recognizes(&absolute, candidate),
            None => Ok(false),
        },
    )?;

    let workspace = ownership(reservation.matches_staging(&absolute, candidate))?;
    classify_matches(retained, workspace)
}

fn retain_match<E>(
    retained: &mut bool,
    matcher: Result<bool, E>,
) -> Result<(), StorageInventoryError> {
    *retained |= ownership(matcher)?;
    Ok(())
}

fn ownership<T, E>(result: Result<T, E>) -> Result<T, StorageInventoryError> {
    result.map_err(|_| StorageInventoryError::OwnershipMismatch)
}

fn classify_matches(
    retained: bool,
    workspace: bool,
) -> Result<StorageAllocationClass, StorageInventoryError> {
    match (retained, workspace) {
        (true, true) => Err(StorageInventoryError::OwnershipMismatch),
        (true, false) => Ok(StorageAllocationClass::Retained),
        (false, true) => Ok(StorageAllocationClass::Workspace),
        (false, false) => Ok(StorageAllocationClass::Unknown),
    }
}

fn with_optional_report_view<T>(
    store: Option<&StorageOwnershipObservation<'_>>,
    consume: impl FnOnce(Option<&ReportViewOwnershipObservation<'_>>) -> T,
) -> Result<T, agent_observability_local_store::ReportViewCatalogError> {
    match store {
        Some(store) => store.with_report_view_ownership_observation(consume),
        None => Ok(consume(None)),
    }
}

#[allow(clippy::too_many_arguments)]
fn revalidate_all(
    freeze: &OwnedStorageFreezeGuard<'_>,
    config: &ConfigAccountingEvidence<'_>,
    reservation: &ReportReservationEvidence<'_>,
    singleton: &SingletonStorageOwnershipEvidence<'_>,
    collector: &CollectorStorageOwnershipEvidence,
    tls: Option<&CollectorTlsOwnershipEvidence>,
    private: &CollectorPrivateStorageObservation,
    dashboard: &DashboardCapabilityStorageOwnershipEvidence,
    report: &StaticReportStorageOwnershipEvidence,
    codex: &CodexConfigSnapshotOwnershipEvidence,
    #[cfg(target_os = "macos")] launch: &LaunchAgentStorageOwnershipEvidence,
) -> Result<(), String> {
    freeze
        .revalidate()
        .map_err(|_| failure("storage freeze revalidation failed"))?;
    config
        .revalidate()
        .map_err(|_| failure("config evidence revalidation failed"))?;
    reservation
        .revalidate()
        .map_err(|_| failure("reservation evidence revalidation failed"))?;
    singleton
        .revalidate()
        .map_err(|_| failure("singleton evidence revalidation failed"))?;
    collector
        .revalidate()
        .map_err(|_| failure("collector evidence revalidation failed"))?;
    if let Some(tls) = tls {
        tls.revalidate()
            .map_err(|_| failure("collector TLS evidence revalidation failed"))?;
    }
    private
        .revalidate()
        .map_err(|_| failure("private artifact evidence revalidation failed"))?;
    dashboard
        .revalidate()
        .map_err(|_| failure("dashboard evidence revalidation failed"))?;
    report
        .revalidate()
        .map_err(|_| failure("static report evidence revalidation failed"))?;
    codex
        .revalidate()
        .map_err(|_| failure("Codex snapshot evidence revalidation failed"))?;
    #[cfg(target_os = "macos")]
    launch
        .revalidate()
        .map_err(|_| failure("LaunchAgent evidence revalidation failed"))?;
    freeze
        .revalidate()
        .map_err(|_| failure("storage freeze revalidation failed"))
}

fn validate_layout(layout: &InstalledLayout) -> Result<(), String> {
    let root = &layout.root;
    let exact = root.is_absolute()
        && layout.config == root.join("config.json")
        && layout.logs == root.join("logs")
        && layout.queue == root.join("queue")
        && layout.state == root.join("state")
        && layout.runtime == root.join("runtime");
    if exact {
        Ok(())
    } else {
        Err(failure("installed layout mismatch"))
    }
}

fn failure(reason: &str) -> String {
    format!("storage accounting unavailable: {reason}")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use agent_observability_codex_integration::LaunchAgentStorageOwnershipEvidence;
    use agent_observability_codex_integration::storage_accounting::capture_codex_config_snapshot_ownership;
    use agent_observability_local_runtime::{
        RuntimeControl, install, load, storage_coherence::StorageBarrier,
    };
    use agent_observability_local_store::LocalStore;
    use std::{
        fs,
        os::unix::fs::{FileTypeExt, OpenOptionsExt, PermissionsExt},
        path::PathBuf,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Fixture {
        base: PathBuf,
        layout: InstalledLayout,
        barrier: StorageBarrier,
        external_config: PathBuf,
        #[cfg(target_os = "macos")]
        home: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let base = std::env::temp_dir().join(format!(
                "cli-storage-accounting-{name}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            assert!(!base.exists());
            fs::create_dir(&base).unwrap();
            fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
            let layout = install(&base.join("runtime-root")).unwrap();
            let mutation =
                agent_observability_local_runtime::MutationGuard::try_acquire(&layout.runtime)
                    .unwrap();
            let barrier = StorageBarrier::initialize(&layout.root, &mutation).unwrap();
            drop(mutation);
            Self {
                external_config: base.join("external-config.toml"),
                #[cfg(target_os = "macos")]
                home: base.join("home"),
                base,
                layout,
                barrier,
            }
        }

        fn freeze(&self) -> OwnedStorageFreezeGuard<'_> {
            let mutation =
                agent_observability_local_runtime::MutationGuard::try_acquire(&self.layout.runtime)
                    .unwrap();
            self.barrier.try_freeze_owned(mutation).unwrap()
        }

        fn observe<T>(
            &self,
            freeze: &OwnedStorageFreezeGuard<'_>,
            consume: impl FnOnce(&AllOwnerStorageObservation) -> Result<T, String>,
        ) -> Result<T, String> {
            let codex =
                capture_codex_config_snapshot_ownership(&self.layout.root, &self.external_config)
                    .unwrap();
            #[cfg(target_os = "macos")]
            let launch =
                LaunchAgentStorageOwnershipEvidence::capture(&self.layout.root, &self.home)
                    .unwrap();
            with_all_owner_storage_observation(
                &self.layout,
                freeze,
                &codex,
                #[cfg(target_os = "macos")]
                Some(&launch),
                #[cfg(not(target_os = "macos"))]
                None,
                consume,
            )
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.base).unwrap();
        }
    }

    #[test]
    fn absent_store_preserves_zero_byte_unknown_and_full_reservation() {
        let fixture = Fixture::new("absent-store");
        let sentinel = fixture.layout.logs.join("sentinel.unknown");
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&sentinel)
            .unwrap();
        let freeze = fixture.freeze();
        let config = load(&fixture.layout.config).unwrap();
        let reservation = RuntimeControl::new(&config)
            .unwrap()
            .reserve_report_build(&fixture.layout.root, freeze.mutation(), 8192)
            .unwrap();

        fixture
            .observe(&freeze, |observation| {
                assert!(observation.allocation.retained_bytes > 0);
                assert_eq!(observation.allocation.workspace_bytes, 0);
                assert_eq!(observation.allocation.unknown_bytes, 0);
                assert_eq!(observation.allocation.unknown_entry_count, 1);
                assert_eq!(observation.report_reserved_bytes, 8192);
                assert!(!observation.config_revision.is_empty());
                Ok(())
            })
            .unwrap();
        reservation
            .release(&fixture.layout.root, freeze.mutation())
            .unwrap();
    }

    #[test]
    fn present_current_store_is_observed_without_exposing_a_writer() {
        let fixture = Fixture::new("present-store");
        let store = LocalStore::open(fixture.layout.state.join("store")).unwrap();
        drop(store);
        let freeze = fixture.freeze();
        fixture
            .observe(&freeze, |observation| {
                assert!(observation.allocation.retained_bytes > 0);
                assert_eq!(observation.allocation.unknown_entry_count, 0);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn malformed_known_artifact_fails_with_sanitized_error() {
        let fixture = Fixture::new("malformed-known");
        fs::write(
            fixture.layout.runtime.join("report-dirty"),
            b"private raw sentinel",
        )
        .unwrap();
        fs::set_permissions(
            fixture.layout.runtime.join("report-dirty"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let freeze = fixture.freeze();
        let error = fixture.observe(&freeze, |_| Ok(())).unwrap_err();
        assert_eq!(
            error,
            "storage accounting unavailable: collector evidence capture failed"
        );
        assert!(!error.contains("private raw sentinel"));
        assert!(!error.contains(fixture.base.to_str().unwrap()));
    }

    #[test]
    fn external_config_authority_is_not_read() {
        let fixture = Fixture::new("external-config");
        assert!(
            Command::new("mkfifo")
                .arg(&fixture.external_config)
                .status()
                .unwrap()
                .success()
        );
        fs::set_permissions(&fixture.external_config, fs::Permissions::from_mode(0o000)).unwrap();
        let freeze = fixture.freeze();
        fixture.observe(&freeze, |_| Ok(())).unwrap();
        assert!(
            fs::symlink_metadata(&fixture.external_config)
                .unwrap()
                .file_type()
                .is_fifo()
        );
    }

    #[test]
    fn matcher_errors_are_not_hidden_by_an_existing_match() {
        let mut retained = false;
        retain_match(&mut retained, Ok::<_, ()>(true)).unwrap();
        assert_eq!(
            retain_match(&mut retained, Err::<bool, _>("private matcher detail")),
            Err(StorageInventoryError::OwnershipMismatch)
        );
        assert!(retained);
    }

    #[test]
    fn retained_workspace_contradiction_fails_closed() {
        assert_eq!(
            classify_matches(true, true),
            Err(StorageInventoryError::OwnershipMismatch)
        );
        assert_eq!(
            classify_matches(false, false),
            Ok(StorageAllocationClass::Unknown)
        );
    }

    #[test]
    fn callback_error_remains_primary_when_postcheck_also_fails() {
        let fixture = Fixture::new("callback-primary");
        let freeze = fixture.freeze();
        let config = load(&fixture.layout.config).unwrap();
        let _reservation = RuntimeControl::new(&config)
            .unwrap()
            .reserve_report_build(&fixture.layout.root, freeze.mutation(), 8192)
            .unwrap();
        let sentinel = "primary callback sentinel";
        let error = fixture
            .observe(&freeze, |_| {
                fs::remove_file(fixture.layout.runtime.join("report-reservation.lock")).unwrap();
                Err::<(), _>(sentinel.into())
            })
            .unwrap_err();
        assert_eq!(error, sentinel);
    }

    #[test]
    fn successful_callback_cannot_hide_failed_global_postcheck() {
        let fixture = Fixture::new("global-postcheck");
        let freeze = fixture.freeze();
        let config = load(&fixture.layout.config).unwrap();
        let _reservation = RuntimeControl::new(&config)
            .unwrap()
            .reserve_report_build(&fixture.layout.root, freeze.mutation(), 8192)
            .unwrap();
        let error = fixture
            .observe(&freeze, |_| {
                fs::remove_file(fixture.layout.runtime.join("report-reservation.lock")).unwrap();
                Ok("must not escape")
            })
            .unwrap_err();
        assert_eq!(
            error,
            "storage accounting unavailable: reservation evidence revalidation failed"
        );
    }

    #[test]
    fn successful_callback_cannot_hide_failed_store_postcheck() {
        let fixture = Fixture::new("store-postcheck");
        let store_directory = fixture.layout.state.join("store");
        let store = LocalStore::open(&store_directory).unwrap();
        drop(store);
        let freeze = fixture.freeze();
        let error = fixture
            .observe(&freeze, |_| {
                fs::set_permissions(
                    store_directory.join("local-store.sqlite3"),
                    fs::Permissions::from_mode(0o644),
                )
                .unwrap();
                Ok("must not escape")
            })
            .unwrap_err();
        assert_eq!(
            error,
            "storage accounting unavailable: store ownership observation failed"
        );
    }

    #[test]
    fn initial_store_journal_is_preserved_and_prevents_callback() {
        let fixture = Fixture::new("initial-journal");
        let store_directory = fixture.layout.state.join("store");
        let store = LocalStore::open(&store_directory).unwrap();
        drop(store);
        let journal = store_directory.join("local-store.sqlite3-journal");
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&journal)
            .unwrap();
        let freeze = fixture.freeze();
        let callback_called = std::cell::Cell::new(false);
        let error = fixture
            .observe(&freeze, |_| {
                callback_called.set(true);
                Ok(())
            })
            .unwrap_err();
        assert_eq!(
            error,
            "storage accounting unavailable: store ownership observation failed"
        );
        assert!(!callback_called.get());
        assert!(journal.is_file());
        assert_eq!(fs::metadata(journal).unwrap().len(), 0);
    }

    #[test]
    fn store_appearance_during_absent_callback_is_rejected() {
        let fixture = Fixture::new("store-appearance");
        let store_directory = fixture.layout.state.join("store");
        assert!(!store_directory.exists());
        let freeze = fixture.freeze();
        let error = fixture
            .observe(&freeze, |_| {
                let store = LocalStore::open(&store_directory).unwrap();
                drop(store);
                Ok("must not escape")
            })
            .unwrap_err();
        assert_eq!(
            error,
            "storage accounting unavailable: store ownership observation failed"
        );
        assert!(store_directory.is_dir());
    }
}
