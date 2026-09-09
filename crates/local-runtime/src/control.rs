use crate::{
    Admission, CollectionPolicyV1, LocalRuntimeConfigV3, MutationGuard, PressureSample,
    ReservationError, Schedule, Scheduler, StorageAccountingError, StorageBudget, StorageError,
    WriteReservation,
};
use std::path::Path;

#[derive(Debug)]
pub struct RuntimeControl {
    policy: CollectionPolicyV1,
    storage: StorageBudget,
    scheduler: Scheduler,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CollectorDiagnostic {
    pub existing_store_allocated_bytes: u64,
    pub max_batch_bytes: u64,
    pub current_report_reserved_bytes: u64,
    pub collector_reservation_bytes: u64,
    pub writable_headroom_bytes: u64,
    pub deficit_bytes: u64,
    pub admission: Admission,
}

/// Estimates the existing conservative full-store ingest allowance without mode checks.
/// This noncreating observation makes no admission decision. Callers enforcing admission
/// must retain matching root mutation ownership and, when accounting is initialized,
/// an exclusive all-writer accounting freeze through collection of all snapshot inputs,
/// evaluation, and commit or rollback. Root mutation ownership alone is not coherent.
pub fn collector_ingest_estimated_allowance(
    root: &Path,
    max_batch_bytes: u64,
) -> Result<u64, ControlError> {
    collector_ingest_allowance_parts(root, max_batch_bytes).map(|(_, allowance)| allowance)
}

fn collector_ingest_allowance_parts(
    root: &Path,
    max_batch_bytes: u64,
) -> Result<(u64, u64), ControlError> {
    let store = root.join("state/store");
    let existing_store_allocated_bytes = match std::fs::symlink_metadata(&store) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => {
            return Err(ControlError::Accounting(StorageAccountingError::Io(error)));
        }
        Ok(_) => {
            StorageBudget::allocated_tree_bytes_strict(&store).map_err(ControlError::Accounting)?
        }
    };
    let allowance = existing_store_allocated_bytes
        .checked_add(max_batch_bytes)
        .ok_or(ControlError::CollectorAdmissionOverflow)?;
    Ok((existing_store_allocated_bytes, allowance))
}

impl RuntimeControl {
    pub fn new(config: &LocalRuntimeConfigV3) -> Result<Self, ControlError> {
        config.validate().map_err(ControlError::Config)?;
        config
            .require_operational_storage_policy()
            .map_err(ControlError::Config)?;
        let storage = StorageBudget::calculate(config.collection.local_storage_budget_bytes, false)
            .map_err(ControlError::Storage)?;
        Ok(Self {
            policy: config.collection.clone(),
            storage,
            scheduler: Scheduler::new(),
        })
    }

    pub fn admit(&self, root: &Path, worst_case_write: u64) -> Result<Admission, ControlError> {
        if worst_case_write > self.writable_headroom(root)? {
            Ok(Admission::Denied)
        } else {
            Ok(Admission::Allowed {
                reserved: worst_case_write,
            })
        }
    }

    /// Reports the collector's existing conservative full-store reservation
    /// without changing the admission policy or creating runtime state.
    ///
    /// Callers using this assessment to enforce admission must retain the matching
    /// root's [`MutationGuard`] and, when accounting is initialized, an exclusive
    /// all-writer accounting freeze through collection of all snapshot inputs,
    /// evaluation, and commit or rollback. Root mutation ownership alone is not
    /// coherent. Observation-only callers may accept a non-coherent snapshot.
    pub fn collector_admission_diagnostic(
        &self,
        root: &Path,
        max_batch_bytes: u64,
    ) -> Result<CollectorDiagnostic, ControlError> {
        let (existing_store_allocated_bytes, collector_reservation_bytes) =
            collector_ingest_allowance_parts(root, max_batch_bytes)?;
        let current_report_reserved_bytes =
            crate::reservation::reserved_bytes(root).map_err(ControlError::Reservation)?;
        let allocated =
            StorageBudget::allocated_tree_bytes_strict(root).map_err(ControlError::Accounting)?;
        let writable_headroom_bytes = Self::headroom_from_allocated(
            root,
            self.storage.writable_limit(),
            current_report_reserved_bytes,
            allocated,
        )?;
        let admission = if collector_reservation_bytes > writable_headroom_bytes {
            Admission::Denied
        } else {
            Admission::Allowed {
                reserved: collector_reservation_bytes,
            }
        };
        let deficit_bytes = collector_reservation_bytes.saturating_sub(writable_headroom_bytes);
        Ok(CollectorDiagnostic {
            existing_store_allocated_bytes,
            max_batch_bytes,
            current_report_reserved_bytes,
            collector_reservation_bytes,
            writable_headroom_bytes,
            deficit_bytes,
            admission,
        })
    }

