use super::{PENDING, ReportArtifactError, write_rendered};
use agent_observability_contracts::{MAX_REPORT_ARTIFACT_BYTES, ReportDtoV2};
use std::io::{self, Write};

const REPORT_DATA_OPEN: &[u8] = b"<script id=\"report-data\" type=\"application/json\">";
const REPORT_DATA_CLOSE: &[u8] = b"</script>";

/// Validates that bytes are exactly one artifact emitted by the current static-report renderer.
///
/// This checks only artifact semantics. It does not prove descriptor identity, filesystem
/// ownership, publication authority, or storage admission.
///
/// # Errors
///
/// Returns [`ReportArtifactError`] when the input exceeds the report limit, does not contain one
/// closed current report DTO, violates that DTO contract, or differs from canonical renderer output.
pub fn validate_semantic_artifact(artifact: &[u8]) -> Result<(), ReportArtifactError> {
    if artifact.len() as u64 > MAX_REPORT_ARTIFACT_BYTES {
        return Err(ReportArtifactError::TooLarge);
    }
    if artifact == PENDING.as_bytes() {
        return Ok(());
    }

    let data_start = find_subslice(artifact, REPORT_DATA_OPEN)
        .and_then(|offset| offset.checked_add(REPORT_DATA_OPEN.len()))
        .ok_or(ReportArtifactError::InvalidArtifact)?;
    let data_end = find_subslice(&artifact[data_start..], REPORT_DATA_CLOSE)
        .and_then(|offset| data_start.checked_add(offset))
        .ok_or(ReportArtifactError::InvalidArtifact)?;
    let report: ReportDtoV2 = serde_json::from_slice(&artifact[data_start..data_end])
        .map_err(|_| ReportArtifactError::InvalidArtifact)?;

    let mut comparator = ExactArtifactWriter::new(artifact);
    let render_result = write_rendered(&mut comparator, &report);
    if comparator.mismatched {
        return Err(ReportArtifactError::InvalidArtifact);
    }
    render_result?;
    if comparator.offset != artifact.len() {
        return Err(ReportArtifactError::InvalidArtifact);
    }
    Ok(())
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

struct ExactArtifactWriter<'a> {
    artifact: &'a [u8],
    offset: usize,
    mismatched: bool,
}

impl<'a> ExactArtifactWriter<'a> {
    fn new(artifact: &'a [u8]) -> Self {
        Self {
            artifact,
            offset: 0,
            mismatched: false,
        }
    }
}

impl Write for ExactArtifactWriter<'_> {
    fn write(&mut self, value: &[u8]) -> io::Result<usize> {
        let matches = self
            .artifact
            .get(self.offset..)
            .is_some_and(|remaining| remaining.starts_with(value));
        if !matches {
            self.mismatched = true;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "rendered report differs from the artifact",
            ));
        }
        self.offset += value.len();
        Ok(value.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
