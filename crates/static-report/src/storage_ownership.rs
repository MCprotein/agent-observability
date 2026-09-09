//! Read-only ownership evidence for the fixed static HTML report artifact.

#[cfg(any(target_os = "linux", target_os = "macos"))]
use super::{
    PRIVATE_O_NOFOLLOW, PRIVATE_O_NONBLOCK, ReportArtifactError, open_private_directory,
    validate_owned_private_file, validate_private_directory_identity, validate_semantic_artifact,
};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use agent_observability_contracts::MAX_REPORT_ARTIFACT_BYTES;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::path::Component;
use std::{
    ffi::OsStr,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

const REPORT_RELATIVE_PATH: &str = "logs/agent-observability-report.html";
const DIRECTORY_RELATIVE_PATHS: [&str; 2] = ["", "logs"];

/// Retained evidence for the exact root, optional logs directory, and fixed report artifact.
///
/// The caller must hold an exclusive all-writer accounting freeze, with every storage writer
/// participating, through capture, matching, final revalidation, and accounting commit.
/// This evidence does not create, write, repair, or activate anything.
/// Its metadata checks detect observed changes but are not an ABA-proof filesystem snapshot
/// against an uncoordinated writer.
pub struct StaticReportStorageOwnershipEvidence {
    root: PathBuf,
    directories: Vec<RetainedDirectory>,
    report: Option<RetainedReport>,
}

struct RetainedDirectory {
    relative: PathBuf,
    file: Option<File>,
}

struct RetainedReport {
    file: File,
    metadata: FileMetadata,
}

#[cfg(unix)]
#[derive(Clone, Copy, Eq, PartialEq)]
struct FileMetadata {
    dev: u64,
    ino: u64,
    mode: u32,
    nlink: u64,
    size: u64,
    blocks: u64,
    mtime: i64,
    mtime_nsec: i64,
    ctime: i64,
    ctime_nsec: i64,
}

#[cfg(not(unix))]
#[derive(Clone, Copy, Eq, PartialEq)]
struct FileMetadata;

impl std::fmt::Debug for StaticReportStorageOwnershipEvidence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StaticReportStorageOwnershipEvidence")
            .finish_non_exhaustive()
    }
}

/// Failure to establish or retain static report ownership evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaticReportStorageOwnershipError {
    Io(io::ErrorKind),
    InvalidContent,
    RootMismatch,
    Replaced,
    InsecurePermissions,
    Symlink,
    Hardlink,
    InvalidType,
    TooLarge,
    UnsupportedPlatform,
}

impl std::fmt::Display for StaticReportStorageOwnershipError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Io(_) => "static report ownership I/O failure",
            Self::InvalidContent => "static report ownership semantic validation failed",
            Self::RootMismatch => "static report ownership root mismatch",
            Self::Replaced => "static report ownership evidence changed",
            Self::InsecurePermissions => "static report ownership path is not private",
            Self::Symlink => "static report ownership refuses symbolic links",
            Self::Hardlink => "static report ownership refuses hard-linked files",
            Self::InvalidType => "static report ownership path has the wrong file type",
            Self::TooLarge => "static report ownership exceeds the 32 MiB bound",
            Self::UnsupportedPlatform => "static report ownership is unsupported",
        })
    }
}

impl std::error::Error for StaticReportStorageOwnershipError {}

impl From<io::Error> for StaticReportStorageOwnershipError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

impl StaticReportStorageOwnershipEvidence {
    /// Captures private evidence for the exact fixed report path without creating any path.
    ///
    /// The report may be absent. If present, its bytes are read within the existing 32 MiB bound,
    /// semantically validated, and dropped before evidence is returned.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-normalized root, unsafe metadata, invalid content, oversized
    /// content, or any observed identity or metadata change.
    pub fn capture(root: &Path) -> Result<Self, StaticReportStorageOwnershipError> {
        capture(root)
    }

