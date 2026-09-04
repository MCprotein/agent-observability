use agent_observability_contracts::{
    AgentSource, AgentV1, AttributesV1, ContentV1, ContractError, CostEstimateV1,
    DURABLE_RECORD_FIELDS, DURABLE_RECORD_SCHEMA, DURABLE_RECORD_VERSION, DurableRecordV1,
    MetricsV1, ObservationEvent, ProjectV1, REDACTION_RECORD_FIELDS, REPORT_DTO_FIELDS,
    REPORT_DTO_SCHEMA, REPORT_DTO_VERSION, RedactionV1, ReportAgentV1, ReportAttributesV1,
    ReportAvailabilityV1, ReportDtoV1, ReportFiltersV1, ReportMetricsV1, ReportSpanV1,
    ReportSummaryV1, ScalarValueV1, SourceObservation, StatusV1, TraceSummaryV1,
    project_durable_record, sanitize_durable_record,
};
use agent_observability_domain::{
    CorrelationIds, DomainSpanState, LifecycleState, ObservationId, SessionId, SourceCursor,
    SourceGeneration, SpanId, SpanKind, StatusCode, Timing, TokenUsage, TraceId, TurnId,
};

const CODEX_SOURCE: &[u8] = include_bytes!("../../../test/fixtures/golden/codex-source.jsonl");
const CLAUDE_SOURCE: &[u8] =
    include_bytes!("../../../test/fixtures/golden/claude-code-source.jsonl");
const CROSS_AGENT_CONTRACT: &[u8] =
    include_bytes!("../../../test/fixtures/golden/cross-agent-contract.json");

#[test]
fn javascript_migration_baseline_is_byte_locked() {
    assert_eq!(fnv1a64(CODEX_SOURCE), 0xfae4_e9d6_12ec_5bea);
    assert_eq!(fnv1a64(CLAUDE_SOURCE), 0xcd0f_27e2_36f7_ab05);
    assert_eq!(fnv1a64(CROSS_AGENT_CONTRACT), 0xea40_6190_5e72_c7af);
}

#[test]
fn typed_contracts_preserve_complete_boundary_versions() {
    let session = DurableRecordV1 {
        schema_version: DURABLE_RECORD_VERSION.into(),
        record_type: "span".into(),
        trace_id: "trace".into(),
        span_id: "session".into(),
        parent_span_id: None,
        span_kind: SpanKind::AgentSession,
        name: "session".into(),
        start_time_unix_ms: 1.0,
        end_time_unix_ms: Some(2.0),
        status: StatusV1 {
            code: StatusCode::Ok,
        },
        agent: AgentV1::default(),
        project: ProjectV1::default(),
        attributes: AttributesV1::default(),
        metrics: MetricsV1::default(),
        content: ContentV1::default(),
        redaction: RedactionV1::default(),
    };
    let report = ReportDtoV1 {
        schema_version: REPORT_DTO_VERSION.into(),
        generated_at: "2026-08-28T00:00:00.000Z".into(),
        title: "Golden".into(),
        summary: ReportSummaryV1 {
            sessions: 1,
            ..ReportSummaryV1::default()
        },
        cost: CostEstimateV1 {
            status: "unknown".into(),
            ..CostEstimateV1::default()
        },
        filters: ReportFiltersV1::default(),
        traces: Vec::new(),
        spans: Vec::new(),
    };

    session.validate().expect("durable contract is valid");
    report.validate().expect("report contract is valid");
    assert_eq!(DURABLE_RECORD_FIELDS.len(), 16);
    assert_eq!(REPORT_DTO_FIELDS.len(), 8);
    assert_eq!(REDACTION_RECORD_FIELDS, ["applied", "count", "fields"]);
    assert_schema_fields(DURABLE_RECORD_SCHEMA, DURABLE_RECORD_FIELDS);
    assert_schema_fields(REPORT_DTO_SCHEMA, REPORT_DTO_FIELDS);

    let mut empty_optional = session.clone();
    empty_optional.agent.name = Some(String::new());
    assert!(empty_optional.validate().is_err());
    let mut wide_negative_identity = session.clone();
    wide_negative_identity.trace_id = "x".repeat(513);
    wide_negative_identity.start_time_unix_ms = -2.0;
    wide_negative_identity.end_time_unix_ms = Some(-1.0);
    wide_negative_identity
        .validate()
        .expect("wire contract accepts unbounded identifiers and negative finite timestamps");
    let mut negative_metric = session.clone();
    negative_metric.metrics.input_tokens = Some(-1.0);
    assert!(negative_metric.validate().is_err());
    let mut non_finite_scalar = session;
    non_finite_scalar.attributes.exit_code = Some(ScalarValueV1::Number(f64::NAN));
    assert!(non_finite_scalar.validate().is_err());
    let mut non_finite_trace = report.clone();
    non_finite_trace.traces.push(TraceSummaryV1 {
        start_time_unix_ms: f64::NAN,
        ..TraceSummaryV1::default()
    });
    assert!(non_finite_trace.validate().is_err());
    let mut non_finite_span = report.clone();
    non_finite_span.spans.push(ReportSpanV1 {
        schema_version: DURABLE_RECORD_VERSION.into(),
        trace_id: "trace".into(),
        span_id: "span".into(),
        parent_span_id: None,
        kind: SpanKind::AgentSession,
        name: "span".into(),
        status: StatusCode::Ok,
        start_time_unix_ms: 1.0,
        end_time_unix_ms: Some(f64::INFINITY),
        repo: String::new(),
        agent: ReportAgentV1::default(),
        availability: ReportAvailabilityV1::default(),
        session_id: None,
        turn_id: None,
        tool_name: None,
        attributes: ReportAttributesV1::default(),
        metrics: ReportMetricsV1::default(),
        estimated_cost: None,
        cost: CostEstimateV1 {
            status: "unknown".into(),
            ..CostEstimateV1::default()
        },
    });
    assert!(non_finite_span.validate().is_err());
    let mut non_finite_report = report;
    non_finite_report.summary.estimated_cost = f64::INFINITY;
    assert!(non_finite_report.validate().is_err());
}

