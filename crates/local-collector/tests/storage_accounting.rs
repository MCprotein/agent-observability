//! Synthetic cross-crate coordination proof, not installed-runtime acceptance.
#![cfg(unix)]

use agent_observability_adapter_codex::{AdapterItem, parse_handoff_jsonl};
use agent_observability_local_runtime::{
    MutationGuard, RuntimeControl, StorageAllocationClass, StorageInventoryError,
    config::ConfigAccountingEvidence,
    install, load,
    reservation::ReportReservationEvidence,
    storage_coherence::{StorageBarrier, StorageCoherenceError, StorageWriteGuard},
};
use agent_observability_local_store::{
    LocalStore, MAX_REPORT_VIEW_BYTES, MISSING_RATE_FINGERPRINT, ReportViewBuildError,
    ReportViewPermitFactory, ReportViewWritePhase, StoreBatchItem,
    build_report_view_staging_bound_coordinated, publish_report_view,
    storage_ownership::with_storage_ownership_observation, with_report_view_ownership_observation,
};
use std::fs;
use std::os::unix::fs::OpenOptionsExt;

struct RuntimePermits<'a> {
    barrier: &'a StorageBarrier,
    runtime: &'a std::path::Path,
    phases: usize,
}

impl<'a> ReportViewPermitFactory for RuntimePermits<'a> {
    type Permit = StorageWriteGuard<'a>;

    fn acquire(&mut self, _: ReportViewWritePhase) -> Result<Self::Permit, ReportViewBuildError> {
        // Every new write phase follows release of the previous permit. An
        // actual exclusive cut is possible here, without releasing render ownership.
        let mutation = MutationGuard::try_acquire(self.runtime).unwrap();
        let freeze = self.barrier.try_freeze_owned(mutation).unwrap();
        freeze.revalidate().unwrap();
        drop(freeze);
        self.phases += 1;
        self.barrier.try_begin_write().map_err(|error| match error {
            StorageCoherenceError::Busy => ReportViewBuildError::Busy,
            _ => ReportViewBuildError::CoordinationDenied,
        })
    }

    fn revalidate(&mut self, permit: &Self::Permit) -> Result<(), ReportViewBuildError> {
        permit
            .revalidate()
            .map_err(|_| ReportViewBuildError::CoordinationDenied)
    }
}

