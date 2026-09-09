//! Private, bounded composition of read-only storage ownership evidence.
//!
//! The caller owns the all-writer freeze and captures integration evidence inside
//! that same freeze. This module does not create, recover, clean, cache, or expose
//! a write-capable store. The ownership composer remains observation-only; the private
//! collector guard applies that evidence to admission but stays dormant and unwired.

#[cfg(target_os = "macos")]
use agent_observability_codex_integration::LaunchAgentStorageOwnershipEvidence;
use agent_observability_codex_integration::storage_accounting::CodexConfigSnapshotOwnershipEvidence;
use agent_observability_local_collector::storage_ownership::{
    CollectorPrivateStorageObservation, CollectorStorageOwnershipEvidence,
    CollectorTlsOwnershipEvidence,
};
use agent_observability_local_collector::{
    CollectorIngestPrecommitError, CollectorIngestPrecommitGuard,
};
use agent_observability_local_runtime::{
    InstalledLayout, LocalRuntimeConfigV3, StorageAllocationClass, StorageBudgetPolicyV1,
    StorageInventoryError,
    config::ConfigAccountingEvidence,
    lock::storage_ownership::SingletonStorageOwnershipEvidence,
    reservation::ReportReservationEvidence,
    storage_coherence::OwnedStorageFreezeGuard,
    storage_inventory::StorageAllocationObservationV1,
    storage_policy::{
        StorageAllocationSnapshotV1, StorageOperation,
        evaluate_storage_admission_with_reserved_total,
    },
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
    // Read only by the dormant precommit implementation until activation is reviewed.
    #[allow(dead_code)]
    pub(crate) storage_budget_policy: StorageBudgetPolicyV1,
}

/// Dormant until the CLI collector composition explicitly installs it after resource validation.
#[allow(dead_code)]
#[derive(Debug)]
struct CliCollectorIngestPrecommitGuard;

impl CollectorIngestPrecommitGuard for CliCollectorIngestPrecommitGuard {
    fn check_precommit(
        &self,
        layout: &InstalledLayout,
        freeze: &OwnedStorageFreezeGuard<'_>,
        config: &LocalRuntimeConfigV3,
        max_batch_bytes: u64,
    ) -> Result<(), CollectorIngestPrecommitError> {
        use agent_observability_codex_integration::storage_accounting::capture_current_codex_config_snapshot_ownership;

        validate_layout(layout).map_err(|_| CollectorIngestPrecommitError::Unavailable)?;
        freeze
            .revalidate()
            .map_err(|_| CollectorIngestPrecommitError::Unavailable)?;

        let codex = capture_current_codex_config_snapshot_ownership(&layout.root)
            .map_err(|_| CollectorIngestPrecommitError::Unavailable)?;
        #[cfg(target_os = "macos")]
        let launch = agent_observability_codex_integration::storage_accounting::capture_current_launch_agent_storage_ownership(&layout.root)
            .map_err(|_| CollectorIngestPrecommitError::Unavailable)?;

        check_cli_collector_ingest_precommit_with(
            layout,
            freeze,
            config,
            max_batch_bytes,
            &codex,
            #[cfg(target_os = "macos")]
            Some(&launch),
            #[cfg(not(target_os = "macos"))]
            None,
            OwnedStorageFreezeGuard::available_space,
            |root, max_batch_bytes| {
                agent_observability_local_runtime::control::collector_ingest_estimated_allowance(
                    root,
                    max_batch_bytes,
                )
                .map_err(|_| ())
            },
        )
    }
}

#[allow(clippy::too_many_arguments)]
// Called only by the dormant guard and its focused tests until activation is reviewed.
#[allow(dead_code)]
fn check_cli_collector_ingest_precommit_with<
    'freeze,
    'barrier,
    AvailableSpace,
    EstimatedAllowance,
