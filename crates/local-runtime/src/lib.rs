#![forbid(unsafe_code)]
#![allow(
    clippy::if_not_else,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]
#![allow(clippy::suspicious_open_options)]

pub mod config;
pub mod control;
pub mod ingress;
pub mod lock;
pub mod policy;
pub mod scheduler;
pub mod storage;

pub use control::{ControlError, RuntimeControl};
pub use ingress::{Ingress, IngressCounters, IngressMessage, IngressOutcome};
pub use lock::{MutationGuard, Singleton, SingletonError};
pub use policy::{CollectionPolicyV1, RetentionPolicyV1};
pub use scheduler::{PressureSample, Schedule, Scheduler, State};
pub use storage::{Admission, Partition, StorageAccountingError, StorageBudget, StorageError};

pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
pub const ENQUEUE_DEADLINE_MS: u64 = 10;
pub const HANDLER_DEADLINE_MS: u64 = 50;
pub const CHANNEL_CAPACITY: usize = 64;
pub const NORMALIZATION_WORKERS: usize = 1;
pub use config::{
    ConfigError, ConfigMutationGuard, ConfigServiceError, InstalledLayout,
    LOCAL_RUNTIME_CONFIG_VERSION, LocalConfigService, LocalRuntimeConfigV3, VersionedLocalConfig,
    install, load, revision, save, save_if_revision,
};