    /// Matches only the retained root, logs directory, and fixed report identity.
    ///
    /// Unknown siblings and descendants return `false`. A known path absent during capture or a
    /// known path with changed evidence fails closed.
    ///
    /// # Errors
    ///
    /// Returns an error when a known candidate is unsafe or differs from retained evidence.
    pub fn matches_entry(
        &self,
        relative: &Path,
        candidate: &File,
    ) -> Result<bool, StaticReportStorageOwnershipError> {
        if relative.as_os_str() == OsStr::new(REPORT_RELATIVE_PATH) {
            let Some(expected) = &self.report else {
                return Err(StaticReportStorageOwnershipError::Replaced);
            };
            let metadata = candidate.metadata()?;
            validate_report_metadata(&metadata)?;
            if file_metadata(&metadata) != expected.metadata {
                return Err(StaticReportStorageOwnershipError::Replaced);
            }
            return Ok(true);
        }

        let Some(directory) = self
            .directories
            .iter()
            .find(|directory| directory.relative.as_os_str() == relative.as_os_str())
        else {
            return Ok(false);
        };
        let Some(expected) = &directory.file else {
            return Err(StaticReportStorageOwnershipError::Replaced);
        };
        let metadata = candidate.metadata()?;
        validate_directory_metadata(&metadata)?;
        if !same_identity(&expected.metadata()?, &metadata) {
            return Err(StaticReportStorageOwnershipError::Replaced);
        }
        Ok(true)
    }

    /// Revalidates exact presence, identities, modes, metadata, and report semantics.
    ///
    /// # Errors
    ///
    /// Returns an error when any captured or absent path changed.
    pub fn revalidate(&self) -> Result<(), StaticReportStorageOwnershipError> {
        self.revalidate_directories()?;
        revalidate_report(&self.root.join(REPORT_RELATIVE_PATH), self.report.as_ref())?;
        self.revalidate_directories()
    }

