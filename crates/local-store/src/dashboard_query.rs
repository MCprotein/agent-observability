//! Stateful bounded query service for immutable local dashboard snapshots.

use super::report_view_catalog::{
    ReportViewCatalogError, ReportViewKernel, ReportViewReadScope, ReportViewSnapshot,
};
use super::report_view_query::{
    DimensionPosition, IdentityDimension, IdentityScanCursor, SpanScanCursor, matches_filters,
    scan_identities, scan_spans,
};
use super::{LocalStore, ReportViewBuildError};
use agent_observability_application::dashboard_summary::{
    DashboardSummaryAccumulator, SummaryCapacityExceeded,
};
use agent_observability_contracts::dashboard::{
    DASHBOARD_FACET_MAX_VALUES, DASHBOARD_QUERY_VERSION,
    DASHBOARD_REQUEST_MAX_SERIALIZED_UTF8_BYTES, DASHBOARD_SAFE_INTEGER_MAX,
    DASHBOARD_TRACE_PAGE_MAX_ROWS, DashboardAvailabilityFieldV1, DashboardAvailabilityReasonV1,
    DashboardBootstrapKindV1, DashboardBootstrapResponseV1, DashboardContractError,
    DashboardFacetDimensionV1, DashboardFacetRowV1, DashboardFacetsKindV1,
    DashboardFacetsResponseV1, DashboardFiltersV1, DashboardPaginationV1, DashboardQueryLimitsV1,
    DashboardQueryRequestV1, DashboardQueryResponseV1, DashboardQueryScopeV1,
    DashboardRequestKindV1, DashboardSnapshotStateV1, DashboardSnapshotV1, DashboardSpanKindV1,
    DashboardSpanResponseV1, DashboardSpanRowV1, DashboardSpansKindV1, DashboardSpansResponseV1,
    DashboardStatusKindV1, DashboardStatusReasonV1, DashboardStatusResponseV1,
    DashboardSummaryKindV1, DashboardSummaryResponseV1, DashboardTraceRowV1, DashboardTracesKindV1,
    DashboardTracesResponseV1, DashboardWorkV1,
};
use agent_observability_contracts::{AvailabilityStateV2, ReportSpanV2};
use agent_observability_domain::{SpanKind, StatusCode};
use rusqlite::{OptionalExtension, params, types::ValueRef};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const MAX_CURSOR_LEASES: usize = 16;
const CURSOR_IDLE_TTL: Duration = Duration::from_mins(2);
const CURSOR_HARD_TTL: Duration = Duration::from_mins(10);
const MAX_SCAN_FACTS: usize = 512;
const MAX_SCAN_DECODED_BYTES: usize = 2 * 1024 * 1024;
const MAX_SCAN_TIME: Duration = Duration::from_millis(50);

/// Single-worker state for bounded dashboard queries.
///
/// The loopback router owns one mutable instance and enforces worker admission outside this type.
/// Leases retain only bounded scalar/cursor state; every request reopens and closes its snapshot.
/// At most 16 cursor IDs are retained globally. Trace/span pages keep replay boundaries inside that
/// window for back navigation, while summary/facet advances consume their parent cursor. An ID
/// evicted beyond the window returns `snapshot_expired` instead of restarting or skipping rows.
#[derive(Debug)]
pub struct DashboardQueryService {
    secret: [u8; 32],
    sequence: u64,
    leases: BTreeMap<String, CursorLease>,
}

impl DashboardQueryService {
    /// Creates a service from router-owned operating-system entropy.
    #[must_use]
    pub const fn new(secret: [u8; 32]) -> Self {
        Self {
            secret,
            sequence: 0,
            leases: BTreeMap::new(),
        }
    }

    /// Handles one already-admitted request and always returns the closed v1 wire contract.
    #[must_use]
    pub fn query(
        &mut self,
        store: &LocalStore,
        mut request: DashboardQueryRequestV1,
    ) -> DashboardQueryResponseV1 {
        self.query_at(store, &mut request, Instant::now())
    }

    fn query_at(
        &mut self,
        store: &LocalStore,
        request: &mut DashboardQueryRequestV1,
        now: Instant,
    ) -> DashboardQueryResponseV1 {
        let scope = match ReportViewReadScope::acquire(store) {
            Ok(scope) => scope,
            Err(error) => return catalog_status(request_kind(request), &error, None),
        };
        self.query_scoped(&scope, request, now)
    }

    fn query_scoped(
        &mut self,
        store: &ReportViewReadScope<'_>,
        request: &mut DashboardQueryRequestV1,
        now: Instant,
    ) -> DashboardQueryResponseV1 {
        self.prune_expired(now);
        let kind = request_kind(request);
        if serde_json::to_vec(request).map_or(true, |wire| {
            wire.len() > DASHBOARD_REQUEST_MAX_SERIALIZED_UTF8_BYTES
        }) {
            return status(kind, DashboardStatusReasonV1::InvalidQuery, None);
        }
        normalize_request(request);
        if request.validate().is_err() {
            return status(kind, DashboardStatusReasonV1::InvalidQuery, None);
        }
        let response = match request {
            DashboardQueryRequestV1::Bootstrap(request) => {
                Self::bootstrap(store, request.filters.clone().unwrap_or_default())
            }
            DashboardQueryRequestV1::Spans(request) => self.spans(
                store,
                &request.snapshot_id,
                request.filters.clone().unwrap_or_default(),
                &request.trace_id,
                request.cursor.as_deref(),
                now,
            ),
            DashboardQueryRequestV1::Summary(request) => self.summary(
                store,
                &request.snapshot_id,
                request.filters.clone().unwrap_or_default(),
                request.cursor.as_deref(),
                now,
            ),
            DashboardQueryRequestV1::Span(request) => Self::span_detail(
                store,
                &request.snapshot_id,
                request.filters.clone().unwrap_or_default(),
                &request.trace_id,
                &request.span_id,
            ),
            DashboardQueryRequestV1::Traces(request) => self.traces(
                store,
                &request.snapshot_id,
                request.filters.clone().unwrap_or_default(),
                request.cursor.as_deref(),
                now,
            ),
            DashboardQueryRequestV1::Facets(request) => self.facets(
                store,
                &request.snapshot_id,
                request.filters.clone().unwrap_or_default(),
                request.cursor.as_deref(),
                now,
            ),
        };
        validated_or_status(kind, response)
    }

    fn bootstrap(
        store: &ReportViewReadScope<'_>,
        filters: DashboardFiltersV1,
    ) -> DashboardQueryResponseV1 {
        match store.current() {
            Ok(Some(snapshot)) => {
                let state = match snapshot_freshness(store, &snapshot) {
                    Ok(state) => state,
                    Err(error) => {
                        return catalog_status(DashboardRequestKindV1::Bootstrap, &error, None);
                    }
                };
                DashboardQueryResponseV1::Bootstrap(DashboardBootstrapResponseV1 {
                    schema_version: DASHBOARD_QUERY_VERSION.into(),
                    kind: DashboardBootstrapKindV1::Bootstrap,
                    snapshot: snapshot_contract(&snapshot, state),
                    scope: scope(filters, None),
                    work: DashboardWorkV1::pending(),
                    limits: DashboardQueryLimitsV1::default(),
                })
            }
            Ok(None) => status(
                DashboardRequestKindV1::Bootstrap,
                DashboardStatusReasonV1::Building,
                None,
            ),
            Err(error) => catalog_status(DashboardRequestKindV1::Bootstrap, &error, None),
        }
    }