fn assert_schema_fields(schema: &str, fields: &[&str]) {
    for field in fields {
        assert!(
            schema.contains(&format!("\"{field}\"")),
            "schema omits {field}"
        );
    }
}

#[test]
fn expected_contract_does_not_contain_raw_source_sentinels() {
    let source =
        String::from_utf8_lossy(CODEX_SOURCE).to_string() + &String::from_utf8_lossy(CLAUDE_SOURCE);
    let expected = String::from_utf8_lossy(CROSS_AGENT_CONTRACT);
    assert!(source.contains("RAW_GOLDEN_"));
    assert!(!expected.contains("RAW_GOLDEN_"));
}

#[test]
fn rust_source_projector_matches_all_frozen_durable_golden_records() {
    let expected: serde_json::Value = serde_json::from_slice(CROSS_AGENT_CONTRACT).unwrap();
    for (source_key, source, trace_id) in [
        ("codex", AgentSource::Codex, "codex:golden-codex"),
        (
            "claude_code",
            AgentSource::ClaudeCode,
            "claude-code:golden-claude",
        ),
    ] {
        for (index, expected_record) in expected["durable"][source_key]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let kind = expected_record["span_kind"].as_str().unwrap();
            let correlation = CorrelationIds {
                session_id: expected_record["session_id"]
                    .as_str()
                    .map(SessionId::parse)
                    .transpose()
                    .unwrap(),
                turn_id: expected_record["turn_id"]
                    .as_str()
                    .map(TurnId::parse)
                    .transpose()
                    .unwrap(),
                request_id: expected_record["request_id"]
                    .as_str()
                    .map(agent_observability_domain::RequestId::parse)
                    .transpose()
                    .unwrap(),
                ..CorrelationIds::default()
            };
            let metrics = &expected_record["metrics"];
            let usage = TokenUsage {
                input: metrics["input_tokens"].as_u64(),
                output: metrics["output_tokens"].as_u64(),
                cached_input: metrics["cached_input_tokens"].as_u64(),
                cache_creation_input: metrics["cache_creation_input_tokens"].as_u64(),
                reasoning_output: metrics["reasoning_output_tokens"].as_u64(),
                ..TokenUsage::default()
            };
            let observation = SourceObservation {
                source,
                source_generation: SourceGeneration::parse(format!("golden-{source_key}")).unwrap(),
                previous_source_cursor: None,
                source_cursor: SourceCursor::parse(index.to_string()).unwrap(),
                observation_id: ObservationId::parse(format!("{source_key}-{index}")).unwrap(),
                trace_id: TraceId::parse(trace_id).unwrap(),
                span_id: SpanId::parse(expected_record["span_id"].as_str().unwrap()).unwrap(),
                parent_span_id: expected_record["parent_span_id"]
                    .as_str()
                    .map(SpanId::parse)
                    .transpose()
                    .unwrap(),
                correlation: correlation.clone(),
                event: match kind {
                    "agent.session" => ObservationEvent::Session {
                        model: expected_record["model"].as_str().map(str::to_owned),
                        project: None,
                    },
                    "turn" => ObservationEvent::Turn,
                    "llm.request" => ObservationEvent::ModelRequest {
                        model: expected_record["model"].as_str().map(str::to_owned),
                    },
                    _ => unreachable!("golden fixture contains supported kinds"),
                },
                lifecycle: LifecycleState::Observed,
                timing: Timing::new(
                    expected_record["start_time_unix_ms"].as_u64().unwrap(),
                    None,
                )
                .unwrap(),
                token_usage: usage,
            };
            let state = DomainSpanState {
                trace_id: observation.trace_id.clone(),
                span_id: observation.span_id.clone(),
                parent_span_id: observation.parent_span_id.clone(),
                kind: match kind {
                    "agent.session" => SpanKind::AgentSession,
                    "turn" => SpanKind::Turn,
                    "llm.request" => SpanKind::LlmRequest,
                    _ => unreachable!("golden fixture contains supported kinds"),
                },
                lifecycle: observation.lifecycle,
                correlation,
                timing: observation.timing,
                token_usage: usage,
            };
            let record = project_durable_record(&observation, &state).unwrap();
            assert_eq!(compact_durable(&record), *expected_record);
        }
    }
}

