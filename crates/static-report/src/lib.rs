//! Self-contained, private HTML artifact assembly for validated report DTOs.

mod semantic_validation;

use agent_observability_contracts::{ContractError, MAX_REPORT_ARTIFACT_BYTES, ReportDtoV2};
use std::fmt::{self, Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const SHELL: &str = include_str!("../../../src/report/generated/report-shell.html");
const PAGED_SHELL: &str = include_str!("../../../src/report/generated/paged-shell.html");
const TITLE_TOKEN: &str = "__AGENT_OBSERVABILITY_REPORT_TITLE__";
const GENERATED_AT_TOKEN: &str = "__AGENT_OBSERVABILITY_REPORT_GENERATED_AT__";
const DATA_TOKEN: &str = "__AGENT_OBSERVABILITY_REPORT_DATA__";
const PENDING: &str = "<!doctype html><html lang=\"ko\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Agent Observability — 갱신 대기</title><main><h1>리포트 갱신 대기</h1><p>데이터 보관 정책을 적용하는 동안 이전 리포트를 숨겼습니다.</p><p>수집기가 실행 중이면 잠시 후 새로고침하세요. 수동 실행 환경에서는 agentobs report 명령으로 다시 만드세요.</p></main></html>";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub use semantic_validation::validate_semantic_artifact;

#[derive(Debug)]
pub enum ReportArtifactError {
    Contract(ContractError),
    Json(serde_json::Error),
    Io(io::Error),
    InvalidArtifact,
    InvalidTemplate,
    InvalidPath,
    InsecurePermissions,
    TooLarge,
    Symlink,
    UnsupportedPlatform,
    Cleanup { primary: Box<ReportArtifactError> },
}

impl Display for ReportArtifactError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Contract(_) => "report DTO does not satisfy its contract",
            Self::Json(_) => "report DTO serialization failed",
            Self::Io(_) => "report artifact I/O failed",
            Self::InvalidArtifact => "static report artifact is not the canonical rendered output",
            Self::InvalidTemplate => "embedded report template is invalid",
            Self::InvalidPath => "report artifact path has the wrong file type",
            Self::InsecurePermissions => "report artifact path is not private",
            Self::TooLarge => "report artifact exceeds the 32 MiB contract",
            Self::Symlink => "report artifact path must not be a symbolic link",
            Self::UnsupportedPlatform => {
                "private report artifacts are unsupported on this platform"
            }
            Self::Cleanup { primary } => {
                return write!(formatter, "{primary}; report artifact cleanup failed");
            }
        })
    }
}

impl std::error::Error for ReportArtifactError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Contract(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Cleanup { primary } => Some(primary.as_ref()),
            _ => None,
        }
    }
}

impl From<io::Error> for ReportArtifactError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Renders one validated DTO into the generated self-contained report shell.
///
/// # Errors
///
/// Returns [`ReportArtifactError`] when the DTO or embedded shell is invalid.
pub fn render(report: &ReportDtoV2) -> Result<String, ReportArtifactError> {
    let mut html = Vec::new();
    write_rendered(&mut html, report)?;
    String::from_utf8(html).map_err(|_| ReportArtifactError::InvalidTemplate)
}

