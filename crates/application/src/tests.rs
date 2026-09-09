use super::*;
use agent_observability_contracts::{
    AgentV1, AttributesV1, ContentV1, DurableRecordV1, MetricsV1, ProjectV1, RedactionV1, StatusV1,
};
use agent_observability_domain::SpanKind;

fn table() -> RateTable {
    normalize_rate_table(RateTableInput {
        version: Some("test-2026-07".into()),
        currency: Some("USD".into()),
        unit: Some("per_1m_tokens".into()),
        assumption: Some("Fixture rates for tests; not billing truth.".into()),
        models: BTreeMap::from([
            (
                String::from("gpt-test"),
                ModelRatesInput {
                    input_tokens: Some(2.0),
                    output_tokens: Some(8.0),
                    cached_input_tokens: Some(0.5),
                    reasoning_output_tokens: Some(10.0),
                    token_semantics: BTreeMap::from([
                        (
                            String::from("cached_input_tokens"),
                            String::from("included_in_total"),
                        ),
                        (
                            String::from("reasoning_output_tokens"),
                            String::from("included_in_total"),
                        ),
                    ]),
                    ..ModelRatesInput::default()
                },
            ),
            (
                String::from("gpt-incomplete"),
                ModelRatesInput {
                    input_tokens: Some(1.0),
                    token_semantics: BTreeMap::from([
                        (
                            String::from("cached_input_tokens"),
                            String::from("included_in_total"),
                        ),
                        (
                            String::from("reasoning_output_tokens"),
                            String::from("included_in_total"),
                        ),
                    ]),
                    ..ModelRatesInput::default()
                },
            ),
        ]),
    })
    .unwrap()
}
fn span(model: &str, metrics: MetricsV1) -> DurableRecordV1 {
    DurableRecordV1 {
        schema_version: "agent_observability.v1".into(),
        record_type: "span".into(),
        trace_id: "trace".into(),
        span_id: model.into(),
        parent_span_id: None,
        span_kind: SpanKind::LlmRequest,
        name: model.into(),
        start_time_unix_ms: 0.0,
        end_time_unix_ms: None,
        status: StatusV1 {
            code: agent_observability_domain::StatusCode::Ok,
        },
        agent: AgentV1 {
            model: Some(model.into()),
            ..AgentV1::default()
        },
        project: ProjectV1::default(),
        attributes: AttributesV1::default(),
        metrics,
        content: ContentV1::default(),
        redaction: RedactionV1::default(),
    }
}
fn full(model: &str) -> DurableRecordV1 {
    span(
        model,
        MetricsV1 {
            input_tokens: Some(1_000_000.0),
            output_tokens: Some(500_000.0),
            cached_input_tokens: Some(100_000.0),
            reasoning_output_tokens: Some(10_000.0),
            ..MetricsV1::default()
        },
    )
}

#[test]
fn paged_summary_preserves_token_precedence_and_rejects_overflow_atomically() {
    use crate::dashboard_summary::{DashboardSummaryAccumulator, dashboard_token_total};
    use agent_observability_contracts::dashboard::{DashboardCostStatusV1, DashboardTokenStatusV1};
    let rates = table();
    let projected = project_report_span(0, &full("gpt-test"), Some(&rates)).unwrap();
    let mut accumulator = DashboardSummaryAccumulator::default();
    accumulator.push(&projected).unwrap();
    let expected = accumulator.clone().finish(1, 1).unwrap();
    assert_eq!(expected.total_tokens, Some(1_500_000));
    assert_eq!(expected.token_status, DashboardTokenStatusV1::Complete);
    let mut invalid = projected.clone();
    invalid.metrics.input_tokens = Some(f64::INFINITY);
    assert!(accumulator.push(&invalid).is_err());
    assert_eq!(accumulator.finish(1, 1).unwrap(), expected);
    let empty = DashboardSummaryAccumulator::default().finish(0, 0).unwrap();
    assert_eq!(empty.token_status, DashboardTokenStatusV1::Unavailable);
    assert_eq!(empty.cost_status, DashboardCostStatusV1::Unknown);
    assert_eq!(empty.estimated_cost, None);
    let mut metrics = projected.metrics.clone();
    metrics.total_tokens = Some(3.0);
    assert_eq!(dashboard_token_total(&metrics), Some(1_500_000.0));
    metrics.output_tokens = None;
    assert_eq!(dashboard_token_total(&metrics), Some(3.0));
}