#[test]
fn rust_serialization_round_trips_full_javascript_durable_golden_records() {
    let expected: serde_json::Value = serde_json::from_slice(CROSS_AGENT_CONTRACT).unwrap();
    for source in ["codex", "claude_code"] {
        for expected_record in expected["durable_full"][source].as_array().unwrap() {
            let record: DurableRecordV1 = serde_json::from_value(expected_record.clone()).unwrap();
            let mut actual = serde_json::to_value(record).unwrap();
            let mut expected_record = expected_record.clone();
            normalize_integral_json_numbers(&mut actual);
            normalize_integral_json_numbers(&mut expected_record);
            assert_eq!(actual, expected_record);
        }
    }
}

#[test]
fn source_projector_serializes_every_supported_event_variant() {
    use agent_observability_domain::{CompactionId, OperationId, PermissionId};

    let cases = [
        (
            ObservationEvent::Session {
                model: Some("model".into()),
                project: Some("project".into()),
            },
            SpanKind::AgentSession,
            "agent.session",
            "session",
        ),
        (ObservationEvent::Turn, SpanKind::Turn, "turn", "turn"),
        (
            ObservationEvent::ModelRequest {
                model: Some("model".into()),
            },
            SpanKind::LlmRequest,
            "llm.request",
            "model_request",
        ),
        (
            ObservationEvent::ToolOperation {
                tool_name: Some("read".into()),
                phase: Some("finish".into()),
            },
            SpanKind::ToolExecution,
            "tool.execution",
            "tool_operation",
        ),
        (
            ObservationEvent::Permission {
                decision: Some("denied".into()),
            },
            SpanKind::Permission,
            "permission",
            "permission",
        ),
        (
            ObservationEvent::Compaction {
                trigger: Some("manual".into()),
            },
            SpanKind::Compaction,
            "compaction",
            "compaction",
        ),
    ];
    for (index, (event, kind, kind_name, event_type)) in cases.into_iter().enumerate() {
        let correlation = CorrelationIds {
            session_id: Some(SessionId::parse("session").unwrap()),
            turn_id: Some(TurnId::parse("turn").unwrap()),
            operation_id: Some(OperationId::parse("operation").unwrap()),
            permission_id: Some(PermissionId::parse("permission").unwrap()),
            compaction_id: Some(CompactionId::parse("compaction").unwrap()),
            ..CorrelationIds::default()
        };
        let observation = SourceObservation {
            source: AgentSource::Codex,
            source_generation: SourceGeneration::parse("generation").unwrap(),
            previous_source_cursor: None,
            source_cursor: SourceCursor::parse(index.to_string()).unwrap(),
            observation_id: ObservationId::parse(format!("observation-{index}")).unwrap(),
            trace_id: TraceId::parse("trace").unwrap(),
            span_id: SpanId::parse(format!("span-{index}")).unwrap(),
            parent_span_id: None,
            correlation: correlation.clone(),
            event,
            lifecycle: LifecycleState::Completed,
            timing: Timing::new(1, Some(2)).unwrap(),
            token_usage: TokenUsage::default(),
        };
        let state = DomainSpanState {
            trace_id: observation.trace_id.clone(),
            span_id: observation.span_id.clone(),
            parent_span_id: None,
            kind,
            lifecycle: observation.lifecycle,
            correlation,
            timing: observation.timing,
            token_usage: observation.token_usage,
        };
        let actual =
            serde_json::to_value(project_durable_record(&observation, &state).unwrap()).unwrap();
        assert_eq!(actual["span_kind"], kind_name);
        assert_eq!(actual["attributes"]["event_type"], event_type);
        assert_eq!(actual["content"], serde_json::json!({}));
        assert_eq!(
            actual["redaction"]["fields"],
            serde_json::json!(["prompt", "output", "tool_input", "tool_output"])
        );
    }
}