/// Renders the data-free loopback dashboard shell, independently of HTML export capacity.
///
/// This entry point is not a self-contained export and must only be served on the private
/// dashboard origin. No source content, filesystem path or capability is embedded.
///
/// # Errors
/// Returns an error if the generated shell placeholders or fixed shell size are invalid.
pub fn render_paged_dashboard() -> Result<String, ReportArtifactError> {
    if PAGED_SHELL.matches(TITLE_TOKEN).count() != 2
        || PAGED_SHELL.matches(GENERATED_AT_TOKEN).count() != 1
        || PAGED_SHELL.matches(DATA_TOKEN).count() != 1
    {
        return Err(ReportArtifactError::InvalidTemplate);
    }
    let html = PAGED_SHELL
        .replace(TITLE_TOKEN, "Agent Observability")
        .replace(GENERATED_AT_TOKEN, "Loading snapshot…")
        .replace(DATA_TOKEN, r#"{"mode":"paged_dashboard_v1"}"#);
    if html.len() > 1024 * 1024 {
        return Err(ReportArtifactError::TooLarge);
    }
    Ok(html)
}

/// Atomically writes one private report file inside an existing private directory.
///
/// # Errors
///
/// Returns [`ReportArtifactError`] for insecure paths, unsupported platforms, or I/O failures.
pub fn write_private(path: &Path, report: &ReportDtoV2) -> Result<u64, ReportArtifactError> {
    write_private_content(path, |writer| write_rendered(writer, report))
}

/// Replaces a managed report with a data-free placeholder before retention commits.
/// The caller must hold the report publication guard through the destructive transaction.
///
/// # Errors
///
/// Returns [`ReportArtifactError`] without authorizing deletion if private publication fails.
pub fn write_refresh_pending(path: &Path) -> Result<u64, ReportArtifactError> {
    write_private_content(path, |writer| {
        writer.write_all(PENDING.as_bytes())?;
        u64::try_from(PENDING.len()).map_err(|_| ReportArtifactError::TooLarge)
    })
}

fn write_private_content(
    path: &Path,
    render_content: impl FnOnce(&mut dyn Write) -> Result<u64, ReportArtifactError>,
) -> Result<u64, ReportArtifactError> {
    let parent = path.parent().ok_or(ReportArtifactError::InvalidPath)?;
    let directory = open_private_directory(parent)?;
    reject_output_path(path)?;
    let temporary = temporary_path(path)?;
    validate_private_directory_identity(&directory, parent)?;
    let mut file = private_create_new(&temporary)?;
    if let Err(error) = validate_owned_private_file(&file, &temporary) {
        return Err(cleanup_failed_private_write(
            error, &directory, parent, &temporary, &file,
        ));
    }
    let result = (|| -> Result<u64, ReportArtifactError> {
        let mut writer = BufWriter::new(&mut file);
        let bytes = render_content(&mut writer)?;
        writer.flush()?;
        drop(writer);
        validate_artifact_size(bytes)?;
        file.sync_all()?;
        Ok(bytes)
    })();
    let bytes = match result {
        Ok(bytes) => bytes,
        Err(error) => {
            return Err(cleanup_failed_private_write(
                error, &directory, parent, &temporary, &file,
            ));
        }
    };
    match publish_private_file(&directory, parent, &temporary, path, &file) {
        Ok(()) => Ok(bytes),
        Err(PublicationFailure::Unpublished(error)) => Err(cleanup_failed_private_write(
            error, &directory, parent, &temporary, &file,
        )),
        Err(PublicationFailure::Published(error)) => Err(error),
    }
}

enum PublicationFailure {
    Unpublished(ReportArtifactError),
    Published(ReportArtifactError),
}

fn publish_private_file(
    directory: &File,
    parent: &Path,
    temporary: &Path,
    output: &Path,
    file: &File,
) -> Result<(), PublicationFailure> {
    let validate_before_rename = || -> Result<(), ReportArtifactError> {
        validate_private_directory_identity(directory, parent)?;
        validate_owned_private_file(file, temporary)?;
        reject_output_path(output)?;
        validate_private_directory_identity(directory, parent)?;
        validate_owned_private_file(file, temporary)
    };
    validate_before_rename().map_err(PublicationFailure::Unpublished)?;
    fs::rename(temporary, output)
        .map_err(ReportArtifactError::from)
        .map_err(PublicationFailure::Unpublished)?;
    validate_owned_private_file(file, output).map_err(PublicationFailure::Published)?;
    sync_private_directory(directory, parent).map_err(PublicationFailure::Published)?;
    validate_owned_private_file(file, output).map_err(PublicationFailure::Published)
}

fn cleanup_failed_private_write(
    primary: ReportArtifactError,
    directory: &File,
    parent: &Path,
    temporary: &Path,
    file: &File,
) -> ReportArtifactError {
    cleanup_failed_private_write_observing(primary, directory, parent, temporary, file, |_| {})
}

fn cleanup_failed_private_write_observing(
    primary: ReportArtifactError,
    directory: &File,
    parent: &Path,
    temporary: &Path,
    file: &File,
    after_unlink: impl FnOnce(&Path),
) -> ReportArtifactError {
    if cleanup_owned_temporary_observing(directory, parent, temporary, file, after_unlink).is_err()
    {
        ReportArtifactError::Cleanup {
            primary: Box::new(primary),
        }
    } else {
        primary
    }
}

fn cleanup_owned_temporary_observing(
    directory: &File,
    parent: &Path,
    temporary: &Path,
    file: &File,
    after_unlink: impl FnOnce(&Path),
) -> Result<(), ReportArtifactError> {
    validate_private_directory_identity(directory, parent)?;
    validate_owned_private_file(file, temporary)?;
    validate_private_directory_identity(directory, parent)?;
    fs::remove_file(temporary)?;
    after_unlink(temporary);
    validate_unlinked_owned_file(file)?;
    validate_path_absent(temporary)?;
    sync_private_directory(directory, parent)?;
    validate_path_absent(temporary)
}

fn validate_path_absent(path: &Path) -> Result<(), ReportArtifactError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(ReportArtifactError::InvalidPath),
        Err(error) => Err(error.into()),
    }
}