    /// Includes actual files plus the full active or stale report reservation.
    pub fn writable_headroom(&self, root: &Path) -> Result<u64, ControlError> {
        let reserved =
            crate::reservation::reserved_bytes(root).map_err(ControlError::Reservation)?;
        Self::headroom(root, self.storage.writable_limit(), reserved)
    }

    pub fn migration_headroom(&self, root: &Path) -> Result<u64, ControlError> {
        let reserved =
            crate::reservation::reserved_bytes(root).map_err(ControlError::Reservation)?;
        Self::headroom(root, self.storage.total, reserved)
    }

    fn headroom(root: &Path, limit: u64, reserved: u64) -> Result<u64, ControlError> {
        let allocated =
            StorageBudget::allocated_tree_bytes(root).map_err(ControlError::Accounting)?;
        Self::headroom_from_allocated(root, limit, reserved, allocated)
    }

    fn headroom_from_allocated(
        root: &Path,
        limit: u64,
        reserved: u64,
        allocated: u64,
    ) -> Result<u64, ControlError> {
        let committed = allocated
            .checked_add(reserved)
            .ok_or(ControlError::Accounting(StorageAccountingError::Overflow))?;
        let filesystem_remaining = fs2::available_space(root)
            .map_err(StorageAccountingError::Io)
            .map_err(ControlError::Accounting)?;
        Ok(limit
            .saturating_sub(committed)
            .min(filesystem_remaining.saturating_sub(reserved)))
    }

    /// Acquires only the lifetime lock; the caller can immediately drop the
    /// mutation guard while building. The ceiling includes build/journal and
    /// publication writes; reservation metadata is separately accounted.
    pub fn reserve_report_build(
        &self,
        root: &Path,
        guard: &MutationGuard,
        byte_ceiling: u64,
    ) -> Result<WriteReservation, ControlError> {
        guard
            .require_root(root)
            .map_err(ReservationError::from)
            .map_err(ControlError::Reservation)?;
        let available = self.writable_headroom(root)?;
        if byte_ceiling == 0
            || byte_ceiling
                .checked_add(crate::REPORT_RESERVATION_METADATA_ALLOWANCE)
                .is_none_or(|required| required > available)
        {
            return Err(ControlError::Reservation(ReservationError::Capacity));
        }
        let reservation = WriteReservation::acquire(root, guard, byte_ceiling)
            .map_err(ControlError::Reservation)?;
        // Count the newly allocated metadata too, before exposing an owner.
        if byte_ceiling > Self::headroom(root, self.storage.writable_limit(), 0)? {
            reservation
                .release(root, guard)
                .map_err(ControlError::Reservation)?;
            return Err(ControlError::Reservation(ReservationError::Capacity));
        }
        Ok(reservation)
    }

    /// Excludes only this validated owner's promise, never its actual staging
    /// allocation. Intended for the short final publication/cleanup boundary.
    pub fn reservation_finalization_headroom(
        &self,
        root: &Path,
        guard: &MutationGuard,
        reservation: &WriteReservation,
    ) -> Result<u64, ControlError> {
        let byte_ceiling = self.validated_report_reservation_bytes(root, guard, reservation)?;
        Ok(Self::headroom(root, self.storage.writable_limit(), 0)?.min(byte_ceiling))
    }

    /// Returns only this exact owner's ceiling after validating its root, guard,
    /// lifetime lock, and current metadata.
    pub fn validated_report_reservation_bytes(
        &self,
        root: &Path,
        guard: &MutationGuard,
        reservation: &WriteReservation,
    ) -> Result<u64, ControlError> {
        reservation
            .validate_owner(root, guard)
            .map_err(ControlError::Reservation)?;
        Ok(reservation.byte_ceiling())
    }

    /// Claim stale metadata before cleaning interrupted staging. The returned
    /// owner keeps the lifetime lock; clean under the mutation guard, then
    /// release it explicitly. Failed cleanup leaves the promise intact.
    pub fn claim_stale_report_reservation(
        &self,
        root: &Path,
        guard: &MutationGuard,
    ) -> Result<Option<WriteReservation>, ControlError> {
        crate::reservation::recover(root, guard).map_err(ControlError::Reservation)
    }

    pub fn evaluate(&mut self, now_ms: u64, sample: PressureSample) -> Schedule {
        self.scheduler
            .evaluate(now_ms, sample, self.policy.file_reconcile_interval_ms)
    }

