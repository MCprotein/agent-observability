use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageBudgetMode {
    Legacy,
    Separated,
}

/// Versioned configuration only; operational admission is owned by runtime control.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StorageBudgetPolicyV1 {
    pub mode: StorageBudgetMode,
    pub retained_target_bytes: u64,
    pub workspace_budget_bytes: u64,
    pub minimum_free_bytes: u64,
}

impl Default for StorageBudgetPolicyV1 {
    fn default() -> Self {
        Self {
            mode: StorageBudgetMode::Legacy,
            retained_target_bytes: default_budget(),
            workspace_budget_bytes: default_budget(),
            minimum_free_bytes: default_budget(),
        }
    }
}

impl StorageBudgetPolicyV1 {
    pub fn validate(&self) -> Result<(), PolicyError> {
        for (field, value) in [
            ("retained_target_bytes", self.retained_target_bytes),
            ("workspace_budget_bytes", self.workspace_budget_bytes),
            ("minimum_free_bytes", self.minimum_free_bytes),
        ] {
            validate_bounds(field, value, 268_435_456, 21_474_836_480)?;
        }
        Ok(())
    }
}

const fn default_file() -> u32 {
    5_000
}
const fn default_flush() -> u32 {
    5_000
}
const fn default_records() -> u16 {
    100
}
const fn default_bytes() -> u32 {
    524_288
}
const fn default_active() -> u32 {
    60_000
}
const fn default_idle() -> u32 {
    300_000
}
const fn default_budget() -> u64 {
    1_073_741_824
}
const fn default_retention_days() -> u16 {
    30
}
const fn default_archive_records() -> u32 {
    10_000
}
const fn default_archive_bytes() -> u64 {
    16_777_216
}
const fn default_lifecycle_enabled() -> bool {
    false
}
const fn default_hot_days() -> u16 {
    7
}
const fn default_warm_days() -> u16 {
    30
}
const fn default_delete_after_days() -> u16 {
    90
}
const fn default_private_raw_days() -> u16 {
    7
}
const fn default_maintenance_interval_seconds() -> u32 {
    300
}
const fn default_max_traces_per_pass() -> u16 {
    32
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StorageLifecyclePolicyV1 {
    #[serde(default = "default_lifecycle_enabled")]
    pub enabled: bool,
    #[serde(default = "default_hot_days")]
    pub hot_days: u16,
    #[serde(default = "default_warm_days")]
    pub warm_days: u16,
    #[serde(default = "default_delete_after_days")]
    pub delete_after_days: u16,
    #[serde(default = "default_private_raw_days")]
    pub private_raw_days: u16,
    #[serde(default = "default_maintenance_interval_seconds")]
    pub maintenance_interval_seconds: u32,
    #[serde(default = "default_max_traces_per_pass")]
    pub max_traces_per_pass: u16,
}

impl Default for StorageLifecyclePolicyV1 {
    fn default() -> Self {
        Self {
            enabled: default_lifecycle_enabled(),
            hot_days: default_hot_days(),
            warm_days: default_warm_days(),
            delete_after_days: default_delete_after_days(),
            private_raw_days: default_private_raw_days(),
            maintenance_interval_seconds: default_maintenance_interval_seconds(),
            max_traces_per_pass: default_max_traces_per_pass(),
        }
    }
}

impl StorageLifecyclePolicyV1 {
    pub fn validate(&self) -> Result<(), PolicyError> {
        validate_bounds("hot_days", u64::from(self.hot_days), 1, 3_650)?;
        validate_bounds("warm_days", u64::from(self.warm_days), 1, 3_650)?;
        validate_bounds(
            "delete_after_days",
            u64::from(self.delete_after_days),
            1,
            3_650,
        )?;
        validate_bounds(
            "private_raw_days",
            u64::from(self.private_raw_days),
            1,
            3_650,
        )?;
        validate_bounds(
            "maintenance_interval_seconds",
            u64::from(self.maintenance_interval_seconds),
            60,
            86_400,
        )?;
        validate_bounds(
            "max_traces_per_pass",
            u64::from(self.max_traces_per_pass),
            1,
            128,
        )?;
        if self.hot_days <= self.warm_days && self.warm_days < self.delete_after_days {
            Ok(())
        } else {
            Err(PolicyError::InvalidLifecycleOrder {
                hot_days: self.hot_days,
                warm_days: self.warm_days,
                delete_after_days: self.delete_after_days,
            })
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetentionPolicyV1 {
    #[serde(default = "default_retention_days")]
    pub max_record_age_days: u16,
    #[serde(default = "default_archive_records")]
    pub max_archive_records: u32,
    #[serde(default = "default_archive_bytes")]
    pub max_archive_bytes: u64,
}

impl Default for RetentionPolicyV1 {
    fn default() -> Self {
        Self {
            max_record_age_days: default_retention_days(),
            max_archive_records: default_archive_records(),
            max_archive_bytes: default_archive_bytes(),
        }
    }
}

impl RetentionPolicyV1 {
    pub fn validate(&self) -> Result<(), PolicyError> {
        validate_bounds(
            "max_record_age_days",
            u64::from(self.max_record_age_days),
            1,
            3_650,
        )?;
        validate_bounds(
            "max_archive_records",
            u64::from(self.max_archive_records),
            1,
            100_000,
        )?;
        validate_bounds(
            "max_archive_bytes",
            self.max_archive_bytes,
            65_536,
            268_435_456,
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CollectionPolicyV1 {
    #[serde(default = "default_file")]
    pub file_reconcile_interval_ms: u32,
    #[serde(default = "default_flush")]
    pub flush_interval_ms: u32,
    #[serde(default = "default_records")]
    pub max_batch_records: u16,
    #[serde(default = "default_bytes")]
    pub max_batch_bytes: u32,
    #[serde(default = "default_active")]
    pub active_heartbeat_interval_ms: u32,
    #[serde(default = "default_idle")]
    pub idle_heartbeat_interval_ms: u32,
    #[serde(default = "default_budget")]
    pub local_storage_budget_bytes: u64,
}

impl Default for CollectionPolicyV1 {
    fn default() -> Self {
        Self {
            file_reconcile_interval_ms: default_file(),
            flush_interval_ms: default_flush(),
            max_batch_records: default_records(),
            max_batch_bytes: default_bytes(),
            active_heartbeat_interval_ms: default_active(),
            idle_heartbeat_interval_ms: default_idle(),
            local_storage_budget_bytes: default_budget(),
        }
    }
}

impl CollectionPolicyV1 {
    pub fn validate(&self) -> Result<(), PolicyError> {
        let checks = [
            (
                "file_reconcile_interval_ms",
                u64::from(self.file_reconcile_interval_ms),
                1_000,
                60_000,
            ),
            (
                "flush_interval_ms",
                u64::from(self.flush_interval_ms),
                1_000,
                60_000,
            ),
            (
                "max_batch_records",
                u64::from(self.max_batch_records),
                1,
                500,
            ),
            (
                "max_batch_bytes",
                u64::from(self.max_batch_bytes),
                16_384,
                2_097_152,
            ),
            (
                "active_heartbeat_interval_ms",
                u64::from(self.active_heartbeat_interval_ms),
                30_000,
                300_000,
            ),
            (
                "idle_heartbeat_interval_ms",
                u64::from(self.idle_heartbeat_interval_ms),
                120_000,
                900_000,
            ),
            (
                "local_storage_budget_bytes",
                self.local_storage_budget_bytes,
                268_435_456,
                21_474_836_480,
            ),
        ];
        checks
            .into_iter()
            .find(|(_, value, min, max)| *value < *min || *value > *max)
            .map_or(Ok(()), |(field, value, min, max)| {
                Err(PolicyError::OutOfBounds {
                    field,
                    value,
                    min,
                    max,
                })
            })
    }
    pub fn from_json(input: &str) -> Result<Self, PolicyError> {
        let policy: Self = serde_json::from_str(input).map_err(PolicyError::Json)?;
        policy.validate()?;
        Ok(policy)
    }
}

fn validate_bounds(field: &'static str, value: u64, min: u64, max: u64) -> Result<(), PolicyError> {
    if value < min || value > max {
        Err(PolicyError::OutOfBounds {
            field,
            value,
            min,
            max,
        })
    } else {
        Ok(())
    }
}

#[derive(Debug)]
pub enum PolicyError {
    Json(serde_json::Error),
    OutOfBounds {
        field: &'static str,
        value: u64,
        min: u64,
        max: u64,
    },
    InvalidLifecycleOrder {
        hot_days: u16,
        warm_days: u16,
        delete_after_days: u16,
    },
}
impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "invalid CollectionPolicyV1 JSON: {e}"),
            Self::OutOfBounds {
                field,
                value,
                min,
                max,
            } => write!(f, "{field}={value} outside {min}..={max}"),
            Self::InvalidLifecycleOrder {
                hot_days,
                warm_days,
                delete_after_days,
            } => write!(
                f,
                "storage lifecycle requires hot_days <= warm_days < delete_after_days; got {hot_days}, {warm_days}, {delete_after_days}"
            ),
        }
    }
}
impl std::error::Error for PolicyError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_strictness() {
        let p = CollectionPolicyV1::from_json("{}").unwrap();
        assert_eq!(p, CollectionPolicyV1::default());
        assert!(CollectionPolicyV1::from_json(r#"{"wat":1}"#).is_err());
    }
    #[test]
    fn bounds_are_rejected() {
        assert!(CollectionPolicyV1::from_json(r#"{"max_batch_bytes":16383}"#).is_err());
        assert!(
            CollectionPolicyV1::from_json(r#"{"local_storage_budget_bytes":21474836480}"#).is_ok()
        );
        assert!(
            CollectionPolicyV1::from_json(r#"{"local_storage_budget_bytes":21474836481}"#).is_err()
        );
        assert!(RetentionPolicyV1::default().validate().is_ok());
        for days in [1, 3_650] {
            assert!(
                RetentionPolicyV1 {
                    max_record_age_days: days,
                    ..RetentionPolicyV1::default()
                }
                .validate()
                .is_ok()
            );
        }
        for days in [0, 3_651] {
            assert!(
                RetentionPolicyV1 {
                    max_record_age_days: days,
                    ..RetentionPolicyV1::default()
                }
                .validate()
                .is_err()
            );
        }
        for records in [1, 100_000] {
            assert!(
                RetentionPolicyV1 {
                    max_archive_records: records,
                    ..RetentionPolicyV1::default()
                }
                .validate()
                .is_ok()
            );
        }
        for records in [0, 100_001] {
            assert!(
                RetentionPolicyV1 {
                    max_archive_records: records,
                    ..RetentionPolicyV1::default()
                }
                .validate()
                .is_err()
            );
        }
        for bytes in [65_536, 268_435_456] {
            assert!(
                RetentionPolicyV1 {
                    max_archive_bytes: bytes,
                    ..RetentionPolicyV1::default()
                }
                .validate()
                .is_ok()
            );
        }
        for bytes in [65_535, 268_435_457] {
            assert!(
                RetentionPolicyV1 {
                    max_archive_bytes: bytes,
                    ..RetentionPolicyV1::default()
                }
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn storage_lifecycle_defaults_are_disabled_and_ordered() {
        let policy = StorageLifecyclePolicyV1::default();
        assert!(!policy.enabled);
        assert_eq!(policy.hot_days, 7);
        assert_eq!(policy.warm_days, 30);
        assert_eq!(policy.delete_after_days, 90);
        assert_eq!(policy.private_raw_days, 7);
        assert_eq!(policy.maintenance_interval_seconds, 300);
        assert_eq!(policy.max_traces_per_pass, 32);
        policy.validate().unwrap();
    }

    #[test]
    fn storage_lifecycle_rejects_invalid_order_and_bounds() {
        for policy in [
            StorageLifecyclePolicyV1 {
                hot_days: 31,
                ..StorageLifecyclePolicyV1::default()
            },
            StorageLifecyclePolicyV1 {
                warm_days: 90,
                ..StorageLifecyclePolicyV1::default()
            },
            StorageLifecyclePolicyV1 {
                delete_after_days: 3_651,
                ..StorageLifecyclePolicyV1::default()
            },
            StorageLifecyclePolicyV1 {
                private_raw_days: 0,
                ..StorageLifecyclePolicyV1::default()
            },
            StorageLifecyclePolicyV1 {
                maintenance_interval_seconds: 59,
                ..StorageLifecyclePolicyV1::default()
            },
            StorageLifecyclePolicyV1 {
                max_traces_per_pass: 129,
                ..StorageLifecyclePolicyV1::default()
            },
        ] {
            assert!(policy.validate().is_err());
        }
    }
}
