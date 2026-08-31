use crate::{
    Admission, CollectionPolicyV1, LocalRuntimeConfigV2, PressureSample, Schedule, Scheduler,
    StorageAccountingError, StorageBudget, StorageError,
};
use std::path::Path;

#[derive(Debug)]
pub struct RuntimeControl {
    policy: CollectionPolicyV1,
    storage: StorageBudget,
    scheduler: Scheduler,
}

impl RuntimeControl {
    pub fn new(config: &LocalRuntimeConfigV2) -> Result<Self, ControlError> {
        config.validate().map_err(ControlError::Config)?;
        let storage = StorageBudget::calculate(config.collection.local_storage_budget_bytes, false)
            .map_err(ControlError::Storage)?;
        Ok(Self {
            policy: config.collection.clone(),
            storage,
            scheduler: Scheduler::new(),
        })
    }

    pub fn admit(&self, root: &Path, worst_case_write: u64) -> Result<Admission, ControlError> {
        let allocated =
            StorageBudget::allocated_tree_bytes(root).map_err(ControlError::Accounting)?;
        Ok(self.storage.admit(allocated, worst_case_write))
    }

    pub fn migration_headroom(&self, root: &Path) -> Result<u64, ControlError> {
        let allocated =
            StorageBudget::allocated_tree_bytes(root).map_err(ControlError::Accounting)?;
        let budget_remaining = self.storage.total.saturating_sub(allocated);
        let filesystem_remaining = fs2::available_space(root)
            .map_err(StorageAccountingError::Io)
            .map_err(ControlError::Accounting)?;
        Ok(budget_remaining.min(filesystem_remaining))
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
}

impl std::fmt::Display for ControlError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(error) => error.fmt(formatter),
            Self::Storage(error) => error.fmt(formatter),
            Self::Accounting(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ControlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::Accounting(error) => Some(error),
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
        let mut control = RuntimeControl::new(&LocalRuntimeConfigV2::default()).unwrap();
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
        let control = RuntimeControl::new(&LocalRuntimeConfigV2::default()).unwrap();
        assert_eq!(
            control
                .admit(&root, control.storage_budget().total + 1)
                .unwrap(),
            Admission::Denied
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn migration_headroom_is_bounded_by_the_configured_budget() {
        let root =
            std::env::temp_dir().join(format!("runtime-migration-headroom-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let control = RuntimeControl::new(&LocalRuntimeConfigV2::default()).unwrap();
        let headroom = control.migration_headroom(&root).unwrap();
        assert!(headroom <= control.storage_budget().total);
        let _ = fs::remove_dir_all(root);
    }
}
