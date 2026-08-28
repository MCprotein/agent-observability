use agent_observability_contracts::{
    AgentV1, AttributesV1, ContentV1, CostEstimateV1, DURABLE_RECORD_FIELDS, DURABLE_RECORD_SCHEMA,
    DURABLE_RECORD_VERSION, DurableRecordV1, MetricsV1, ProjectV1, REDACTION_RECORD_FIELDS,
    REPORT_DTO_FIELDS, REPORT_DTO_SCHEMA, REPORT_DTO_VERSION, RedactionV1, ReportAgentV1,
    ReportAttributesV1, ReportDtoV1, ReportFiltersV1, ReportMetricsV1, ReportSpanV1,
    ReportSummaryV1, ScalarValueV1, StatusV1, TraceSummaryV1,
};
use agent_observability_domain::{SpanKind, StatusCode};

const CODEX_SOURCE: &[u8] = include_bytes!("../../../test/fixtures/golden/codex-source.jsonl");
const CLAUDE_SOURCE: &[u8] =
    include_bytes!("../../../test/fixtures/golden/claude-code-source.jsonl");
const CROSS_AGENT_CONTRACT: &[u8] =
    include_bytes!("../../../test/fixtures/golden/cross-agent-contract.json");

#[test]
fn javascript_migration_baseline_is_byte_locked() {
    assert_eq!(fnv1a64(CODEX_SOURCE), 0xfae4_e9d6_12ec_5bea);
    assert_eq!(fnv1a64(CLAUDE_SOURCE), 0xcd0f_27e2_36f7_ab05);
    assert_eq!(fnv1a64(CROSS_AGENT_CONTRACT), 0xd05f_eb07_2ce3_18bf);
}

#[test]
fn typed_contracts_preserve_complete_boundary_versions() {
    let session = DurableRecordV1 {
        schema_version: DURABLE_RECORD_VERSION,
        record_type: "span",
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
        schema_version: REPORT_DTO_VERSION,
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