#[test]
fn paged_summary_without_token_usage_validates_as_complete_wire_response() {
    use crate::dashboard_summary::DashboardSummaryAccumulator;
    use agent_observability_contracts::dashboard::DashboardQueryResponseV1;
    let mut record = span("tool", MetricsV1::default());
    record.span_kind = SpanKind::ToolExecution;
    let projected = project_report_span(0, &record, None).unwrap();
    for include_tool in [false, true] {
        let mut accumulator = DashboardSummaryAccumulator::default();
        if include_tool {
            accumulator.push(&projected).unwrap();
        }
        let kpis = accumulator.finish(0, 0).unwrap();
        assert_eq!(kpis.input_tokens, None);
        assert_eq!(kpis.output_tokens, None);
        assert_eq!(kpis.total_tokens, None);
        let wire = serde_json::json!({
            "schemaVersion": "agent_observability.dashboard_query.v1",
            "kind": "summary",
            "snapshot": {
                "id": "a".repeat(64), "generation": "0", "visibilityEpoch": "0",
                "generatedAt": "2026-09-07T00:00:00.000Z", "state": "current"
            },
            "scope": { "filters": {}, "selectedTraceId": null, "coldExcluded": true },
            "work": { "state": "complete", "kpis": kpis },
            "pagination": { "nextCursor": null }
        });
        let response: DashboardQueryResponseV1 = serde_json::from_value(wire).unwrap();
        response.validate().unwrap();
    }
}

#[test]
fn paged_summary_never_fabricates_missing_token_components() {
    use crate::dashboard_summary::DashboardSummaryAccumulator;
    use agent_observability_contracts::dashboard::DashboardTokenStatusV1;
    let total_only = project_report_span(
        0,
        &span(
            "total-only",
            MetricsV1 {
                total_tokens: Some(3.0),
                ..MetricsV1::default()
            },
        ),
        None,
    )
    .unwrap();
    let mut accumulator = DashboardSummaryAccumulator::default();
    accumulator.push(&total_only).unwrap();
    let total = accumulator.clone().finish(1, 1).unwrap();
    assert_eq!(total.token_status, DashboardTokenStatusV1::Complete);
    assert_eq!(total.total_tokens, Some(3));
    assert_eq!(total.input_tokens, None);
    assert_eq!(total.output_tokens, None);
    let direct = project_report_span(
        1,
        &span(
            "direct",
            MetricsV1 {
                input_tokens: Some(0.0),
                output_tokens: Some(2.0),
                ..MetricsV1::default()
            },
        ),
        None,
    )
    .unwrap();
    let mut direct_only = DashboardSummaryAccumulator::default();
    direct_only.push(&direct).unwrap();
    let direct_total = direct_only.finish(1, 1).unwrap();
    assert_eq!(direct_total.input_tokens, Some(0));
    assert_eq!(direct_total.output_tokens, Some(2));
    accumulator.push(&direct).unwrap();
    let mixed = accumulator.finish(2, 2).unwrap();
    assert_eq!(mixed.total_tokens, Some(5));
    assert_eq!(mixed.input_tokens, None);
    assert_eq!(mixed.output_tokens, None);
}

#[test]
fn paged_summary_marks_partial_tokens_and_mixed_currency_without_false_totals() {
    use crate::dashboard_summary::DashboardSummaryAccumulator;
    use agent_observability_contracts::dashboard::{DashboardCostStatusV1, DashboardTokenStatusV1};
    let mut projected = project_report_span(0, &full("gpt-test"), None).unwrap();
    projected.estimated_cost = Some(1.0);
    projected.cost.status = "estimated".into();
    projected.cost.currency = Some("USD".into());
    let mut accumulator = DashboardSummaryAccumulator::default();
    accumulator.push(&projected).unwrap();
    projected.cost.currency = Some("EUR".into());
    projected.availability.tokens.state = AvailabilityStateV2::SourceUnavailable;
    accumulator.push(&projected).unwrap();
    let result = accumulator.finish(1, 2).unwrap();
    assert_eq!(result.token_status, DashboardTokenStatusV1::Incomplete);
    assert_eq!(result.total_tokens, None);
    assert_eq!(result.cost_status, DashboardCostStatusV1::Incomplete);
    assert_eq!(result.estimated_cost, None);
    assert_eq!(result.currency, None);
}

fn sorted_hashes(values: &[&str]) -> Vec<String> {
    let mut values = values
        .iter()
        .map(|value| hash_opaque_identifier(value))
        .collect::<Vec<_>>();
    values.sort();
    values
}

