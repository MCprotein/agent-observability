//! Read-only ownership facades for storage-accounting composition.

use agent_observability_codex_config::CodexConfigManager;
pub use agent_observability_codex_config::{
    CodexConfigSnapshotOwnershipError, CodexConfigSnapshotOwnershipEvidence,
};
use std::path::Path;

/// Captures bounded Codex config snapshot ownership without exposing a write-capable manager.
///
/// `external_config_path` is explicit authority used only to validate the path recorded in the
/// private ownership snapshot. This function does not read or repair that external file and does
/// not call integration status or recovery.
///
/// # Errors
///
/// Returns an error when the root-bound state directory, snapshot, retained identities, or
/// bounded absence does not validate.
pub fn capture_codex_config_snapshot_ownership(
    root: &Path,
    external_config_path: &Path,
) -> Result<CodexConfigSnapshotOwnershipEvidence, CodexConfigSnapshotOwnershipError> {
    let manager = CodexConfigManager::from_ownership_snapshot(
        external_config_path,
        root.join("runtime/integrations/codex"),
    );
    CodexConfigSnapshotOwnershipEvidence::capture(root, &manager)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use agent_observability_codex_config::{CodexConfigManager, ExporterSecurity};
    use agent_observability_local_collector::{
        CollectorSettings, install_settings, storage_ownership::CollectorTlsOwnershipEvidence,
    };
    use agent_observability_local_runtime::{
        InstalledLayout, MutationGuard, StorageAllocationClass, StorageInventoryError, install,
        storage_coherence::StorageBarrier,
    };
    use std::{
        collections::BTreeMap,
        fs,
        os::unix::fs::{FileTypeExt, PermissionsExt},
        path::{Path, PathBuf},
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    const CONFIG_SNAPSHOT: &str = "runtime/integrations/codex/codex-config-ownership-v1.json";
    const LAUNCH_AGENT_TRANSACTION: &str =
        "runtime/integrations/codex/launch-agent-ownership-v1.json";
    #[cfg(target_os = "macos")]
    const LIFECYCLE_LOCK: &str = "runtime/integrations/codex/lifecycle/mutation.lock";

    struct Fixture {
        base: PathBuf,
        layout: InstalledLayout,
        #[cfg(target_os = "macos")]
        home: PathBuf,
        external_config: PathBuf,
        settings: CollectorSettings,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let base = std::env::temp_dir().join(format!(
                "codex-integration-storage-accounting-{name}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            assert!(!base.exists());
            fs::create_dir(&base).unwrap();
            fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
            let layout = install(&base.join("root")).unwrap();
            let settings = install_settings(&layout.root).unwrap();
            let external_config = base.join("external-config.toml");
            let security = ExporterSecurity::new(
                layout.runtime.join(&settings.credentials.ca_certificate),
                settings.auth_token.clone(),
            )
            .unwrap();
            let manager = CodexConfigManager::new(
                &external_config,
                layout.runtime.join("integrations/codex"),
                base.join("agentobs"),
                &layout.root,
                settings.port,
                security,
            )
            .unwrap();
            manager.connect().unwrap();
            Self {
                #[cfg(target_os = "macos")]
                home: base.join("home"),
                base,
                layout,
                external_config,
                settings,
            }
        }

        fn replace_external_with_fifo(&self) {
            fs::remove_file(&self.external_config).unwrap();
            make_fifo(&self.external_config);
        }

        fn initialize_barrier(&self) -> StorageBarrier {
            let mutation = MutationGuard::try_acquire(&self.layout.runtime).unwrap();
            let barrier = StorageBarrier::initialize(&self.layout.root, &mutation).unwrap();
            drop(mutation);
            barrier
        }

        fn write_private(&self, relative: &str, bytes: &[u8]) {
            let path = self.layout.root.join(relative);
            fs::write(path, bytes).unwrap();
            fs::set_permissions(
                self.layout.root.join(relative),
                fs::Permissions::from_mode(0o600),
            )
            .unwrap();
        }

        fn tls_credentials(&self) -> [PathBuf; 3] {
            [
                PathBuf::from("runtime").join(&self.settings.credentials.ca_certificate),
                PathBuf::from("runtime").join(&self.settings.credentials.server_certificate),
                PathBuf::from("runtime").join(&self.settings.credentials.server_private_key),
            ]
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.base).unwrap();
        }
    }

    fn make_fifo(path: &Path) {
        assert!(Command::new("mkfifo").arg(path).status().unwrap().success());
        fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
    }

    fn ownership<T, E>(result: Result<T, E>) -> Result<T, StorageInventoryError> {
        result.map_err(|_| StorageInventoryError::OwnershipMismatch)
    }

    #[cfg(target_os = "macos")]
    fn write_launch_agent_state(fixture: &Fixture) -> PathBuf {
        use crate::{
            LAUNCH_AGENT_OWNERSHIP_VERSION, LaunchAgentFileState, LaunchAgentOperation,
            LaunchAgentPhase, LaunchAgentTransaction,
        };

        let lifecycle = fixture
            .layout
            .root
            .join("runtime/integrations/codex/lifecycle");
        fs::create_dir(&lifecycle).unwrap();
        fs::set_permissions(&lifecycle, fs::Permissions::from_mode(0o700)).unwrap();
        let expected_plist =
            crate::expected_launch_agent_plist_path(&fixture.layout.root, &fixture.home);
        fs::create_dir_all(expected_plist.parent().unwrap()).unwrap();
        make_fifo(&expected_plist);
        let missing = LaunchAgentFileState {
            existed: false,
            bytes: Vec::new(),
            mode: 0,
        };
        let transaction = LaunchAgentTransaction {
            schema_version: LAUNCH_AGENT_OWNERSHIP_VERSION.into(),
            plist_path: expected_plist.clone(),
            prior_plist: missing.clone(),
            prior_loaded: false,
            rollback_plist: missing,
            rollback_loaded: false,
            desired_plist: LaunchAgentFileState {
                existed: true,
                bytes: b"managed plist".to_vec(),
                mode: 0o644,
            },
            desired_loaded: true,
            operation: LaunchAgentOperation::Connect,
            phase: LaunchAgentPhase::Owned,
        };
        fixture.write_private(
            LAUNCH_AGENT_TRANSACTION,
            &serde_json::to_vec(&transaction).unwrap(),
        );
        fixture.write_private(LIFECYCLE_LOCK, b"");
        expected_plist
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn config_launch_agent_and_tls_coexist_under_one_root_freeze_without_external_reads() {
        use crate::LaunchAgentStorageOwnershipEvidence;

        let fixture = Fixture::new("macos-coexistence");
        let expected_plist = write_launch_agent_state(&fixture);
        let sentinel = "runtime/integrations/codex/foreign-launch-agent";
        fixture.write_private(sentinel, b"");
        fixture.replace_external_with_fifo();
        let barrier = fixture.initialize_barrier();
        let mutation = MutationGuard::try_acquire(&fixture.layout.runtime).unwrap();
        let freeze = barrier.try_freeze(&mutation).unwrap();
        let config =
            capture_codex_config_snapshot_ownership(&fixture.layout.root, &fixture.external_config)
                .unwrap();
        let tls = CollectorTlsOwnershipEvidence::capture(&fixture.layout)
            .unwrap()
            .unwrap();
        let launch_agent =
            LaunchAgentStorageOwnershipEvidence::capture(&fixture.layout.root, &fixture.home)
                .unwrap();
        let mut classes = BTreeMap::new();
        freeze
            .classify(|relative, candidate| {
                let mut retained = false;
                retained |= ownership(config.matches_entry(relative, candidate))?;
                retained |= ownership(tls.matches_entry(relative, candidate))?;
                retained |= ownership(launch_agent.matches_entry(relative, candidate))?;
                let class = if retained {
                    StorageAllocationClass::Retained
                } else {
                    StorageAllocationClass::Unknown
                };
                classes.insert(relative.to_path_buf(), class);
                Ok(class)
            })
            .unwrap();
        assert_eq!(
            classes[Path::new(CONFIG_SNAPSHOT)],
            StorageAllocationClass::Retained
        );
        assert_eq!(
            classes[Path::new(LAUNCH_AGENT_TRANSACTION)],
            StorageAllocationClass::Retained
        );
        assert_eq!(
            classes[Path::new(LIFECYCLE_LOCK)],
            StorageAllocationClass::Retained
        );
        for credential in fixture.tls_credentials() {
            assert_eq!(classes[&credential], StorageAllocationClass::Retained);
        }
        assert_eq!(
            classes[Path::new(sentinel)],
            StorageAllocationClass::Unknown
        );
        config.revalidate().unwrap();
        tls.revalidate().unwrap();
        launch_agent.revalidate().unwrap();
        freeze.revalidate().unwrap();
        assert!(
            fs::symlink_metadata(&fixture.external_config)
                .unwrap()
                .file_type()
                .is_fifo()
        );
        assert!(
            fs::symlink_metadata(expected_plist)
                .unwrap()
                .file_type()
                .is_fifo()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn foreign_launch_agent_transaction_stays_unknown_on_linux() {
        let fixture = Fixture::new("linux-foreign-launch-agent");
        fixture.write_private(LAUNCH_AGENT_TRANSACTION, b"foreign launch agent");
        fixture.replace_external_with_fifo();
        let barrier = fixture.initialize_barrier();
        let mutation = MutationGuard::try_acquire(&fixture.layout.runtime).unwrap();
        let freeze = barrier.try_freeze(&mutation).unwrap();
        let config =
            capture_codex_config_snapshot_ownership(&fixture.layout.root, &fixture.external_config)
                .unwrap();
        let tls = CollectorTlsOwnershipEvidence::capture(&fixture.layout)
            .unwrap()
            .unwrap();
        let mut classes = BTreeMap::new();
        freeze
            .classify(|relative, candidate| {
                let mut retained = false;
                retained |= ownership(config.matches_entry(relative, candidate))?;
                retained |= ownership(tls.matches_entry(relative, candidate))?;
                let class = if retained {
                    StorageAllocationClass::Retained
                } else {
                    StorageAllocationClass::Unknown
                };
                classes.insert(relative.to_path_buf(), class);
                Ok(class)
            })
            .unwrap();
        assert_eq!(
            classes[Path::new(CONFIG_SNAPSHOT)],
            StorageAllocationClass::Retained
        );
        for credential in fixture.tls_credentials() {
            assert_eq!(classes[&credential], StorageAllocationClass::Retained);
        }
        assert_eq!(
            classes[Path::new(LAUNCH_AGENT_TRANSACTION)],
            StorageAllocationClass::Unknown
        );
        config.revalidate().unwrap();
        tls.revalidate().unwrap();
        freeze.revalidate().unwrap();
        assert!(
            fs::symlink_metadata(&fixture.external_config)
                .unwrap()
                .file_type()
                .is_fifo()
        );
    }
}