    fn spans(
        &mut self,
        store: &ReportViewReadScope<'_>,
        snapshot_id: &str,
        filters: DashboardFiltersV1,
        trace_id: &str,
        cursor_id: Option<&str>,
        now: Instant,
    ) -> DashboardQueryResponseV1 {
        let binding = CursorBinding::new(
            snapshot_id,
            &filters,
            DashboardRequestKindV1::Spans,
            Some(trace_id),
        );
        let (scan_cursor, parent, replay) = match self.resolve_span_cursor(cursor_id, &binding, now)
        {
            Ok(value) => value,
            Err(reason) => return status(binding.kind, reason, None),
        };
        let current = match current_snapshot_state(store, snapshot_id) {
            Ok(value) => value,
            Err(failure) => return failure.response(DashboardRequestKindV1::Spans),
        };
        let result = store.with_snapshot(snapshot_id, |connection, metadata| {
            let batch = scan_spans(connection, &scan_cursor, &filters, Some(trace_id), 200)
                .map_err(ReportViewCatalogError::Build)?;
            Ok((metadata.clone(), batch))
        });
        let (metadata, batch) = match result {
            Ok(value) => value,
            Err(error) => return catalog_status(binding.kind, &error, Some(current.snapshot)),
        };
        let next_cursor = match self.finish_history_page(
            parent,
            binding,
            CursorState::Spans(batch.cursor),
            batch.exhausted,
            replay,
            now,
        ) {
            Ok(cursor) => cursor,
            Err(reason) => return status(DashboardRequestKindV1::Spans, reason, None),
        };
        DashboardQueryResponseV1::Spans(DashboardSpansResponseV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardSpansKindV1::Spans,
            snapshot: snapshot_contract(&metadata, current.state),
            scope: scope(filters, Some(trace_id.to_owned())),
            work: DashboardWorkV1::pending(),
            rows: batch.rows.iter().map(span_row).collect(),
            pagination: DashboardPaginationV1 {
                next_cursor,
                total: None,
            },
        })
    }

    fn summary(
        &mut self,
        store: &ReportViewReadScope<'_>,
        snapshot_id: &str,
        filters: DashboardFiltersV1,
        cursor_id: Option<&str>,
        now: Instant,
    ) -> DashboardQueryResponseV1 {
        let binding =
            CursorBinding::new(snapshot_id, &filters, DashboardRequestKindV1::Summary, None);
        let (progress, parent) = match self.resolve_summary_cursor(cursor_id, &binding, now) {
            Ok(value) => value,
            Err(reason) => return status(binding.kind, reason, None),
        };
        let current = match current_snapshot_state(store, snapshot_id) {
            Ok(value) => value,
            Err(failure) => return failure.response(DashboardRequestKindV1::Summary),
        };
        let result = store.with_snapshot_kernel(snapshot_id, |connection, metadata, kernel| {
            let step = advance_summary(connection, progress, kernel, &filters)
                .map_err(summary_catalog_error)?;
            Ok((metadata.clone(), step))
        });
        let (metadata, step) = match result {
            Ok(value) => value,
            Err(error) => return catalog_status(binding.kind, &error, Some(current.snapshot)),
        };
        let (work, next_cursor) = match step {
            SummaryStep::Pending(progress) => {
                let cursor = match self.issue_lease(
                    parent.as_deref(),
                    binding,
                    CursorState::Summary(progress),
                    now,
                ) {
                    Ok(cursor) => cursor,
                    Err(reason) => {
                        return status(DashboardRequestKindV1::Summary, reason, None);
                    }
                };
                (DashboardWorkV1::pending(), Some(cursor))
            }
            SummaryStep::Complete(kpis) => (DashboardWorkV1::complete(kpis), None),
        };
        DashboardQueryResponseV1::Summary(DashboardSummaryResponseV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardSummaryKindV1::Summary,
            snapshot: snapshot_contract(&metadata, current.state),
            scope: scope(filters, None),
            work,
            pagination: DashboardPaginationV1 {
                next_cursor,
                total: None,
            },
        })
    }

    fn span_detail(
        store: &ReportViewReadScope<'_>,
        snapshot_id: &str,
        filters: DashboardFiltersV1,
        trace_id: &str,
        span_id: &str,
    ) -> DashboardQueryResponseV1 {
        let current = match current_snapshot_state(store, snapshot_id) {
            Ok(value) => value,
            Err(failure) => return failure.response(DashboardRequestKindV1::Span),
        };
        let result = store.with_snapshot(snapshot_id, |connection, metadata| {
            let json: Option<String> = connection
                .query_row(
                    "SELECT span_json FROM spans WHERE span_id=?1 AND trace_id=?2 LIMIT 1",
                    [span_id, trace_id],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(json) = json else {
                return Err(ReportViewCatalogError::Build(
                    ReportViewBuildError::InvalidStagingState,
                ));
            };
            let detail: ReportSpanV2 = serde_json::from_str(&json)?;
            detail.validate().map_err(|_| {
                ReportViewCatalogError::Build(ReportViewBuildError::InvalidStagingState)
            })?;
            if !matches_filters(&detail, &filters) {
                return Err(ReportViewCatalogError::Build(
                    ReportViewBuildError::InvalidStagingState,
                ));
            }
            Ok((metadata.clone(), detail))
        });
        let (metadata, detail) = match result {
            Ok(value) => value,
            Err(error) => {
                return catalog_status(
                    DashboardRequestKindV1::Span,
                    &error,
                    Some(current.snapshot),
                );
            }
        };
        DashboardQueryResponseV1::Span(DashboardSpanResponseV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardSpanKindV1::Span,
            snapshot: snapshot_contract(&metadata, current.state),
            scope: scope(filters, Some(trace_id.to_owned())),
            work: DashboardWorkV1::pending(),
            detail: Box::new(detail),
        })
    }

    fn traces(
        &mut self,
        store: &ReportViewReadScope<'_>,
        snapshot_id: &str,
        filters: DashboardFiltersV1,
        cursor_id: Option<&str>,
        now: Instant,
    ) -> DashboardQueryResponseV1 {
        let kind = DashboardRequestKindV1::Traces;
        let binding = CursorBinding::new(snapshot_id, &filters, kind, None);
        let (cursor, parent, replay) = match self.resolve_trace_cursor(cursor_id, &binding, now) {
            Ok(value) => value,
            Err(reason) => return status(kind, reason, None),
        };
        let current = match current_snapshot_state(store, snapshot_id) {
            Ok(value) => value,
            Err(failure) => return failure.response(kind),
        };
        let result = store.with_snapshot(snapshot_id, |connection, metadata| {
            let batch = scan_trace_rows(connection, cursor, &filters)
                .map_err(ReportViewCatalogError::Build)?;
            Ok((metadata.clone(), batch))
        });
        let (metadata, batch) = match result {
            Ok(value) => value,
            Err(error) => return catalog_status(kind, &error, Some(current.snapshot)),
        };
        let next_cursor = match self.finish_history_page(
            parent,
            binding,
            CursorState::Traces(batch.cursor),
            batch.exhausted,
            replay,
            now,
        ) {
            Ok(cursor) => cursor,
            Err(reason) => return status(kind, reason, None),
        };
        DashboardQueryResponseV1::Traces(DashboardTracesResponseV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardTracesKindV1::Traces,
            snapshot: snapshot_contract(&metadata, current.state),
            scope: scope(filters, None),
            work: DashboardWorkV1::pending(),
            rows: batch.rows,
            pagination: DashboardPaginationV1 {
                next_cursor,
                total: None,
            },
        })
    }

    fn facets(
        &mut self,
        store: &ReportViewReadScope<'_>,
        snapshot_id: &str,
        filters: DashboardFiltersV1,
        cursor_id: Option<&str>,
        now: Instant,
    ) -> DashboardQueryResponseV1 {
        let kind = DashboardRequestKindV1::Facets;
        let binding = CursorBinding::new(snapshot_id, &filters, kind, None);
        let (cursor, parent) = match self.resolve_facet_cursor(cursor_id, &binding, now) {
            Ok(value) => value,
            Err(reason) => return status(kind, reason, None),
        };
        let current = match current_snapshot_state(store, snapshot_id) {
            Ok(value) => value,
            Err(failure) => return failure.response(kind),
        };
        let result = store.with_snapshot_kernel(snapshot_id, |connection, metadata, kernel| {
            let batch = scan_facet_rows(connection, cursor, kernel, &filters)
                .map_err(ReportViewCatalogError::Build)?;
            Ok((metadata.clone(), batch))
        });
        let (metadata, batch) = match result {
            Ok(value) => value,
            Err(error) => return catalog_status(kind, &error, Some(current.snapshot)),
        };
        let next_cursor = if batch.exhausted {
            None
        } else {
            match self.issue_lease(
                parent.as_deref(),
                binding,
                CursorState::Facets(batch.cursor),
                now,
            ) {
                Ok(cursor) => Some(cursor),
                Err(reason) => return status(kind, reason, None),
            }
        };
        DashboardQueryResponseV1::Facets(DashboardFacetsResponseV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardFacetsKindV1::Facets,
            snapshot: snapshot_contract(&metadata, current.state),
            scope: scope(filters, None),
            work: DashboardWorkV1::pending(),
            rows: batch.rows,
            pagination: DashboardPaginationV1 {
                next_cursor,
                total: None,
            },
        })
    }

    fn resolve_trace_cursor(
        &mut self,
        cursor_id: Option<&str>,
        binding: &CursorBinding,
        now: Instant,
    ) -> Result<(TraceScanCursor, Option<String>, Option<ReplayBoundary>), DashboardStatusReasonV1>
    {
        let Some(cursor_id) = cursor_id else {
            return Ok((TraceScanCursor::default(), None, None));
        };
        let lease = self.resolve_lease(cursor_id, binding, now)?;
        let CursorState::Traces(cursor) = lease.state else {
            return Err(DashboardStatusReasonV1::InvalidQuery);
        };
        Ok((cursor, Some(cursor_id.to_owned()), lease.replay))
    }

    fn resolve_facet_cursor(
        &mut self,
        cursor_id: Option<&str>,
        binding: &CursorBinding,
        now: Instant,
    ) -> Result<(FacetScanCursor, Option<String>), DashboardStatusReasonV1> {
        let Some(cursor_id) = cursor_id else {
            return Ok((FacetScanCursor::default(), None));
        };
        let lease = self.resolve_lease(cursor_id, binding, now)?;
        let CursorState::Facets(cursor) = lease.state else {
            return Err(DashboardStatusReasonV1::InvalidQuery);
        };
        Ok((cursor, Some(cursor_id.to_owned())))
    }

    fn resolve_span_cursor(
        &mut self,
        cursor_id: Option<&str>,
        binding: &CursorBinding,
        now: Instant,
    ) -> Result<(SpanScanCursor, Option<String>, Option<ReplayBoundary>), DashboardStatusReasonV1>
    {
        let Some(cursor_id) = cursor_id else {
            return Ok((SpanScanCursor::default(), None, None));
        };
        let lease = self.resolve_lease(cursor_id, binding, now)?;
        let CursorState::Spans(cursor) = lease.state else {
            return Err(DashboardStatusReasonV1::InvalidQuery);
        };
        Ok((cursor, Some(cursor_id.to_owned()), lease.replay))
    }

    fn resolve_summary_cursor(
        &mut self,
        cursor_id: Option<&str>,
        binding: &CursorBinding,
        now: Instant,
    ) -> Result<(SummaryProgress, Option<String>), DashboardStatusReasonV1> {
        let Some(cursor_id) = cursor_id else {
            return Ok((SummaryProgress::default(), None));
        };
        let lease = self.resolve_lease(cursor_id, binding, now)?;
        let CursorState::Summary(progress) = lease.state else {
            return Err(DashboardStatusReasonV1::InvalidQuery);
        };
        Ok((progress, Some(cursor_id.to_owned())))
    }

    fn resolve_lease(
        &mut self,
        cursor_id: &str,
        binding: &CursorBinding,
        now: Instant,
    ) -> Result<CursorLease, DashboardStatusReasonV1> {
        let Some(lease) = self.leases.get_mut(cursor_id) else {
            return Err(DashboardStatusReasonV1::SnapshotExpired);
        };
        if &lease.binding != binding {
            return Err(DashboardStatusReasonV1::InvalidQuery);
        }
        lease.last_used = now;
        Ok(lease.clone())
    }

    fn issue_lease(
        &mut self,
        parent_id: Option<&str>,
        binding: CursorBinding,
        state: CursorState,
        now: Instant,
    ) -> Result<String, DashboardStatusReasonV1> {
        let inherited_created_at = parent_id
            .and_then(|id| self.leases.get(id))
            .map(|lease| lease.created_at);
        if parent_id.is_some() && inherited_created_at.is_none() {
            return Err(DashboardStatusReasonV1::SnapshotExpired);
        }
        let cursor = self.next_cursor_id();
        let created_at = inherited_created_at.unwrap_or(now);
        let retain_parent = is_history_kind(binding.kind) && parent_id.is_some();
        let replay_state = retain_parent.then(|| state.clone());
        if !retain_parent && let Some(parent_id) = parent_id {
            self.leases.remove(parent_id);
        }
        self.make_room(parent_id);
        self.leases.insert(
            cursor.clone(),
            CursorLease {
                binding,
                state,
                created_at,
                last_used: now,
                ordinal: self.sequence,
                replay: None,
            },
        );
        if retain_parent {
            let output_state = replay_state.expect("history cursor retains replay state");
            let parent = self
                .leases
                .get_mut(parent_id.expect("retained parent exists"))
                .ok_or(DashboardStatusReasonV1::SnapshotExpired)?;
            parent.replay = Some(ReplayBoundary {
                output_state,
                exhausted: false,
                next_cursor: Some(cursor.clone()),
            });
        }
        Ok(cursor)
    }

    fn finish_history_page(
        &mut self,
        parent_id: Option<String>,
        binding: CursorBinding,
        output_state: CursorState,
        exhausted: bool,
        replay: Option<ReplayBoundary>,
        now: Instant,
    ) -> Result<Option<String>, DashboardStatusReasonV1> {
        if let Some(replay) = replay {
            let faithful = replay.exhausted == exhausted
                && replay.output_state.same_position(&output_state)
                && replay
                    .next_cursor
                    .as_ref()
                    .is_none_or(|cursor| self.leases.contains_key(cursor));
            if faithful {
                return Ok(replay.next_cursor);
            }
            if let Some(parent_id) = parent_id {
                self.leases.remove(&parent_id);
            }
            return Err(DashboardStatusReasonV1::SnapshotExpired);
        }
        if exhausted {
            if let Some(parent_id) = parent_id {
                let parent = self
                    .leases
                    .get_mut(&parent_id)
                    .ok_or(DashboardStatusReasonV1::SnapshotExpired)?;
                parent.replay = Some(ReplayBoundary {
                    output_state,
                    exhausted: true,
                    next_cursor: None,
                });
            }
            return Ok(None);
        }
        self.issue_lease(parent_id.as_deref(), binding, output_state, now)
            .map(Some)
    }

    fn make_room(&mut self, protected: Option<&str>) {
        while self.leases.len() >= MAX_CURSOR_LEASES {
            let oldest = self
                .leases
                .iter()
                .filter(|(cursor, _)| protected != Some(cursor.as_str()))
                .min_by_key(|(_, lease)| lease.ordinal)
                .map(|(cursor, _)| cursor.clone());
            let Some(oldest) = oldest else {
                break;
            };
            self.leases.remove(&oldest);
        }
    }

    fn next_cursor_id(&mut self) -> String {
        self.sequence = self.sequence.wrapping_add(1);
        let mut digest = Sha256::new();
        digest.update(self.secret);
        digest.update(std::process::id().to_le_bytes());
        digest.update(self.sequence.to_le_bytes());
        lowercase_hex(&digest.finalize())
    }

    fn prune_expired(&mut self, now: Instant) {
        self.leases.retain(|_, lease| {
            now.saturating_duration_since(lease.last_used) <= CURSOR_IDLE_TTL
                && now.saturating_duration_since(lease.created_at) <= CURSOR_HARD_TTL
        });
    }
}