#[test]
fn report_projection_is_ordered_aggregated_and_content_free() {
    let mut later = full("gpt-test");
    later.trace_id = "trace-b".into();
    later.span_id = "later".into();
    later.start_time_unix_ms = 20.0;
    later.project.name = Some("repo-b".into());
    later.attributes.session_id = Some(agent_observability_contracts::ScalarValueV1::String(
        "session-2".into(),
    ));
    later.attributes.turn_id = Some(agent_observability_contracts::ScalarValueV1::String(
        "turn-2".into(),
    ));
    later.content.prompt = Some(agent_observability_contracts::JsonValue::String(
        "RAW_REPORT_SENTINEL".into(),
    ));

    let mut earlier = full("gpt-test");
    earlier.trace_id = "trace-a".into();
    earlier.span_id = "earlier".into();
    earlier.start_time_unix_ms = 10.0;
    earlier.project.name = Some("repo-a".into());
    earlier.attributes.session_id = Some(agent_observability_contracts::ScalarValueV1::String(
        "session-1".into(),
    ));
    earlier.attributes.turn_id = Some(agent_observability_contracts::ScalarValueV1::String(
        "turn-1".into(),
    ));
    earlier.attributes.request_id = Some(agent_observability_contracts::ScalarValueV1::String(
        "request".into(),
    ));

    let report = project_report(
        &[later, earlier],
        "2026-08-28T00:00:00.000Z",
        "Test report",
        Some(&table()),
    )
    .unwrap();

    assert_eq!(
        report
            .spans
            .iter()
            .map(|span| span.span_id.as_str())
            .collect::<Vec<_>>(),
        [
            hash_opaque_identifier("earlier"),
            hash_opaque_identifier("later")
        ]
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
    );
    assert_eq!(report.summary.generated_spans, 2);
    assert_eq!(report.summary.input_tokens, 2_000_000);
    assert_eq!(report.summary.output_tokens, 1_000_000);
    assert!((report.summary.estimated_cost - 11.74).abs() < f64::EPSILON);
    assert_eq!(report.filters.repos, ["repo-a", "repo-b"]);
    assert_eq!(
        report.filters.sessions,
        sorted_hashes(&["session-1", "session-2"])
    );
    assert_eq!(report.filters.turns, sorted_hashes(&["turn-1", "turn-2"]));
    assert_eq!(report.filters.agents, ["unknown"]);
    assert_eq!(report.filters.models, ["gpt-test"]);
    assert_eq!(
        report
            .traces
            .iter()
            .map(|trace| trace.trace_id.as_str())
            .collect::<Vec<_>>(),
        [
            hash_opaque_identifier("trace-a"),
            hash_opaque_identifier("trace-b")
        ]
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
    );
    assert_eq!(report.cost.estimated_cost, Some(11.74));

    let json = serde_json::to_value(&report).unwrap();
    let top_keys = json
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        top_keys,
        [
            "cost",
            "filters",
            "generatedAt",
            "schemaVersion",
            "spans",
            "summary",
            "title",
            "traces"
        ]
    );
    assert!(!json.to_string().contains("RAW_REPORT_SENTINEL"));
    assert!(json["spans"][0].get("content").is_none());
    report.validate().unwrap();
}

#[test]
fn report_explains_missing_fields_and_propagates_verified_trace_repository() {
    let mut session = full("gpt-test");
    session.trace_id = "trace-project".into();
    session.span_id = "session-project".into();
    session.span_kind = SpanKind::AgentSession;
    session.agent.model = None;
    session.project = ProjectV1::default();
    session.metrics = MetricsV1::default();

    let mut turn = full("gpt-test");
    turn.trace_id = "trace-project".into();
    turn.span_id = "turn-project".into();
    turn.span_kind = SpanKind::Turn;
    turn.agent.name = Some("codex".into());
    turn.project.name = Some("agent-observability".into());
    turn.attributes.source = Some(ScalarValueV1::String("codex".into()));
    turn.attributes.event_type = Some(ScalarValueV1::String("turn".into()));
    turn.attributes.turn_id = Some(ScalarValueV1::String("turn-private".into()));
    turn.metrics = MetricsV1::default();

    let mut request = full("gpt-test");
    request.trace_id = "trace-project".into();
    request.span_id = "request-project".into();
    request.project.name = Some("agent-observability".into());

    let report = project_report(
        &[session, turn, request],
        "2026-09-05T00:00:00Z",
        "availability",
        None,
    )
    .unwrap();
    let session = report
        .spans
        .iter()
        .find(|span| span.span_id == hash_opaque_identifier("session-project"))
        .unwrap();
    let turn = report
        .spans
        .iter()
        .find(|span| span.span_id == hash_opaque_identifier("turn-project"))
        .unwrap();
    assert_eq!(session.repo, "agent-observability");
    assert_eq!(
        session.availability.repository.reason,
        "derived_from_trace_context"
    );
    assert_eq!(
        session.availability.turn.state,
        agent_observability_contracts::AvailabilityStateV2::SourceUnavailable
    );
    assert_eq!(
        turn.availability.source_location.state,
        agent_observability_contracts::AvailabilityStateV2::PrivateLookup
    );
    assert_eq!(
        turn.availability.request_content.reason,
        "local_opt_in_lookup_required"
    );
}