>(
    layout: &InstalledLayout,
    freeze: &'freeze OwnedStorageFreezeGuard<'barrier>,
    config: &LocalRuntimeConfigV3,
    max_batch_bytes: u64,
    codex: &CodexConfigSnapshotOwnershipEvidence,
    launch: Option<&LaunchAgentStorageOwnershipEvidence>,
    available_space: AvailableSpace,
    estimated_allowance: EstimatedAllowance,
) -> Result<(), CollectorIngestPrecommitError>
where
    AvailableSpace: FnOnce(
        &'freeze OwnedStorageFreezeGuard<'barrier>,
    ) -> Result<
        u64,
        agent_observability_local_runtime::storage_coherence::StorageCoherenceError,
    >,
    EstimatedAllowance: FnOnce(&Path, u64) -> Result<u64, ()>,
{
    config
        .validate()
        .map_err(|_| CollectorIngestPrecommitError::Unavailable)?;
    if max_batch_bytes != u64::from(config.collection.max_batch_bytes) {
        return Err(CollectorIngestPrecommitError::Unavailable);
    }
    let supplied_revision = agent_observability_local_runtime::revision(config)
        .map_err(|_| CollectorIngestPrecommitError::Unavailable)?;

    with_all_owner_storage_observation(layout, freeze, codex, launch, |observation| {
        let numeric = (|| {
            if observation.config_revision != supplied_revision
                || observation.storage_budget_policy != config.storage_budget
            {
                return Err(CollectorIngestPrecommitError::Unavailable);
            }
            let filesystem_free_bytes =
                available_space(freeze).map_err(|_| CollectorIngestPrecommitError::Unavailable)?;
            let estimated_allowance_bytes = estimated_allowance(&layout.root, max_batch_bytes)
                .map_err(|()| CollectorIngestPrecommitError::Unavailable)?;
            evaluate_collector_ingest_precommit(
                observation,
                estimated_allowance_bytes,
                filesystem_free_bytes,
            )
        })();
        // Keep typed rejection nested so owner/store/global postchecks still run.
        Ok(numeric)
    })
    .map_err(|_| CollectorIngestPrecommitError::Unavailable)?
}