fn sync_private_directory(directory: &File, path: &Path) -> Result<(), ReportArtifactError> {
    validate_private_directory_identity(directory, path)?;
    directory.sync_all()?;
    validate_private_directory_identity(directory, path)
}

fn validate_artifact_size(bytes: u64) -> Result<(), ReportArtifactError> {
    if bytes > MAX_REPORT_ARTIFACT_BYTES {
        Err(ReportArtifactError::TooLarge)
    } else {
        Ok(())
    }
}

fn write_rendered(
    mut output: impl Write,
    report: &ReportDtoV2,
) -> Result<u64, ReportArtifactError> {
    report.validate().map_err(ReportArtifactError::Contract)?;
    if SHELL.matches(TITLE_TOKEN).count() != 2
        || SHELL.matches(GENERATED_AT_TOKEN).count() != 1
        || SHELL.matches(DATA_TOKEN).count() != 1
    {
        return Err(ReportArtifactError::InvalidTemplate);
    }
    let title = escape_html(&report.title);
    let generated_at = escape_html(&report.generated_at);
    let mut remaining = SHELL;
    let mut bytes = 0_u64;
    while !remaining.is_empty() {
        let next = [TITLE_TOKEN, GENERATED_AT_TOKEN, DATA_TOKEN]
            .into_iter()
            .filter_map(|token| remaining.find(token).map(|offset| (offset, token)))
            .min_by_key(|(offset, _)| *offset);
        let Some((offset, token)) = next else {
            write_counted(&mut output, remaining.as_bytes(), &mut bytes)?;
            break;
        };
        write_counted(&mut output, &remaining.as_bytes()[..offset], &mut bytes)?;
        match token {
            TITLE_TOKEN => write_counted(&mut output, title.as_bytes(), &mut bytes)?,
            GENERATED_AT_TOKEN => {
                write_counted(&mut output, generated_at.as_bytes(), &mut bytes)?;
            }
            DATA_TOKEN => {
                let mut escaped = JsonScriptWriter::new(&mut output, &mut bytes);
                serde_json::to_writer(&mut escaped, report).map_err(ReportArtifactError::Json)?;
            }
            _ => return Err(ReportArtifactError::InvalidTemplate),
        }
        remaining = &remaining[offset + token.len()..];
    }
    Ok(bytes)
}

fn write_counted(output: &mut impl Write, value: &[u8], bytes: &mut u64) -> Result<(), io::Error> {
    output.write_all(value)?;
    *bytes = bytes.saturating_add(value.len() as u64);
    Ok(())
}

struct JsonScriptWriter<'a, W> {
    output: &'a mut W,
    bytes: &'a mut u64,
}

impl<'a, W> JsonScriptWriter<'a, W> {
    fn new(output: &'a mut W, bytes: &'a mut u64) -> Self {
        Self { output, bytes }
    }
}

impl<W: Write> Write for JsonScriptWriter<'_, W> {
    fn write(&mut self, value: &[u8]) -> io::Result<usize> {
        let mut start = 0;
        for (index, byte) in value.iter().enumerate() {
            let escaped: &[u8] = match byte {
                b'&' => b"\\u0026",
                b'<' => b"\\u003c",
                b'>' => b"\\u003e",
                _ => continue,
            };
            if start < index {
                self.output.write_all(&value[start..index])?;
                *self.bytes = self.bytes.saturating_add((index - start) as u64);
            }
            self.output.write_all(escaped)?;
            *self.bytes = self.bytes.saturating_add(escaped.len() as u64);
            start = index + 1;
        }
        if start < value.len() {
            self.output.write_all(&value[start..])?;
            *self.bytes = self.bytes.saturating_add((value.len() - start) as u64);
        }
        Ok(value.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

fn temporary_path(path: &Path) -> Result<PathBuf, ReportArtifactError> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(ReportArtifactError::InvalidPath)?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(path.with_file_name(format!(".{name}.tmp.{}.{}", std::process::id(), sequence)))
}

fn reject_output_path(path: &Path) -> Result<(), ReportArtifactError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(ReportArtifactError::Symlink),
        Ok(metadata) if !metadata.is_file() => Err(ReportArtifactError::InvalidPath),
        Ok(_) => private_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn private_directory(path: &Path) -> Result<(), ReportArtifactError> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(ReportArtifactError::Symlink);
    }
    if !metadata.is_dir() {
        return Err(ReportArtifactError::InvalidPath);
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(ReportArtifactError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(all(
    target_os = "linux",
    any(
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "m68k",
        target_arch = "powerpc",
        target_arch = "powerpc64"
    )
))]
const PRIVATE_O_NOFOLLOW: i32 = 0x8000;

#[cfg(all(
    target_os = "linux",
    not(any(
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "m68k",
        target_arch = "powerpc",
        target_arch = "powerpc64"
    ))
))]
const PRIVATE_O_NOFOLLOW: i32 = 0x20_000;