#[test]
fn report_private_lookup_is_limited_to_current_codex_notify_turns() {
    let mut codex_request = full("gpt-test");
    codex_request.agent.name = Some("codex".into());
    codex_request.attributes.source = Some(ScalarValueV1::String("codex".into()));
    codex_request.attributes.event_type = Some(ScalarValueV1::String("model_request".into()));
    codex_request.attributes.turn_id = Some(ScalarValueV1::String("turn-codex".into()));

    let mut claude_turn = full("gpt-test");
    claude_turn.span_id = "claude-turn".into();
    claude_turn.span_kind = SpanKind::Turn;
    claude_turn.agent.name = Some("claude-code".into());
    claude_turn.attributes.source = Some(ScalarValueV1::String("claude-code".into()));
    claude_turn.attributes.event_type = Some(ScalarValueV1::String("turn".into()));
    claude_turn.attributes.turn_id = Some(ScalarValueV1::String("turn-claude".into()));

    let mut cursor_turn = claude_turn.clone();
    cursor_turn.span_id = "cursor-turn".into();
    cursor_turn.agent.name = Some("cursor".into());
    cursor_turn.attributes.source = Some(ScalarValueV1::String("cursor".into()));

    let mut historical_codex_turn = claude_turn.clone();
    historical_codex_turn.span_id = "historical-codex-turn".into();
    historical_codex_turn.agent.name = Some("codex".into());
    historical_codex_turn.attributes.source = Some(ScalarValueV1::String(
        "codex.notify_or_session_jsonl".into(),
    ));

    let report = project_report(
        &[
            codex_request,
            claude_turn,
            cursor_turn,
            historical_codex_turn,
        ],
        "2026-09-05T00:00:00Z",
        "private lookup eligibility",
        None,
    )
    .unwrap();
    let reason = |span_id: &str| {
        report
            .spans
            .iter()
            .find(|span| span.span_id == hash_opaque_identifier(span_id))
            .map(|span| {
                (
                    span.availability.source_location.state,
                    span.availability.source_location.reason.as_str(),
                )
            })
            .unwrap()
    };
    assert_eq!(
        reason("gpt-test"),
        (
            agent_observability_contracts::AvailabilityStateV2::NotApplicable,
            "codex_span_not_notify_derived"
        )
    );
    assert_eq!(
        reason("claude-turn"),
        (
            agent_observability_contracts::AvailabilityStateV2::NotApplicable,
            "claude_private_lookup_not_supported"
        )
    );
    assert_eq!(
        reason("cursor-turn"),
        (
            agent_observability_contracts::AvailabilityStateV2::NotApplicable,
            "cursor_private_lookup_not_supported"
        )
    );
    assert_eq!(
        reason("historical-codex-turn"),
        (
            agent_observability_contracts::AvailabilityStateV2::SourceUnavailable,
            "historical_codex_source_not_lookup_eligible"
        )
    );
}

#[test]
fn report_distinguishes_complete_and_partial_aggregate_token_metrics() {
    let mut record = span("gpt-test", MetricsV1::default());
    record.metrics.total_input_tokens = Some(10.0);
    let partial = project_report(&[record.clone()], "generated", "tokens", None).unwrap();

    assert_eq!(
        partial.spans[0].availability.tokens.state,
        agent_observability_contracts::AvailabilityStateV2::SourceUnavailable
    );
    assert_eq!(
        partial.spans[0].availability.tokens.reason,
        "partial_token_metrics"
    );
    partial.validate().unwrap();

    record.metrics.total_output_tokens = Some(2.0);
    let complete = project_report(&[record], "generated", "tokens", None).unwrap();
    assert_eq!(
        complete.spans[0].availability.tokens.state,
        agent_observability_contracts::AvailabilityStateV2::Available
    );
    complete.validate().unwrap();
}

#[test]
fn report_keeps_unknown_repository_when_trace_has_conflicting_projects() {
    let mut session = full("gpt-test");
    session.trace_id = "trace-project".into();
    session.span_id = "session-project".into();
    session.project = ProjectV1::default();

    let mut first = full("gpt-test");
    first.trace_id = "trace-project".into();
    first.span_id = "first-project".into();
    first.project.name = Some("agent-observability".into());

    let mut second = full("gpt-test");
    second.trace_id = "trace-project".into();
    second.span_id = "second-project".into();
    second.project.name = Some("different-project".into());

    let report = project_report(
        &[session, first, second],
        "2026-09-05T00:00:00Z",
        "availability",
        None,
    )
    .unwrap();
    let session = report
        .spans
        .iter()
        .find(|span| span.span_id == hash_opaque_identifier("session-project"))
        .unwrap();

    assert_eq!(session.repo, "unknown");
    assert_eq!(
        session.availability.repository.state,
        agent_observability_contracts::AvailabilityStateV2::SourceUnavailable
    );
    assert_eq!(
        session.availability.repository.reason,
        "ambiguous_trace_repository"
    );
    assert_eq!(report.traces.len(), 1);
    assert_eq!(report.traces[0].repo, "unknown");
}