// Called only by the dormant guard and its focused tests until activation is reviewed.
#[allow(dead_code)]
fn evaluate_collector_ingest_precommit(
    observation: &AllOwnerStorageObservation,
    estimated_allowance_bytes: u64,
    filesystem_free_bytes: u64,
) -> Result<(), CollectorIngestPrecommitError> {
    evaluate_storage_admission_with_reserved_total(
        &observation.storage_budget_policy,
        StorageAllocationSnapshotV1 {
            retained_bytes: observation.allocation.retained_bytes,
            workspace_bytes: observation.allocation.workspace_bytes,
            unknown_bytes: observation.allocation.unknown_bytes,
            unknown_entry_count: observation.allocation.unknown_entry_count,
        },
        observation.report_reserved_bytes,
        Some(estimated_allowance_bytes),
        filesystem_free_bytes,
        StorageOperation::Ingest,
    )
    .map(|_| ())
    .map_err(|_| CollectorIngestPrecommitError::Denied)
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
                storage_budget_policy: config.storage_budget_policy().clone(),
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
        LocalRuntimeConfigV3, MutationGuard, RuntimeControl, SingletonError, StorageBudgetMode,
        install, load, storage::MAX_ACCOUNTING_ENTRIES, storage_coherence::StorageBarrier,
    };
    use agent_observability_local_store::LocalStore;
    use std::{
        fs,
        io::Write,
        os::unix::fs::{DirBuilderExt, FileTypeExt, OpenOptionsExt, PermissionsExt},
        path::PathBuf,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
        time::Instant,
    };

    const DIAGNOSTIC_SAMPLES: usize = 3;
    // Test-fixture mirrors of local-collector's private artifact contract. Source pointers:
    // PRIVATE_TURN_DETAIL_{DIRECTORY,STATUS_DIRECTORY,STATUS_VERSION},
    // MAX_PRIVATE_TURN_DETAIL_FILES, and validate_private_turn_detail_status in
    // crates/local-collector/src/lib.rs. Production constants and types remain private.
    const PRIVATE_DETAIL_DIRECTORY: &str = "private-codex-turn-details";
    const PRIVATE_STATUS_DIRECTORY: &str = "private-codex-turn-detail-statuses";
    const PRIVATE_STATUS_VERSION: &str = "private_codex_turn_detail_status.v1";
    const PRIVATE_ARTIFACT_CAPACITY: usize = 1_024;
    const PRIVATE_STATUS_BYTES: usize = 1_024;

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

        fn separated_config(&self) -> LocalRuntimeConfigV3 {
            let mut config = LocalRuntimeConfigV3::default();
            config.storage_budget.mode = StorageBudgetMode::Separated;
            let original = fs::read_to_string(&self.layout.config).unwrap();
            let updated = original.replacen("\"mode\": \"legacy\"", "\"mode\": \"separated\"", 1);
            assert_ne!(updated, original);
            fs::write(&self.layout.config, updated).unwrap();
            config
        }

        fn captures(
            &self,
        ) -> (
            CodexConfigSnapshotOwnershipEvidence,
            Option<LaunchAgentStorageOwnershipEvidence>,
        ) {
            let codex =
                capture_codex_config_snapshot_ownership(&self.layout.root, &self.external_config)
                    .unwrap();
            #[cfg(target_os = "macos")]
            let launch = Some(
                LaunchAgentStorageOwnershipEvidence::capture(&self.layout.root, &self.home)
                    .unwrap(),
            );
            #[cfg(not(target_os = "macos"))]
            let launch = None;
            (codex, launch)
        }

        fn check_precommit(
            &self,
            freeze: &OwnedStorageFreezeGuard<'_>,
            config: &LocalRuntimeConfigV3,
            max_batch_bytes: u64,
        ) -> Result<(), CollectorIngestPrecommitError> {
            let (codex, launch) = self.captures();
            check_cli_collector_ingest_precommit_with(
                &self.layout,
                freeze,
                config,
                max_batch_bytes,
                &codex,
                launch.as_ref(),
                OwnedStorageFreezeGuard::available_space,
                |root, max_batch_bytes| {
                    agent_observability_local_runtime::control::collector_ingest_estimated_allowance(
                        root,
                        max_batch_bytes,
                    )
                    .map_err(|_| ())
                },
            )
        }

        fn initialize_current_store(&self) {
            drop(LocalStore::open(self.layout.state.join("store")).unwrap());
        }

        fn populate_private_artifact_capacity(&self) {
            let detail_directory = self.layout.state.join(PRIVATE_DETAIL_DIRECTORY);
            let status_directory = self.layout.state.join(PRIVATE_STATUS_DIRECTORY);
            for directory in [&detail_directory, &status_directory] {
                fs::DirBuilder::new().mode(0o700).create(directory).unwrap();
            }

            for index in 0..PRIVATE_ARTIFACT_CAPACITY {
                let (turn_id, detail) = private_detail_at_byte_bound(index);
                let digest = turn_id.strip_prefix("id:sha256:").unwrap();
                write_private_fixture_file(
                    &detail_directory.join(format!("{digest}.json")),
                    &detail,
                );
                let mut status = format!(
                    "{{\"schema_version\":\"{PRIVATE_STATUS_VERSION}\",\"turn_id\":\"{turn_id}\",\"state\":\"available\",\"code\":\"ok\"}}"
                )
                .into_bytes();
                assert!(status.len() < PRIVATE_STATUS_BYTES);
                status.resize(PRIVATE_STATUS_BYTES, b' ');
                write_private_fixture_file(
                    &status_directory.join(format!("{digest}.json")),
                    &status,
                );
            }
        }

        fn populate_to_inventory_entries(&self, target_entries: usize) {
            let current_entries = tree_entry_count(&self.layout.root);
            assert!(current_entries <= target_entries);
            for index in current_entries..target_entries {
                write_private_fixture_file(
                    &self.layout.logs.join(format!("diagnostic-limit-{index}")),
                    &[],
                );
            }
            assert_eq!(tree_entry_count(&self.layout.root), target_entries);
        }
    }

    fn private_detail_at_byte_bound(index: usize) -> (String, Vec<u8>) {
        use agent_observability_adapter_codex::{
            MAX_PRIVATE_TURN_DETAIL_BYTES, project_notify_with_private_detail,
        };

        let turn = format!("synthetic-turn-{index:04}");
        let payload = |message: &str| {
            format!(
                "{{\"type\":\"agent-turn-complete\",\"thread-id\":\"synthetic-thread\",\"turn-id\":\"{turn}\",\"cwd\":\"/synthetic-only\",\"input-messages\":[],\"last-assistant-message\":\"{message}\"}}"
            )
        };
        let (_, empty) = project_notify_with_private_detail(payload("").as_bytes()).unwrap();
        let empty_bytes = empty.to_json().unwrap().len();
        let padding = MAX_PRIVATE_TURN_DETAIL_BYTES
            .checked_sub(empty_bytes)
            .unwrap();
        let (_, detail) =
            project_notify_with_private_detail(payload(&"x".repeat(padding)).as_bytes()).unwrap();
        let encoded = detail.to_json().unwrap();
        assert_eq!(encoded.len(), MAX_PRIVATE_TURN_DETAIL_BYTES);
        (detail.turn_id().to_owned(), encoded)
    }

    fn write_private_fixture_file(path: &Path, bytes: &[u8]) {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(bytes).unwrap();
    }

    fn tree_entry_count(path: &Path) -> usize {
        let mut count = 1;
        if path.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                count += tree_entry_count(&entry.unwrap().path());
            }
        }
        count
    }

    fn sample_dormant_trait_guard(
        fixture: &Fixture,
        freeze: &OwnedStorageFreezeGuard<'_>,
        config: &LocalRuntimeConfigV3,
        expected: Result<(), CollectorIngestPrecommitError>,
    ) -> [u128; DIAGNOSTIC_SAMPLES] {
        std::array::from_fn(|_| {
            let started = Instant::now();
            let result = CollectorIngestPrecommitGuard::check_precommit(
                &CliCollectorIngestPrecommitGuard,
                &fixture.layout,
                freeze,
                config,
                u64::from(config.collection.max_batch_bytes),
            );
            let elapsed_ns = started.elapsed().as_nanos();
            assert_eq!(result, expected);
            elapsed_ns
        })
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

    #[test]
    fn separated_precommit_accepts_a_coherent_numeric_snapshot_without_reacquiring_locks() {
        let fixture = Fixture::new("precommit-allowed");
        let config = fixture.separated_config();
        let freeze = fixture.freeze();
        let (codex, launch) = fixture.captures();
        let result = check_cli_collector_ingest_precommit_with(
            &fixture.layout,
            &freeze,
            &config,
            u64::from(config.collection.max_batch_bytes),
            &codex,
            launch.as_ref(),
            |freeze| {
                let barrier = StorageBarrier::open_existing(&fixture.layout.root).unwrap();
                assert!(matches!(
                    barrier.try_begin_write(),
                    Err(agent_observability_local_runtime::storage_coherence::StorageCoherenceError::Busy)
                ));
                assert!(matches!(
                    MutationGuard::try_acquire(&fixture.layout.runtime),
                    Err(SingletonError::AlreadyRunning)
                ));
                freeze.available_space()
            },
            |root, max_batch_bytes| {
                agent_observability_local_runtime::control::collector_ingest_estimated_allowance(
                    root,
                    max_batch_bytes,
                )
                .map_err(|_| ())
            },
        );
        assert_eq!(result, Ok(()));

        let store_fixture = Fixture::new("precommit-allowed-current-store");
        let store_config = store_fixture.separated_config();
        drop(LocalStore::open(store_fixture.layout.state.join("store")).unwrap());
        let store_freeze = store_fixture.freeze();
        assert_eq!(
            store_fixture.check_precommit(
                &store_freeze,
                &store_config,
                u64::from(store_config.collection.max_batch_bytes),
            ),
            Ok(())
        );
    }

    #[test]
    fn dormant_trait_entry_uses_current_read_only_evidence_facades() {
        let fixture = Fixture::new("precommit-trait-entry");
        let config = fixture.separated_config();
        let freeze = fixture.freeze();
        assert_eq!(
            CollectorIngestPrecommitGuard::check_precommit(
                &CliCollectorIngestPrecommitGuard,
                &fixture.layout,
                &freeze,
                &config,
                u64::from(config.collection.max_batch_bytes),
            ),
            Ok(())
        );
    }

    #[test]
    #[ignore = "manual bounded resource diagnostic; does not establish an activation SLO"]
    fn dormant_whole_guard_private_capacity_and_inventory_limit_diagnostic() {
        let baseline = Fixture::new("precommit-diagnostic-baseline");
        let baseline_config = baseline.separated_config();
        baseline.initialize_current_store();
        let baseline_freeze = baseline.freeze();
        let baseline_elapsed =
            sample_dormant_trait_guard(&baseline, &baseline_freeze, &baseline_config, Ok(()));

        let capacity = Fixture::new("precommit-diagnostic-private-capacity");
        let capacity_config = capacity.separated_config();
        capacity.initialize_current_store();
        capacity.populate_private_artifact_capacity();
        let capacity_freeze = capacity.freeze();
        let capacity_elapsed =
            sample_dormant_trait_guard(&capacity, &capacity_freeze, &capacity_config, Ok(()));

        let inventory_limit = Fixture::new("precommit-diagnostic-inventory-limit");
        let inventory_limit_config = inventory_limit.separated_config();
        inventory_limit.initialize_current_store();
        inventory_limit.populate_to_inventory_entries(MAX_ACCOUNTING_ENTRIES);
        let inventory_limit_freeze = inventory_limit.freeze();
        let inventory_limit_elapsed = sample_dormant_trait_guard(
            &inventory_limit,
            &inventory_limit_freeze,
            &inventory_limit_config,
            Err(CollectorIngestPrecommitError::Denied),
        );
        drop(inventory_limit_freeze);
        inventory_limit.populate_to_inventory_entries(MAX_ACCOUNTING_ENTRIES + 1);
        let over_limit_freeze = inventory_limit.freeze();
        let over_limit_elapsed = sample_dormant_trait_guard(
            &inventory_limit,
            &over_limit_freeze,
            &inventory_limit_config,
            Err(CollectorIngestPrecommitError::Unavailable),
        );

        println!(
            "baseline_ns={},{},{} capacity_ns={},{},{} inventory_limit_ns={},{},{} over_limit_ns={},{},{}",
            baseline_elapsed[0],
            baseline_elapsed[1],
            baseline_elapsed[2],
            capacity_elapsed[0],
            capacity_elapsed[1],
            capacity_elapsed[2],
            inventory_limit_elapsed[0],
            inventory_limit_elapsed[1],
            inventory_limit_elapsed[2],
            over_limit_elapsed[0],
            over_limit_elapsed[1],
            over_limit_elapsed[2],
        );
    }

    #[test]
    fn separated_precommit_rejects_revision_and_batch_mismatches_as_unavailable() {
        let fixture = Fixture::new("precommit-input-mismatch");
        let config = fixture.separated_config();
        let freeze = fixture.freeze();
        let mut stale = config.clone();
        stale.enabled = !stale.enabled;
        assert_eq!(
            fixture.check_precommit(&freeze, &stale, u64::from(stale.collection.max_batch_bytes)),
            Err(CollectorIngestPrecommitError::Unavailable)
        );
        assert_eq!(
            fixture.check_precommit(
                &freeze,
                &config,
                u64::from(config.collection.max_batch_bytes) + 1
            ),
            Err(CollectorIngestPrecommitError::Unavailable)
        );
        let mut invalid = config.clone();
        invalid.collection.max_batch_bytes = 0;
        assert_eq!(
            fixture.check_precommit(&freeze, &invalid, 0),
            Err(CollectorIngestPrecommitError::Unavailable)
        );
    }

    #[test]
    fn separated_precommit_denies_a_zero_byte_unknown_entry() {
        let fixture = Fixture::new("precommit-zero-byte-unknown");
        let config = fixture.separated_config();
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(fixture.layout.logs.join("unknown.empty"))
            .unwrap();
        let freeze = fixture.freeze();
        assert_eq!(
            fixture.check_precommit(
                &freeze,
                &config,
                u64::from(config.collection.max_batch_bytes)
            ),
            Err(CollectorIngestPrecommitError::Denied)
        );
    }

    #[test]
    fn separated_precommit_distinguishes_probe_errors_from_observed_zero_space() {
        let fixture = Fixture::new("precommit-probe-results");
        let config = fixture.separated_config();
        let freeze = fixture.freeze();
        let (codex, launch) = fixture.captures();
        macro_rules! check {
            ($available_space:expr, $estimated_allowance:expr $(,)?) => {
                check_cli_collector_ingest_precommit_with(
                    &fixture.layout,
                    &freeze,
                    &config,
                    u64::from(config.collection.max_batch_bytes),
                    &codex,
                    launch.as_ref(),
                    $available_space,
                    $estimated_allowance,
                )
            };
        }

        assert_eq!(
            check!(
                |_| {
                    Err(agent_observability_local_runtime::storage_coherence::StorageCoherenceError::Io(
                        std::io::ErrorKind::Other,
                    ))
                },
                |_, _| Ok(1),
            ),
            Err(CollectorIngestPrecommitError::Unavailable)
        );
        assert_eq!(
            check!(|_| Ok(0), |_, _| Ok(1)),
            Err(CollectorIngestPrecommitError::Denied)
        );
        assert_eq!(
            check!(|_| Ok(u64::MAX), |_, _| Err(())),
            Err(CollectorIngestPrecommitError::Unavailable)
        );
    }

    #[test]
    fn separated_precommit_counts_the_full_reservation_without_workspace_discount() {
        let mut config = LocalRuntimeConfigV3::default();
        config.storage_budget.mode = StorageBudgetMode::Separated;
        config.storage_budget.workspace_budget_bytes = 256 * 1_048_576;
        let observation = AllOwnerStorageObservation {
            allocation: StorageAllocationObservationV1 {
                retained_bytes: 1,
                workspace_bytes: 64 * 1_048_576,
                unknown_bytes: 0,
                unknown_entry_count: 0,
            },
            report_reserved_bytes: 128 * 1_048_576,
            config_revision: "revision".into(),
            storage_budget_policy: config.storage_budget.clone(),
        };
        assert_eq!(
            evaluate_collector_ingest_precommit(&observation, 65 * 1_048_576, u64::MAX),
            Err(CollectorIngestPrecommitError::Denied)
        );
    }

    #[test]
    fn separated_precommit_maps_missing_guard_config_and_store_evidence_to_unavailable() {
        let guard_fixture = Fixture::new("precommit-wrong-guard");
        let config_fixture = Fixture::new("precommit-missing-config");
        let config = config_fixture.separated_config();
        let wrong_freeze = guard_fixture.freeze();
        assert_eq!(
            config_fixture.check_precommit(
                &wrong_freeze,
                &config,
                u64::from(config.collection.max_batch_bytes)
            ),
            Err(CollectorIngestPrecommitError::Unavailable)
        );

        let freeze = config_fixture.freeze();
        fs::remove_file(&config_fixture.layout.config).unwrap();
        assert_eq!(
            config_fixture.check_precommit(
                &freeze,
                &config,
                u64::from(config.collection.max_batch_bytes)
            ),
            Err(CollectorIngestPrecommitError::Unavailable)
        );

        let store_fixture = Fixture::new("precommit-store-error");
        let store_config = store_fixture.separated_config();
        let store_directory = store_fixture.layout.state.join("store");
        drop(LocalStore::open(&store_directory).unwrap());
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(store_directory.join("local-store.sqlite3-journal"))
            .unwrap();
        let store_freeze = store_fixture.freeze();
        assert_eq!(
            store_fixture.check_precommit(
                &store_freeze,
                &store_config,
                u64::from(store_config.collection.max_batch_bytes)
            ),
            Err(CollectorIngestPrecommitError::Unavailable)
        );
    }

    #[test]
    fn allowed_numeric_result_cannot_escape_a_replaced_config_postcheck() {
        let fixture = Fixture::new("precommit-postcheck");
        let config = fixture.separated_config();
        let freeze = fixture.freeze();
        let (codex, launch) = fixture.captures();
        let result = check_cli_collector_ingest_precommit_with(
            &fixture.layout,
            &freeze,
            &config,
            u64::from(config.collection.max_batch_bytes),
            &codex,
            launch.as_ref(),
            OwnedStorageFreezeGuard::available_space,
            |root, max_batch_bytes| {
                let allowance = agent_observability_local_runtime::control::collector_ingest_estimated_allowance(
                    root,
                    max_batch_bytes,
                )
                .map_err(|_| ())?;
                let replacement = fixture.layout.root.join("replacement-config.json");
                fs::write(&replacement, fs::read(&fixture.layout.config).unwrap()).unwrap();
                fs::rename(replacement, &fixture.layout.config).unwrap();
                Ok(allowance)
            },
        );
        assert_eq!(result, Err(CollectorIngestPrecommitError::Unavailable));
    }
}
