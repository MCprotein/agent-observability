//! Runtime/store coordination at the automatic-report composition boundary.
//! This preserves legacy admission; it does not enable the separated budget policy.

use agent_observability_local_runtime::{
    MutationGuard,
    storage_coherence::{
        OwnedStorageFreezeGuard, StorageBarrier, StorageCoherenceError, StorageWriteGuard,
    },
};
use agent_observability_local_store::{
    ReportViewBuildError, ReportViewPermitFactory, ReportViewWritePhase,
};

pub(super) enum ReportMutationScope<'a> {
    Legacy(MutationGuard),
    Coordinated(OwnedStorageFreezeGuard<'a>),
}

impl<'a> ReportMutationScope<'a> {
    pub(super) fn acquire(
        mutation: MutationGuard,
        barrier: Option<&'a StorageBarrier>,
    ) -> Result<Self, super::ReportFailure> {
        match barrier {
            Some(barrier) => barrier
                .try_freeze_owned(mutation)
                .map(Self::Coordinated)
                .map_err(report_coherence_failure),
            None => Ok(Self::Legacy(mutation)),
        }
    }

    pub(super) fn mutation(&self) -> &MutationGuard {
        match self {
            Self::Legacy(mutation) => mutation,
            Self::Coordinated(scope) => scope.mutation(),
        }
    }

    pub(super) fn required_freeze(
        &self,
    ) -> Result<&OwnedStorageFreezeGuard<'a>, super::ReportFailure> {
        match self {
            Self::Legacy(_) => Err(super::ReportFailure::Publish),
            Self::Coordinated(freeze) => Ok(freeze),
        }
    }

    pub(super) fn revalidate(&self) -> Result<(), super::ReportFailure> {
        match self {
            Self::Legacy(_) => Ok(()),
            Self::Coordinated(scope) => scope.revalidate().map_err(report_coherence_failure),
        }
    }
}

pub(super) struct ReportWritePermits<'a> {
    pub(super) barrier: &'a StorageBarrier,
}

impl<'a> ReportViewPermitFactory for ReportWritePermits<'a> {
    type Permit = StorageWriteGuard<'a>;

    fn acquire(&mut self, _: ReportViewWritePhase) -> Result<Self::Permit, ReportViewBuildError> {
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

pub(super) const fn report_coherence_failure(error: StorageCoherenceError) -> super::ReportFailure {
    match error {
        StorageCoherenceError::Busy => super::ReportFailure::RenderGuard,
        _ => super::ReportFailure::Publish,
    }
}