#[test]
fn report_projection_rejects_any_invalid_input_record() {
    let mut invalid = full("gpt-test");
    invalid.metrics.input_tokens = Some(-1.0);
    let error = project_report(
        &[full("gpt-test"), invalid],
        "generated",
        "title",
        Some(&table()),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ReportProjectionError::InvalidRecord { index: 1, .. }
    ));
}

#[test]
fn owned_report_projection_matches_borrowed_projection() {
    let mut first = full("gpt-test");
    first.trace_id = "trace-a".into();
    first.span_id = "span-a".into();
    first.attributes.session_id = Some(ScalarValueV1::String("session-a".into()));
    first.content.prompt = Some(agent_observability_contracts::JsonValue::String(
        "OWNED_RAW_SENTINEL".into(),
    ));
    let mut second = full("gpt-incomplete");
    second.trace_id = "trace-b".into();
    second.span_id = "span-b".into();
    second.start_time_unix_ms = 1.0;
    let records = vec![first, second];

    let borrowed = project_report(&records, "generated", "title", Some(&table())).unwrap();
    let rates = table();
    let mut owned_projector = ReportProjector::new(0, Some(&rates));
    for (index, record) in records.into_iter().enumerate() {
        owned_projector.push_owned(index, record).unwrap();
    }
    let owned = owned_projector.finish("generated", "title").unwrap();

    assert_eq!(owned, borrowed);
    assert!(
        !serde_json::to_string(&owned)
            .unwrap()
            .contains("OWNED_RAW_SENTINEL")
    );
}

#[test]
fn individual_report_span_projection_matches_single_record_report_and_pricing() {
    let mut record = full("gpt-test");
    record.trace_id = "trace-single".into();
    record.span_id = "span-single".into();
    record.project.name = Some("known-repo".into());
    record.content.prompt = Some(agent_observability_contracts::JsonValue::String(
        "RAW_SINGLE_SPAN_SENTINEL".into(),
    ));
    let rates = table();

    let report = project_report(
        std::slice::from_ref(&record),
        "generated",
        "title",
        Some(&rates),
    )
    .unwrap();
    let borrowed = project_report_span(7, &record, Some(&rates)).unwrap();
    let owned = project_owned_report_span(7, record, Some(&rates)).unwrap();

    assert_eq!(borrowed, report.spans[0]);
    assert_eq!(owned, report.spans[0]);
    assert_eq!(borrowed.repo, "known-repo");
    assert_eq!(borrowed.estimated_cost, Some(5.87));
    assert!(
        !serde_json::to_string(&borrowed)
            .unwrap()
            .contains("RAW_SINGLE_SPAN_SENTINEL")
    );
}

#[test]
fn indexed_repository_resolution_matches_trace_context_rules() {
    let mut record = full("gpt-test");
    record.project = ProjectV1::default();
    let original = project_report_span(0, &record, None).unwrap();
    assert_eq!(original.repo, "unknown");
    for (candidates, expected_repo, expected_reason) in [
        (
            vec![],
            "unknown",
            original.availability.repository.reason.as_str(),
        ),
        (
            vec!["unknown"],
            "unknown",
            original.availability.repository.reason.as_str(),
        ),
        (vec!["repo-a"], "repo-a", "derived_from_trace_context"),
        (
            vec!["repo-a", "repo-a"],
            "repo-a",
            "derived_from_trace_context",
        ),
        (
            vec!["repo-a", "repo-b"],
            "unknown",
            "ambiguous_trace_repository",
        ),
    ] {
        let mut span = original.clone();
        resolve_report_repository(&mut span, candidates);
        assert_eq!(span.repo, expected_repo);
        assert_eq!(span.availability.repository.reason, expected_reason);
        span.validate().unwrap();
    }
    let mut known = original;
    known.repo = "authoritative-repo".into();
    let before = known.clone();
    resolve_report_repository(&mut known, ["other-repo"]);
    assert_eq!(known, before);
}

#[test]
fn individual_report_span_projection_rejects_malformed_records() {
    let mut record = full("gpt-test");
    record.metrics.input_tokens = Some(-1.0);

    assert!(matches!(
        project_report_span(23, &record, Some(&table())),
        Err(ReportProjectionError::InvalidRecord { index: 23, .. })
    ));
    assert!(matches!(
        project_owned_report_span(29, record, Some(&table())),
        Err(ReportProjectionError::InvalidRecord { index: 29, .. })
    ));
}

#[test]
fn individual_report_span_projection_rejects_invalid_projected_cost() {
    let mut rates = table();
    rates.models.get_mut("gpt-test").unwrap().input_tokens = Some(f64::MAX);

    assert!(matches!(
        project_report_span(31, &full("gpt-test"), Some(&rates)),
        Err(ReportProjectionError::InvalidReport(_))
    ));
    assert!(matches!(
        project_owned_report_span(37, full("gpt-test"), Some(&rates)),
        Err(ReportProjectionError::InvalidReport(_))
    ));
}