#[test]
fn three_synthetic_generations_preserve_reservations_and_classify_under_real_freeze() {
    let root = std::env::temp_dir().join(format!(
        "agentobs-storage-composition-{}",
        std::process::id()
    ));
    assert!(
        !root.exists(),
        "fixture must not overwrite an existing directory"
    );
    let layout = install(&root).unwrap();
    let root = layout.root.clone();
    let config = load(&layout.config).unwrap();
    let control = RuntimeControl::new(&config).unwrap();
    let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
    let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
    let setup_freeze = barrier.try_freeze(&mutation).unwrap();
    let mut store = LocalStore::open(layout.state.join("store")).unwrap();
    // Even a zero-allocation file needs semantic ownership; its name never suffices.
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(layout.logs.join("unowned.control"))
        .unwrap();
    drop(setup_freeze);
    drop(mutation);
    let mut previous_view = None;
    for generation in 0..3 {
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let mut initial_freeze = Some(barrier.try_freeze_owned(mutation).unwrap());
        ingest_generation(&mut store, generation);
        let mut reservation = control
            .reserve_report_build(
                &root,
                initial_freeze.as_ref().unwrap().mutation(),
                MAX_REPORT_VIEW_BYTES,
            )
            .unwrap();
        let mut permits = RuntimePermits {
            barrier: &barrier,
            runtime: &layout.runtime,
            phases: 0,
        };
        let staging = build_report_view_staging_bound_coordinated(
            &store,
            MISSING_RATE_FINGERPRINT,
            MAX_REPORT_VIEW_BYTES,
            None,
            |path, file| {
                reservation
                    .bind_staging(
                        &root,
                        initial_freeze.as_ref().unwrap().mutation(),
                        path,
                        file,
                    )
                    .unwrap();
                drop(initial_freeze.take());
                Ok(())
            },
            &mut permits,
            |_| {
                let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
                assert!(matches!(
                    barrier.try_freeze(&mutation),
                    Err(StorageCoherenceError::Busy)
                ));
            },
        )
        .unwrap();
        assert!(permits.phases >= 3);
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let freeze = barrier.try_freeze(&mutation).unwrap();
        assert_accounting(&root, &mutation, &freeze, &store);
        reservation
            .validate_staging(&root, &mutation, staging.path(), staging.identity_file())
            .unwrap();
        let publication = publish_report_view(&store, staging).unwrap();
        assert_eq!(
            publication.retired().map(|view| view.view_id().to_owned()),
            previous_view
        );
        previous_view = Some(publication.current().view_id().to_owned());
        reservation.release(&root, &mutation).unwrap();
        assert!(!root.join("runtime/report-reservation.meta").exists());
        freeze.revalidate().unwrap();
        drop(freeze);
    }
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

fn ingest_generation(store: &mut LocalStore, generation: u32) {
    let input = include_str!("../../../examples/codex-handoff.v1.jsonl")
        .replace("example-v1.5.0", &format!("accounting-{generation}"))
        .replace(
            "example-conversation",
            &format!("accounting-session-{generation}"),
        );
    let batch = parse_handoff_jsonl(&input).unwrap();
    let items = batch
        .items
        .iter()
        .map(|item| match item {
            AdapterItem::Observation(observation) => StoreBatchItem::Observation(observation),
            AdapterItem::Disposition(diagnostic) => StoreBatchItem::Disposition {
                checkpoint: &diagnostic.checkpoint,
                disposition: diagnostic.disposition,
                code: diagnostic.code,
                canonical_payload_hash: diagnostic.payload_hash.as_deref(),
            },
        })
        .collect::<Vec<_>>();
    store
        .ingest_ordered_batch_deferred_projection(&items)
        .unwrap();
}

fn assert_accounting(
    root: &std::path::Path,
    mutation: &MutationGuard,
    freeze: &agent_observability_local_runtime::storage_coherence::StorageFreezeGuard<'_, '_>,
    store: &LocalStore,
) {
    let config_evidence = ConfigAccountingEvidence::capture(root, mutation).unwrap();
    let reservation_evidence = ReportReservationEvidence::capture(root, mutation).unwrap();
    assert_eq!(
        reservation_evidence.captured_reserved_bytes(),
        MAX_REPORT_VIEW_BYTES
    );
    assert!(store.try_acquire_report_render_guard().unwrap().is_none());
    let mutation_path = std::path::Path::new("runtime/mutation.lock");
    assert!(
        mutation
            .matches_accounting_lock(
                root,
                mutation_path,
                &fs::File::open(root.join(mutation_path)).unwrap()
            )
            .unwrap()
    );
    let accounting_path = std::path::Path::new("runtime/storage-accounting.lock");
    assert!(
        freeze
            .matches_accounting_lock(
                accounting_path,
                &fs::File::open(root.join(accounting_path)).unwrap()
            )
            .unwrap()
    );
    let mut unknown = std::collections::BTreeSet::new();
    let allocation = with_storage_ownership_observation(store, |authority| {
        with_report_view_ownership_observation(store, |published| {
            freeze.classify(|relative, file| {
                let path = root.join(relative);
                if config_evidence
                    .matches_entry(relative, file)
                    .map_err(|_| StorageInventoryError::OwnershipMismatch)?
                    || mutation
                        .matches_accounting_lock(root, relative, file)
                        .map_err(|_| StorageInventoryError::OwnershipMismatch)?
                    || freeze
                        .matches_accounting_lock(relative, file)
                        .map_err(|_| StorageInventoryError::OwnershipMismatch)?
                    || reservation_evidence
                        .matches_control_entry(relative, file)
                        .map_err(|_| StorageInventoryError::OwnershipMismatch)?
                    || published
                        .recognizes(&path, file)
                        .map_err(|_| StorageInventoryError::OwnershipMismatch)?
                    || authority
                        .recognizes(&path, file)
                        .map_err(|_| StorageInventoryError::OwnershipMismatch)?
                {
                    return Ok(StorageAllocationClass::Retained);
                }
                if reservation_evidence
                    .matches_staging(&path, file)
                    .map_err(|_| StorageInventoryError::OwnershipMismatch)?
                {
                    return Ok(StorageAllocationClass::Workspace);
                }
                unknown.insert(relative.to_path_buf());
                Ok(StorageAllocationClass::Unknown)
            })
        })
    })
    .unwrap()
    .unwrap()
    .unwrap();
    assert!(allocation.retained_bytes > 0);
    assert!(allocation.workspace_bytes > 0);
    let expected_unknown = ["logs/unowned.control"]
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unknown, expected_unknown);
    assert_eq!(allocation.unknown_entry_count, 1);
    assert_eq!(allocation.unknown_bytes, 0);
    config_evidence.revalidate().unwrap();
    reservation_evidence.revalidate().unwrap();
    assert_eq!(
        reservation_evidence.captured_reserved_bytes(),
        MAX_REPORT_VIEW_BYTES
    );
}
