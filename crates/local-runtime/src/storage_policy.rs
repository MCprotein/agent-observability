//! Pure P2 arithmetic, not an operational write permit or filesystem quota.
//!
//! Callers must separately establish coherent accounting, file ownership, the current
//! config revision and the matching mutation guard. This module performs no I/O and
//! cannot authenticate those facts. The separated-mode operational gate remains closed.
use crate::policy::{StorageBudgetMode, StorageBudgetPolicyV1};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Numeric inputs only; constructing this value does not validate a filesystem snapshot.
pub struct StorageAllocationSnapshotV1 {
    pub retained_bytes: u64,
    pub workspace_bytes: u64,
    pub unknown_bytes: u64,
    pub unknown_entry_count: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// Full outstanding promises, without subtracting already allocated staging bytes.
/// An active promise that becomes stale moves categories; it is not counted twice.
/// Excluding an owner's promise requires validation outside this pure function.
pub struct StorageReservationSnapshotV1 {
    pub active_bytes: u64,
    pub stale_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageOperation {
    Ingest,
    Import,
    PrivateCapture,
    Report,
    Cleanup,
    Recovery,
    Migration,
}

impl StorageOperation {
    const fn requires_retention_headroom(self) -> bool {
        matches!(self, Self::Ingest | Self::Import | Self::PrivateCapture)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageAdmissionArithmetic {
    TotalAllocated,
    Reservations,
    WorkspaceRequired,
    FilesystemRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageAdmissionRejection {
    InvalidPolicy,
    LegacyMode,
    EstimateUnavailable,
    UnknownStorage {
        bytes: u64,
        entry_count: u64,
    },
    ArithmeticOverflow(StorageAdmissionArithmetic),
    WorkspaceOverrun {
        workspace_bytes: u64,
        budget_bytes: u64,
    },
    WorkspaceInsufficient {
        required_bytes: u64,
        budget_bytes: u64,
    },
    FilesystemInsufficient {
        required_bytes: u64,
        available_bytes: u64,
    },
    RetentionTargetReached {
        retained_bytes: u64,
        target_bytes: u64,
    },
}

/// Returns the supplied allowance if all numeric admission conditions hold.
///
/// `None` is an unavailable estimate, never a zero-byte fallback. `Some(0)` is
/// meaningful only for an operation independently proved to require no extra space.
/// Failures contain numeric policy facts only, never paths or source content.
/// This does not release reservations, authorize cleanup, or interrupt a transaction.
pub fn evaluate_storage_admission(
    policy: &StorageBudgetPolicyV1,
    allocation: StorageAllocationSnapshotV1,
    reservations: StorageReservationSnapshotV1,
    estimated_allowance_bytes: Option<u64>,
    filesystem_free_bytes: u64,
    operation: StorageOperation,
) -> Result<u64, StorageAdmissionRejection> {
    let reserved_total = reservations
        .active_bytes
        .checked_add(reservations.stale_bytes)
        .ok_or(StorageAdmissionRejection::ArithmeticOverflow(
            StorageAdmissionArithmetic::Reservations,
        ));
    evaluate_storage_admission_inner(
        policy,
        allocation,
        reserved_total,
        estimated_allowance_bytes,
        filesystem_free_bytes,
        operation,
    )
}

/// Evaluates admission using the caller-validated full outstanding reservation total.
///
/// This does not infer active/stale ownership or liveness, validate snapshot coherence,
/// or grant write authority. Already allocated staging bytes are not deducted from
/// the total. The same numeric conditions and error precedence apply as in
/// [`evaluate_storage_admission`].
pub fn evaluate_storage_admission_with_reserved_total(
    policy: &StorageBudgetPolicyV1,
    allocation: StorageAllocationSnapshotV1,
    reserved_total: u64,
    estimated_allowance_bytes: Option<u64>,
    filesystem_free_bytes: u64,
    operation: StorageOperation,
) -> Result<u64, StorageAdmissionRejection> {
    evaluate_storage_admission_inner(
        policy,
        allocation,
        Ok(reserved_total),
        estimated_allowance_bytes,
        filesystem_free_bytes,
        operation,
    )
}

fn evaluate_storage_admission_inner(
    policy: &StorageBudgetPolicyV1,
    allocation: StorageAllocationSnapshotV1,
    reserved_total: Result<u64, StorageAdmissionRejection>,
    estimated_allowance_bytes: Option<u64>,
    filesystem_free_bytes: u64,
    operation: StorageOperation,
) -> Result<u64, StorageAdmissionRejection> {
    policy
        .validate()
        .map_err(|_| StorageAdmissionRejection::InvalidPolicy)?;
    if policy.mode == StorageBudgetMode::Legacy {
        return Err(StorageAdmissionRejection::LegacyMode);
    }

    let allowance =
        estimated_allowance_bytes.ok_or(StorageAdmissionRejection::EstimateUnavailable)?;
    let _total_allocated_bytes = allocation
        .retained_bytes
        .checked_add(allocation.workspace_bytes)
        .and_then(|total| total.checked_add(allocation.unknown_bytes))
        .ok_or(StorageAdmissionRejection::ArithmeticOverflow(
            StorageAdmissionArithmetic::TotalAllocated,
        ))?;
    let reservation_bytes = reserved_total?;
    let workspace_required_bytes = allocation
        .workspace_bytes
        .checked_add(reservation_bytes)
        .and_then(|required| required.checked_add(allowance))
        .ok_or(StorageAdmissionRejection::ArithmeticOverflow(
            StorageAdmissionArithmetic::WorkspaceRequired,
        ))?;
    let filesystem_required_bytes = policy
        .minimum_free_bytes
        .checked_add(reservation_bytes)
        .and_then(|required| required.checked_add(allowance))
        .ok_or(StorageAdmissionRejection::ArithmeticOverflow(
            StorageAdmissionArithmetic::FilesystemRequired,
        ))?;

    if allocation.unknown_bytes > 0 || allocation.unknown_entry_count > 0 {
        return Err(StorageAdmissionRejection::UnknownStorage {
            bytes: allocation.unknown_bytes,
            entry_count: allocation.unknown_entry_count,
        });
    }
    if allocation.workspace_bytes > policy.workspace_budget_bytes {
        return Err(StorageAdmissionRejection::WorkspaceOverrun {
            workspace_bytes: allocation.workspace_bytes,
            budget_bytes: policy.workspace_budget_bytes,
        });
    }
    if workspace_required_bytes > policy.workspace_budget_bytes {
        return Err(StorageAdmissionRejection::WorkspaceInsufficient {
            required_bytes: workspace_required_bytes,
            budget_bytes: policy.workspace_budget_bytes,
        });
    }
    if filesystem_required_bytes > filesystem_free_bytes {
        return Err(StorageAdmissionRejection::FilesystemInsufficient {
            required_bytes: filesystem_required_bytes,
            available_bytes: filesystem_free_bytes,
        });
    }
    if operation.requires_retention_headroom()
        && allocation.retained_bytes >= policy.retained_target_bytes
    {
        return Err(StorageAdmissionRejection::RetentionTargetReached {
            retained_bytes: allocation.retained_bytes,
            target_bytes: policy.retained_target_bytes,
        });
    }

    Ok(allowance)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1_048_576;
    const GIB: u64 = 1_073_741_824;

    fn policy(retained: u64, workspace: u64, free: u64) -> StorageBudgetPolicyV1 {
        StorageBudgetPolicyV1 {
            mode: StorageBudgetMode::Separated,
            retained_target_bytes: retained,
            workspace_budget_bytes: workspace,
            minimum_free_bytes: free,
        }
    }

    fn allocation(retained_mib: u64, workspace_mib: u64) -> StorageAllocationSnapshotV1 {
        StorageAllocationSnapshotV1 {
            retained_bytes: retained_mib * MIB,
            workspace_bytes: workspace_mib * MIB,
            unknown_bytes: 0,
            unknown_entry_count: 0,
        }
    }

    fn reservations(active_mib: u64, stale_mib: u64) -> StorageReservationSnapshotV1 {
        StorageReservationSnapshotV1 {
            active_bytes: active_mib * MIB,
            stale_bytes: stale_mib * MIB,
        }
    }

    fn evaluate(
        allocation: StorageAllocationSnapshotV1,
        reservations: StorageReservationSnapshotV1,
        allowance_mib: u64,
        free_mib: u64,
        operation: StorageOperation,
    ) -> Result<u64, StorageAdmissionRejection> {
        evaluate_storage_admission(
            &policy(GIB, GIB, GIB),
            allocation,
            reservations,
            Some(allowance_mib * MIB),
            free_mib * MIB,
            operation,
        )
    }

    #[test]
    fn full_reservation_total_matches_split_inputs_without_liveness_inference() {
        let policies = [
            policy(GIB, GIB, GIB),
            policy(0, GIB, GIB),
            StorageBudgetPolicyV1 {
                mode: StorageBudgetMode::Legacy,
                ..policy(GIB, GIB, GIB)
            },
        ];
        let allocations = [
            allocation(700, 64),
            allocation(1024, 0),
            allocation(0, 1025),
            StorageAllocationSnapshotV1 {
                unknown_entry_count: 1,
                ..allocation(0, 0)
            },
            StorageAllocationSnapshotV1 {
                unknown_bytes: 1,
                ..allocation(0, 0)
            },
            StorageAllocationSnapshotV1 {
                retained_bytes: u64::MAX,
                workspace_bytes: 1,
                ..allocation(0, 0)
            },
        ];
        for policy in policies {
            for allocation in allocations {
                for total in [0, 128 * MIB, u64::MAX] {
                    for split in [0, total / 2, total] {
                        let reservations = StorageReservationSnapshotV1 {
                            active_bytes: split,
                            stale_bytes: total - split,
                        };
                        for estimate in [None, Some(0), Some(832 * MIB), Some(u64::MAX)] {
                            for free in [0, 1984 * MIB, u64::MAX] {
                                for operation in [
                                    StorageOperation::Ingest,
                                    StorageOperation::Import,
                                    StorageOperation::PrivateCapture,
                                    StorageOperation::Report,
                                    StorageOperation::Cleanup,
                                    StorageOperation::Recovery,
                                    StorageOperation::Migration,
                                ] {
                                    assert_eq!(
                                        evaluate_storage_admission_with_reserved_total(
                                            &policy, allocation, total, estimate, free, operation
                                        ),
                                        evaluate_storage_admission(
                                            &policy,
                                            allocation,
                                            reservations,
                                            estimate,
                                            free,
                                            operation
                                        ),
                                        "policy={policy:?} allocation={allocation:?} reservations={reservations:?} estimate={estimate:?} free={free} operation={operation:?}",
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn p0_vectors_preserve_separate_retained_workspace_and_free_floor_rules() {
        assert_eq!(
            evaluate(
                allocation(700, 0),
                reservations(0, 0),
                660,
                2048,
                StorageOperation::Ingest,
            ),
            Ok(660 * MIB),
        );
        assert_eq!(
            evaluate(
                allocation(700, 64),
                reservations(128, 0),
                832,
                1984,
                StorageOperation::Ingest,
            ),
            Ok(832 * MIB),
        );
        assert_eq!(
            evaluate(
                allocation(700, 64),
                reservations(128, 0),
                833,
                4096,
                StorageOperation::Ingest,
            ),
            Err(StorageAdmissionRejection::WorkspaceInsufficient {
                required_bytes: 1025 * MIB,
                budget_bytes: GIB,
            }),
        );
        assert_eq!(
            evaluate(
                allocation(700, 0),
                reservations(128, 0),
                660,
                1811,
                StorageOperation::Ingest,
            ),
            Err(StorageAdmissionRejection::FilesystemInsufficient {
                required_bytes: 1812 * MIB,
                available_bytes: 1811 * MIB,
            }),
        );
        assert_eq!(
            evaluate(
                allocation(1024, 0),
                reservations(0, 0),
                64,
                2048,
                StorageOperation::Ingest,
            ),
            Err(StorageAdmissionRejection::RetentionTargetReached {
                retained_bytes: GIB,
                target_bytes: GIB,
            }),
        );
        assert_eq!(
            evaluate(
                allocation(1200, 0),
                reservations(0, 0),
                64,
                2048,
                StorageOperation::Cleanup,
            ),
            Ok(64 * MIB),
        );
    }

    #[test]
    fn split_reservation_overflow_keeps_existing_error_precedence() {
        let overflowing = StorageReservationSnapshotV1 {
            active_bytes: u64::MAX,
            stale_bytes: 1,
        };
        let valid = policy(GIB, GIB, GIB);
        let mut legacy = valid.clone();
        legacy.mode = StorageBudgetMode::Legacy;
        let invalid = policy(0, GIB, GIB);
        for (candidate, estimate, expected) in [
            (&invalid, None, StorageAdmissionRejection::InvalidPolicy),
            (&legacy, None, StorageAdmissionRejection::LegacyMode),
            (&valid, None, StorageAdmissionRejection::EstimateUnavailable),
            (
                &valid,
                Some(0),
                StorageAdmissionRejection::ArithmeticOverflow(
                    StorageAdmissionArithmetic::Reservations,
                ),
            ),
        ] {
            assert_eq!(
                evaluate_storage_admission(
                    candidate,
                    allocation(0, 0),
                    overflowing,
                    estimate,
                    u64::MAX,
                    StorageOperation::Report
                ),
                Err(expected),
            );
        }
        let overflow_allocation = StorageAllocationSnapshotV1 {
            retained_bytes: u64::MAX,
            workspace_bytes: 1,
            unknown_bytes: 0,
            unknown_entry_count: 0,
        };
        assert_eq!(
            evaluate_storage_admission(
                &valid,
                overflow_allocation,
                overflowing,
                Some(0),
                u64::MAX,
                StorageOperation::Report
            ),
            Err(StorageAdmissionRejection::ArithmeticOverflow(
                StorageAdmissionArithmetic::TotalAllocated
            )),
        );
    }

    #[test]
    fn policy_bounds_are_inclusive_and_reject_each_plus_or_minus_one() {
        let min = 256 * MIB;
        let max = 20 * 1024 * MIB;
        for candidate in [policy(min, min, min), policy(max, max, max)] {
            assert_eq!(
                evaluate_storage_admission(
                    &candidate,
                    allocation(0, 0),
                    StorageReservationSnapshotV1::default(),
                    Some(0),
                    u64::MAX,
                    StorageOperation::Report,
                ),
                Ok(0),
            );
        }
        for candidate in [
            policy(min - 1, min, min),
            policy(max + 1, min, min),
            policy(min, min - 1, min),
            policy(min, max + 1, min),
            policy(min, min, min - 1),
            policy(min, min, max + 1),
        ] {
            assert_eq!(
                evaluate_storage_admission(
                    &candidate,
                    allocation(0, 0),
                    StorageReservationSnapshotV1::default(),
                    Some(0),
                    u64::MAX,
                    StorageOperation::Report,
                ),
                Err(StorageAdmissionRejection::InvalidPolicy),
            );
        }
    }

    #[test]
    fn legacy_policy_is_not_applicable() {
        let mut legacy = policy(GIB, GIB, GIB);
        legacy.mode = StorageBudgetMode::Legacy;
        assert_eq!(
            evaluate_storage_admission(
                &legacy,
                allocation(0, 0),
                StorageReservationSnapshotV1::default(),
                Some(0),
                GIB,
                StorageOperation::Report,
            ),
            Err(StorageAdmissionRejection::LegacyMode),
        );
    }

    #[test]
    fn missing_estimate_never_falls_back_to_zero() {
        assert_eq!(
            evaluate_storage_admission(
                &policy(GIB, GIB, GIB),
                allocation(0, 0),
                StorageReservationSnapshotV1::default(),
                None,
                GIB,
                StorageOperation::Migration,
            ),
            Err(StorageAdmissionRejection::EstimateUnavailable),
        );
    }

    #[test]
    fn a_zero_allocated_unknown_entry_still_rejects() {
        let mut snapshot = allocation(0, 0);
        snapshot.unknown_entry_count = 1;
        assert_eq!(
            evaluate_storage_admission(
                &policy(GIB, GIB, GIB),
                snapshot,
                StorageReservationSnapshotV1::default(),
                Some(0),
                GIB,
                StorageOperation::Recovery,
            ),
            Err(StorageAdmissionRejection::UnknownStorage {
                bytes: 0,
                entry_count: 1,
            }),
        );
    }

    #[test]
    fn current_workspace_overrun_is_distinct_from_new_allowance_shortfall() {
        assert_eq!(
            evaluate(
                allocation(0, 1025),
                reservations(0, 0),
                0,
                2048,
                StorageOperation::Recovery,
            ),
            Err(StorageAdmissionRejection::WorkspaceOverrun {
                workspace_bytes: 1025 * MIB,
                budget_bytes: GIB,
            }),
        );
    }

    #[test]
    fn active_and_stale_reservation_accounting_is_monotonic() {
        let base = allocation(0, 512);
        assert_eq!(
            evaluate(
                base,
                reservations(255, 256),
                1,
                2048,
                StorageOperation::Report,
            ),
            Ok(MIB),
        );
        for held in [reservations(256, 256), reservations(255, 257)] {
            assert_eq!(
                evaluate(base, held, 1, 2048, StorageOperation::Report),
                Err(StorageAdmissionRejection::WorkspaceInsufficient {
                    required_bytes: 1025 * MIB,
                    budget_bytes: GIB,
                }),
            );
        }
    }

    #[test]
    fn only_growth_operations_require_retention_headroom() {
        for operation in [
            StorageOperation::Ingest,
            StorageOperation::Import,
            StorageOperation::PrivateCapture,
        ] {
            assert!(matches!(
                evaluate(allocation(1024, 0), reservations(0, 0), 0, 1024, operation,),
                Err(StorageAdmissionRejection::RetentionTargetReached { .. })
            ));
        }
        for operation in [
            StorageOperation::Report,
            StorageOperation::Cleanup,
            StorageOperation::Recovery,
            StorageOperation::Migration,
        ] {
            assert_eq!(
                evaluate(allocation(1024, 0), reservations(0, 0), 0, 1024, operation,),
                Ok(0),
            );
        }
    }

    #[test]
    fn filesystem_floor_does_not_subtract_existing_workspace_again() {
        assert_eq!(
            evaluate(
                allocation(0, 900),
                reservations(0, 0),
                100,
                1124,
                StorageOperation::Report,
            ),
            Ok(100 * MIB),
        );
    }

    #[test]
    fn admission_boundaries_are_exact_to_one_byte() {
        let separated = policy(GIB, GIB, GIB);
        let held = StorageReservationSnapshotV1 {
            active_bytes: 7,
            stale_bytes: 11,
        };
        let snapshot = StorageAllocationSnapshotV1 {
            retained_bytes: GIB - 1,
            workspace_bytes: 13,
            unknown_bytes: 0,
            unknown_entry_count: 0,
        };
        let at_workspace_limit = GIB - 13 - 18;
        for allowance in [at_workspace_limit - 1, at_workspace_limit] {
            assert_eq!(
                evaluate_storage_admission(
                    &separated,
                    snapshot,
                    held,
                    Some(allowance),
                    3 * GIB,
                    StorageOperation::Ingest,
                ),
                Ok(allowance)
            );
        }
        assert!(matches!(
            evaluate_storage_admission(
                &separated,
                snapshot,
                held,
                Some(at_workspace_limit + 1),
                3 * GIB,
                StorageOperation::Ingest,
            ),
            Err(StorageAdmissionRejection::WorkspaceInsufficient { .. })
        ));

        let allowance = 17;
        let at_free_limit = GIB + 18 + allowance;
        for free in [at_free_limit, at_free_limit + 1] {
            assert_eq!(
                evaluate_storage_admission(
                    &separated,
                    snapshot,
                    held,
                    Some(allowance),
                    free,
                    StorageOperation::Ingest,
                ),
                Ok(allowance)
            );
        }
        assert!(matches!(
            evaluate_storage_admission(
                &separated,
                snapshot,
                held,
                Some(allowance),
                at_free_limit - 1,
                StorageOperation::Ingest,
            ),
            Err(StorageAdmissionRejection::FilesystemInsufficient { .. })
        ));
        for retained_bytes in [GIB, GIB + 1] {
            assert!(matches!(
                evaluate_storage_admission(
                    &separated,
                    StorageAllocationSnapshotV1 {
                        retained_bytes,
                        ..snapshot
                    },
                    held,
                    Some(allowance),
                    3 * GIB,
                    StorageOperation::Ingest,
                ),
                Err(StorageAdmissionRejection::RetentionTargetReached { .. })
            ));
        }
    }

    #[test]
    fn unknown_bytes_reject_even_if_entry_count_was_not_supplied() {
        assert!(matches!(
            evaluate_storage_admission(
                &policy(GIB, GIB, GIB),
                StorageAllocationSnapshotV1 {
                    unknown_bytes: 1,
                    ..allocation(0, 0)
                },
                StorageReservationSnapshotV1::default(),
                Some(0),
                GIB,
                StorageOperation::Report,
            ),
            Err(StorageAdmissionRejection::UnknownStorage { .. })
        ));
    }

    #[test]
    fn every_admission_sum_rejects_overflow() {
        let separated = policy(GIB, GIB, GIB);
        let cases = [
            (
                StorageAllocationSnapshotV1 {
                    retained_bytes: u64::MAX,
                    workspace_bytes: 1,
                    unknown_bytes: 0,
                    unknown_entry_count: 0,
                },
                StorageReservationSnapshotV1::default(),
                Some(0),
                StorageAdmissionArithmetic::TotalAllocated,
            ),
            (
                allocation(0, 0),
                StorageReservationSnapshotV1 {
                    active_bytes: u64::MAX,
                    stale_bytes: 1,
                },
                Some(0),
                StorageAdmissionArithmetic::Reservations,
            ),
            (
                allocation(0, 0),
                StorageReservationSnapshotV1 {
                    active_bytes: u64::MAX,
                    stale_bytes: 0,
                },
                Some(1),
                StorageAdmissionArithmetic::WorkspaceRequired,
            ),
            (
                allocation(0, 0),
                StorageReservationSnapshotV1 {
                    active_bytes: u64::MAX,
                    stale_bytes: 0,
                },
                Some(0),
                StorageAdmissionArithmetic::FilesystemRequired,
            ),
        ];
        for (snapshot, held, estimate, arithmetic) in cases {
            assert_eq!(
                evaluate_storage_admission(
                    &separated,
                    snapshot,
                    held,
                    estimate,
                    u64::MAX,
                    StorageOperation::Report,
                ),
                Err(StorageAdmissionRejection::ArithmeticOverflow(arithmetic)),
            );
        }
    }
}