#[test]
fn report_projection_rejects_fractional_summary_metrics() {
    let mut record = span("gpt-test", MetricsV1::default());
    record.metrics.latency_ms = Some(1.5);
    let error = project_report(&[record], "generated", "title", Some(&table())).unwrap_err();
    assert!(matches!(
        error,
        ReportProjectionError::InvalidSummaryMetric {
            field: "latencyMs",
            ..
        }
    ));
}

#[test]
fn report_projection_rejects_fractional_inputs_even_when_the_sum_is_integral() {
    let mut first = span("gpt-test", MetricsV1::default());
    first.span_id = "first".into();
    first.metrics.input_tokens = Some(0.5);
    let mut second = span("gpt-test", MetricsV1::default());
    second.span_id = "second".into();
    second.metrics.input_tokens = Some(0.5);
    assert!(matches!(
        project_report(&[first, second], "generated", "title", Some(&table())),
        Err(ReportProjectionError::InvalidSummaryMetric {
            field: "inputTokens",
            ..
        })
    ));
}

#[test]
fn report_projection_redacts_hostile_metadata_and_title() {
    let mut record = full("gpt-test");
    record.attributes.tool_name = Some(ScalarValueV1::String("password=RAW_SECRET".into()));
    record.project.repo_path = Some("/workspace/.env".into());
    record.content.output = Some(agent_observability_contracts::JsonValue::String(
        "RAW_OUTPUT".into(),
    ));
    let report = project_report(
        &[record],
        "generated",
        "Authorization: Bearer RAW_TITLE",
        Some(&table()),
    )
    .unwrap();
    let json = serde_json::to_string(&report).unwrap();
    for sentinel in ["RAW_SECRET", "RAW_OUTPUT", "RAW_TITLE", "/workspace/.env"] {
        assert!(!json.contains(sentinel));
    }
    assert_eq!(report.title, "[redacted]");
}

#[test]
fn parity_estimates_span_and_normalizes() {
    let cost = estimate_span_cost(&full("gpt-test"), Some(&table()));
    assert_eq!(cost.status, "estimated");
    assert_eq!(cost.estimated_cost, Some(5.87));
    assert!((cost.cost.components["input_tokens"].tokens - 900_000.0).abs() < f64::EPSILON);
}
#[test]
fn parity_marks_missing_rates_incomplete() {
    let cost = estimate_span_cost(&full("gpt-incomplete"), Some(&table()));
    assert_eq!(cost.status, "incomplete");
    assert_eq!(cost.reason.as_deref(), Some("missing_token_rates"));
    assert_eq!(cost.estimated_cost, Some(0.9));
    assert_eq!(
        cost.cost.missing,
        [
            "output_tokens",
            "cached_input_tokens",
            "reasoning_output_tokens"
        ]
    );
}
#[test]
fn parity_marks_unknown_model_and_table() {
    assert_eq!(
        estimate_span_cost(&full("missing"), Some(&table()))
            .reason
            .as_deref(),
        Some("missing_model_rate")
    );
    assert_eq!(
        estimate_cost_for_records(&[full("gpt-test")], None)
            .reason
            .as_deref(),
        Some("missing_rate_table")
    );
}
#[test]
fn parity_aggregates_states() {
    let cost = estimate_cost_for_records(
        &[full("gpt-test"), full("gpt-incomplete"), full("missing")],
        Some(&table()),
    );
    assert_eq!(cost.status, "incomplete");
    assert_eq!(cost.estimated_cost, Some(6.77));
    assert_eq!(cost.cost.incomplete_count, Some(1));
    assert_eq!(cost.cost.unknown_count, Some(1));
}

#[test]
fn browser_view_cost_status_matches_the_versioned_contract() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/report-view-reduction-v1.fixture.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["schemaVersion"],
        "agent_observability.report_view_reduction.v1"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let costs: Vec<_> = case["statuses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|status| CostEstimateV1 {
                status: status.as_str().unwrap().into(),
                ..CostEstimateV1::default()
            })
            .collect();
        assert_eq!(
            aggregate_costs(&costs).status,
            case["expectedStatus"].as_str().unwrap(),
            "{}",
            case["name"].as_str().unwrap()
        );
    }
}
#[test]
fn parity_rejects_unsupported_units_and_rates() {
    let mut input = RateTableInput {
        models: BTreeMap::new(),
        ..RateTableInput {
            version: None,
            currency: None,
            unit: Some("per_token".into()),
            assumption: None,
            models: BTreeMap::new(),
        }
    };
    assert_eq!(
        normalize_rate_table(input.clone()).unwrap_err(),
        PricingError::UnsupportedUnit
    );
    input.unit = None;
    input.models.insert(
        "x".into(),
        ModelRatesInput {
            input_tokens: Some(f64::NAN),
            ..ModelRatesInput::default()
        },
    );
    assert!(matches!(
        normalize_rate_table(input),
        Err(PricingError::InvalidRate(_))
    ));
}