#[cfg(all(
    target_os = "linux",
    any(
        target_arch = "mips",
        target_arch = "mips32r6",
        target_arch = "mips64",
        target_arch = "mips64r6"
    )
))]
const PRIVATE_O_NONBLOCK: i32 = 0x80;

#[cfg(all(
    target_os = "linux",
    any(target_arch = "sparc", target_arch = "sparc64")
))]
const PRIVATE_O_NONBLOCK: i32 = 0x4000;

#[cfg(all(
    target_os = "linux",
    not(any(
        target_arch = "mips",
        target_arch = "mips32r6",
        target_arch = "mips64",
        target_arch = "mips64r6",
        target_arch = "sparc",
        target_arch = "sparc64"
    ))
))]
const PRIVATE_O_NONBLOCK: i32 = 0x800;

#[cfg(all(
    target_os = "android",
    any(target_arch = "aarch64", target_arch = "arm")
))]
const PRIVATE_O_NOFOLLOW: i32 = 0x8000;

#[cfg(all(target_os = "android", target_arch = "riscv64"))]
const PRIVATE_O_NOFOLLOW: i32 = 0x40_0000;

#[cfg(all(
    target_os = "android",
    not(any(target_arch = "aarch64", target_arch = "arm", target_arch = "riscv64"))
))]
const PRIVATE_O_NOFOLLOW: i32 = 0x20_000;

#[cfg(target_os = "android")]
const PRIVATE_O_NONBLOCK: i32 = 0x800;

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
const PRIVATE_O_NOFOLLOW: i32 = 0x100;

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
const PRIVATE_O_NONBLOCK: i32 = 0x4;

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
fn open_private_directory(path: &Path) -> Result<File, ReportArtifactError> {
    open_private_directory_observing(path, |_| {})
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
fn open_private_directory_observing(
    path: &Path,
    before_open: impl FnOnce(&Path),
) -> Result<File, ReportArtifactError> {
    use std::os::unix::fs::OpenOptionsExt;

    private_directory(path)?;
    before_open(path);
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(PRIVATE_O_NOFOLLOW | PRIVATE_O_NONBLOCK)
        .open(path)?;
    validate_private_directory_identity(&directory, path)?;
    Ok(directory)
}

#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))
))]
fn open_private_directory(_path: &Path) -> Result<File, ReportArtifactError> {
    Err(ReportArtifactError::UnsupportedPlatform)
}

#[cfg(not(unix))]
fn open_private_directory(_path: &Path) -> Result<File, ReportArtifactError> {
    Err(ReportArtifactError::UnsupportedPlatform)
}

