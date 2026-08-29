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
    }
}