#[test]
fn rate_table_document_is_versioned_closed_and_normalized() {
    let rates = parse_rate_table_json(
        r#"{
          "schema_version":"agent_observability.rate_table.v1",
          "version":"stable-test",
          "currency":"USD",
          "unit":"per_1m_tokens",
          "assumption":"Fixture rates only.",
          "models":{"gpt-test":{"input_tokens":2,"output_tokens":8}}
        }"#,
    )
    .unwrap();
    assert_eq!(rates.version, "stable-test");
    assert_eq!(rates.models["gpt-test"].input_tokens, Some(2.0));

    assert_eq!(
        parse_rate_table_json(
            r#"{"schema_version":"agent_observability.rate_table.v2","models":{}}"#
        )
        .unwrap_err(),
        PricingError::UnsupportedVersion
    );
    assert_eq!(
        parse_rate_table_json(
            r#"{"schema_version":"agent_observability.rate_table.v1","models":{},"extra":true}"#
        )
        .unwrap_err(),
        PricingError::InvalidDocument
    );
}

#[test]
fn absent_overlap_semantics_are_incomplete() {
    let mut rates = table();
    rates
        .models
        .get_mut("gpt-test")
        .unwrap()
        .token_semantics
        .clear();
    let cost = estimate_span_cost(&full("gpt-test"), Some(&rates));
    assert_eq!(cost.status, "incomplete");
    assert_eq!(cost.reason.as_deref(), Some("ambiguous_token_semantics"));
    assert_eq!(cost.estimated_cost, Some(6.0));
    assert_eq!(
        cost.cost.semantic_errors,
        [
            "cached_input_tokens:missing_semantics",
            "reasoning_output_tokens:missing_semantics"
        ]
    );
}

#[test]
fn included_cache_creation_is_priced_after_subtracting_from_input() {
    let record = span(
        "cache",
        MetricsV1 {
            input_tokens: Some(1_000.0),
            cache_creation_input_tokens: Some(200.0),
            ..MetricsV1::default()
        },
    );
    let rates = normalize_rate_table(RateTableInput {
        models: BTreeMap::from([(
            String::from("cache"),
            ModelRatesInput {
                input_tokens: Some(2.0),
                cache_creation_input_tokens: Some(4.0),
                token_semantics: BTreeMap::from([(
                    String::from("cache_creation_input_tokens"),
                    String::from("included_in_total"),
                )]),
                ..ModelRatesInput::default()
            },
        )]),
        ..RateTableInput {
            version: None,
            currency: None,
            unit: None,
            assumption: None,
            models: BTreeMap::new(),
        }
    })
    .unwrap();
    let cost = estimate_span_cost(&record, Some(&rates));
    assert_eq!(cost.status, "estimated");
    assert_eq!(cost.estimated_cost, Some(0.0024));
    assert!((cost.cost.components["input_tokens"].tokens - 800.0).abs() < f64::EPSILON);
}

#[test]
fn cumulative_totals_are_never_billed() {
    let cost = estimate_span_cost(
        &span(
            "m",
            MetricsV1 {
                input_tokens: Some(100.0),
                output_tokens: Some(50.0),
                total_input_tokens: Some(10_000.0),
                total_output_tokens: Some(5_000.0),
                total_accumulated_tokens: Some(15_000.0),
                ..MetricsV1::default()
            },
        ),
        Some(
            &normalize_rate_table(RateTableInput {
                models: BTreeMap::from([(
                    String::from("m"),
                    ModelRatesInput {
                        input_tokens: Some(2.0),
                        output_tokens: Some(8.0),
                        ..ModelRatesInput::default()
                    },
                )]),
                ..RateTableInput {
                    version: None,
                    currency: None,
                    unit: None,
                    assumption: None,
                    models: BTreeMap::new(),
                }
            })
            .unwrap(),
        ),
    );
    assert_eq!(cost.estimated_cost, Some(0.0006));
    assert!(!cost.cost.components.contains_key("total_input_tokens"));
}

#[test]
fn rust_report_matches_the_frozen_cross_agent_golden_contract() {
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/fixtures/golden/cross-agent-contract.json"
    ))
    .unwrap();
    let mut records = Vec::new();
    for source in ["codex", "claude_code"] {
        for value in expected["durable_full"][source].as_array().unwrap() {
            records.push(serde_json::from_value::<DurableRecordV1>(value.clone()).unwrap());
        }
    }
    let report = project_report(
        &records,
        "2026-08-01T01:00:00.000Z",
        "Agent Observability Report",
        Some(&golden_rate_table()),
    )
    .unwrap();
    let mut actual = serde_json::to_value(report).unwrap();
    let mut expected_report = expected["report_full"].clone();
    for span in expected_report["spans"].as_array_mut().unwrap() {
        span["availability"] = expected_cross_agent_availability(span);
    }
    normalize_integral_json_numbers(&mut actual);
    normalize_integral_json_numbers(&mut expected_report);
    assert_eq!(actual, expected_report);
}