    fn revalidate_directories(&self) -> Result<(), StaticReportStorageOwnershipError> {
        for retained in &self.directories {
            revalidate_optional_directory(
                &self.root.join(&retained.relative),
                retained.file.as_ref(),
            )?;
        }
        Ok(())
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn capture(
    root: &Path,
) -> Result<StaticReportStorageOwnershipEvidence, StaticReportStorageOwnershipError> {
    capture_with_observer(root, || {})
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn capture(
    _root: &Path,
) -> Result<StaticReportStorageOwnershipEvidence, StaticReportStorageOwnershipError> {
    Err(StaticReportStorageOwnershipError::UnsupportedPlatform)
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
fn capture_observing(
    root: &Path,
    after_report_open: impl FnOnce(),
) -> Result<StaticReportStorageOwnershipEvidence, StaticReportStorageOwnershipError> {
    capture_with_observer(root, after_report_open)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn capture_with_observer(
    root: &Path,
    after_report_open: impl FnOnce(),
) -> Result<StaticReportStorageOwnershipEvidence, StaticReportStorageOwnershipError> {
    if !valid_boundary(root) {
        return Err(StaticReportStorageOwnershipError::RootMismatch);
    }

    let mut directories = Vec::with_capacity(DIRECTORY_RELATIVE_PATHS.len());
    for relative in DIRECTORY_RELATIVE_PATHS {
        let relative = PathBuf::from(relative);
        let path = root.join(&relative);
        let file = if relative.as_os_str().is_empty() {
            Some(open_directory(&path)?)
        } else {
            capture_optional_directory(&path)?
        };
        directories.push(RetainedDirectory { relative, file });
    }

    let report = capture_optional_report(&root.join(REPORT_RELATIVE_PATH), after_report_open)?;
    let evidence = StaticReportStorageOwnershipEvidence {
        root: root.to_path_buf(),
        directories,
        report,
    };
    evidence.revalidate_directories()?;
    Ok(evidence)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn valid_boundary(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
        && normalized_path_bytes(path)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn normalized_path_bytes(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;

    let bytes = path.as_os_str().as_bytes();
    bytes == b"/"
        || (!bytes.ends_with(b"/")
            && bytes
                .split(|byte| *byte == b'/')
                .skip(1)
                .all(|component| !component.is_empty() && component != b"." && component != b".."))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn capture_optional_directory(
    path: &Path,
) -> Result<Option<File>, StaticReportStorageOwnershipError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_named_directory_metadata(&metadata)?;
            Ok(Some(open_directory(path)?))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn capture_optional_report(
    path: &Path,
    after_report_open: impl FnOnce(),
) -> Result<Option<RetainedReport>, StaticReportStorageOwnershipError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_named_report_metadata(&metadata)?;
            let file = open_report(path)?;
            let expected = file_metadata(&file.metadata()?);
            after_report_open();
            validate_report_read(&file, path, expected)?;
            Ok(Some(RetainedReport {
                file,
                metadata: expected,
            }))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn revalidate_optional_directory(
    path: &Path,
    expected: Option<&File>,
) -> Result<(), StaticReportStorageOwnershipError> {
    let Some(expected) = expected else {
        return validate_absence(path);
    };
    let current = open_directory(path)?;
    if !same_identity(&expected.metadata()?, &current.metadata()?) {
        return Err(StaticReportStorageOwnershipError::Replaced);
    }
    Ok(())
}

fn revalidate_report(
    path: &Path,
    expected: Option<&RetainedReport>,
) -> Result<(), StaticReportStorageOwnershipError> {
    let Some(expected) = expected else {
        return validate_absence(path);
    };
    if file_metadata(&expected.file.metadata()?) != expected.metadata {
        return Err(StaticReportStorageOwnershipError::Replaced);
    }
    let current = open_report(path)?;
    if file_metadata(&current.metadata()?) != expected.metadata {
        return Err(StaticReportStorageOwnershipError::Replaced);
    }
    validate_report_read(&current, path, expected.metadata)
}

fn validate_absence(path: &Path) -> Result<(), StaticReportStorageOwnershipError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(StaticReportStorageOwnershipError::Replaced),
        Err(error) => Err(error.into()),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn open_directory(path: &Path) -> Result<File, StaticReportStorageOwnershipError> {
    let named = fs::symlink_metadata(path)?;
    validate_named_directory_metadata(&named)?;
    let file = open_private_directory(path).map_err(map_report_error)?;
    validate_directory_metadata(&file.metadata()?)?;
    validate_private_directory_identity(&file, path).map_err(map_report_error)?;
    if !same_identity(&named, &file.metadata()?) {
        return Err(StaticReportStorageOwnershipError::Replaced);
    }
    Ok(file)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn open_directory(_path: &Path) -> Result<File, StaticReportStorageOwnershipError> {
    Err(StaticReportStorageOwnershipError::UnsupportedPlatform)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn open_report(path: &Path) -> Result<File, StaticReportStorageOwnershipError> {
    use std::os::unix::fs::OpenOptionsExt;

    let named = fs::symlink_metadata(path)?;
    validate_named_report_metadata(&named)?;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(PRIVATE_O_NOFOLLOW | PRIVATE_O_NONBLOCK)
        .open(path)?;
    validate_owned_private_file(&file, path).map_err(map_report_error)?;
    let opened = file.metadata()?;
    validate_report_metadata(&opened)?;
    if !same_identity(&named, &opened) {
        return Err(StaticReportStorageOwnershipError::Replaced);
    }
    Ok(file)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn open_report(_path: &Path) -> Result<File, StaticReportStorageOwnershipError> {
    Err(StaticReportStorageOwnershipError::UnsupportedPlatform)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn validate_report_read(
    file: &File,
    path: &Path,
    expected: FileMetadata,
) -> Result<(), StaticReportStorageOwnershipError> {
    let before = file_metadata(&file.metadata()?);
    if before != expected {
        return Err(StaticReportStorageOwnershipError::Replaced);
    }
    let bytes = read_bounded(file)?;
    let after = file_metadata(&file.metadata()?);
    if after != before {
        return Err(StaticReportStorageOwnershipError::Replaced);
    }
    let validation = validate_semantic_artifact(&bytes).map_err(|error| map_semantic_error(&error));
    drop(bytes);
    validation?;
    validate_owned_private_file(file, path).map_err(map_report_error)?;
    if file_metadata(&file.metadata()?) != expected
        || file_metadata(&fs::symlink_metadata(path)?) != expected
    {
        return Err(StaticReportStorageOwnershipError::Replaced);
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn read_bounded(file: &File) -> Result<Vec<u8>, StaticReportStorageOwnershipError> {
    use std::os::unix::fs::FileExt;

    let limit = usize::try_from(MAX_REPORT_ARTIFACT_BYTES)
        .map_err(|_| StaticReportStorageOwnershipError::TooLarge)?;
    if file.metadata()?.len() > MAX_REPORT_ARTIFACT_BYTES {
        return Err(StaticReportStorageOwnershipError::TooLarge);
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(file.metadata()?.len())
            .unwrap_or(limit)
            .min(limit),
    );
    let mut offset = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let remaining = limit.saturating_add(1).saturating_sub(bytes.len());
        if remaining == 0 {
            return Err(StaticReportStorageOwnershipError::TooLarge);
        }
        let chunk_len = buffer.len().min(remaining);
        let read = file.read_at(&mut buffer[..chunk_len], offset)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        offset = offset.saturating_add(read as u64);
    }
    if bytes.len() > limit {
        return Err(StaticReportStorageOwnershipError::TooLarge);
    }
    Ok(bytes)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn validate_named_directory_metadata(
    metadata: &fs::Metadata,
) -> Result<(), StaticReportStorageOwnershipError> {
    if metadata.file_type().is_symlink() {
        return Err(StaticReportStorageOwnershipError::Symlink);
    }
    validate_directory_metadata(metadata)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn validate_named_report_metadata(
    metadata: &fs::Metadata,
) -> Result<(), StaticReportStorageOwnershipError> {
    if metadata.file_type().is_symlink() {
        return Err(StaticReportStorageOwnershipError::Symlink);
    }
    validate_report_metadata(metadata)
}

#[cfg(unix)]
fn validate_directory_metadata(
    metadata: &fs::Metadata,
) -> Result<(), StaticReportStorageOwnershipError> {
    use std::os::unix::fs::PermissionsExt;

    if !metadata.is_dir() {
        return Err(StaticReportStorageOwnershipError::InvalidType);
    }
    if metadata.permissions().mode() & 0o7777 != 0o700 {
        return Err(StaticReportStorageOwnershipError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_directory_metadata(
    _metadata: &fs::Metadata,
) -> Result<(), StaticReportStorageOwnershipError> {
    Err(StaticReportStorageOwnershipError::UnsupportedPlatform)
}

#[cfg(unix)]
fn validate_report_metadata(
    metadata: &fs::Metadata,
) -> Result<(), StaticReportStorageOwnershipError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if !metadata.is_file() {
        return Err(StaticReportStorageOwnershipError::InvalidType);
    }
    if metadata.permissions().mode() & 0o7777 != 0o600 {
        return Err(StaticReportStorageOwnershipError::InsecurePermissions);
    }
    if metadata.nlink() != 1 {
        return Err(StaticReportStorageOwnershipError::Hardlink);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_report_metadata(
    _metadata: &fs::Metadata,
) -> Result<(), StaticReportStorageOwnershipError> {
    Err(StaticReportStorageOwnershipError::UnsupportedPlatform)
}

#[cfg(unix)]
fn file_metadata(metadata: &fs::Metadata) -> FileMetadata {
    use std::os::unix::fs::MetadataExt;

    FileMetadata {
        dev: metadata.dev(),
        ino: metadata.ino(),
        mode: metadata.mode(),
        nlink: metadata.nlink(),
        size: metadata.size(),
        blocks: metadata.blocks(),
        mtime: metadata.mtime(),
        mtime_nsec: metadata.mtime_nsec(),
        ctime: metadata.ctime(),
        ctime_nsec: metadata.ctime_nsec(),
    }
}

#[cfg(not(unix))]
fn file_metadata(_metadata: &fs::Metadata) -> FileMetadata {
    FileMetadata
}

#[cfg(unix)]
fn same_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    (left.dev(), left.ino()) == (right.dev(), right.ino())
}

#[cfg(not(unix))]
fn same_identity(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn map_semantic_error(error: &ReportArtifactError) -> StaticReportStorageOwnershipError {
    match error {
        ReportArtifactError::TooLarge => StaticReportStorageOwnershipError::TooLarge,
        _ => StaticReportStorageOwnershipError::InvalidContent,
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn map_report_error(error: ReportArtifactError) -> StaticReportStorageOwnershipError {
    match error {
        ReportArtifactError::Io(error) => StaticReportStorageOwnershipError::Io(error.kind()),
        ReportArtifactError::InsecurePermissions => {
            StaticReportStorageOwnershipError::InsecurePermissions
        }
        ReportArtifactError::Symlink => StaticReportStorageOwnershipError::Symlink,
        ReportArtifactError::TooLarge => StaticReportStorageOwnershipError::TooLarge,
        ReportArtifactError::UnsupportedPlatform => {
            StaticReportStorageOwnershipError::UnsupportedPlatform
        }
        ReportArtifactError::InvalidPath => StaticReportStorageOwnershipError::Replaced,
        _ => StaticReportStorageOwnershipError::InvalidContent,
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;
    use agent_observability_contracts::{MAX_REPORT_ARTIFACT_BYTES, ReportDtoV2};
    use std::{
        fs::{self, File, OpenOptions},
        io::Write as _,
        os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink},
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "static-report-owner-{label}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            assert!(!root.exists());
            fs::create_dir(&root).unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            Self { root }
        }

        fn with_logs(label: &str) -> Self {
            let fixture = Self::new(label);
            fs::create_dir(fixture.logs()).unwrap();
            fs::set_permissions(fixture.logs(), fs::Permissions::from_mode(0o700)).unwrap();
            fixture
        }

        fn logs(&self) -> PathBuf {
            self.root.join("logs")
        }

        fn report(&self) -> PathBuf {
            self.logs().join("agent-observability-report.html")
        }

        fn write(&self, bytes: &[u8]) {
            write_private(&self.report(), bytes);
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.root);
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn write_private(path: &Path, bytes: &[u8]) {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn rendered_report() -> Vec<u8> {
        let report: ReportDtoV2 = serde_json::from_slice(include_bytes!(
            "../../../contracts/report-dto-v2.fixture.json"
        ))
        .unwrap();
        crate::render(&report).unwrap().into_bytes()
    }

    fn open(path: &Path) -> File {
        OpenOptions::new().read(true).open(path).unwrap()
    }

    #[test]
    fn captures_current_render_and_pending_placeholder_without_retaining_content() {
        for (label, bytes) in [
            ("render", rendered_report()),
            ("pending", crate::PENDING.as_bytes().to_vec()),
        ] {
            let fixture = Fixture::with_logs(label);
            fixture.write(&bytes);

            let evidence = StaticReportStorageOwnershipEvidence::capture(&fixture.root).unwrap();
            assert!(
                evidence
                    .matches_entry(Path::new(""), &open(&fixture.root))
                    .unwrap()
            );
            assert!(
                evidence
                    .matches_entry(Path::new("logs"), &open(&fixture.logs()))
                    .unwrap()
            );
            assert!(
                evidence
                    .matches_entry(Path::new(REPORT_RELATIVE_PATH), &open(&fixture.report()))
                    .unwrap()
            );
            assert_eq!(
                format!("{evidence:?}"),
                "StaticReportStorageOwnershipEvidence { .. }"
            );
            evidence.revalidate().unwrap();
        }
    }

    #[test]
    fn rejects_malformed_and_shell_mutated_artifacts() {
        let fixture = Fixture::with_logs("invalid-content");
        fixture.write(b"not html");
        assert_eq!(
            StaticReportStorageOwnershipEvidence::capture(&fixture.root).unwrap_err(),
            StaticReportStorageOwnershipError::InvalidContent
        );

        let mutated = String::from_utf8(rendered_report()).unwrap().replacen(
            "<main class=\"wrap\">",
            "<main class=\"wrap changed\">",
            1,
        );
        fixture.write(mutated.as_bytes());
        assert_eq!(
            StaticReportStorageOwnershipEvidence::capture(&fixture.root).unwrap_err(),
            StaticReportStorageOwnershipError::InvalidContent
        );
    }

    #[test]
    fn retains_optional_absence_without_creating_and_rejects_appearance() {
        let fixture = Fixture::new("absence");
        let evidence = StaticReportStorageOwnershipEvidence::capture(&fixture.root).unwrap();
        assert!(!fixture.logs().exists());
        assert!(!fixture.report().exists());
        assert_eq!(
            evidence.matches_entry(Path::new("logs"), &open(&fixture.root)),
            Err(StaticReportStorageOwnershipError::Replaced)
        );

        fs::create_dir(fixture.logs()).unwrap();
        fs::set_permissions(fixture.logs(), fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            evidence.revalidate(),
            Err(StaticReportStorageOwnershipError::Replaced)
        );

        let fixture = Fixture::with_logs("report-absence");
        let evidence = StaticReportStorageOwnershipEvidence::capture(&fixture.root).unwrap();
        assert!(!fixture.report().exists());
        assert_eq!(
            evidence.matches_entry(Path::new(REPORT_RELATIVE_PATH), &open(&fixture.root)),
            Err(StaticReportStorageOwnershipError::Replaced)
        );
        fixture.write(crate::PENDING.as_bytes());
        assert_eq!(
            evidence.revalidate(),
            Err(StaticReportStorageOwnershipError::Replaced)
        );
    }

    #[test]
    fn unknown_siblings_and_wrong_root_candidates_are_not_owned() {
        let fixture = Fixture::with_logs("unknown");
        fixture.write(crate::PENDING.as_bytes());
        let evidence = StaticReportStorageOwnershipEvidence::capture(&fixture.root).unwrap();
        let unknown = fixture.root.join("unknown");
        write_private(&unknown, b"unknown");

        assert!(
            !evidence
                .matches_entry(Path::new("unknown"), &open(&unknown))
                .unwrap()
        );
        assert!(
            !evidence
                .matches_entry(Path::new("../logs"), &open(&fixture.logs()))
                .unwrap()
        );
        assert!(
            !evidence
                .matches_entry(
                    Path::new("logs/./agent-observability-report.html"),
                    &open(&fixture.report())
                )
                .unwrap()
        );
        assert_eq!(
            evidence.matches_entry(Path::new(REPORT_RELATIVE_PATH), &open(&unknown)),
            Err(StaticReportStorageOwnershipError::Replaced)
        );
        assert_eq!(
            StaticReportStorageOwnershipEvidence::capture(Path::new("relative")).unwrap_err(),
            StaticReportStorageOwnershipError::RootMismatch
        );
        assert_eq!(
            StaticReportStorageOwnershipEvidence::capture(&fixture.root.join(".")).unwrap_err(),
            StaticReportStorageOwnershipError::RootMismatch
        );
    }

    #[test]
    fn requires_exact_private_modes_for_root_logs_and_report() {
        for (label, target) in [("root-mode", 0), ("logs-mode", 1), ("file-mode", 2)] {
            let fixture = Fixture::with_logs(label);
            fixture.write(crate::PENDING.as_bytes());
            let path = [&fixture.root, &fixture.logs(), &fixture.report()][target].clone();
            let mode = if target == 2 { 0o640 } else { 0o750 };
            fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
            assert_eq!(
                StaticReportStorageOwnershipEvidence::capture(&fixture.root).unwrap_err(),
                StaticReportStorageOwnershipError::InsecurePermissions
            );
        }
    }

    #[test]
    fn rejects_report_symlink_hardlink_and_fifo() {
        let symlink_fixture = Fixture::with_logs("symlink");
        let target = symlink_fixture.logs().join("target");
        write_private(&target, crate::PENDING.as_bytes());
        symlink(&target, symlink_fixture.report()).unwrap();
        assert_eq!(
            StaticReportStorageOwnershipEvidence::capture(&symlink_fixture.root).unwrap_err(),
            StaticReportStorageOwnershipError::Symlink
        );

        let hardlink_fixture = Fixture::with_logs("hardlink");
        hardlink_fixture.write(crate::PENDING.as_bytes());
        fs::hard_link(
            hardlink_fixture.report(),
            hardlink_fixture.logs().join("alias"),
        )
        .unwrap();
        assert_eq!(
            StaticReportStorageOwnershipEvidence::capture(&hardlink_fixture.root).unwrap_err(),
            StaticReportStorageOwnershipError::Hardlink
        );

        let fifo_fixture = Fixture::with_logs("fifo");
        assert!(
            Command::new("mkfifo")
                .arg(fifo_fixture.report())
                .status()
                .unwrap()
                .success()
        );
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "storage_ownership::tests::fifo_capture_probe",
                "--nocapture",
            ])
            .env("STATIC_REPORT_OWNER_FIFO", &fifo_fixture.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let _ = child.wait();
                panic!("static report FIFO capture blocked");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn fifo_capture_probe() {
        let Some(root) = std::env::var_os("STATIC_REPORT_OWNER_FIFO") else {
            return;
        };
        assert_eq!(
            StaticReportStorageOwnershipEvidence::capture(Path::new(&root)).unwrap_err(),
            StaticReportStorageOwnershipError::InvalidType
        );
    }

    #[test]
    fn rejects_replacement_disappearance_and_in_place_change() {
        let replacement = Fixture::with_logs("replacement");
        replacement.write(crate::PENDING.as_bytes());
        let evidence = StaticReportStorageOwnershipEvidence::capture(&replacement.root).unwrap();
        let displaced = replacement.logs().join("displaced");
        fs::rename(replacement.report(), &displaced).unwrap();
        replacement.write(crate::PENDING.as_bytes());
        assert_eq!(
            evidence.revalidate(),
            Err(StaticReportStorageOwnershipError::Replaced)
        );

        let disappearance = Fixture::with_logs("disappearance");
        disappearance.write(crate::PENDING.as_bytes());
        let evidence = StaticReportStorageOwnershipEvidence::capture(&disappearance.root).unwrap();
        fs::remove_file(disappearance.report()).unwrap();
        assert_eq!(
            evidence.revalidate(),
            Err(StaticReportStorageOwnershipError::Replaced)
        );

        let in_place = Fixture::with_logs("in-place");
        in_place.write(crate::PENDING.as_bytes());
        let evidence = StaticReportStorageOwnershipEvidence::capture(&in_place.root).unwrap();
        in_place.write(b"changed in place");
        assert_eq!(
            evidence.revalidate(),
            Err(StaticReportStorageOwnershipError::Replaced)
        );
    }

    #[test]
    fn capture_rejects_replacement_and_in_place_changes_during_read() {
        for replace in [false, true] {
            let fixture = Fixture::with_logs(if replace {
                "capture-replace"
            } else {
                "capture-write"
            });
            fixture.write(crate::PENDING.as_bytes());
            let result = capture_observing(&fixture.root, || {
                if replace {
                    fs::rename(fixture.report(), fixture.logs().join("displaced")).unwrap();
                }
                fixture.write(b"changed during capture");
            });
            assert_eq!(
                result.unwrap_err(),
                StaticReportStorageOwnershipError::Replaced
            );
        }
    }

    #[test]
    fn bounded_capture_rejects_oversize_artifact() {
        let fixture = Fixture::with_logs("oversize");
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(fixture.report())
            .unwrap();
        file.set_len(MAX_REPORT_ARTIFACT_BYTES + 1).unwrap();
        assert_eq!(
            StaticReportStorageOwnershipEvidence::capture(&fixture.root).unwrap_err(),
            StaticReportStorageOwnershipError::TooLarge
        );
    }
}