#[derive(Clone, Debug)]
struct CursorLease {
    binding: CursorBinding,
    state: CursorState,
    created_at: Instant,
    last_used: Instant,
    ordinal: u64,
    replay: Option<ReplayBoundary>,
}

#[derive(Clone, Debug)]
struct ReplayBoundary {
    output_state: CursorState,
    exhausted: bool,
    next_cursor: Option<String>,
}

#[derive(Clone, Debug)]
enum CursorState {
    Traces(TraceScanCursor),
    Spans(SpanScanCursor),
    Summary(SummaryProgress),
    Facets(FacetScanCursor),
}

impl CursorState {
    fn same_position(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Traces(left), Self::Traces(right)) => left == right,
            (Self::Spans(left), Self::Spans(right)) => left == right,
            _ => false,
        }
    }
}

const fn is_history_kind(kind: DashboardRequestKindV1) -> bool {
    matches!(
        kind,
        DashboardRequestKindV1::Traces | DashboardRequestKindV1::Spans
    )
}

#[derive(Clone, Debug, Default, PartialEq)]
struct TraceScanCursor {
    after: Option<(String, f64, String)>,
    partial: Option<TraceAccumulator>,
}

#[derive(Clone, Debug, PartialEq)]
struct TraceAccumulator {
    trace_id: String,
    repo: String,
    span_count: u64,
    error_count: u64,
    start_time_unix_ms: f64,
    end_time_unix_ms: Option<f64>,
    availability_reasons: Vec<DashboardAvailabilityReasonV1>,
}

impl TraceAccumulator {
    fn new(span: &ReportSpanV2) -> Self {
        Self {
            trace_id: span.trace_id.clone(),
            repo: span.repo.clone(),
            span_count: 0,
            error_count: 0,
            start_time_unix_ms: span.start_time_unix_ms,
            end_time_unix_ms: span.end_time_unix_ms,
            availability_reasons: Vec::new(),
        }
    }

    fn push(&mut self, span: &ReportSpanV2) -> Result<(), ReportViewBuildError> {
        self.span_count = bounded_increment(self.span_count, 1)?;
        if span.status == StatusCode::Error {
            self.error_count = bounded_increment(self.error_count, 1)?;
        }
        self.start_time_unix_ms = self.start_time_unix_ms.min(span.start_time_unix_ms);
        if let Some(end) = span.end_time_unix_ms {
            self.end_time_unix_ms = Some(self.end_time_unix_ms.map_or(end, |prior| prior.max(end)));
        }
        for reason in availability_reasons(span) {
            if !self
                .availability_reasons
                .iter()
                .any(|existing| existing.field == reason.field)
            {
                self.availability_reasons.push(reason);
            }
        }
        Ok(())
    }

