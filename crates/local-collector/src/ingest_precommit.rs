//! Optional collector-owned check at the existing ingest commit boundary.
//!
//! This module provides opt-in plumbing, not an operational storage policy.
//! The default `serve` entrypoint and current CLI do not install a guard.
//! Supplying a guard does not activate separated admission or replace existing checks.

use agent_observability_local_runtime::{
    InstalledLayout, LocalRuntimeConfigV3, storage_coherence::OwnedStorageFreezeGuard,
};

/// Content-free outcomes; neither variant carries paths, payloads, or diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectorIngestPrecommitError {
    Denied,
    Unavailable,
}

impl std::fmt::Display for CollectorIngestPrecommitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Denied => "collector ingest precommit denied",
            Self::Unavailable => "collector ingest precommit unavailable",
        })
    }
}

impl std::error::Error for CollectorIngestPrecommitError {}

/// Additional synchronous admission under the caller's retained exclusive freeze.
/// Used only through `serve_with_ingest_precommit_guard`; no implementation is enabled by default.
/// Implementations must not reacquire root/accounting locks or mutate storage.
/// No payload, correlation state, or write-capable store is supplied.
pub trait CollectorIngestPrecommitGuard: Send + Sync + std::fmt::Debug {
    fn check_precommit(
        &self,
        layout: &InstalledLayout,
        freeze: &OwnedStorageFreezeGuard<'_>,
        config: &LocalRuntimeConfigV3,
        max_batch_bytes: u64,
    ) -> Result<(), CollectorIngestPrecommitError>;
}
