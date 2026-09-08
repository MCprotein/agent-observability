//! Streaming semantic ownership of bounded local-only detail and status files.

use super::{
    CollectorStorageOwnershipError as Error, open_directory, open_file, read_bounded,
    same_identity, validate_directory_metadata, validate_file_metadata,
};
use agent_observability_local_runtime::InstalledLayout;
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

/// Single-pass private artifact observation for an outer frozen inventory scan.
///
/// The caller retains the accounting freeze and uses the inventory's per-entry final
/// metadata revalidation. This object retains parent identities only, never raw payloads
/// or a descriptor per artifact. It is not a reusable admission cache or a standalone
/// proof that file contents stayed unchanged after a match.
pub struct CollectorPrivateStorageObservation {
    root: PathBuf,
    state: File,
    directories: [PrivateDirectory; 2],
}

struct PrivateDirectory {
    relative: PathBuf,
    file: Option<File>,
    kind: Kind,
    checked_entries: usize,
}

#[derive(Clone, Copy)]
enum Kind {
    Detail,
    Status,
}

impl std::fmt::Debug for CollectorPrivateStorageObservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CollectorPrivateStorageObservation")
            .finish_non_exhaustive()
    }
}

impl CollectorPrivateStorageObservation {
    /// Opens only existing private parents; does not enumerate or create artifact files.
    pub fn capture(layout: &InstalledLayout) -> Result<Self, Error> {
        let directory = |name, kind| {
            let relative = PathBuf::from("state").join(name);
            Ok::<_, Error>(PrivateDirectory {
                file: optional_directory(&layout.root.join(&relative))?,
                relative,
                kind,
                checked_entries: 0,
            })
        };
        let observation = Self {
            root: layout.root.clone(),
            state: open_directory(&layout.state)?,
            directories: [
                directory(crate::PRIVATE_TURN_DETAIL_DIRECTORY, Kind::Detail)?,
                directory(crate::PRIVATE_TURN_DETAIL_STATUS_DIRECTORY, Kind::Status)?,
            ],
        };
        observation.revalidate()?;
        Ok(observation)
    }

    /// Validates one exact candidate and existing schema, without retaining its payload.
    /// Unknown names remain unowned; a recognized name with invalid content fails closed.
    pub fn matches_entry(&mut self, relative: &Path, candidate: &File) -> Result<bool, Error> {
        if relative == Path::new("state") {
            return matches_directory(&self.state, candidate).map(|()| true);
        }
        let Some(directory) = self.directories.iter_mut().find(|directory| {
            relative == directory.relative
                || relative.parent() == Some(directory.relative.as_path())
        }) else {
            return Ok(false);
        };
        let Some(parent) = &directory.file else {
            return Err(Error::Replaced);
        };
        if relative == directory.relative {
            return matches_directory(parent, candidate).map(|()| true);
        }
        let Some(name) = relative.file_name().and_then(|name| name.to_str()) else {
            return Ok(false);
        };
        let Some(digest) = name.strip_suffix(".json") else {
            return Ok(false);
        };
        let turn_id = format!("id:sha256:{digest}");
        let Ok(expected_path) = crate::private_turn_detail_path(&directory.relative, &turn_id)
        else {
            return Ok(false);
        };
        if expected_path != relative {
            return Ok(false);
        }
        if directory.checked_entries >= crate::MAX_PRIVATE_TURN_DETAIL_FILES {
            return Err(Error::InvalidContent);
        }
        let metadata = candidate.metadata()?;
        validate_file_metadata(&metadata)?;
        let named = open_file(&self.root.join(relative))?;
        if !same_identity(&metadata, &named.metadata()?) {
            return Err(Error::Replaced);
        }
        let maximum = match directory.kind {
            Kind::Detail => crate::MAX_PRIVATE_TURN_DETAIL_BYTES as u64,
            Kind::Status => crate::MAX_PRIVATE_TURN_DETAIL_STATUS_BYTES,
        };
        let bytes = read_bounded(candidate, maximum)?;
        match directory.kind {
            Kind::Detail => {
                let detail = crate::PrivateCodexTurnDetailV1::from_json(&bytes)
                    .map_err(|_| Error::InvalidContent)?;
                if detail.turn_id() != turn_id {
                    return Err(Error::InvalidContent);
                }
            }
            Kind::Status => {
                crate::validate_private_turn_detail_status(&bytes, &turn_id)
                    .map_err(|_| Error::InvalidContent)?;
            }
        }
        directory.checked_entries += 1;
        Ok(true)
    }