    fn finish(self) -> DashboardTraceRowV1 {
        DashboardTraceRowV1 {
            trace_id: self.trace_id,
            repo: self.repo,
            span_count: self.span_count,
            error_count: self.error_count,
            start_time_unix_ms: self.start_time_unix_ms,
            end_time_unix_ms: self.end_time_unix_ms,
            availability_reasons: self.availability_reasons,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum FacetPhase {
    #[default]
    Repo,
    Session,
    Agent,
    Model,
}

impl FacetPhase {
    const fn next(self) -> Option<Self> {
        match self {
            Self::Repo => Some(Self::Session),
            Self::Session => Some(Self::Agent),
            Self::Agent => Some(Self::Model),
            Self::Model => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct FacetScanCursor {
    phase: FacetPhase,
    after: DimensionPosition,
    last_emitted: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CursorBinding {
    snapshot_id: String,
    filter_fingerprint: [u8; 32],
    kind: DashboardRequestKindV1,
    trace_id: Option<String>,
}

impl CursorBinding {
    fn new(
        snapshot_id: &str,
        filters: &DashboardFiltersV1,
        kind: DashboardRequestKindV1,
        trace_id: Option<&str>,
    ) -> Self {
        let encoded = serde_json::to_vec(filters).expect("validated dashboard filters serialize");
        Self {
            snapshot_id: snapshot_id.to_owned(),
            filter_fingerprint: Sha256::digest(encoded).into(),
            kind,
            trace_id: trace_id.map(str::to_owned),
        }
    }
}

struct TraceScanBatch {
    rows: Vec<DashboardTraceRowV1>,
    cursor: TraceScanCursor,
    exhausted: bool,
}

fn scan_trace_rows(
    connection: &rusqlite::Connection,
    cursor: TraceScanCursor,
    filters: &DashboardFiltersV1,
) -> Result<TraceScanBatch, ReportViewBuildError> {
    let (trace, time, span) = cursor
        .after
        .as_ref()
        .map_or(("", f64::MIN, ""), |(trace, time, span)| {
            (trace.as_str(), *time, span.as_str())
        });
    let mut statement = connection.prepare(
        "SELECT trace_id,start_time_unix_ms,span_id,span_json FROM spans \
         INDEXED BY spans_trace_order_idx \
         WHERE (trace_id,start_time_unix_ms,span_id) > (?1,?2,?3) \
         ORDER BY trace_id,start_time_unix_ms,span_id LIMIT 512",
    )?;
    let mut source = statement.query(params![trace, time, span])?;
    let mut batch = TraceScanBatch {
        rows: Vec::new(),
        cursor,
        exhausted: false,
    };
    let started = Instant::now();
    let mut scanned = 0;
    let mut decoded_bytes = 0;
    loop {
        if scanned > 0
            && (scanned >= MAX_SCAN_FACTS
                || batch.rows.len() >= DASHBOARD_TRACE_PAGE_MAX_ROWS
                || started.elapsed() >= MAX_SCAN_TIME)
        {
            break;
        }
        let Some(row) = source.next()? else {
            if let Some(partial) = batch.cursor.partial.take() {
                batch.rows.push(partial.finish());
            }
            batch.exhausted = true;
            break;
        };
        let ValueRef::Text(json) = row.get_ref(3)? else {
            return Err(ReportViewBuildError::InvalidStagingState);
        };
        if json.len() > MAX_SCAN_DECODED_BYTES {
            return Err(ReportViewBuildError::CapacityExceeded);
        }
        if decoded_bytes + json.len() > MAX_SCAN_DECODED_BYTES {
            break;
        }
        let row_trace: String = row.get(0)?;
        if batch
            .cursor
            .partial
            .as_ref()
            .is_some_and(|partial| partial.trace_id != row_trace)
        {
            let complete = batch.cursor.partial.take().expect("partial was checked");
            batch.rows.push(complete.finish());
            if batch.rows.len() >= DASHBOARD_TRACE_PAGE_MAX_ROWS {
                break;
            }
        }
        let projected: ReportSpanV2 = serde_json::from_slice(json)?;
        projected
            .validate()
            .map_err(|_| ReportViewBuildError::InvalidStagingState)?;
        if matches_filters(&projected, filters) {
            let partial = batch
                .cursor
                .partial
                .get_or_insert_with(|| TraceAccumulator::new(&projected));
            partial.push(&projected)?;
        }
        let row_time = row.get(1)?;
        let row_span = row.get(2)?;
        batch.cursor.after = Some((row_trace, row_time, row_span));
        scanned += 1;
        decoded_bytes += json.len();
    }
    Ok(batch)
}

struct FacetScanBatch {
    rows: Vec<DashboardFacetRowV1>,
    cursor: FacetScanCursor,
    exhausted: bool,
}

fn scan_facet_rows(
    connection: &rusqlite::Connection,
    mut cursor: FacetScanCursor,
    kernel: ReportViewKernel,
    filters: &DashboardFiltersV1,
) -> Result<FacetScanBatch, ReportViewBuildError> {
    cursor.after.bind(kernel)?;
    let (sql, dimension) = facet_query(cursor.phase, kernel);
    let parameters = cursor.after.parameters();
    let mut statement = connection.prepare(sql)?;
    let mut source = statement.query(rusqlite::params_from_iter(parameters))?;
    let mut batch = FacetScanBatch {
        rows: Vec::new(),
        cursor,
        exhausted: false,
    };
    let started = Instant::now();
    let mut scanned = 0;
    let mut decoded_bytes = 0;
    loop {
        if scanned > 0
            && (scanned >= MAX_SCAN_FACTS
                || batch.rows.len() >= DASHBOARD_FACET_MAX_VALUES
                || started.elapsed() >= MAX_SCAN_TIME)
        {
            break;
        }
        let Some(row) = source.next()? else {
            if let Some(next) = batch.cursor.phase.next() {
                batch.cursor = FacetScanCursor {
                    phase: next,
                    after: DimensionPosition::new(kernel),
                    last_emitted: None,
                };
            } else {
                batch.exhausted = true;
            }
            break;
        };
        let ValueRef::Text(json) = row.get_ref(4)? else {
            return Err(ReportViewBuildError::InvalidStagingState);
        };
        if json.len() > MAX_SCAN_DECODED_BYTES {
            return Err(ReportViewBuildError::CapacityExceeded);
        }
        if decoded_bytes + json.len() > MAX_SCAN_DECODED_BYTES {
            break;
        }
        let projected: ReportSpanV2 = serde_json::from_slice(json)?;
        projected
            .validate()
            .map_err(|_| ReportViewBuildError::InvalidStagingState)?;
        let row_value: String = row.get(0)?;
        let emit = !row_value.is_empty()
            && batch.cursor.last_emitted.as_deref() != Some(row_value.as_str())
            && matches_filters(&projected, filters);
        if emit && batch.rows.len() >= DASHBOARD_FACET_MAX_VALUES {
            break;
        }
        if emit {
            batch.rows.push(DashboardFacetRowV1 {
                dimension,
                value: row_value.clone(),
            });
            batch.cursor.last_emitted = Some(row_value.clone());
        }
        batch.cursor.after.advance(row_value, row)?;
        scanned += 1;
        decoded_bytes += json.len();
    }
    Ok(batch)
}

const fn facet_query(
    phase: FacetPhase,
    kernel: ReportViewKernel,
) -> (&'static str, DashboardFacetDimensionV1) {
    if matches!(kernel, ReportViewKernel::V2) {
        return match phase {
            FacetPhase::Repo => (
                "SELECT repo,source_order,NULL,NULL,span_json FROM spans INDEXED BY spans_repo_order_idx WHERE (repo,source_order) > (?1,?2) ORDER BY repo,source_order LIMIT 512",
                DashboardFacetDimensionV1::Repo,
            ),
            FacetPhase::Session => (
                "SELECT session_id,source_order,NULL,NULL,span_json FROM spans INDEXED BY spans_session_order_idx WHERE (session_id,source_order) > (?1,?2) ORDER BY session_id,source_order LIMIT 512",
                DashboardFacetDimensionV1::Session,
            ),
            FacetPhase::Agent => (
                "SELECT agent,source_order,NULL,NULL,span_json FROM spans INDEXED BY spans_agent_order_idx WHERE (agent,source_order) > (?1,?2) ORDER BY agent,source_order LIMIT 512",
                DashboardFacetDimensionV1::Agent,
            ),
            FacetPhase::Model => (
                "SELECT model,source_order,NULL,NULL,span_json FROM spans INDEXED BY spans_model_order_idx WHERE (model,source_order) > (?1,?2) ORDER BY model,source_order LIMIT 512",
                DashboardFacetDimensionV1::Model,
            ),
        };
    }
    match phase {
        FacetPhase::Repo => (
            "SELECT repo,start_time_unix_ms,trace_id,span_id,span_json FROM spans INDEXED BY spans_repo_order_idx WHERE (repo,start_time_unix_ms,trace_id,span_id) > (?1,?2,?3,?4) ORDER BY repo,start_time_unix_ms,trace_id,span_id LIMIT 512",
            DashboardFacetDimensionV1::Repo,
        ),
        FacetPhase::Session => (
            "SELECT session_id,start_time_unix_ms,trace_id,span_id,span_json FROM spans INDEXED BY spans_session_order_idx WHERE session_id IS NOT NULL AND (session_id,start_time_unix_ms,trace_id,span_id) > (?1,?2,?3,?4) ORDER BY session_id,start_time_unix_ms,trace_id,span_id LIMIT 512",
            DashboardFacetDimensionV1::Session,
        ),
        FacetPhase::Agent => (
            "SELECT agent,start_time_unix_ms,trace_id,span_id,span_json FROM spans INDEXED BY spans_agent_order_idx WHERE agent IS NOT NULL AND (agent,start_time_unix_ms,trace_id,span_id) > (?1,?2,?3,?4) ORDER BY agent,start_time_unix_ms,trace_id,span_id LIMIT 512",
            DashboardFacetDimensionV1::Agent,
        ),
        FacetPhase::Model => (
            "SELECT model,start_time_unix_ms,trace_id,span_id,span_json FROM spans INDEXED BY spans_model_order_idx WHERE model IS NOT NULL AND (model,start_time_unix_ms,trace_id,span_id) > (?1,?2,?3,?4) ORDER BY model,start_time_unix_ms,trace_id,span_id LIMIT 512",
            DashboardFacetDimensionV1::Model,
        ),
    }
}

fn bounded_increment(value: u64, amount: u64) -> Result<u64, ReportViewBuildError> {
    value
        .checked_add(amount)
        .filter(|sum| *sum <= DASHBOARD_SAFE_INTEGER_MAX)
        .ok_or(ReportViewBuildError::CapacityExceeded)
}

#[derive(Clone, Debug, Default)]
enum SummaryProgress {
    #[default]
    Spans,
    SpansAt {
        kernel: ReportViewKernel,
        cursor: SpanScanCursor,
        accumulator: DashboardSummaryAccumulator,
    },
    Sessions {
        accumulator: DashboardSummaryAccumulator,
        cursor: IdentityScanCursor,
    },
    Turns {
        accumulator: DashboardSummaryAccumulator,
        sessions: u64,
        cursor: IdentityScanCursor,
    },
}

enum SummaryStep {
    Pending(SummaryProgress),
    Complete(agent_observability_contracts::dashboard::DashboardKpisV1),
}

fn advance_summary(
    connection: &rusqlite::Connection,
    progress: SummaryProgress,
    kernel: ReportViewKernel,
    filters: &DashboardFiltersV1,
) -> Result<SummaryStep, SummaryAdvanceError> {
    match progress {
        SummaryProgress::Spans => advance_summary_spans(
            connection,
            &SpanScanCursor::default(),
            DashboardSummaryAccumulator::default(),
            kernel,
            filters,
        ),
        SummaryProgress::SpansAt {
            kernel: prior,
            cursor,
            accumulator,
        } => {
            if prior != kernel {
                return Err(ReportViewBuildError::InvalidStagingState.into());
            }
            advance_summary_spans(connection, &cursor, accumulator, kernel, filters)
        }
        SummaryProgress::Sessions {
            accumulator,
            cursor,
        } => {
            let (cursor, exhausted) = scan_identities(
                connection,
                IdentityDimension::Session,
                kernel,
                &cursor,
                filters,
            )?;
            if exhausted {
                Ok(SummaryStep::Pending(SummaryProgress::Turns {
                    accumulator,
                    sessions: cursor.distinct,
                    cursor: IdentityScanCursor {
                        after: DimensionPosition::new(kernel),
                        ..IdentityScanCursor::default()
                    },
                }))
            } else {
                Ok(SummaryStep::Pending(SummaryProgress::Sessions {
                    accumulator,
                    cursor,
                }))
            }
        }
        SummaryProgress::Turns {
            accumulator,
            sessions,
            cursor,
        } => {
            let (cursor, exhausted) = scan_identities(
                connection,
                IdentityDimension::Turn,
                kernel,
                &cursor,
                filters,
            )?;
            if exhausted {
                let kpis = accumulator.finish(sessions, cursor.distinct)?;
                Ok(SummaryStep::Complete(kpis))
            } else {
                Ok(SummaryStep::Pending(SummaryProgress::Turns {
                    accumulator,
                    sessions,
                    cursor,
                }))
            }
        }
    }
}

fn advance_summary_spans(
    connection: &rusqlite::Connection,
    cursor: &SpanScanCursor,
    mut accumulator: DashboardSummaryAccumulator,
    kernel: ReportViewKernel,
    filters: &DashboardFiltersV1,
) -> Result<SummaryStep, SummaryAdvanceError> {
    let batch = scan_spans(connection, cursor, filters, None, 512)?;
    for span in batch.rows {
        accumulator.push(&span)?;
    }
    if batch.exhausted {
        Ok(SummaryStep::Pending(SummaryProgress::Sessions {
            accumulator,
            cursor: IdentityScanCursor {
                after: DimensionPosition::new(kernel),
                ..IdentityScanCursor::default()
            },
        }))
    } else {
        Ok(SummaryStep::Pending(SummaryProgress::SpansAt {
            kernel,
            cursor: batch.cursor,
            accumulator,
        }))
    }
}

enum SummaryAdvanceError {
    Build(ReportViewBuildError),
    Capacity,
}

impl From<ReportViewBuildError> for SummaryAdvanceError {
    fn from(error: ReportViewBuildError) -> Self {
        Self::Build(error)
    }
}

impl From<SummaryCapacityExceeded> for SummaryAdvanceError {
    fn from(_: SummaryCapacityExceeded) -> Self {
        Self::Capacity
    }
}

fn summary_catalog_error(error: SummaryAdvanceError) -> ReportViewCatalogError {
    match error {
        SummaryAdvanceError::Build(error) => ReportViewCatalogError::Build(error),
        SummaryAdvanceError::Capacity => {
            ReportViewCatalogError::Build(ReportViewBuildError::CapacityExceeded)
        }
    }
}

struct CurrentSnapshotState {
    snapshot: DashboardSnapshotV1,
    state: DashboardSnapshotStateV1,
}

struct CurrentSnapshotFailure {
    reason: DashboardStatusReasonV1,
    snapshot: Option<DashboardSnapshotV1>,
}

impl CurrentSnapshotFailure {
    fn response(self, kind: DashboardRequestKindV1) -> DashboardQueryResponseV1 {
        status(kind, self.reason, self.snapshot)
    }
}

fn current_snapshot_state(
    store: &ReportViewReadScope<'_>,
    requested_id: &str,
) -> Result<CurrentSnapshotState, CurrentSnapshotFailure> {
    match store.current() {
        Ok(Some(current)) => {
            let freshness =
                snapshot_freshness(store, &current).map_err(|error| CurrentSnapshotFailure {
                    reason: catalog_reason(&error),
                    snapshot: None,
                })?;
            let state = if current.view_id() == requested_id {
                freshness
            } else {
                DashboardSnapshotStateV1::Stale
            };
            Ok(CurrentSnapshotState {
                snapshot: snapshot_contract(&current, freshness),
                state,
            })
        }
        Ok(None) => Err(CurrentSnapshotFailure {
            reason: DashboardStatusReasonV1::RefreshPending,
            snapshot: None,
        }),
        Err(error) => Err(CurrentSnapshotFailure {
            reason: catalog_reason(&error),
            snapshot: None,
        }),
    }
}

fn snapshot_freshness(
    store: &ReportViewReadScope<'_>,
    snapshot: &ReportViewSnapshot,
) -> Result<DashboardSnapshotStateV1, ReportViewCatalogError> {
    Ok(if store.source_generation()? == snapshot.generation() {
        DashboardSnapshotStateV1::Current
    } else {
        DashboardSnapshotStateV1::Stale
    })
}

fn snapshot_contract(
    snapshot: &ReportViewSnapshot,
    state: DashboardSnapshotStateV1,
) -> DashboardSnapshotV1 {
    DashboardSnapshotV1 {
        id: snapshot.view_id().to_owned(),
        generation: snapshot.generation().to_string(),
        visibility_epoch: snapshot.visibility_epoch().to_string(),
        generated_at: snapshot.generated_at().to_owned(),
        state,
    }
}

fn scope(filters: DashboardFiltersV1, selected_trace_id: Option<String>) -> DashboardQueryScopeV1 {
    DashboardQueryScopeV1 {
        filters,
        selected_trace_id,
        cold_excluded: true,
    }
}

fn span_row(span: &ReportSpanV2) -> DashboardSpanRowV1 {
    DashboardSpanRowV1 {
        trace_id: span.trace_id.clone(),
        span_id: span.span_id.clone(),
        parent_span_id: span.parent_span_id.clone(),
        kind: span_kind(span.kind).into(),
        name: span.name.clone(),
        status: status_code(span.status).into(),
        start_time_unix_ms: span.start_time_unix_ms,
        end_time_unix_ms: span.end_time_unix_ms,
        repo: span.repo.clone(),
        agent: span.agent.name.clone(),
        model: span.agent.model.clone(),
        session_id: span.session_id.clone(),
        turn_id: span.turn_id.clone(),
        tool_name: span.tool_name.clone(),
        availability_reasons: availability_reasons(span),
    }
}

fn availability_reasons(span: &ReportSpanV2) -> Vec<DashboardAvailabilityReasonV1> {
    [
        (
            DashboardAvailabilityFieldV1::Repository,
            &span.availability.repository,
        ),
        (DashboardAvailabilityFieldV1::Turn, &span.availability.turn),
        (
            DashboardAvailabilityFieldV1::Model,
            &span.availability.model,
        ),
        (
            DashboardAvailabilityFieldV1::Tokens,
            &span.availability.tokens,
        ),
        (
            DashboardAvailabilityFieldV1::Latency,
            &span.availability.latency,
        ),
        (
            DashboardAvailabilityFieldV1::SourceLocation,
            &span.availability.source_location,
        ),
        (
            DashboardAvailabilityFieldV1::RequestContent,
            &span.availability.request_content,
        ),
        (
            DashboardAvailabilityFieldV1::ResponseContent,
            &span.availability.response_content,
        ),
    ]
    .into_iter()
    .filter(|(_, availability)| availability.state != AvailabilityStateV2::Available)
    .map(|(field, availability)| DashboardAvailabilityReasonV1 {
        field,
        availability: availability.clone(),
    })
    .collect()
}

const fn span_kind(kind: SpanKind) -> &'static str {
    match kind {
        SpanKind::LlmRequest => "llm.request",
        SpanKind::ToolExecution => "tool.execution",
        SpanKind::AgentSession => "agent.session",
        SpanKind::Workstream => "workstream",
        SpanKind::Turn => "turn",
        SpanKind::Permission => "permission",
        SpanKind::Compaction => "compaction",
    }
}

const fn status_code(status: StatusCode) -> &'static str {
    match status {
        StatusCode::Ok => "ok",
        StatusCode::Error => "error",
        StatusCode::Unset => "unset",
    }
}

fn normalize_request(request: &mut DashboardQueryRequestV1) {
    let filters = match request {
        DashboardQueryRequestV1::Bootstrap(request) => &mut request.filters,
        DashboardQueryRequestV1::Traces(request) => &mut request.filters,
        DashboardQueryRequestV1::Spans(request) => &mut request.filters,
        DashboardQueryRequestV1::Summary(request) => &mut request.filters,
        DashboardQueryRequestV1::Span(request) => &mut request.filters,
        DashboardQueryRequestV1::Facets(request) => &mut request.filters,
    };
    let filters = filters.get_or_insert_with(DashboardFiltersV1::default);
    for values in [
        &mut filters.repo,
        &mut filters.session,
        &mut filters.agent,
        &mut filters.model,
    ] {
        values.sort();
        values.dedup();
    }
    if let Some(text) = &mut filters.text {
        *text = text.to_lowercase();
    }
}

const fn request_kind(request: &DashboardQueryRequestV1) -> DashboardRequestKindV1 {
    match request {
        DashboardQueryRequestV1::Bootstrap(_) => DashboardRequestKindV1::Bootstrap,
        DashboardQueryRequestV1::Traces(_) => DashboardRequestKindV1::Traces,
        DashboardQueryRequestV1::Spans(_) => DashboardRequestKindV1::Spans,
        DashboardQueryRequestV1::Summary(_) => DashboardRequestKindV1::Summary,
        DashboardQueryRequestV1::Span(_) => DashboardRequestKindV1::Span,
        DashboardQueryRequestV1::Facets(_) => DashboardRequestKindV1::Facets,
    }
}

fn validated_or_status(
    kind: DashboardRequestKindV1,
    response: DashboardQueryResponseV1,
) -> DashboardQueryResponseV1 {
    match response.validate() {
        Ok(()) => response,
        Err(DashboardContractError::ResponseTooLarge) => {
            status(kind, DashboardStatusReasonV1::Capacity, None)
        }
        Err(_) => status(kind, DashboardStatusReasonV1::InvalidQuery, None),
    }
}

fn catalog_status(
    kind: DashboardRequestKindV1,
    error: &ReportViewCatalogError,
    snapshot: Option<DashboardSnapshotV1>,
) -> DashboardQueryResponseV1 {
    status(kind, catalog_reason(error), snapshot)
}

fn catalog_reason(error: &ReportViewCatalogError) -> DashboardStatusReasonV1 {
    match error {
        ReportViewCatalogError::Busy => DashboardStatusReasonV1::Busy,
        ReportViewCatalogError::SnapshotExpired => DashboardStatusReasonV1::SnapshotExpired,
        ReportViewCatalogError::RefreshPending | ReportViewCatalogError::SnapshotChanged => {
            DashboardStatusReasonV1::RefreshPending
        }
        ReportViewCatalogError::CatalogCapacityExceeded
        | ReportViewCatalogError::Build(ReportViewBuildError::CapacityExceeded) => {
            DashboardStatusReasonV1::Capacity
        }
        _ => DashboardStatusReasonV1::InvalidQuery,
    }
}

fn status(
    request_kind: DashboardRequestKindV1,
    reason: DashboardStatusReasonV1,
    snapshot: Option<DashboardSnapshotV1>,
) -> DashboardQueryResponseV1 {
    DashboardQueryResponseV1::Status(DashboardStatusResponseV1 {
        schema_version: DASHBOARD_QUERY_VERSION.into(),
        kind: DashboardStatusKindV1::Status,
        request_kind,
        reason,
        snapshot,
    })
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report_view::MISSING_RATE_FINGERPRINT;
    use crate::report_view_catalog::publish_report_view;
    use agent_observability_contracts::dashboard::{
        DashboardSpansRequestV1, DashboardSummaryKindV1, DashboardSummaryRequestV1,
    };
    use agent_observability_contracts::{AgentSource, ObservationEvent, SourceObservation};
    use agent_observability_domain::{
        CorrelationIds, LifecycleState, ObservationId, SourceCursor, SourceGeneration, SpanId,
        Timing, TokenUsage, TraceId,
    };
    use std::fs;
    use std::path::PathBuf;

    fn service() -> DashboardQueryService {
        DashboardQueryService::new([7; 32])
    }

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agent-observability-dashboard-query-{label}-{}",
            std::process::id()
        ))
    }

    fn published_empty_store(label: &str) -> (PathBuf, LocalStore, String) {
        let directory = temp_dir(label);
        let _ = fs::remove_dir_all(&directory);
        let store = LocalStore::open(&directory).unwrap();
        let staging = crate::build_report_view_staging(
            &store,
            MISSING_RATE_FINGERPRINT,
            128 * 1024 * 1024,
            None,
        )
        .unwrap();
        let published = publish_report_view(&store, staging).unwrap();
        let snapshot = published.current().view_id().to_owned();
        (directory, store, snapshot)
    }

    fn published_store_with_spans(
        label: &str,
        count: usize,
    ) -> (PathBuf, LocalStore, String, String) {
        let directory = temp_dir(label);
        let _ = fs::remove_dir_all(&directory);
        let mut store = LocalStore::open(&directory).unwrap();
        let observations = (0..count)
            .map(|index| {
                let cursor = format!("cursor-{index:05}");
                let previous_source_cursor = index
                    .checked_sub(1)
                    .map(|previous| SourceCursor::parse(format!("cursor-{previous:05}")).unwrap());
                SourceObservation {
                    source: AgentSource::Codex,
                    source_generation: SourceGeneration::parse("generation").unwrap(),
                    previous_source_cursor,
                    source_cursor: SourceCursor::parse(cursor.clone()).unwrap(),
                    observation_id: ObservationId::parse(format!("observation-{index:05}"))
                        .unwrap(),
                    trace_id: TraceId::parse("trace").unwrap(),
                    span_id: SpanId::parse(format!("span-{index:05}")).unwrap(),
                    parent_span_id: None,
                    correlation: CorrelationIds::default(),
                    event: ObservationEvent::Turn,
                    lifecycle: LifecycleState::Completed,
                    timing: Timing::new(
                        u64::try_from(index).unwrap(),
                        Some(u64::try_from(index + 1).unwrap()),
                    )
                    .unwrap(),
                    token_usage: TokenUsage::default(),
                }
            })
            .collect::<Vec<_>>();
        store
            .ingest_batch_deferred_projection(&observations)
            .unwrap();
        store.rebuild_projection().unwrap();
        let staging = crate::build_report_view_staging(
            &store,
            MISSING_RATE_FINGERPRINT,
            128 * 1024 * 1024,
            None,
        )
        .unwrap();
        let published = publish_report_view(&store, staging).unwrap();
        let snapshot = published.current().view_id().to_owned();
        (
            directory,
            store,
            snapshot,
            agent_observability_contracts::hash_opaque_identifier("trace"),
        )
    }

    fn query_database(count: usize) -> rusqlite::Connection {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE spans(
                    trace_id TEXT NOT NULL,
                    span_id TEXT NOT NULL,
                    start_time_unix_ms REAL NOT NULL,
                    repo TEXT NOT NULL,
                    session_id TEXT,
                    agent TEXT,
                    model TEXT,
                    span_json TEXT NOT NULL
                );
                CREATE INDEX spans_trace_order_idx ON spans(trace_id,start_time_unix_ms,span_id);
                CREATE INDEX spans_repo_order_idx ON spans(repo,start_time_unix_ms,trace_id,span_id);
                CREATE INDEX spans_session_order_idx ON spans(session_id,start_time_unix_ms,trace_id,span_id);
                CREATE INDEX spans_agent_order_idx ON spans(agent,start_time_unix_ms,trace_id,span_id);
                CREATE INDEX spans_model_order_idx ON spans(model,start_time_unix_ms,trace_id,span_id);",
            )
            .unwrap();
        let report: agent_observability_contracts::ReportDtoV2 = serde_json::from_str(
            include_str!("../../../contracts/report-dto-v2.fixture.json"),
        )
        .unwrap();
        let mut span = report.spans[0].clone();
        span.repo = "repo-a".into();
        span.session_id = Some(agent_observability_contracts::hash_opaque_identifier(
            "session-a",
        ));
        span.agent.name = Some("agent-a".into());
        span.agent.model = Some("model-a".into());
        for index in 0..count {
            span.trace_id = if index < 600 {
                format!("id:sha256:{}", "1".repeat(64))
            } else {
                format!("id:sha256:{}", "2".repeat(64))
            };
            span.span_id =
                agent_observability_contracts::hash_opaque_identifier(&format!("span-{index}"));
            span.start_time_unix_ms =
                f64::from(u32::try_from(index).expect("test row count fits u32"));
            connection
                .execute(
                    "INSERT INTO spans VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![
                        span.trace_id,
                        span.span_id,
                        span.start_time_unix_ms,
                        span.repo,
                        span.session_id,
                        span.agent.name,
                        span.agent.model,
                        serde_json::to_string(&span).unwrap(),
                    ],
                )
                .unwrap();
        }
        connection
    }

