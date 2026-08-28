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
    normalize_integral_json_numbers(&mut actual);
    normalize_integral_json_numbers(&mut expected_report);
    assert_eq!(actual, expected_report);
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