fn compact_durable(record: &DurableRecordV1) -> serde_json::Value {
    let span_kind = match record.span_kind {
        SpanKind::Workstream => "workstream",
        SpanKind::AgentSession => "agent.session",
        SpanKind::Turn => "turn",
        SpanKind::LlmRequest => "llm.request",
        SpanKind::ToolExecution => "tool.execution",
        SpanKind::Permission => "permission",
        SpanKind::Compaction => "compaction",
    };
    let mut value = serde_json::json!({
        "span_kind": span_kind,
        "span_id": record.span_id.clone(),
        "parent_span_id": record.parent_span_id.clone(),
        "agent_name": record.agent.name.clone(),
        "model": record.agent.model.clone(),
        "session_id": record.attributes.session_id.clone(),
        "turn_id": record.attributes.turn_id.clone(),
        "request_id": record.attributes.request_id.clone(),
        "start_time_unix_ms": record.start_time_unix_ms,
        "metrics": record.metrics.clone(),
    });
    normalize_integral_json_numbers(&mut value);
    value
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
                let exact = value as u64;
                *number = serde_json::Number::from(exact);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::String(_) => {}
    }
}

#[test]
fn durable_json_is_schema_shaped_and_round_trips() {
    let record = DurableRecordV1 {
        schema_version: DURABLE_RECORD_VERSION.into(),
        record_type: "span".into(),
        trace_id: "trace".into(),
        span_id: "span".into(),
        parent_span_id: None,
        span_kind: SpanKind::LlmRequest,
        name: "llm.request".into(),
        start_time_unix_ms: 10.0,
        end_time_unix_ms: None,
        status: StatusV1 {
            code: StatusCode::Unset,
        },
        agent: AgentV1 {
            name: Some("codex".into()),
            ..AgentV1::default()
        },
        project: ProjectV1::default(),
        attributes: AttributesV1 {
            source: Some(ScalarValueV1::String("codex".into())),
            ..AttributesV1::default()
        },
        metrics: MetricsV1 {
            input_tokens: Some(3.0),
            ..MetricsV1::default()
        },
        content: ContentV1::default(),
        redaction: RedactionV1::default(),
    };
    let value = serde_json::to_value(&record).expect("durable serializes");
    assert_eq!(value["span_kind"], "llm.request");
    assert_eq!(value["parent_span_id"], serde_json::Value::Null);
    assert_eq!(value["end_time_unix_ms"], serde_json::Value::Null);
    assert_eq!(value["metrics"], serde_json::json!({"input_tokens": 3.0}));
    assert_eq!(value["content"], serde_json::json!({}));
    assert!(value["agent"].get("version").is_none());
    let decoded: DurableRecordV1 = serde_json::from_value(value).expect("durable deserializes");
    assert_eq!(decoded, record);
}

