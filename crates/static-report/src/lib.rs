//! Self-contained, private HTML artifact assembly for validated report DTOs.

use agent_observability_contracts::{ContractError, ReportDtoV1};
use std::fmt::{self, Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const SHELL: &str = include_str!("../../../src/report/generated/report-shell.html");
const TITLE_TOKEN: &str = "__AGENT_OBSERVABILITY_REPORT_TITLE__";
const GENERATED_AT_TOKEN: &str = "__AGENT_OBSERVABILITY_REPORT_GENERATED_AT__";
const DATA_TOKEN: &str = "__AGENT_OBSERVABILITY_REPORT_DATA__";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum ReportArtifactError {
    Contract(ContractError),
    Json(serde_json::Error),
    Io(io::Error),
    InvalidTemplate,
    InvalidPath,
    InsecurePermissions,
    Symlink,
    UnsupportedPlatform,
}

impl Display for ReportArtifactError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Contract(_) => "report DTO does not satisfy its contract",
            Self::Json(_) => "report DTO serialization failed",
            Self::Io(_) => "report artifact I/O failed",
            Self::InvalidTemplate => "embedded report template is invalid",
            Self::InvalidPath => "report artifact path has the wrong file type",
            Self::InsecurePermissions => "report artifact path is not private",
            Self::Symlink => "report artifact path must not be a symbolic link",
            Self::UnsupportedPlatform => {
                "private report artifacts are unsupported on this platform"
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
pub fn render(report: &ReportDtoV1) -> Result<String, ReportArtifactError> {
    report.validate().map_err(ReportArtifactError::Contract)?;
    if SHELL.matches(TITLE_TOKEN).count() != 2
        || SHELL.matches(GENERATED_AT_TOKEN).count() != 1
        || SHELL.matches(DATA_TOKEN).count() != 1
    {
        return Err(ReportArtifactError::InvalidTemplate);
    }
    let data = serde_json::to_string(report)
        .map_err(ReportArtifactError::Json)?
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e");
    Ok(SHELL
        .replace(TITLE_TOKEN, &escape_html(&report.title))
        .replace(GENERATED_AT_TOKEN, &escape_html(&report.generated_at))
        .replace(DATA_TOKEN, &data))
}

/// Atomically writes one private report file inside an existing private directory.
///
/// # Errors
///
/// Returns [`ReportArtifactError`] for insecure paths, unsupported platforms, or I/O failures.
pub fn write_private(path: &Path, report: &ReportDtoV1) -> Result<u64, ReportArtifactError> {
    let parent = path.parent().ok_or(ReportArtifactError::InvalidPath)?;
    private_directory(parent)?;
    reject_output_path(path)?;
    let html = render(report)?;
    let temporary = temporary_path(path)?;
    let mut file = private_create_new(&temporary)?;
    if let Err(error) = (|| -> Result<(), io::Error> {
        file.write_all(html.as_bytes())?;
        file.sync_all()?;
        Ok(())
    })() {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    private_file(path)?;
    File::open(parent)?.sync_all()?;
    u64::try_from(html.len()).map_err(|_| ReportArtifactError::InvalidTemplate)
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

    fn report(title: &str) -> ReportDtoV1 {
        ReportDtoV1 {
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
    fn render_embeds_validated_data_without_script_breakout() {
        let html = render(&report("Stable </script><title> report")).unwrap();
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("Stable &lt;/script&gt;&lt;title&gt; report"));
        assert!(html.contains("Stable \\u003c/script\\u003e\\u003ctitle\\u003e report"));
        assert!(!html.contains(TITLE_TOKEN));
        assert!(!html.contains(DATA_TOKEN));
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
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
        assert!(write_private(&output, &report("Private report")).unwrap() > 0);
        assert_eq!(
            fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o600
        );

        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            write_private(&output, &report("Rejected report")),
            Err(ReportArtifactError::InsecurePermissions)
        ));
        let _ = fs::remove_dir_all(root);
    }
}
