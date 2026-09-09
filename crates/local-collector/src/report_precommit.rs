//! Dormant, optional checks at automatic-report start and publication boundaries.
//! No service, scheduler, or CLI entrypoint installs this port. It adds admission
//! conditions without replacing legacy checks or enabling separated policy.

use agent_observability_local_runtime::{
    InstalledLayout, LocalRuntimeConfigV3, storage_coherence::OwnedStorageFreezeGuard,
};

/// Content-free outcomes, without paths or diagnostic payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectorReportPrecommitError {
    Denied,
    Unavailable,
}

impl std::fmt::Display for CollectorReportPrecommitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Denied => "collector report precommit denied",
            Self::Unavailable => "collector report precommit unavailable",
        })
    }
}

impl std::error::Error for CollectorReportPrecommitError {}

/// Optional synchronous checks under the caller's retained exclusive freeze.
/// Implementations must not mutate storage or reacquire root/accounting locks.
/// No write-capable store, staging handle, or reservation owner is supplied.
/// The default report path does not use this port.
pub trait CollectorReportPrecommitGuard: Send + Sync + std::fmt::Debug {
    /// Checks before recovery or reservation creation, with the full checked
    /// build ceiling plus publication and reservation-metadata allowance.
    fn check_start(
        &self,
        layout: &InstalledLayout,
        freeze: &OwnedStorageFreezeGuard<'_>,
        config: &LocalRuntimeConfigV3,
        estimated_allowance: u64,
    ) -> Result<(), CollectorReportPrecommitError>;

    /// Checks immediately before publication using the latest configuration.
    /// Only the validated self promise may be subtracted from observed full R,
    /// with checked subtraction; actual staging allocation must remain counted.
    /// `publication_allowance` is the collector's remaining publication estimate.
    fn check_publication(
        &self,
        layout: &InstalledLayout,
        freeze: &OwnedStorageFreezeGuard<'_>,
        config: &LocalRuntimeConfigV3,
        publication_allowance: u64,
        validated_self_reservation_bytes: u64,
    ) -> Result<(), CollectorReportPrecommitError>;
}
