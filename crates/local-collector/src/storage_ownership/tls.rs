//! Bounded ownership of the current settings-referenced TLS generation.

use super::{
    CollectorStorageOwnershipError as Error, OwnedFileKind, capture_optional, open_directory,
    open_file, read_bounded, same_identity, validate_directory_metadata, validate_file_metadata,
};
use agent_observability_local_runtime::InstalledLayout;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Read-only evidence for exactly the active v3 generation, not arbitrary TLS descendants.
/// The caller must retain the outer accounting freeze through capture and final revalidation.
/// Bounded credential bytes remain private in memory and are never exposed by this API or Debug.
pub struct CollectorTlsOwnershipEvidence {
    root: PathBuf,
    settings: File,
    settings_bytes: Vec<u8>,
    entries: Vec<TlsEntry>,
}

struct TlsEntry {
    relative: PathBuf,
    file: File,
    kind: EntryKind,
}

enum EntryKind {
    Directory,
    Credential(Vec<u8>),
}

impl std::fmt::Debug for CollectorTlsOwnershipEvidence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CollectorTlsOwnershipEvidence")
            .finish_non_exhaustive()
    }
}

impl CollectorTlsOwnershipEvidence {
    /// Captures current v3 TLS files using retained, bounded, nonblocking descriptors.
    /// Missing settings and valid legacy settings yield no TLS evidence; legacy generations
    /// and unreferenced files remain unknown, not implicitly owned or deleted.
    pub fn capture(layout: &InstalledLayout) -> Result<Option<Self>, Error> {
        let Some(settings) = capture_optional(
            &layout.runtime.join("collector.json"),
            OwnedFileKind::Settings,
        )?
        else {
            return Ok(None);
        };
        let settings_bytes = read_bounded(&settings, crate::MAX_SETTINGS_BYTES)?;
        let Ok(metadata) = crate::parse_owned_settings(&settings_bytes) else {
            // Validate this exact read too; an earlier valid read is not evidence for it.
            super::validate_settings_metadata(&settings_bytes)?;
            return Ok(None);
        };
        let mut entries = Vec::with_capacity(8);
        for relative in [
            PathBuf::from("runtime"),
            PathBuf::from("runtime/integrations"),
            PathBuf::from("runtime/integrations/codex"),
            PathBuf::from("runtime").join(crate::TLS_DIRECTORY),
            PathBuf::from("runtime")
                .join(crate::TLS_DIRECTORY)
                .join(&metadata.generation),
        ] {
            entries.push(TlsEntry {
                file: open_directory(&layout.root.join(&relative))?,
                relative,
                kind: EntryKind::Directory,
            });
        }
        for credential in [
            &metadata.credentials.ca_certificate,
            &metadata.credentials.server_certificate,
            &metadata.credentials.server_private_key,
        ] {
            let relative = PathBuf::from("runtime").join(credential);
            let file = open_file(&layout.root.join(&relative))?;
            let bytes = read_bounded(&file, crate::MAX_CREDENTIAL_BYTES)?;
            entries.push(TlsEntry {
                relative,
                file,
                kind: EntryKind::Credential(bytes),
            });
        }
        let bytes = |index: usize| match &entries[index].kind {
            EntryKind::Credential(bytes) => bytes.as_slice(),
            EntryKind::Directory => unreachable!("fixed credential positions"),
        };
        validate_credentials(bytes(5), bytes(6), bytes(7))?;
        let evidence = Self {
            root: layout.root.clone(),
            settings,
            settings_bytes,
            entries,
        };
        evidence.revalidate()?;
        Ok(Some(evidence))
    }

    /// Matches exact captured descriptors only; a known path with another identity fails closed.
    pub fn matches_entry(&self, relative: &Path, candidate: &File) -> Result<bool, Error> {
        let Some(entry) = self.entries.iter().find(|entry| entry.relative == relative) else {
            return Ok(false);
        };
        let metadata = candidate.metadata()?;
        match &entry.kind {
            EntryKind::Directory => validate_directory_metadata(&metadata)?,
            EntryKind::Credential(_) => validate_file_metadata(&metadata)?,
        }
        if !same_identity(&entry.file.metadata()?, &metadata) {
            return Err(Error::Replaced);
        }
        Ok(true)
    }

    /// Verifies settings binding, all ancestor identities and exact credential bytes again.
    pub fn revalidate(&self) -> Result<(), Error> {
        self.revalidate_directories()?;
        self.revalidate_settings()?;
        for entry in &self.entries {
            if let EntryKind::Credential(expected) = &entry.kind {
                let current = open_file(&self.root.join(&entry.relative))?;
                if !same_identity(&entry.file.metadata()?, &current.metadata()?)
                    || read_bounded(&current, crate::MAX_CREDENTIAL_BYTES)? != *expected
                {
                    return Err(Error::Replaced);
                }
            }
        }
        self.revalidate_settings()?;
        self.revalidate_directories()
    }

    fn revalidate_settings(&self) -> Result<(), Error> {
        let settings = open_file(&self.root.join("runtime/collector.json"))?;
        if !same_identity(&self.settings.metadata()?, &settings.metadata()?)
            || read_bounded(&settings, crate::MAX_SETTINGS_BYTES)? != self.settings_bytes
        {
            return Err(Error::Replaced);
        }
        Ok(())
    }