fn expected_cross_agent_availability(span: &serde_json::Value) -> serde_json::Value {
    let available = |reason: &str| serde_json::json!({"state": "available", "reason": reason});
    let unavailable =
        |reason: &str| serde_json::json!({"state": "source_unavailable", "reason": reason});
    let not_applicable =
        |reason: &str| serde_json::json!({"state": "not_applicable", "reason": reason});
    let kind = span["kind"].as_str().unwrap();
    let has_tokens = span["metrics"].as_object().unwrap().keys().any(|key| {
        matches!(
            key.as_str(),
            "inputTokens"
                | "outputTokens"
                | "cachedInputTokens"
                | "cacheCreationInputTokens"
                | "reasoningOutputTokens"
                | "totalTokens"
                | "totalInputTokens"
                | "totalOutputTokens"
                | "totalCachedInputTokens"
                | "totalReasoningOutputTokens"
                | "totalAccumulatedTokens"
                | "contextWindowTokens"
        )
    });
    let private_detail = match span["attributes"]["source"].as_str() {
        Some("codex.notify_or_session_jsonl") => {
            unavailable("historical_codex_source_not_lookup_eligible")
        }
        Some("claude_code.hook" | "claude_code.transcript") => {
            not_applicable("claude_private_lookup_not_supported")
        }
        _ => not_applicable("agent_private_lookup_not_supported"),
    };
    serde_json::json!({
        "repository": available("reported_by_adapter"),
        "turn": if span.get("turnId").is_some() {
            available("reported_by_adapter")
        } else {
            unavailable("source_not_provided")
        },
        "model": if span["agent"].get("model").is_some() {
            available("reported_by_adapter")
        } else if matches!(kind, "llm.request" | "agent.session") {
            unavailable("source_not_provided")
        } else {
            not_applicable("span_kind_not_model_backed")
        },
        "tokens": if has_tokens {
            available("reported_by_adapter")
        } else if kind == "llm.request" {
            unavailable("source_not_provided")
        } else {
            not_applicable("span_kind_has_no_token_usage")
        },
        "latency": if span["metrics"].get("latencyMs").is_some()
            || span["metrics"].get("durationMs").is_some()
        {
            available("reported_by_adapter")
        } else if matches!(kind, "llm.request" | "tool.execution") {
            unavailable("source_not_provided")
        } else {
            not_applicable("span_kind_has_no_latency")
        },
        "sourceLocation": private_detail,
        "requestContent": private_detail,
        "responseContent": private_detail
    })
}

fn normalize_integral_json_numbers(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                normalize_integral_json_numbers(value);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                normalize_integral_json_numbers(value);
            }
        }
        serde_json::Value::Number(number) => {
            if let Some(value) = number.as_f64()
                && value >= 0.0
                && value.fract() == 0.0
                && value <= 9_007_199_254_740_991.0
            {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let integer = value as u64;
                *number = integer.into();
            }
        }
        _ => {}
    }
}

fn golden_rate_table() -> RateTable {
    normalize_rate_table(RateTableInput {
        version: Some("golden-rates".into()),
        currency: Some("USD".into()),
        unit: Some("per_1m_tokens".into()),
        assumption: Some("Golden fixture rates.".into()),
        models: BTreeMap::from([
            (
                "gpt-golden".into(),
                ModelRatesInput {
                    input_tokens: Some(2.0),
                    output_tokens: Some(8.0),
                    cached_input_tokens: Some(1.0),
                    reasoning_output_tokens: Some(20.0),
                    token_semantics: BTreeMap::from([
                        ("cached_input_tokens".into(), "included_in_total".into()),
                        ("reasoning_output_tokens".into(), "included_in_total".into()),
                    ]),
                    ..ModelRatesInput::default()
                },
            ),
            (
                "claude-golden".into(),
                ModelRatesInput {
                    input_tokens: Some(3.0),
                    output_tokens: Some(9.0),
                    cached_input_tokens: Some(1.0),
                    cache_creation_input_tokens: Some(4.0),
                    token_semantics: BTreeMap::from([
                        ("cached_input_tokens".into(), "included_in_total".into()),
                        (
                            "cache_creation_input_tokens".into(),
                            "included_in_total".into(),
                        ),
                    ]),
                    ..ModelRatesInput::default()
                },
            ),
        ]),
    })
    .unwrap()
}