#[test]
fn report_json_uses_camel_case_and_omits_optional_nested_fields() {
    let report = ReportDtoV1 {
        schema_version: REPORT_DTO_VERSION.into(),
        generated_at: "2026-08-28T00:00:00.000Z".into(),
        title: "Golden".into(),
        summary: ReportSummaryV1 {
            generated_spans: 1,
            ..ReportSummaryV1::default()
        },
        cost: CostEstimateV1 {
            status: "unknown".into(),
            ..CostEstimateV1::default()
        },
        filters: ReportFiltersV1::default(),
        traces: Vec::new(),
        spans: Vec::new(),
    };
    let value = serde_json::to_value(&report).expect("report serializes");
    assert_eq!(value["schemaVersion"], REPORT_DTO_VERSION);
    assert_eq!(value["generatedAt"], "2026-08-28T00:00:00.000Z");
    assert_eq!(value["summary"]["generatedSpans"], 1);
    assert!(value["summary"].get("generated_spans").is_none());
    assert!(value["cost"].get("reason").is_none());
    let decoded: ReportDtoV1 = serde_json::from_value(value).expect("report deserializes");
    assert_eq!(decoded, report);
    let mut legacy = serde_json::to_value(&report).expect("legacy report serializes");
    let filters = legacy["filters"]
        .as_object_mut()
        .expect("filters are an object");
    filters.remove("agents");
    filters.remove("models");
    let decoded: ReportDtoV1 =
        serde_json::from_value(legacy).expect("pre-v0.12 report deserializes");
    assert_eq!(decoded, report);
}

#[test]
fn projector_is_fail_closed_and_never_exports_content() {
    let trace_id = TraceId::parse("trace").unwrap();
    let span_id = SpanId::parse("span").unwrap();
    let correlation = CorrelationIds::default();
    let timing = Timing::new(10, Some(20)).unwrap();
    let usage = TokenUsage {
        input: Some(4),
        output: Some(2),
        ..TokenUsage::default()
    };
    let observation = SourceObservation {
        source: AgentSource::Codex,
        source_generation: SourceGeneration::parse("generation").unwrap(),
        previous_source_cursor: None,
        source_cursor: SourceCursor::parse("cursor").unwrap(),
        observation_id: ObservationId::parse("observation").unwrap(),
        trace_id: trace_id.clone(),
        span_id: span_id.clone(),
        parent_span_id: None,
        correlation: correlation.clone(),
        event: ObservationEvent::ModelRequest {
            model: Some("gpt".into()),
        },
        lifecycle: LifecycleState::Completed,
        timing,
        token_usage: usage,
    };
    let state = DomainSpanState {
        trace_id,
        span_id,
        parent_span_id: None,
        kind: SpanKind::LlmRequest,
        lifecycle: LifecycleState::Completed,
        correlation,
        timing,
        token_usage: usage,
    };
    let record = project_durable_record(&observation, &state).expect("matching state projects");
    assert!(record.content.prompt.is_none());
    assert!(record.content.output.is_none());
    assert_eq!(record.agent.model, Some("gpt".into()));
    assert_eq!(record.redaction.count, 4);
    assert_eq!(record.trace_id, "trace");
    assert!(
        serde_json::to_string(&record)
            .unwrap()
            .contains("\"content\":{}")
    );

    let mut hostile = record.clone();
    hostile.agent.model = Some("Authorization: Bearer RAW_SECRET".into());
    hostile.project.repo_path = Some("/workspace/.env".into());
    hostile.attributes.tool_name = Some(ScalarValueV1::String("password=RAW_SECRET".into()));
    hostile.content.prompt = Some(agent_observability_contracts::JsonValue::String(
        "RAW_PROMPT".into(),
    ));
    let sanitized = sanitize_durable_record(&hostile).unwrap();
    let sanitized_json = serde_json::to_string(&sanitized).unwrap();
    assert!(!sanitized_json.contains("RAW_SECRET"));
    assert!(!sanitized_json.contains("RAW_PROMPT"));
    assert!(!sanitized_json.contains("/workspace/.env"));
    assert!(sanitized.trace_id.starts_with("id:sha256:"));
    assert_eq!(sanitized.agent.model.as_deref(), Some("[redacted]"));

    let mut oversized = state.clone();
    oversized.token_usage.input = Some(9_007_199_254_740_992);
    assert!(matches!(
        project_durable_record(&observation, &oversized),
        Err(ContractError::IntegerPrecisionLoss)
    ));

    let mut mismatched = state;
    mismatched.kind = SpanKind::Turn;
    assert!(project_durable_record(&observation, &mismatched).is_err());
}

const fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        index += 1;
    }
    hash
}
