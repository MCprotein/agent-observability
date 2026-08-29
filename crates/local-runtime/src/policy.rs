use serde::{Deserialize, Serialize};

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
}
