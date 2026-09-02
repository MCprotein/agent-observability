//! Self-contained, private HTML artifact assembly for validated report DTOs.

use agent_observability_contracts::{ContractError, ReportDtoV1};
use std::fmt::{self, Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
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
    let mut html = Vec::new();
    write_rendered(&mut html, report)?;
    String::from_utf8(html).map_err(|_| ReportArtifactError::InvalidTemplate)
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
    let temporary = temporary_path(path)?;
    let mut file = private_create_new(&temporary)?;
    let result = (|| -> Result<u64, ReportArtifactError> {
        let mut writer = BufWriter::new(&mut file);
        let bytes = write_rendered(&mut writer, report)?;
        writer.flush()?;
        drop(writer);
        file.sync_all()?;
        Ok(bytes)
    })();
    let bytes = match result {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
    };
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    private_file(path)?;
    File::open(parent)?.sync_all()?;
    Ok(bytes)
}

fn write_rendered(
    mut output: impl Write,
    report: &ReportDtoV1,
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