    pub fn storage_percent(&self, allocated_bytes: u64) -> u8 {
        let bounded_percent = allocated_bytes
            .saturating_mul(100)
            .checked_div(self.storage.total)
            .unwrap_or(u64::MAX)
            .min(u64::from(u8::MAX));
        u8::try_from(bounded_percent).unwrap_or(u8::MAX)
    }

    pub fn storage_budget(&self) -> StorageBudget {
        self.storage
    }
}

#[derive(Debug)]
pub enum ControlError {
    Config(crate::ConfigError),
    Storage(StorageError),
    Accounting(StorageAccountingError),
    Reservation(ReservationError),
    CollectorAdmissionOverflow,
}

impl std::fmt::Display for ControlError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(error) => error.fmt(formatter),
            Self::Storage(error) => error.fmt(formatter),
            Self::Accounting(error) => error.fmt(formatter),
            Self::Reservation(error) => error.fmt(formatter),
            Self::CollectorAdmissionOverflow => {
                formatter.write_str("collector admission reservation overflow")
            }
        }
    }
}

impl std::error::Error for ControlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::Accounting(error) => Some(error),
            Self::Reservation(error) => Some(error),
            Self::CollectorAdmissionOverflow => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::State;
    use std::fs;

    #[test]
    fn configured_budget_and_scheduler_share_one_control() {
        let root = std::env::temp_dir().join(format!("runtime-control-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
        assert!(matches!(
            control.admit(&root, 1).unwrap(),
            Admission::Allowed { .. }
        ));
        assert_eq!(
            control
                .evaluate(
                    0,
                    PressureSample {
                        resource_percent: 0,
                        disk_percent: 90,
                        queue_percent: 0,
                    },
                )
                .state,
            State::Protected
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn admission_denies_a_worst_case_write_over_the_hard_cap() {
        let root =
            std::env::temp_dir().join(format!("runtime-control-denied-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
        assert_eq!(
            control
                .admit(&root, control.storage_budget().total + 1)
                .unwrap(),
            Admission::Denied
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_partitions_ignore_but_preserve_inactive_separated_values() {
        let mut config = LocalRuntimeConfigV3::default();
        let expected = RuntimeControl::new(&config).unwrap().storage_budget();
        config.storage_budget.retained_target_bytes = 268_435_456;
        config.storage_budget.workspace_budget_bytes = 21_474_836_480;
        config.storage_budget.minimum_free_bytes = 536_870_912;
        assert_eq!(
            RuntimeControl::new(&config).unwrap().storage_budget(),
            expected
        );
        assert_eq!(config.storage_budget.workspace_budget_bytes, 21_474_836_480);
    }

    #[test]
    fn collector_estimated_allowance_matches_existing_diagnostic() {
        let root = std::env::temp_dir().join(format!(
            "collector-estimate-existing-{}",
            std::process::id()
        ));
        let layout = crate::install(&root).unwrap();
        std::fs::create_dir(layout.state.join("store")).unwrap();
        std::fs::write(layout.state.join("store/data"), vec![1_u8; 8192]).unwrap();
        let control = RuntimeControl::new(&crate::load(&layout.config).unwrap()).unwrap();
        let diagnostic = control.collector_admission_diagnostic(&root, 4096).unwrap();
        assert!(diagnostic.existing_store_allocated_bytes > 0);
        assert_eq!(
            super::collector_ingest_estimated_allowance(&root, 4096).unwrap(),
            diagnostic.collector_reservation_bytes
        );
        assert_eq!(
            diagnostic.collector_reservation_bytes,
            diagnostic.existing_store_allocated_bytes + 4096
        );
        assert!(matches!(
            super::collector_ingest_estimated_allowance(&root, u64::MAX),
            Err(super::ControlError::CollectorAdmissionOverflow)
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn collector_estimated_allowance_preserves_absence_without_creation() {
        let root =
            std::env::temp_dir().join(format!("collector-estimate-absent-{}", std::process::id()));
        assert!(!root.exists());
        assert_eq!(
            super::collector_ingest_estimated_allowance(&root, 4096).unwrap(),
            4096
        );
        assert!(!root.exists());
        let layout = crate::install(&root).unwrap();
        assert_eq!(
            super::collector_ingest_estimated_allowance(&root, u64::MAX).unwrap(),
            u64::MAX
        );
        assert!(!layout.state.join("store").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn collector_estimated_allowance_preserves_unsafe_accounting_errors() {
        let root = std::env::temp_dir().join(format!("ce-unsafe-{}", std::process::id()));
        let layout = crate::install(&root).unwrap();
        std::os::unix::fs::symlink(root.join("absent"), layout.state.join("store")).unwrap();
        assert!(matches!(
            super::collector_ingest_estimated_allowance(&root, 4096),
            Err(super::ControlError::Accounting(
                crate::StorageAccountingError::Symlink
            ))
        ));
        std::fs::remove_file(layout.state.join("store")).unwrap();
        std::fs::create_dir(layout.state.join("store")).unwrap();
        std::os::unix::fs::symlink(root.join("absent"), layout.state.join("store/unsafe")).unwrap();
        assert!(matches!(
            super::collector_ingest_estimated_allowance(&root, 4096),
            Err(super::ControlError::Accounting(
                crate::StorageAccountingError::Symlink
            ))
        ));
        std::fs::remove_file(layout.state.join("store/unsafe")).unwrap();
        let socket =
            std::os::unix::net::UnixListener::bind(layout.state.join("store/unsafe")).unwrap();
        assert!(matches!(
            super::collector_ingest_estimated_allowance(&root, 4096),
            Err(super::ControlError::Accounting(
                crate::StorageAccountingError::UnsupportedFileType
            ))
        ));
        drop(socket);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn collector_admission_keeps_the_full_existing_store_reservation() {
        let root = std::env::temp_dir().join(format!(
            "runtime-collector-full-store-admission-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("state/store")).unwrap();
        fs::write(root.join("state/store/data"), [0x5a; 4096]).unwrap();
        let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
        let existing_store =
            StorageBudget::allocated_tree_bytes(&root.join("state/store")).unwrap();
        let available = control.writable_headroom(&root).unwrap();
        let max_batch_bytes = available - existing_store + 1;

        assert!(matches!(
            control.admit(&root, max_batch_bytes).unwrap(),
            Admission::Allowed { .. }
        ));
        let diagnostic = control
            .collector_admission_diagnostic(&root, max_batch_bytes)
            .unwrap();
        assert_eq!(diagnostic.existing_store_allocated_bytes, existing_store);
        assert_eq!(
            diagnostic.collector_reservation_bytes,
            existing_store + max_batch_bytes
        );
        assert_eq!(diagnostic.deficit_bytes, 1);
        assert_eq!(diagnostic.admission, Admission::Denied);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn collector_admission_accounts_for_current_report_reservation() {
        let root = std::env::temp_dir().join(format!(
            "runtime-collector-reserved-admission-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
        let before = control.collector_admission_diagnostic(&root, 1).unwrap();
        let max_batch_bytes = before.writable_headroom_bytes;
        assert_eq!(
            control
                .collector_admission_diagnostic(&root, max_batch_bytes)
                .unwrap()
                .admission,
            Admission::Allowed {
                reserved: max_batch_bytes
            }
        );
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let reservation = control.reserve_report_build(&root, &guard, 8192).unwrap();

        let diagnostic = control
            .collector_admission_diagnostic(&root, max_batch_bytes)
            .unwrap();
        assert_eq!(diagnostic.existing_store_allocated_bytes, 0);
        assert_eq!(diagnostic.current_report_reserved_bytes, 8192);
        assert_eq!(diagnostic.collector_reservation_bytes, max_batch_bytes);
        assert!(diagnostic.deficit_bytes >= 8192);
        assert_eq!(diagnostic.admission, Admission::Denied);

        reservation.release(&root, &guard).unwrap();
        drop(guard);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn collector_admission_rejects_unknown_reservation_metadata() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "runtime-collector-unknown-reservation-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        drop(guard);
        let metadata = root.join("runtime/report-reservation.meta");
        fs::write(&metadata, br#"{"version":2,"kind":"unknown"}"#).unwrap();
        fs::set_permissions(&metadata, fs::Permissions::from_mode(0o600)).unwrap();
        let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();

        assert!(matches!(
            control.collector_admission_diagnostic(&root, 4096),
            Err(ControlError::Reservation(ReservationError::Corrupt))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn collector_admission_treats_a_missing_store_as_zero() {
        let root = std::env::temp_dir().join(format!(
            "runtime-collector-missing-store-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();

        let diagnostic = control.collector_admission_diagnostic(&root, 4096).unwrap();
        assert_eq!(diagnostic.existing_store_allocated_bytes, 0);
        assert_eq!(diagnostic.max_batch_bytes, 4096);
        assert_eq!(diagnostic.collector_reservation_bytes, 4096);
        assert_eq!(diagnostic.deficit_bytes, 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn collector_admission_preserves_overflow_and_unsafe_accounting_failures() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "runtime-collector-admission-errors-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("state/store")).unwrap();
        fs::write(root.join("state/store/data"), [0x5a; 4096]).unwrap();
        let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
        assert!(matches!(
            control.collector_admission_diagnostic(&root, u64::MAX),
            Err(ControlError::CollectorAdmissionOverflow)
        ));

        fs::remove_dir_all(root.join("state/store")).unwrap();
        symlink(root.join("unowned"), root.join("state/store")).unwrap();
        assert!(matches!(
            control.collector_admission_diagnostic(&root, 4096),
            Err(ControlError::Accounting(StorageAccountingError::Symlink))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn writable_headroom_counts_current_retired_staging_catalog_and_journal() {
        let root =
            std::env::temp_dir().join(format!("runtime-snapshot-headroom-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        for name in [
            "authority",
            "current",
            "retired",
            "staging",
            "staging-journal",
            ".catalog.tmp",
        ] {
            fs::write(root.join(name), [0x5a; 4096]).unwrap();
        }
        let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
        let allocated = StorageBudget::allocated_tree_bytes(&root).unwrap();
        let available = control.writable_headroom(&root).unwrap();
        assert!(available + allocated <= control.storage_budget().writable_limit());
        assert!(
            available + allocated + control.storage_budget().headroom.bytes
                <= control.storage_budget().total
        );
        assert!(available <= control.migration_headroom(&root).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_headroom_is_bounded_by_the_configured_budget() {
        let root =
            std::env::temp_dir().join(format!("runtime-migration-headroom-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
        let headroom = control.migration_headroom(&root).unwrap();
        assert!(headroom <= control.storage_budget().total);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn validated_report_reservation_returns_exact_owner_ceiling_and_preserves_headroom() {
        let root = std::env::temp_dir().join(format!(
            "runtime-validated-reservation-{}",
            std::process::id()
        ));
        let layout = crate::install(&root).unwrap();
        let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
        let guard = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let ceiling = 8192;
        let reservation = control
            .reserve_report_build(&root, &guard, ceiling)
            .unwrap();

        assert_eq!(
            control
                .validated_report_reservation_bytes(&root, &guard, &reservation)
                .unwrap(),
            ceiling
        );
        let expected = RuntimeControl::headroom(&root, control.storage.writable_limit(), 0)
            .unwrap()
            .min(ceiling);
        assert_eq!(
            control
                .reservation_finalization_headroom(&root, &guard, &reservation)
                .unwrap(),
            expected
        );

        reservation.release(&root, &guard).unwrap();
        drop(guard);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validated_report_reservation_rejects_wrong_root_and_guard() {
        let root = std::env::temp_dir().join(format!(
            "runtime-validated-reservation-owner-{}",
            std::process::id()
        ));
        let other = std::env::temp_dir().join(format!(
            "runtime-validated-reservation-other-{}",
            std::process::id()
        ));
        let layout = crate::install(&root).unwrap();
        let other_layout = crate::install(&other).unwrap();
        let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
        let guard = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let wrong_guard = MutationGuard::try_acquire(&other_layout.runtime).unwrap();
        let reservation = control.reserve_report_build(&root, &guard, 8192).unwrap();

        assert!(matches!(
            control.validated_report_reservation_bytes(&root, &wrong_guard, &reservation),
            Err(ControlError::Reservation(ReservationError::Lock(_)))
        ));
        assert!(matches!(
            control.validated_report_reservation_bytes(&other, &guard, &reservation),
            Err(ControlError::Reservation(ReservationError::Lock(_)))
        ));

        reservation.release(&root, &guard).unwrap();
        drop((guard, wrong_guard));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(other).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn validated_report_reservation_rejects_replaced_metadata() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "runtime-validated-reservation-replaced-{}",
            std::process::id()
        ));
        let layout = crate::install(&root).unwrap();
        let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
        let guard = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let reservation = control.reserve_report_build(&root, &guard, 8192).unwrap();
        let metadata = layout.runtime.join("report-reservation.meta");
        let original = fs::read(&metadata).unwrap();
        let replacement = layout.runtime.join("replacement-reservation.meta");
        fs::write(&replacement, b"replaced").unwrap();
        fs::set_permissions(&replacement, fs::Permissions::from_mode(0o600)).unwrap();
        fs::rename(&replacement, &metadata).unwrap();

        assert!(matches!(
            control.validated_report_reservation_bytes(&root, &guard, &reservation),
            Err(ControlError::Reservation(ReservationError::Corrupt))
        ));

        fs::write(&metadata, original).unwrap();
        reservation.release(&root, &guard).unwrap();
        drop(guard);
        fs::remove_dir_all(root).unwrap();
    }
}