    fn revalidate_directories(&self) -> Result<(), Error> {
        for entry in &self.entries {
            if matches!(entry.kind, EntryKind::Directory) {
                let current = open_directory(&self.root.join(&entry.relative))?;
                if !same_identity(&entry.file.metadata()?, &current.metadata()?) {
                    return Err(Error::Replaced);
                }
            }
        }
        Ok(())
    }
}

fn validate_credentials(ca: &[u8], certificate: &[u8], key: &[u8]) -> Result<(), Error> {
    let certificates = |bytes| {
        CertificateDer::pem_slice_iter(bytes)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Error::InvalidContent)
    };
    let ca = certificates(ca)?
        .into_iter()
        .next()
        .ok_or(Error::InvalidContent)?;
    crate::root_store(ca).map_err(|_| Error::InvalidContent)?;
    let certificates = certificates(certificate)?;
    if certificates.is_empty() {
        return Err(Error::InvalidContent);
    }
    let key = PrivateKeyDer::from_pem_slice(key).map_err(|_| Error::InvalidContent)?;
    rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|_| Error::InvalidContent)?
    .with_no_client_auth()
    .with_single_cert(certificates, key)
    .map_err(|_| Error::InvalidContent)?;
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

    struct Fixture(InstalledLayout);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "collector-tls-owner-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            assert!(!root.exists());
            Self(agent_observability_local_runtime::install(&root).unwrap())
        }

        fn installed() -> Self {
            let fixture = Self::new();
            crate::install_settings(&fixture.0.root).unwrap();
            fixture
        }

        fn evidence(&self) -> CollectorTlsOwnershipEvidence {
            CollectorTlsOwnershipEvidence::capture(&self.0)
                .unwrap()
                .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0.root).unwrap();
        }
    }

    #[test]
    fn current_tls_generation_has_owner_evidence() {
        let fixture = Fixture::installed();
        let evidence = fixture.evidence();
        assert_eq!(evidence.entries.len(), 8);
        for entry in &evidence.entries {
            assert!(
                evidence
                    .matches_entry(&entry.relative, &entry.file)
                    .unwrap()
            );
        }
        assert!(
            !evidence
                .matches_entry(
                    Path::new("runtime/integrations/codex/tls/unreferenced/server-key.pem"),
                    &evidence.entries[7].file
                )
                .unwrap()
        );
        assert_eq!(
            format!("{evidence:?}"),
            "CollectorTlsOwnershipEvidence { .. }"
        );
        evidence.revalidate().unwrap();
    }

    #[test]
    fn equal_bytes_do_not_authorize_replaced_credential_identity() {
        let fixture = Fixture::installed();
        let evidence = fixture.evidence();
        let entry = &evidence.entries[7];
        let path = fixture.0.root.join(&entry.relative);
        let bytes = fs::read(&path).unwrap();
        fs::rename(&path, path.with_extension("retained")).unwrap();
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            evidence.matches_entry(&entry.relative, &open_file(&path).unwrap()),
            Err(Error::Replaced)
        );
        assert_eq!(evidence.revalidate(), Err(Error::Replaced));
    }

    #[test]
    fn in_place_credential_and_settings_changes_invalidate_binding() {
        for change_settings in [false, true] {
            let fixture = Fixture::installed();
            let evidence = fixture.evidence();
            let path = if change_settings {
                fixture.0.runtime.join("collector.json")
            } else {
                fixture.0.root.join(&evidence.entries[7].relative)
            };
            let mut bytes = fs::read(&path).unwrap();
            bytes.push(b' ');
            fs::write(&path, bytes).unwrap();
            assert_eq!(evidence.revalidate(), Err(Error::Replaced));
        }
    }

    #[test]
    fn missing_or_legacy_settings_do_not_claim_tls_descendants() {
        let fixture = Fixture::new();
        assert!(
            CollectorTlsOwnershipEvidence::capture(&fixture.0)
                .unwrap()
                .is_none()
        );
        let path = fixture.0.runtime.join("collector.json");
        assert!(!path.exists());
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schema_version":"local_collector.v1", "port":4318,
            "token":"b".repeat(64), "source_generation":crate::SOURCE_GENERATION
        }))
        .unwrap();
        fs::write(&path, &bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            CollectorTlsOwnershipEvidence::capture(&fixture.0)
                .unwrap()
                .is_none()
        );
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert!(!fixture.0.runtime.join(crate::TLS_DIRECTORY).exists());
    }

    #[test]
    fn malformed_oversized_and_nonprivate_credentials_fail_closed() {
        for scenario in 0..5 {
            let fixture = Fixture::installed();
            let evidence = fixture.evidence();
            let path = fixture.0.root.join(&evidence.entries[7].relative);
            match scenario {
                0 => fs::write(&path, b"invalid private key").unwrap(),
                1 => fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_len(crate::MAX_CREDENTIAL_BYTES + 1)
                    .unwrap(),
                2 => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
                3 => fs::hard_link(&path, path.with_extension("linked")).unwrap(),
                _ => {
                    fs::remove_file(&path).unwrap();
                }
            }
            assert!(CollectorTlsOwnershipEvidence::capture(&fixture.0).is_err());
            assert!(evidence.revalidate().is_err());
        }
    }

    #[test]
    fn generation_directory_replacement_is_not_authorized() {
        let fixture = Fixture::installed();
        let evidence = fixture.evidence();
        let directory = fixture.0.root.join(&evidence.entries[4].relative);
        fs::rename(&directory, directory.with_extension("old")).unwrap();
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(evidence.revalidate(), Err(Error::Replaced));
    }
}
