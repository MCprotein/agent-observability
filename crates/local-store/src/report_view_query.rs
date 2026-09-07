//! Bounded read kernels; callers hold the publication guard and validate the snapshot epoch.

use agent_observability_contracts::{ReportSpanV2, dashboard::DashboardFiltersV1};
use rusqlite::{Connection, params, types::ValueRef};
use std::time::{Duration, Instant};

use super::ReportViewBuildError;

const MAX_SCAN_ROWS: usize = 512;
const MAX_DECODED_BYTES: usize = 2 * 1024 * 1024;
const SLICE_TIME: Duration = Duration::from_millis(50);
const TRACE_SCAN_SQL: &str = "SELECT start_time_unix_ms,trace_id,span_id,span_json FROM spans \
    WHERE trace_id = ?2 AND (start_time_unix_ms,span_id) > (?1,?3) \
    ORDER BY start_time_unix_ms,span_id LIMIT 512";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IdentityDimension {
    Session,
    Turn,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct IdentityScanCursor {
    pub after: Option<(String, f64, String, String)>,
    pub last_matching: Option<String>,
    pub distinct: u64,
}

/// Counts matching distinct IDs in index order, retaining only the last matching identity.
pub(crate) fn scan_identities(
    connection: &Connection,
    dimension: IdentityDimension,
    cursor: &IdentityScanCursor,
    filters: &DashboardFiltersV1,
) -> Result<(IdentityScanCursor, bool), ReportViewBuildError> {
    let sql = match dimension {
        IdentityDimension::Session => {
            "SELECT session_id,start_time_unix_ms,trace_id,span_id,span_json FROM spans WHERE (session_id,start_time_unix_ms,trace_id,span_id) > (?1,?2,?3,?4) ORDER BY session_id,start_time_unix_ms,trace_id,span_id LIMIT 512"
        }
        IdentityDimension::Turn => {
            "SELECT turn_id,start_time_unix_ms,trace_id,span_id,span_json FROM spans WHERE (turn_id,start_time_unix_ms,trace_id,span_id) > (?1,?2,?3,?4) ORDER BY turn_id,start_time_unix_ms,trace_id,span_id LIMIT 512"
        }
    };
    let mut next = cursor.clone();
    let (identity, time, trace, span) = cursor
        .after
        .as_ref()
        .map_or(("", f64::MIN, "", ""), |(identity, time, trace, span)| {
            (identity.as_str(), *time, trace.as_str(), span.as_str())
        });
    let mut statement = connection.prepare(sql)?;
    let mut rows = statement.query(params![identity, time, trace, span])?;
    let started = Instant::now();
    let mut decoded = 0;
    for ordinal in 0..MAX_SCAN_ROWS {
        if ordinal > 0 && started.elapsed() >= SLICE_TIME {
            return Ok((next, false));
        }
        let Some(row) = rows.next()? else {
            return Ok((next, true));
        };
        let ValueRef::Text(json) = row.get_ref(4)? else {
            return Err(ReportViewBuildError::InvalidStagingState);
        };
        if json.len() > MAX_DECODED_BYTES {
            return Err(ReportViewBuildError::CapacityExceeded);
        }
        if decoded + json.len() > MAX_DECODED_BYTES {
            return Ok((next, false));
        }
        decoded += json.len();
        let projected: ReportSpanV2 = serde_json::from_slice(json)?;
        projected
            .validate()
            .map_err(|_| ReportViewBuildError::InvalidStagingState)?;
        let identity: String = row.get(0)?;
        if !identity.is_empty()
            && next.last_matching.as_ref() != Some(&identity)
            && matches_filters(&projected, filters)
        {
            next.distinct = next
                .distinct
                .checked_add(1)
                .filter(|count| {
                    *count <= agent_observability_contracts::dashboard::DASHBOARD_SAFE_INTEGER_MAX
                })
                .ok_or(ReportViewBuildError::CapacityExceeded)?;
            next.last_matching = Some(identity.clone());
        }
        next.after = Some((identity, row.get(1)?, row.get(2)?, row.get(3)?));
    }
    Ok((next, false))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SpanScanCursor {
    pub after: Option<(f64, String, String)>,
}

#[derive(Debug)]
pub(crate) struct SpanScanBatch {
    pub rows: Vec<ReportSpanV2>,
    pub cursor: SpanScanCursor,
    pub exhausted: bool,
    pub scanned: usize,
    pub decoded_bytes: usize,
}

/// A bounded index walk, not a sparse SQL filter followed by an unbounded scan.
pub(crate) fn scan_spans(
    connection: &Connection,
    cursor: &SpanScanCursor,
    filters: &DashboardFiltersV1,
    trace_id: Option<&str>,
    output_limit: usize,
) -> Result<SpanScanBatch, ReportViewBuildError> {
    if output_limit == 0 || output_limit > MAX_SCAN_ROWS {
        return Err(ReportViewBuildError::InvalidStagingState);
    }
    let started = Instant::now();
    let mut batch = SpanScanBatch {
        rows: Vec::new(),
        cursor: cursor.clone(),
        exhausted: false,
        scanned: 0,
        decoded_bytes: 0,
    };
    let (time, trace, span) = cursor
        .after
        .as_ref()
        .map_or((f64::MIN, "", ""), |(time, trace, span)| {
            (*time, trace.as_str(), span.as_str())
        });
    let sql = if trace_id.is_some() {
        TRACE_SCAN_SQL
    } else {
        "SELECT start_time_unix_ms,trace_id,span_id,span_json FROM spans \
         WHERE (start_time_unix_ms,trace_id,span_id) > (?1,?2,?3) \
         ORDER BY start_time_unix_ms,trace_id,span_id LIMIT 512"
    };
    let mut statement = connection.prepare(sql)?;
    let mut rows = statement.query(params![time, trace_id.unwrap_or(trace), span])?;
    loop {
        if batch.scanned > 0
            && (batch.scanned >= MAX_SCAN_ROWS
                || batch.rows.len() >= output_limit
                || started.elapsed() >= SLICE_TIME)
        {
            break;
        }
        let Some(row) = rows.next()? else {
            batch.exhausted = true;
            break;
        };
        let ValueRef::Text(json) = row.get_ref(3)? else {
            return Err(ReportViewBuildError::InvalidStagingState);
        };
        if json.len() > MAX_DECODED_BYTES {
            return Err(ReportViewBuildError::CapacityExceeded);
        }
        if batch.decoded_bytes + json.len() > MAX_DECODED_BYTES {
            break;
        }
        let projected: ReportSpanV2 = serde_json::from_slice(json)?;
        projected
            .validate()
            .map_err(|_| ReportViewBuildError::InvalidStagingState)?;
        batch.cursor.after = Some((row.get(0)?, row.get(1)?, row.get(2)?));
        batch.scanned += 1;
        batch.decoded_bytes += json.len();
        if trace_id.is_none_or(|trace| projected.trace_id == trace)
            && matches_filters(&projected, filters)
        {
            batch.rows.push(projected);
        }
    }
    Ok(batch)
}

pub(crate) fn matches_filters(span: &ReportSpanV2, filters: &DashboardFiltersV1) -> bool {
    let dimensions = [
        (&filters.repo, Some(span.repo.as_str())),
        (&filters.session, span.session_id.as_deref()),
        (
            &filters.agent,
            Some(span.agent.name.as_deref().unwrap_or("unknown")),
        ),
        (
            &filters.model,
            Some(span.agent.model.as_deref().unwrap_or("unknown")),
        ),
    ];
    if dimensions.into_iter().any(|(allowed, value)| {
        !allowed.is_empty()
            && !allowed
                .iter()
                .any(|candidate| Some(candidate.as_str()) == value)
    }) {
        return false;
    }
    let Some(text) = &filters.text else {
        return true;
    };
    // Enum wire values preserve the existing static browser's searchable fields.
    let kind = match span.kind {
        agent_observability_domain::SpanKind::LlmRequest => "llm.request",
        agent_observability_domain::SpanKind::ToolExecution => "tool.execution",
        agent_observability_domain::SpanKind::AgentSession => "agent.session",
        agent_observability_domain::SpanKind::Workstream => "workstream",
        agent_observability_domain::SpanKind::Turn => "turn",
        agent_observability_domain::SpanKind::Permission => "permission",
        agent_observability_domain::SpanKind::Compaction => "compaction",
    };
    let status = match span.status {
        agent_observability_domain::StatusCode::Ok => "ok",
        agent_observability_domain::StatusCode::Error => "error",
        agent_observability_domain::StatusCode::Unset => "unset",
    };
    [
        span.name.as_str(),
        kind,
        status,
        span.tool_name.as_deref().unwrap_or(""),
        &span.trace_id,
        &span.span_id,
        &span.repo,
        span.session_id.as_deref().unwrap_or(""),
        span.agent.name.as_deref().unwrap_or(""),
        span.agent.model.as_deref().unwrap_or(""),
    ]
    .join(" ")
    .to_lowercase()
    .contains(&text.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database(count: usize) -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE spans(start_time_unix_ms REAL,trace_id TEXT,span_id TEXT,span_json TEXT); CREATE INDEX stable ON spans(start_time_unix_ms,trace_id,span_id);").unwrap();
        let report: agent_observability_contracts::ReportDtoV2 = serde_json::from_str(
            include_str!("../../../contracts/report-dto-v2.fixture.json"),
        )
        .unwrap();
        let mut span = report.spans[0].clone();
        for index in 0..count {
            span.span_id =
                agent_observability_contracts::hash_opaque_identifier(&format!("query-{index}"));
            span.start_time_unix_ms = 0.0;
            connection
                .execute(
                    "INSERT INTO spans VALUES(0,?1,?2,?3)",
                    params![
                        span.trace_id,
                        span.span_id,
                        serde_json::to_string(&span).unwrap()
                    ],
                )
                .unwrap();
        }
        connection
    }

    #[test]
    fn sparse_empty_page_advances_and_does_not_claim_end() {
        let connection = database(600);
        let filters = DashboardFiltersV1 {
            repo: vec!["missing".into()],
            ..DashboardFiltersV1::default()
        };
        let batch =
            scan_spans(&connection, &SpanScanCursor::default(), &filters, None, 200).unwrap();
        assert!(batch.rows.is_empty());
        assert!(!batch.exhausted);
        assert!(batch.cursor.after.is_some());
        assert!((1..=512).contains(&batch.scanned));
        assert!(batch.decoded_bytes <= MAX_DECODED_BYTES);
    }

    #[test]
    fn selected_trace_seeks_past_unrelated_rows_in_one_bounded_slice() {
        let connection = database(600);
        connection
            .execute_batch(
                "CREATE INDEX spans_trace_order_idx ON spans(trace_id,start_time_unix_ms,span_id)",
            )
            .unwrap();
        let json: String = connection
            .query_row("SELECT span_json FROM spans LIMIT 1", [], |row| row.get(0))
            .unwrap();
        let mut selected: ReportSpanV2 = serde_json::from_str(&json).unwrap();
        selected.trace_id = agent_observability_contracts::hash_opaque_identifier("selected-trace");
        selected.start_time_unix_ms = 100.0;
        selected.end_time_unix_ms = None;
        connection
            .execute(
                "INSERT INTO spans VALUES(?1,?2,?3,?4)",
                params![
                    selected.start_time_unix_ms,
                    selected.trace_id,
                    selected.span_id,
                    serde_json::to_string(&selected).unwrap()
                ],
            )
            .unwrap();
        let page = scan_spans(
            &connection,
            &SpanScanCursor::default(),
            &DashboardFiltersV1::default(),
            Some(&selected.trace_id),
            200,
        )
        .unwrap();
        assert_eq!(page.scanned, 1);
        assert_eq!(page.rows.len(), 1);
        assert!(page.exhausted);
        let plan: String = connection
            .query_row(
                &format!("EXPLAIN QUERY PLAN {TRACE_SCAN_SQL}"),
                params![f64::MIN, selected.trace_id, ""],
                |row| row.get(3),
            )
            .unwrap();
        assert!(plan.contains("spans_trace_order_idx"), "{plan}");
    }

    #[test]
    fn stable_equal_time_keysets_return_every_row_once() {
        let connection = database(55);
        let mut cursor = SpanScanCursor::default();
        let mut identifiers = std::collections::BTreeSet::new();
        loop {
            let batch = scan_spans(
                &connection,
                &cursor,
                &DashboardFiltersV1::default(),
                None,
                7,
            )
            .unwrap();
            assert!(batch.rows.len() <= 7);
            for span in batch.rows {
                assert!(identifiers.insert(span.span_id));
            }
            cursor = batch.cursor;
            if batch.exhausted {
                break;
            }
        }
        assert_eq!(identifiers.len(), 55);
    }

    #[test]
    fn single_oversized_row_fails_without_a_skip_cursor() {
        let connection = database(1);
        connection
            .execute(
                "UPDATE spans SET span_json=?1",
                ["x".repeat(MAX_DECODED_BYTES + 1)],
            )
            .unwrap();
        assert!(matches!(
            scan_spans(
                &connection,
                &SpanScanCursor::default(),
                &DashboardFiltersV1::default(),
                None,
                1
            ),
            Err(ReportViewBuildError::CapacityExceeded)
        ));
    }

    #[test]
    fn distinct_identity_passes_cross_slices_without_sets_or_double_counting() {
        let connection = database(600);
        connection.execute_batch("ALTER TABLE spans ADD COLUMN session_id TEXT; ALTER TABLE spans ADD COLUMN turn_id TEXT; UPDATE spans SET session_id='session-a',turn_id='turn-a'; CREATE INDEX sessions ON spans(session_id,start_time_unix_ms,trace_id,span_id); CREATE INDEX turns ON spans(turn_id,start_time_unix_ms,trace_id,span_id);").unwrap();
        for dimension in [IdentityDimension::Session, IdentityDimension::Turn] {
            let mut cursor = IdentityScanCursor::default();
            let mut slices = 0;
            loop {
                let (next, done) = scan_identities(
                    &connection,
                    dimension,
                    &cursor,
                    &DashboardFiltersV1::default(),
                )
                .unwrap();
                cursor = next;
                slices += 1;
                if done {
                    break;
                }
            }
            assert_eq!(cursor.distinct, 1);
            assert!(slices >= 2);
        }
        let filtered = DashboardFiltersV1 {
            repo: vec!["missing".into()],
            ..DashboardFiltersV1::default()
        };
        let mut cursor = IdentityScanCursor::default();
        loop {
            let (next, done) =
                scan_identities(&connection, IdentityDimension::Session, &cursor, &filtered)
                    .unwrap();
            cursor = next;
            if done {
                break;
            }
        }
        assert_eq!(cursor.distinct, 0);
    }
}