    fn summary_request(snapshot_id: &str, cursor: Option<String>) -> DashboardQueryRequestV1 {
        DashboardQueryRequestV1::Summary(DashboardSummaryRequestV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardSummaryKindV1::Summary,
            filters: None,
            snapshot_id: snapshot_id.into(),
            cursor,
        })
    }

    fn spans_request(
        snapshot_id: &str,
        trace_id: &str,
        cursor: Option<String>,
    ) -> DashboardQueryRequestV1 {
        DashboardQueryRequestV1::Spans(DashboardSpansRequestV1 {
            schema_version: DASHBOARD_QUERY_VERSION.into(),
            kind: DashboardSpansKindV1::Spans,
            filters: None,
            snapshot_id: snapshot_id.into(),
            trace_id: trace_id.into(),
            cursor,
        })
    }

    fn spans_page(response: DashboardQueryResponseV1) -> DashboardSpansResponseV1 {
        let DashboardQueryResponseV1::Spans(response) = response else {
            panic!("expected spans response");
        };
        response
    }

    fn pending_cursor(response: DashboardQueryResponseV1) -> String {
        let DashboardQueryResponseV1::Summary(response) = response else {
            panic!("expected summary response");
        };
        assert!(matches!(response.work, DashboardWorkV1::Pending(_)));
        response.pagination.next_cursor.unwrap()
    }

    #[test]
    fn read_scope_holds_publication_guard_after_database_callback_and_response_assembly() {
        let (directory, store, snapshot, trace) = published_store_with_spans("response-guard", 2);
        let scope = ReportViewReadScope::acquire(&store).unwrap();
        scope.with_snapshot(&snapshot, |_, _| Ok(())).unwrap();
        // The database callback has returned, but destructive maintenance must still wait.
        assert!(store.try_acquire_report_render_guard().unwrap().is_none());
        let mut service = service();
        let response = service.query_scoped(
            &scope,
            &mut spans_request(&snapshot, &trace, None),
            Instant::now(),
        );
        response.validate().unwrap();
        assert_eq!(spans_page(response).rows.len(), 2);
        assert!(store.try_acquire_report_render_guard().unwrap().is_none());
        drop(scope);
        assert!(store.try_acquire_report_render_guard().unwrap().is_some());
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn authority_generation_advance_marks_retained_snapshot_stale_without_revoking_rows() {
        let (directory, store, snapshot, trace) =
            published_store_with_spans("source-generation-stale", 2);
        store.db.execute(
            "UPDATE metadata SET value = CAST(value AS INTEGER) + 1 WHERE key = 'report_generation'",
            [],
        ).unwrap();
        let mut service = service();
        let response = service.query(&store, spans_request(&snapshot, &trace, None));
        response.validate().unwrap();
        let page = spans_page(response);
        assert_eq!(page.snapshot.state, DashboardSnapshotStateV1::Stale);
        assert_eq!(page.rows.len(), 2);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn normalization_sorts_deduplicates_and_lowercases_text() {
        let mut request = DashboardQueryRequestV1::Bootstrap(
            agent_observability_contracts::dashboard::DashboardBootstrapRequestV1 {
                schema_version: DASHBOARD_QUERY_VERSION.into(),
                kind: DashboardBootstrapKindV1::Bootstrap,
                filters: Some(DashboardFiltersV1 {
                    repo: vec!["z".into(), "a".into(), "z".into()],
                    text: Some("MiXeD".into()),
                    ..DashboardFiltersV1::default()
                }),
            },
        );
        normalize_request(&mut request);
        let DashboardQueryRequestV1::Bootstrap(request) = request else {
            unreachable!();
        };
        let filters = request.filters.unwrap();
        assert_eq!(filters.repo, ["a", "z"]);
        assert_eq!(filters.text.as_deref(), Some("mixed"));
    }

    #[test]
    fn consumed_summary_cursor_expires_and_new_cursor_finishes_all_passes() {
        let (directory, store, snapshot) = published_empty_store("summary-replay");
        let mut service = service();
        let now = Instant::now();
        let first = service.query_at(&store, &mut summary_request(&snapshot, None), now);
        let first_cursor = pending_cursor(first);

        let second = service.query_at(
            &store,
            &mut summary_request(&snapshot, Some(first_cursor.clone())),
            now,
        );
        let second_cursor = pending_cursor(second);
        let replay = service.query_at(
            &store,
            &mut summary_request(&snapshot, Some(first_cursor)),
            now,
        );
        let DashboardQueryResponseV1::Status(replay) = replay else {
            panic!("consumed cursor must expire");
        };
        assert_eq!(replay.reason, DashboardStatusReasonV1::SnapshotExpired);

        let complete = service.query_at(
            &store,
            &mut summary_request(&snapshot, Some(second_cursor)),
            now,
        );
        let DashboardQueryResponseV1::Summary(complete) = complete else {
            panic!("expected complete summary");
        };
        let DashboardWorkV1::Complete(complete) = complete.work else {
            panic!("summary must remain pending through all three scans");
        };
        assert_eq!(complete.kpis.sessions, 0);
        assert_eq!(complete.kpis.turns, 0);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn one_query_can_advance_more_than_one_hundred_pages_with_bounded_history() {
        let mut service = service();
        let now = Instant::now();
        let binding = CursorBinding::new(
            &"a".repeat(64),
            &DashboardFiltersV1::default(),
            DashboardRequestKindV1::Spans,
            Some(&format!("id:sha256:{}", "b".repeat(64))),
        );
        let first_cursor = service
            .issue_lease(
                None,
                binding.clone(),
                CursorState::Spans(SpanScanCursor::default()),
                now,
            )
            .unwrap();
        let mut cursor = first_cursor.clone();
        for index in 0..100_u32 {
            service.resolve_lease(&cursor, &binding, now).unwrap();
            cursor = service
                .issue_lease(
                    Some(&cursor),
                    binding.clone(),
                    CursorState::Spans(SpanScanCursor {
                        after: Some((f64::from(index), index.to_string(), index.to_string())),
                    }),
                    now,
                )
                .unwrap();
            assert!(service.leases.len() <= MAX_CURSOR_LEASES);
        }
        assert!(service.leases.contains_key(&cursor));
        assert_eq!(service.leases.len(), MAX_CURSOR_LEASES);
        assert!(matches!(
            service.resolve_lease(&first_cursor, &binding, now),
            Err(DashboardStatusReasonV1::SnapshotExpired)
        ));

        let independent = CursorBinding::new(
            &"c".repeat(64),
            &DashboardFiltersV1::default(),
            DashboardRequestKindV1::Summary,
            None,
        );
        assert!(
            service
                .issue_lease(
                    None,
                    independent,
                    CursorState::Summary(SummaryProgress::default()),
                    now,
                )
                .is_ok()
        );
        assert_eq!(service.leases.len(), MAX_CURSOR_LEASES);
    }

    #[test]
    fn replay_boundary_drift_expires_parent_without_replacing_child() {
        let mut service = service();
        let now = Instant::now();
        let binding = CursorBinding::new(
            &"a".repeat(64),
            &DashboardFiltersV1::default(),
            DashboardRequestKindV1::Spans,
            Some(&format!("id:sha256:{}", "b".repeat(64))),
        );
        let parent = service
            .issue_lease(
                None,
                binding.clone(),
                CursorState::Spans(SpanScanCursor::default()),
                now,
            )
            .unwrap();
        let expected = CursorState::Spans(SpanScanCursor {
            after: Some((1.0, "trace".into(), "span-1".into())),
        });
        let child = service
            .finish_history_page(
                Some(parent.clone()),
                binding.clone(),
                expected,
                false,
                None,
                now,
            )
            .unwrap()
            .unwrap();
        let replay = service
            .resolve_lease(&parent, &binding, now)
            .unwrap()
            .replay;
        let drifted = CursorState::Spans(SpanScanCursor {
            after: Some((2.0, "trace".into(), "span-2".into())),
        });
        assert!(matches!(
            service
                .finish_history_page(Some(parent.clone()), binding, drifted, false, replay, now,),
            Err(DashboardStatusReasonV1::SnapshotExpired)
        ));
        assert!(!service.leases.contains_key(&parent));
        assert!(service.leases.contains_key(&child));
    }

    #[test]
    fn spans_back_navigation_replays_and_forward_progress_exceeds_history_bound() {
        let (directory, store, snapshot, trace) =
            published_store_with_spans("spans-history", 3_601);
        let mut service = service();

        let first = spans_page(service.query(&store, spans_request(&snapshot, &trace, None)));
        let first_cursor = first.pagination.next_cursor.unwrap();
        let second = spans_page(service.query(
            &store,
            spans_request(&snapshot, &trace, Some(first_cursor.clone())),
        ));
        let second_cursor = second.pagination.next_cursor.clone().unwrap();
        let third = spans_page(service.query(
            &store,
            spans_request(&snapshot, &trace, Some(second_cursor.clone())),
        ));
        let third_cursor = third.pagination.next_cursor.clone().unwrap();

        let previous = spans_page(service.query(
            &store,
            spans_request(&snapshot, &trace, Some(first_cursor.clone())),
        ));
        assert_eq!(previous.rows, second.rows);
        assert_eq!(previous.pagination.next_cursor, Some(second_cursor));

        let mut next = Some(third_cursor);
        let mut cursor_advances = 2;
        while let Some(cursor) = next {
            let page =
                spans_page(service.query(&store, spans_request(&snapshot, &trace, Some(cursor))));
            next = page.pagination.next_cursor;
            cursor_advances += 1;
        }
        assert!(cursor_advances > MAX_CURSOR_LEASES);
        assert!(service.leases.len() <= MAX_CURSOR_LEASES);

        let expired = service.query(&store, spans_request(&snapshot, &trace, Some(first_cursor)));
        let DashboardQueryResponseV1::Status(expired) = expired else {
            panic!("cursor beyond retained history must expire");
        };
        assert_eq!(expired.reason, DashboardStatusReasonV1::SnapshotExpired);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn cursor_idle_expiry_is_typed_and_never_restarts_at_zero() {
        let (directory, store, snapshot) = published_empty_store("cursor-expiry");
        let mut service = service();
        let now = Instant::now();
        let first = service.query_at(&store, &mut summary_request(&snapshot, None), now);
        let cursor = pending_cursor(first);
        let expired = service.query_at(
            &store,
            &mut summary_request(&snapshot, Some(cursor)),
            now + CURSOR_IDLE_TTL + Duration::from_millis(1),
        );
        let DashboardQueryResponseV1::Status(expired) = expired else {
            panic!("expired cursor must return status");
        };
        assert_eq!(expired.reason, DashboardStatusReasonV1::SnapshotExpired);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn trace_scan_retains_one_partial_trace_and_emits_only_exact_complete_rows() {
        let connection = query_database(601);
        let mut cursor = TraceScanCursor::default();
        let mut rows = Vec::new();
        let mut slices = 0;
        loop {
            let batch =
                scan_trace_rows(&connection, cursor, &DashboardFiltersV1::default()).unwrap();
            if slices == 0 {
                assert!(batch.rows.is_empty());
                assert!(batch.cursor.partial.is_some());
            }
            rows.extend(batch.rows);
            cursor = batch.cursor;
            slices += 1;
            if batch.exhausted {
                break;
            }
        }
        assert!(slices >= 2);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].span_count, 600);
        assert_eq!(rows[1].span_count, 1);
    }

    #[test]
    fn facet_scan_walks_dimensions_serially_without_duplicate_values() {
        let connection = query_database(601);
        let mut cursor = FacetScanCursor::default();
        let mut rows = Vec::new();
        loop {
            let batch = scan_facet_rows(
                &connection,
                cursor,
                ReportViewKernel::V1,
                &DashboardFiltersV1::default(),
            )
            .unwrap();
            rows.extend(batch.rows);
            cursor = batch.cursor;
            if batch.exhausted {
                break;
            }
        }
        let encoded = rows
            .iter()
            .map(|row| serde_json::to_string(row).unwrap())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(encoded.len(), rows.len());
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().any(|row| {
            row.dimension == DashboardFacetDimensionV1::Repo && row.value == "repo-a"
        }));
        assert!(rows.iter().any(|row| {
            row.dimension == DashboardFacetDimensionV1::Session
                && row.value == agent_observability_contracts::hash_opaque_identifier("session-a")
        }));
        assert!(rows.iter().any(|row| {
            row.dimension == DashboardFacetDimensionV1::Agent && row.value == "agent-a"
        }));
        assert!(rows.iter().any(|row| {
            row.dimension == DashboardFacetDimensionV1::Model && row.value == "model-a"
        }));
    }
    fn narrow_test_indexes(connection: &rusqlite::Connection) {
        for (name, dimension) in [
            ("repo", "repo"),
            ("session", "session_id"),
            ("turn", "turn_id"),
            ("agent", "agent"),
            ("model", "model"),
        ] {
            connection.execute_batch(&format!("DROP INDEX IF EXISTS spans_{name}_order_idx; CREATE INDEX spans_{name}_order_idx ON spans({dimension},source_order)")).unwrap();
        }
    }

    fn complete_test_summary(
        connection: &rusqlite::Connection,
        kernel: ReportViewKernel,
        filters: &DashboardFiltersV1,
    ) -> String {
        let mut progress = SummaryProgress::default();
        for _ in 0..100 {
            match advance_summary(connection, progress, kernel, filters)
                .unwrap_or_else(|_| panic!("summary failed"))
            {
                SummaryStep::Pending(next) => progress = next,
                SummaryStep::Complete(kpis) => return serde_json::to_string(&kpis).unwrap(),
            }
        }
        panic!("summary did not converge")
    }

    fn complete_test_facets(
        connection: &rusqlite::Connection,
        kernel: ReportViewKernel,
        filters: &DashboardFiltersV1,
    ) -> Vec<String> {
        let mut cursor = FacetScanCursor::default();
        let mut rows = Vec::new();
        for _ in 0..100 {
            let batch = scan_facet_rows(connection, cursor, kernel, filters).unwrap();
            rows.extend(
                batch
                    .rows
                    .iter()
                    .map(|row| serde_json::to_string(row).unwrap()),
            );
            cursor = batch.cursor;
            if batch.exhausted {
                return rows;
            }
        }
        panic!("facets did not converge")
    }

    #[test]
    fn dual_kernel_summary_and_facets_match_with_sparse_null_reverse_rows() {
        let connection = query_database(1100);
        connection.execute_batch("ALTER TABLE spans ADD COLUMN source_order INTEGER; UPDATE spans SET source_order=1101-rowid,start_time_unix_ms=0; ALTER TABLE spans ADD COLUMN turn_id TEXT; UPDATE spans SET turn_id=session_id; CREATE INDEX spans_turn_order_idx ON spans(turn_id,start_time_unix_ms,trace_id,span_id); CREATE INDEX stable ON spans(start_time_unix_ms,trace_id,span_id)").unwrap();
        let mut statement = connection
            .prepare("SELECT rowid,span_json FROM spans")
            .unwrap();
        let values = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for (id, json) in values {
            let mut span: ReportSpanV2 = serde_json::from_str(&json).unwrap();
            span.start_time_unix_ms = 0.0;
            span.name = if id >= 1098 {
                "late-match"
            } else {
                "not-matching"
            }
            .into();
            span.session_id = if id < 3 {
                None
            } else {
                Some(agent_observability_contracts::hash_opaque_identifier(
                    if id < 1099 { "a" } else { "b" },
                ))
            };
            connection
                .execute(
                    "UPDATE spans SET session_id=?1,turn_id=?1,span_json=?2 WHERE rowid=?3",
                    params![span.session_id, serde_json::to_string(&span).unwrap(), id],
                )
                .unwrap();
        }
        let filters = DashboardFiltersV1 {
            text: Some("late-match".into()),
            ..DashboardFiltersV1::default()
        };
        let summary = complete_test_summary(&connection, ReportViewKernel::V1, &filters);
        let facets = complete_test_facets(&connection, ReportViewKernel::V1, &filters);
        assert_eq!(facets.len(), 5);
        narrow_test_indexes(&connection);
        assert_eq!(
            summary,
            complete_test_summary(&connection, ReportViewKernel::V2, &filters)
        );
        assert_eq!(
            facets,
            complete_test_facets(&connection, ReportViewKernel::V2, &filters)
        );
        for phase in [
            FacetPhase::Repo,
            FacetPhase::Session,
            FacetPhase::Agent,
            FacetPhase::Model,
        ] {
            let sql = format!(
                "EXPLAIN QUERY PLAN {}",
                facet_query(phase, ReportViewKernel::V2).0
            );
            let plan = connection
                .prepare(&sql)
                .unwrap()
                .query_map(params!["a", 500], |row| row.get::<_, String>(3))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join(" ");
            assert!(plan.contains("SEARCH spans USING INDEX spans_"), "{plan}");
            assert!(!plan.contains("TEMP B-TREE"), "{plan}");
        }
    }

    #[test]
    fn facets_and_summary_reject_cross_kernel_state_even_at_phase_boundary() {
        let connection = query_database(0);
        for (before, after) in [
            (ReportViewKernel::V1, ReportViewKernel::V2),
            (ReportViewKernel::V2, ReportViewKernel::V1),
        ] {
            let cursor = FacetScanCursor {
                after: DimensionPosition::new(before),
                ..FacetScanCursor::default()
            };
            assert!(matches!(
                scan_facet_rows(&connection, cursor, after, &DashboardFiltersV1::default()),
                Err(ReportViewBuildError::InvalidStagingState)
            ));
            let progress = SummaryProgress::SpansAt {
                kernel: before,
                cursor: SpanScanCursor::default(),
                accumulator: DashboardSummaryAccumulator::default(),
            };
            assert!(matches!(
                advance_summary(&connection, progress, after, &DashboardFiltersV1::default()),
                Err(SummaryAdvanceError::Build(
                    ReportViewBuildError::InvalidStagingState
                ))
            ));
        }
    }
    #[test]
    fn dual_kernel_facets_cross_output_page_limits_in_lexical_order() {
        let connection = query_database(700);
        connection.execute_batch("ALTER TABLE spans ADD COLUMN source_order INTEGER; UPDATE spans SET source_order=701-rowid; ALTER TABLE spans ADD COLUMN turn_id TEXT").unwrap();
        let mut statement = connection
            .prepare("SELECT rowid,span_json FROM spans ORDER BY rowid DESC")
            .unwrap();
        let values = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for (id, json) in values {
            let mut span: ReportSpanV2 = serde_json::from_str(&json).unwrap();
            span.repo = format!("repo-{id:04}");
            connection
                .execute(
                    "UPDATE spans SET repo=?1,span_json=?2 WHERE rowid=?3",
                    params![span.repo, serde_json::to_string(&span).unwrap(), id],
                )
                .unwrap();
        }
        let filters = DashboardFiltersV1::default();
        let expected = complete_test_facets(&connection, ReportViewKernel::V1, &filters);
        assert_eq!(expected.len(), 703);
        assert!(expected.first().unwrap().contains("repo-0001"));
        assert!(expected[699].contains("repo-0700"));
        narrow_test_indexes(&connection);
        let first = scan_facet_rows(
            &connection,
            FacetScanCursor::default(),
            ReportViewKernel::V2,
            &filters,
        )
        .unwrap();
        assert!(first.rows.len() <= DASHBOARD_FACET_MAX_VALUES);
        assert!(!first.exhausted);
        assert_eq!(
            expected,
            complete_test_facets(&connection, ReportViewKernel::V2, &filters)
        );
    }
}