#[cfg(unix)]
fn validate_private_directory_identity(
    directory: &File,
    path: &Path,
) -> Result<(), ReportArtifactError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let held = directory.metadata()?;
    let named = fs::symlink_metadata(path)?;
    if named.file_type().is_symlink() {
        return Err(ReportArtifactError::Symlink);
    }
    if !held.is_dir()
        || !named.is_dir()
        || held.permissions().mode() & 0o077 != 0
        || named.permissions().mode() & 0o077 != 0
        || (held.dev(), held.ino()) != (named.dev(), named.ino())
    {
        return Err(ReportArtifactError::InvalidPath);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_directory_identity(
    _directory: &File,
    _path: &Path,
) -> Result<(), ReportArtifactError> {
    Err(ReportArtifactError::UnsupportedPlatform)
}

#[cfg(unix)]
fn validate_owned_private_file(file: &File, path: &Path) -> Result<(), ReportArtifactError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let held = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    if named.file_type().is_symlink() {
        return Err(ReportArtifactError::Symlink);
    }
    if !held.is_file()
        || !named.is_file()
        || held.permissions().mode() & 0o077 != 0
        || named.permissions().mode() & 0o077 != 0
        || held.nlink() != 1
        || named.nlink() != 1
        || (held.dev(), held.ino()) != (named.dev(), named.ino())
    {
        return Err(ReportArtifactError::InvalidPath);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_owned_private_file(_file: &File, _path: &Path) -> Result<(), ReportArtifactError> {
    Err(ReportArtifactError::UnsupportedPlatform)
}

#[cfg(unix)]
fn validate_unlinked_owned_file(file: &File) -> Result<(), ReportArtifactError> {
    use std::os::unix::fs::MetadataExt;

    if file.metadata()?.nlink() == 0 {
        Ok(())
    } else {
        Err(ReportArtifactError::InvalidPath)
    }
}

#[cfg(not(unix))]
fn validate_unlinked_owned_file(_file: &File) -> Result<(), ReportArtifactError> {
    Err(ReportArtifactError::UnsupportedPlatform)
}

#[cfg(not(unix))]
fn private_directory(_path: &Path) -> Result<(), ReportArtifactError> {
    Err(ReportArtifactError::UnsupportedPlatform)
}

#[cfg(unix)]
fn private_file(path: &Path) -> Result<(), ReportArtifactError> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(ReportArtifactError::Symlink);
    }
    if !metadata.is_file() {
        return Err(ReportArtifactError::InvalidPath);
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(ReportArtifactError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn private_file(_path: &Path) -> Result<(), ReportArtifactError> {
    Err(ReportArtifactError::UnsupportedPlatform)
}

#[cfg(unix)]
fn private_create_new(path: &Path) -> Result<File, ReportArtifactError> {
    use std::os::unix::fs::OpenOptionsExt;

    Ok(OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?)
}

#[cfg(not(unix))]
fn private_create_new(_path: &Path) -> Result<File, ReportArtifactError> {
    Err(ReportArtifactError::UnsupportedPlatform)
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_observability_contracts::{
        CostDetailV1, CostEstimateV1, REPORT_DTO_VERSION, RateTableRefV1, ReportFiltersV1,
        ReportSummaryV1,
    };

    #[cfg(unix)]
    fn private_test_directory(label: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "agent-observability-static-report-{label}-{}-{sequence}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        root
    }

    #[cfg(unix)]
    fn run_bounded_probe(test_name: &str, environment: &str, path: &Path) {
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test_name, "--nocapture"])
            .env(environment, path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let _ = child.wait();
                panic!("private directory probe blocked: {test_name}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(status.success());
    }

    #[test]
    fn paged_shell_is_data_free_and_does_not_require_an_export() {
        let html = super::render_paged_dashboard().unwrap();
        assert!(html.contains(r#"{"mode":"paged_dashboard_v1"}"#));
        assert!(!html.contains(super::DATA_TOKEN));
        assert!(!html.contains(super::TITLE_TOKEN));
        assert!(!html.contains(super::GENERATED_AT_TOKEN));
        assert!(html.len() <= 1024 * 1024);
    }

    fn report(title: &str) -> ReportDtoV2 {
        ReportDtoV2 {
            schema_version: REPORT_DTO_VERSION.into(),
            generated_at: "2026-08-29T00:00:00.000Z".into(),
            title: title.into(),
            summary: ReportSummaryV1::default(),
            cost: CostEstimateV1 {
                status: "unknown".into(),
                reason: Some("missing_rate_table".into()),
                rate_table: RateTableRefV1::default(),
                cost: CostDetailV1 {
                    assumption: "No local rate table was supplied.".into(),
                    unknown_count: Some(0),
                    ..CostDetailV1::default()
                },
                ..CostEstimateV1::default()
            },
            filters: ReportFiltersV1::default(),
            traces: Vec::new(),
            spans: Vec::new(),
        }
    }

    #[test]
    fn private_artifact_size_contract_accepts_boundary_and_rejects_overflow() {
        assert!(validate_artifact_size(MAX_REPORT_ARTIFACT_BYTES).is_ok());
        assert!(matches!(
            validate_artifact_size(MAX_REPORT_ARTIFACT_BYTES + 1),
            Err(ReportArtifactError::TooLarge)
        ));
    }

    #[test]
    fn render_embeds_validated_data_without_script_breakout() {
        let report = report("Stable </script><title> & \"report\"");
        let html = render(&report).unwrap();
        let legacy = {
            let data = serde_json::to_string(&report)
                .unwrap()
                .replace('&', "\\u0026")
                .replace('<', "\\u003c")
                .replace('>', "\\u003e");
            SHELL
                .replace(TITLE_TOKEN, &escape_html(&report.title))
                .replace(GENERATED_AT_TOKEN, &escape_html(&report.generated_at))
                .replace(DATA_TOKEN, &data)
        };
        assert_eq!(html, legacy, "streamed renderer changed report bytes");
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("Stable &lt;/script&gt;&lt;title&gt; &amp; &quot;report&quot;"));
        assert!(
            html.contains("Stable \\u003c/script\\u003e\\u003ctitle\\u003e \\u0026 \\\"report\\\"")
        );
        assert!(!html.contains(TITLE_TOKEN));
        assert!(!html.contains(DATA_TOKEN));
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
    }

    #[test]
    fn render_treats_template_token_text_in_values_as_literal_data() {
        let title = format!("{TITLE_TOKEN} {GENERATED_AT_TOKEN} {DATA_TOKEN}");
        let html = render(&report(&title)).unwrap();

        for token in [TITLE_TOKEN, GENERATED_AT_TOKEN, DATA_TOKEN] {
            assert_eq!(
                html.matches(token).count(),
                3,
                "report data containing {token} was interpreted as template syntax"
            );
        }
    }

    fn replace_embedded_report_data(html: &str, replacement: &str) -> Vec<u8> {
        const OPEN: &str = r#"<script id="report-data" type="application/json">"#;
        const CLOSE: &str = "</script>";

        let data_start = html.find(OPEN).unwrap() + OPEN.len();
        let data_end = data_start + html[data_start..].find(CLOSE).unwrap();
        let mut artifact =
            Vec::with_capacity(html.len() - (data_end - data_start) + replacement.len());
        artifact.extend_from_slice(&html.as_bytes()[..data_start]);
        artifact.extend_from_slice(replacement.as_bytes());
        artifact.extend_from_slice(&html.as_bytes()[data_end..]);
        artifact
    }

    #[test]
    fn semantic_validation_accepts_empty_and_populated_current_reports() {
        let empty = render(&report("Empty report")).unwrap();
        assert!(validate_semantic_artifact(empty.as_bytes()).is_ok());

        let populated: ReportDtoV2 = serde_json::from_slice(include_bytes!(
            "../../../contracts/report-dto-v2.fixture.json"
        ))
        .unwrap();
        assert!(!populated.spans.is_empty());
        let populated = render(&populated).unwrap();
        assert!(validate_semantic_artifact(populated.as_bytes()).is_ok());
    }

    #[test]
    fn semantic_validation_accepts_escaped_script_text_and_unicode() {
        let html = render(&report("서울 </script> & 리포트 🚀")).unwrap();

        assert!(validate_semantic_artifact(html.as_bytes()).is_ok());
    }

    #[test]
    fn semantic_validation_accepts_only_the_exact_pending_placeholder() {
        const EXPECTED_PENDING: &str = "<!doctype html><html lang=\"ko\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Agent Observability — 갱신 대기</title><main><h1>리포트 갱신 대기</h1><p>데이터 보관 정책을 적용하는 동안 이전 리포트를 숨겼습니다.</p><p>수집기가 실행 중이면 잠시 후 새로고침하세요. 수동 실행 환경에서는 agentobs report 명령으로 다시 만드세요.</p></main></html>";

        assert!(validate_semantic_artifact(EXPECTED_PENDING.as_bytes()).is_ok());
        assert!(validate_semantic_artifact(format!("{EXPECTED_PENDING}\n").as_bytes()).is_err());
    }

    #[test]
    fn semantic_validation_rejects_malformed_unknown_and_invalid_contract_json() {
        let report = report("JSON contract");
        let html = render(&report).unwrap();
        assert!(validate_semantic_artifact(&replace_embedded_report_data(&html, "{")).is_err());

        let mut unknown = serde_json::to_value(&report).unwrap();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), serde_json::Value::Bool(true));
        assert!(
            validate_semantic_artifact(&replace_embedded_report_data(
                &html,
                &serde_json::to_string(&unknown).unwrap(),
            ))
            .is_err()
        );

        let mut invalid_contract = report;
        invalid_contract.schema_version = "agent_observability.report.v999".into();
        assert!(
            validate_semantic_artifact(&replace_embedded_report_data(
                &html,
                &serde_json::to_string(&invalid_contract).unwrap(),
            ))
            .is_err()
        );
    }

    #[test]
    fn semantic_validation_rejects_shell_title_time_and_trailing_mutations() {
        let html = render(&report("Canonical report")).unwrap();

        let mutated_shell =
            html.replacen("<main class=\"wrap\">", "<main class=\"wrap changed\">", 1);
        assert!(validate_semantic_artifact(mutated_shell.as_bytes()).is_err());

        let mismatched_title = html.replacen("Canonical report", "Different report", 1);
        assert!(validate_semantic_artifact(mismatched_title.as_bytes()).is_err());

        let mismatched_time =
            html.replacen("2026-08-29T00:00:00.000Z", "2026-08-30T00:00:00.000Z", 1);
        assert!(validate_semantic_artifact(mismatched_time.as_bytes()).is_err());

        let mut trailing = html.as_bytes().to_vec();
        trailing.extend_from_slice(b"\n");
        assert!(validate_semantic_artifact(&trailing).is_err());
    }

    #[test]
    fn semantic_validation_rejects_duplicate_report_data_tags_and_oversize_input() {
        let html = render(&report("Duplicate data tag")).unwrap();
        let duplicate = html.replacen(
            "</body>",
            r#"<script id="report-data" type="application/json">{}</script></body>"#,
            1,
        );
        assert!(validate_semantic_artifact(duplicate.as_bytes()).is_err());

        let oversize = vec![b'x'; usize::try_from(MAX_REPORT_ARTIFACT_BYTES).unwrap() + 1];
        assert!(matches!(
            validate_semantic_artifact(&oversize),
            Err(ReportArtifactError::TooLarge)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn write_is_private_and_rejects_broad_parent() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "agent-observability-static-report-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let output = root.join("report.html");
        let private_report = report("Private report");
        let expected = render(&private_report).unwrap();
        assert_eq!(
            write_private(&output, &private_report).unwrap(),
            expected.len() as u64
        );
        assert_eq!(fs::read_to_string(&output).unwrap(), expected);
        super::write_refresh_pending(&output).unwrap();
        let pending = fs::read_to_string(&output).unwrap();
        assert!(pending.contains("갱신 대기"));
        assert!(!pending.contains("Private report"));
        assert!(!pending.contains("<script"));
        assert!(!pending.contains("http://"));
        assert!(!pending.contains("https://"));
        assert_eq!(
            fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let linked = root.join("linked.html");
        std::os::unix::fs::symlink(&output, &linked).unwrap();
        assert!(super::write_refresh_pending(&linked).is_err());
        assert_eq!(fs::read_to_string(&output).unwrap(), pending);

        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            write_private(&output, &report("Rejected report")),
            Err(ReportArtifactError::InsecurePermissions)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn render_error_removes_only_the_owned_temporary_and_preserves_primary_error() {
        let root = private_test_directory("render-error");
        let output = root.join("report.html");

        let error = write_private_content(&output, |_| Err(ReportArtifactError::InvalidTemplate))
            .unwrap_err();

        assert!(matches!(error, ReportArtifactError::InvalidTemplate));
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_preserves_a_foreign_replacement_and_reports_the_primary_error() {
        use std::io::Write as _;

        let root = private_test_directory("cleanup-replacement");
        let directory = open_private_directory(&root).unwrap();
        let temporary = root.join(".report.html.tmp.owned");
        let displaced = root.join("displaced-owned-temp");
        let mut owned = private_create_new(&temporary).unwrap();
        owned.write_all(b"owned").unwrap();
        fs::rename(&temporary, &displaced).unwrap();
        let mut foreign = private_create_new(&temporary).unwrap();
        foreign.write_all(b"FOREIGN_REPORT_TEMP").unwrap();

        let error = cleanup_failed_private_write(
            ReportArtifactError::InvalidTemplate,
            &directory,
            &root,
            &temporary,
            &owned,
        );

        assert!(matches!(
            error,
            ReportArtifactError::Cleanup { ref primary }
                if matches!(primary.as_ref(), ReportArtifactError::InvalidTemplate)
        ));
        assert_eq!(
            error.to_string(),
            "embedded report template is invalid; report artifact cleanup failed"
        );
        assert_eq!(fs::read(&temporary).unwrap(), b"FOREIGN_REPORT_TEMP");
        assert_eq!(fs::read(&displaced).unwrap(), b"owned");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_postcheck_preserves_a_replacement_and_the_primary_error() {
        use std::io::Write as _;

        let root = private_test_directory("cleanup-postcheck");
        let directory = open_private_directory(&root).unwrap();
        let temporary = root.join(".report.html.tmp.owned");
        let owned = private_create_new(&temporary).unwrap();

        let error = cleanup_failed_private_write_observing(
            ReportArtifactError::InvalidTemplate,
            &directory,
            &root,
            &temporary,
            &owned,
            |path| {
                let mut foreign = private_create_new(path).unwrap();
                foreign.write_all(b"FOREIGN_AFTER_UNLINK").unwrap();
            },
        );

        assert!(matches!(
            error,
            ReportArtifactError::Cleanup { ref primary }
                if matches!(primary.as_ref(), ReportArtifactError::InvalidTemplate)
        ));
        assert_eq!(
            error.to_string(),
            "embedded report template is invalid; report artifact cleanup failed"
        );
        assert_eq!(fs::read(&temporary).unwrap(), b"FOREIGN_AFTER_UNLINK");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn publication_rejects_a_foreign_temporary_replacement_without_deleting_it() {
        use std::io::Write as _;

        let root = private_test_directory("publication-replacement");
        let directory = open_private_directory(&root).unwrap();
        let temporary = root.join(".report.html.tmp.owned");
        let output = root.join("report.html");
        let displaced = root.join("displaced-owned-temp");
        let mut owned = private_create_new(&temporary).unwrap();
        owned.write_all(b"owned").unwrap();
        fs::rename(&temporary, &displaced).unwrap();
        let mut foreign = private_create_new(&temporary).unwrap();
        foreign.write_all(b"FOREIGN_REPORT_TEMP").unwrap();

        assert!(publish_private_file(&directory, &root, &temporary, &output, &owned).is_err());
        assert!(!output.exists());
        assert_eq!(fs::read(&temporary).unwrap(), b"FOREIGN_REPORT_TEMP");
        assert_eq!(fs::read(&displaced).unwrap(), b"owned");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn hardlinked_temporary_is_neither_published_nor_cleaned_up() {
        let root = private_test_directory("hardlink");
        let directory = open_private_directory(&root).unwrap();
        let temporary = root.join(".report.html.tmp.owned");
        let alias = root.join("foreign-hardlink");
        let output = root.join("report.html");
        let owned = private_create_new(&temporary).unwrap();
        fs::hard_link(&temporary, &alias).unwrap();

        assert!(publish_private_file(&directory, &root, &temporary, &output, &owned).is_err());
        let error = cleanup_failed_private_write(
            ReportArtifactError::InvalidTemplate,
            &directory,
            &root,
            &temporary,
            &owned,
        );
        assert!(matches!(error, ReportArtifactError::Cleanup { .. }));
        assert!(temporary.exists());
        assert!(alias.exists());
        assert!(!output.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn existing_temporary_collision_is_preserved() {
        use std::io::Write as _;

        let root = private_test_directory("collision");
        let temporary = root.join(".report.html.tmp.collision");
        let mut foreign = private_create_new(&temporary).unwrap();
        foreign.write_all(b"FOREIGN_COLLISION").unwrap();

        assert!(private_create_new(&temporary).is_err());
        assert_eq!(fs::read(&temporary).unwrap(), b"FOREIGN_COLLISION");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn private_directory_fifo_probe() {
        let Some(path) = std::env::var_os("STATIC_REPORT_FIFO_PROBE") else {
            return;
        };
        let path = PathBuf::from(path);
        let displaced = path.with_extension("displaced");
        let directory = open_private_directory(&path).unwrap();
        fs::rename(&path, &displaced).unwrap();
        assert!(
            std::process::Command::new("mkfifo")
                .args(["-m", "700"])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        assert!(matches!(
            sync_private_directory(&directory, &path),
            Err(ReportArtifactError::InvalidPath)
        ));
        fs::remove_file(path).unwrap();
        fs::remove_dir_all(displaced).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn private_directory_fifo_open_probe() {
        let Some(path) = std::env::var_os("STATIC_REPORT_FIFO_OPEN_PROBE") else {
            return;
        };
        let path = PathBuf::from(path);
        let displaced = path.with_extension("displaced");
        assert!(matches!(
            open_private_directory_observing(&path, |path| {
                fs::rename(path, &displaced).unwrap();
                assert!(
                    std::process::Command::new("mkfifo")
                        .args(["-m", "700"])
                        .arg(path)
                        .status()
                        .unwrap()
                        .success()
                );
            }),
            Err(ReportArtifactError::InvalidPath)
        ));
        fs::remove_file(path).unwrap();
        fs::remove_dir_all(displaced).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn opening_a_parent_fifo_is_subprocess_bounded() {
        let root = private_test_directory("fifo-open");

        run_bounded_probe(
            "tests::private_directory_fifo_open_probe",
            "STATIC_REPORT_FIFO_OPEN_PROBE",
            &root,
        );
        let _ = fs::remove_file(&root);
        let _ = fs::remove_dir_all(root.with_extension("displaced"));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn opening_a_replaced_parent_fifo_is_subprocess_bounded() {
        let root = private_test_directory("fifo-parent");
        run_bounded_probe(
            "tests::private_directory_fifo_probe",
            "STATIC_REPORT_FIFO_PROBE",
            &root,
        );
        let _ = fs::remove_file(&root);
        let _ = fs::remove_dir_all(root.with_extension("displaced"));
        let _ = fs::remove_dir_all(root);
    }
}