    /// Revalidates retained parents and optional absence, in addition to inventory checks.
    pub fn revalidate(&self) -> Result<(), Error> {
        matches_directory(&self.state, &open_directory(&self.root.join("state"))?)?;
        for directory in &self.directories {
            let current = optional_directory(&self.root.join(&directory.relative))?;
            match (&directory.file, current) {
                (None, None) => {}
                (Some(expected), Some(current)) => matches_directory(expected, &current)?,
                _ => return Err(Error::Replaced),
            }
        }
        Ok(())
    }
}

fn optional_directory(path: &Path) -> Result<Option<File>, Error> {
    match fs::symlink_metadata(path) {
        Ok(_) => open_directory(path).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn matches_directory(expected: &File, candidate: &File) -> Result<(), Error> {
    let metadata = candidate.metadata()?;
    validate_directory_metadata(&metadata)?;
    if !same_identity(&expected.metadata()?, &metadata) {
        return Err(Error::Replaced);
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicU64, Ordering},
    };

    fn fixture() -> InstalledLayout {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "private-owner-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(!root.exists());
        agent_observability_local_runtime::install(&root).unwrap()
    }

    fn write_artifact(layout: &InstalledLayout, kind: Kind, digest: &str, bytes: &[u8]) -> PathBuf {
        let name = match kind {
            Kind::Detail => crate::PRIVATE_TURN_DETAIL_DIRECTORY,
            Kind::Status => crate::PRIVATE_TURN_DETAIL_STATUS_DIRECTORY,
        };
        let directory = layout.state.join(name);
        if !directory.exists() {
            fs::create_dir(&directory).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let path = directory.join(format!("{digest}.json"));
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        path
    }

    fn detail_bytes(digest: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion":agent_observability_adapter_codex::PRIVATE_TURN_DETAIL_SCHEMA_VERSION,
            "turnId":format!("id:sha256:{digest}"), "cwd":"/private/SENTINEL_PATH",
            "inputMessages":["SENTINEL_PROMPT"], "lastAssistantMessage":"SENTINEL_RESPONSE"
        }))
        .unwrap()
    }

    fn matches(
        observation: &mut CollectorPrivateStorageObservation,
        layout: &InstalledLayout,
        path: &Path,
    ) -> Result<bool, Error> {
        observation.matches_entry(
            path.strip_prefix(&layout.root).unwrap(),
            &File::open(path).unwrap(),
        )
    }

    #[test]
    fn valid_raw_is_owned_without_status_or_content_in_debug() {
        let layout = fixture();
        let bytes = detail_bytes(&"a".repeat(64));
        let path = write_artifact(&layout, Kind::Detail, &"a".repeat(64), &bytes);
        let mut observation = CollectorPrivateStorageObservation::capture(&layout).unwrap();
        assert!(matches(&mut observation, &layout, &path).unwrap());
        assert_eq!(
            format!("{observation:?}"),
            "CollectorPrivateStorageObservation { .. }"
        );
        observation.revalidate().unwrap();
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert!(
            !layout
                .state
                .join(crate::PRIVATE_TURN_DETAIL_STATUS_DIRECTORY)
                .exists()
        );
        fs::remove_dir_all(layout.root).unwrap();
    }

    #[test]
    fn malformed_and_misbound_raw_fail_closed_but_unknown_names_are_not_claimed() {
        for scenario in 0..4 {
            let layout = fixture();
            let digest = if scenario == 2 {
                "A".repeat(64)
            } else {
                "a".repeat(64)
            };
            let bytes = if scenario == 0 {
                b"{}".to_vec()
            } else {
                detail_bytes(&"b".repeat(64))
            };
            let mut path = write_artifact(&layout, Kind::Detail, &digest, &bytes);
            if scenario == 3 {
                let temporary = path.with_extension("tmp");
                fs::rename(&path, &temporary).unwrap();
                path = temporary;
            }
            let mut observation = CollectorPrivateStorageObservation::capture(&layout).unwrap();
            if scenario < 2 {
                assert_eq!(
                    matches(&mut observation, &layout, &path),
                    Err(Error::InvalidContent)
                );
            } else {
                assert!(!matches(&mut observation, &layout, &path).unwrap());
            }
            assert_eq!(fs::read(path).unwrap(), bytes);
            fs::remove_dir_all(layout.root).unwrap();
        }
    }

    #[test]
    fn status_requires_valid_schema_id_and_state_code_tuple() {
        for scenario in 0..4 {
            let layout = fixture();
            let digest = "a".repeat(64);
            let mut value = serde_json::json!({
                "schema_version":crate::PRIVATE_TURN_DETAIL_STATUS_VERSION,
                "turn_id":format!("id:sha256:{digest}"), "state":"available", "code":"ok"
            });
            match scenario {
                0 => value["state"] = "failed".into(),
                1 => value["code"] = "unrecognized".into(),
                2 => value["turn_id"] = format!("id:sha256:{}", "b".repeat(64)).into(),
                _ => value["extra"] = "SENTINEL_SECRET".into(),
            }
            let path = write_artifact(
                &layout,
                Kind::Status,
                &digest,
                &serde_json::to_vec(&value).unwrap(),
            );
            let mut observation = CollectorPrivateStorageObservation::capture(&layout).unwrap();
            assert_eq!(
                matches(&mut observation, &layout, &path),
                Err(Error::InvalidContent)
            );
            fs::remove_dir_all(layout.root).unwrap();
        }
    }

    #[test]
    fn candidate_identity_privacy_links_and_size_are_checked() {
        for scenario in 0..4 {
            let layout = fixture();
            let path = write_artifact(
                &layout,
                Kind::Detail,
                &"a".repeat(64),
                &detail_bytes(&"a".repeat(64)),
            );
            let mut observation = CollectorPrivateStorageObservation::capture(&layout).unwrap();
            let candidate = File::open(&path).unwrap();
            match scenario {
                0 => {
                    fs::rename(&path, path.with_extension("old")).unwrap();
                    fs::write(&path, detail_bytes(&"a".repeat(64))).unwrap();
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
                }
                1 => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
                2 => fs::hard_link(&path, path.with_extension("alias")).unwrap(),
                _ => fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_len(crate::MAX_PRIVATE_TURN_DETAIL_BYTES as u64 + 1)
                    .unwrap(),
            }
            assert!(
                observation
                    .matches_entry(path.strip_prefix(&layout.root).unwrap(), &candidate)
                    .is_err()
            );
            fs::remove_dir_all(layout.root).unwrap();
        }
    }

    #[test]
    fn optional_parents_are_noncreating_and_changes_invalidate_observation() {
        let layout = fixture();
        let observation = CollectorPrivateStorageObservation::capture(&layout).unwrap();
        assert!(
            !layout
                .state
                .join(crate::PRIVATE_TURN_DETAIL_DIRECTORY)
                .exists()
        );
        observation.revalidate().unwrap();
        write_artifact(
            &layout,
            Kind::Detail,
            &"a".repeat(64),
            &detail_bytes(&"a".repeat(64)),
        );
        assert_eq!(observation.revalidate(), Err(Error::Replaced));
        let observation = CollectorPrivateStorageObservation::capture(&layout).unwrap();
        let directory = layout.state.join(crate::PRIVATE_TURN_DETAIL_DIRECTORY);
        fs::rename(&directory, directory.with_extension("old")).unwrap();
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(observation.revalidate(), Err(Error::Replaced));
        fs::remove_dir_all(layout.root).unwrap();
    }

    #[test]
    fn observation_work_is_bounded_without_retaining_artifact_descriptors() {
        let layout = fixture();
        let path = write_artifact(
            &layout,
            Kind::Detail,
            &"a".repeat(64),
            &detail_bytes(&"a".repeat(64)),
        );
        let mut observation = CollectorPrivateStorageObservation::capture(&layout).unwrap();
        for _ in 0..crate::MAX_PRIVATE_TURN_DETAIL_FILES {
            assert!(matches(&mut observation, &layout, &path).unwrap());
        }
        assert_eq!(
            matches(&mut observation, &layout, &path),
            Err(Error::InvalidContent)
        );
        fs::remove_dir_all(layout.root).unwrap();
    }

    #[test]
    fn valid_status_is_owned_without_a_raw_pair() {
        let root =
            std::env::temp_dir().join(format!("collector-private-owner-{}", std::process::id()));
        assert!(!root.exists());
        let layout = agent_observability_local_runtime::install(&root).unwrap();
        let directory = layout
            .state
            .join(crate::PRIVATE_TURN_DETAIL_STATUS_DIRECTORY);
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let turn_id = format!("id:sha256:{}", "a".repeat(64));
        let path = crate::private_turn_detail_status_path(&directory, &turn_id).unwrap();
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schema_version":crate::PRIVATE_TURN_DETAIL_STATUS_VERSION,
            "turn_id":turn_id, "state":"failed", "code":"capture_disabled"
        }))
        .unwrap();
        fs::write(&path, &bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let mut owner = CollectorPrivateStorageObservation::capture(&layout).unwrap();
        assert!(
            owner
                .matches_entry(
                    path.strip_prefix(&layout.root).unwrap(),
                    &File::open(&path).unwrap()
                )
                .unwrap()
        );
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert!(
            !layout
                .state
                .join(crate::PRIVATE_TURN_DETAIL_DIRECTORY)
                .exists()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
