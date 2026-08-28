use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

pub const MAX_IDENTIFIER_BYTES: usize = 512;

macro_rules! identifier_type {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            /// Parses a bounded, non-empty opaque identifier.
            ///
            /// # Errors
            ///
            /// Returns [`DomainError`] when the value is empty or too long.
            pub fn parse(value: impl Into<String>) -> Result<Self, DomainError> {
                Ok(Self(validate_identifier(value.into())?))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

identifier_type!(TraceId);
identifier_type!(SpanId);
identifier_type!(SessionId);
identifier_type!(TurnId);
identifier_type!(RequestId);
identifier_type!(OperationId);
identifier_type!(PermissionId);
identifier_type!(CompactionId);
identifier_type!(SourceCursor);
identifier_type!(SourceGeneration);
identifier_type!(ObservationId);

fn validate_identifier(value: String) -> Result<String, DomainError> {
    if value.is_empty() {
        return Err(DomainError::EmptyIdentifier);
    }
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(DomainError::IdentifierTooLong {
            actual: value.len(),
            maximum: MAX_IDENTIFIER_BYTES,
        });
    }
    Ok(value)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpanKind {
    Workstream,
    AgentSession,
    Turn,
    LlmRequest,
    ToolExecution,
    Permission,
    Compaction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusCode {
    Unset,
    Ok,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleState {
    Observed,
    Running,
    Completed,
    Failed,
    Interrupted,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TokenUsage {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub cached_input: Option<u64>,
    pub cache_creation_input: Option<u64>,
    pub reasoning_output: Option<u64>,
    pub total: Option<u64>,
    pub total_input: Option<u64>,
    pub total_output: Option<u64>,
    pub total_cached_input: Option<u64>,
    pub total_reasoning_output: Option<u64>,
    pub total_accumulated: Option<u64>,
    pub context_window: Option<u64>,
    pub input_before: Option<u64>,
    pub input_after: Option<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CorrelationIds {
    pub session_id: Option<SessionId>,
    pub turn_id: Option<TurnId>,
    pub request_id: Option<RequestId>,
    pub operation_id: Option<OperationId>,
    pub permission_id: Option<PermissionId>,
    pub compaction_id: Option<CompactionId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timing {
    pub start_unix_ms: u64,
    pub end_unix_ms: Option<u64>,
}

impl Timing {
    /// Creates a valid time range.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::EndBeforeStart`] when the end precedes the start.
    pub fn new(start_unix_ms: u64, end_unix_ms: Option<u64>) -> Result<Self, DomainError> {
        if end_unix_ms.is_some_and(|end| end < start_unix_ms) {
            return Err(DomainError::EndBeforeStart);
        }
        Ok(Self {
            start_unix_ms,
            end_unix_ms,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainSpanState {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub kind: SpanKind,
    pub lifecycle: LifecycleState,
    pub correlation: CorrelationIds,
    pub timing: Timing,
    pub token_usage: TokenUsage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DomainError {
    EmptyIdentifier,
    IdentifierTooLong {
        actual: usize,
        maximum: usize,
    },
    EndBeforeStart,
    LifecycleConflict {
        span_id: SpanId,
    },
    FieldConflict {
        span_id: SpanId,
        field: &'static str,
    },
    SelfParent {
        span_id: SpanId,
    },
    CrossTraceParent {
        span_id: SpanId,
        parent_span_id: SpanId,
    },
    InvalidParentKind {
        span_id: SpanId,
        parent_span_id: SpanId,
    },
    Cycle {
        span_id: SpanId,
    },
}

impl Display for DomainError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyIdentifier => formatter.write_str("identifier must not be empty"),
            Self::IdentifierTooLong { actual, maximum } => {
                write!(
                    formatter,
                    "identifier length {actual} exceeds {maximum} bytes"
                )
            }
            Self::EndBeforeStart => formatter.write_str("end time must not precede start time"),
            Self::LifecycleConflict { span_id } => {
                write!(
                    formatter,
                    "conflicting terminal lifecycle for span {}",
                    span_id.as_str()
                )
            }
            Self::FieldConflict { span_id, field } => {
                write!(
                    formatter,
                    "conflicting {field} for span {}",
                    span_id.as_str()
                )
            }
            Self::SelfParent { span_id } => {
                write!(formatter, "span {} cannot parent itself", span_id.as_str())
            }
            Self::CrossTraceParent {
                span_id,
                parent_span_id,
            } => write!(
                formatter,
                "span {} references parent {} in another trace",
                span_id.as_str(),
                parent_span_id.as_str()
            ),
            Self::InvalidParentKind {
                span_id,
                parent_span_id,
            } => write!(
                formatter,
                "span {} has invalid parent {} kind relation",
                span_id.as_str(),
                parent_span_id.as_str()
            ),
            Self::Cycle { span_id } => {
                write!(
                    formatter,
                    "topology cycle includes span {}",
                    span_id.as_str()
                )
            }
        }
    }
}

impl Error for DomainError {}

/// Reduces lifecycle observations without depending on arrival order.
#[derive(Clone, Debug, Default)]
pub struct LifecycleReducer {
    states: BTreeMap<SpanId, DomainSpanState>,
}

impl LifecycleReducer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one observation, merging only fields for which a merge is defined.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] when immutable fields conflict or the lifecycle cannot converge.
    pub fn observe(&mut self, observation: DomainSpanState) -> Result<(), DomainError> {
        if observation
            .timing
            .end_unix_ms
            .is_some_and(|end| end < observation.timing.start_unix_ms)
        {
            return Err(DomainError::EndBeforeStart);
        }
        if let Some(existing) = self.states.get_mut(&observation.span_id) {
            merge_state(existing, &observation)
        } else {
            self.states.insert(observation.span_id.clone(), observation);
            Ok(())
        }
    }

    /// Returns spans in stable span-id order.
    #[must_use]
    pub fn finish(self) -> Vec<DomainSpanState> {
        self.states.into_values().collect()
    }

    /// Reduces a batch independently of its input order.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] when any observation conflicts with the reduced span state.
    pub fn reduce<I>(observations: I) -> Result<Vec<DomainSpanState>, DomainError>
    where
        I: IntoIterator<Item = DomainSpanState>,
    {
        let mut reducer = Self::new();
        for observation in observations {
            reducer.observe(observation)?;
        }
        Ok(reducer.finish())
    }
}

fn merge_state(
    existing: &mut DomainSpanState,
    incoming: &DomainSpanState,
) -> Result<(), DomainError> {
    if existing.trace_id != incoming.trace_id {
        return Err(DomainError::FieldConflict {
            span_id: existing.span_id.clone(),
            field: "trace_id",
        });
    }
    if existing.kind != incoming.kind {
        return Err(DomainError::FieldConflict {
            span_id: existing.span_id.clone(),
            field: "kind",
        });
    }
    if existing.parent_span_id != incoming.parent_span_id {
        return Err(DomainError::FieldConflict {
            span_id: existing.span_id.clone(),
            field: "parent_span_id",
        });
    }
    merge_correlation(
        &mut existing.correlation,
        &incoming.correlation,
        &existing.span_id,
    )?;
    existing.lifecycle =
        merge_lifecycle(existing.lifecycle, incoming.lifecycle, &existing.span_id)?;
    existing.timing.start_unix_ms = existing
        .timing
        .start_unix_ms
        .min(incoming.timing.start_unix_ms);
    existing.timing.end_unix_ms = match (existing.timing.end_unix_ms, incoming.timing.end_unix_ms) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (left, right) => left.or(right),
    };
    merge_metrics(
        &mut existing.token_usage,
        incoming.token_usage,
        &existing.span_id,
    )
}

fn merge_lifecycle(
    left: LifecycleState,
    right: LifecycleState,
    span_id: &SpanId,
) -> Result<LifecycleState, DomainError> {
    use LifecycleState::{Observed, Running};
    match (left, right) {
        (a, b) if a == b => Ok(a),
        (Observed, Running) | (Running, Observed) => Ok(Running),
        (Observed | Running, terminal) | (terminal, Observed | Running) => Ok(terminal),
        _ => Err(DomainError::LifecycleConflict {
            span_id: span_id.clone(),
        }),
    }
}

fn merge_optional<T: Clone + Eq>(
    left: &mut Option<T>,
    right: Option<&T>,
    span_id: &SpanId,
    field: &'static str,
) -> Result<(), DomainError> {
    match (left.as_ref(), right) {
        (Some(a), Some(b)) if a != b => Err(DomainError::FieldConflict {
            span_id: span_id.clone(),
            field,
        }),
        (None, Some(value)) => {
            *left = Some(T::clone(value));
            Ok(())
        }
        _ => Ok(()),
    }
}

fn merge_correlation(
    left: &mut CorrelationIds,
    right: &CorrelationIds,
    span_id: &SpanId,
) -> Result<(), DomainError> {
    merge_optional(
        &mut left.session_id,
        right.session_id.as_ref(),
        span_id,
        "correlation.session_id",
    )?;
    merge_optional(
        &mut left.turn_id,
        right.turn_id.as_ref(),
        span_id,
        "correlation.turn_id",
    )?;
    merge_optional(
        &mut left.request_id,
        right.request_id.as_ref(),
        span_id,
        "correlation.request_id",
    )?;
    merge_optional(
        &mut left.operation_id,
        right.operation_id.as_ref(),
        span_id,
        "correlation.operation_id",
    )?;
    merge_optional(
        &mut left.permission_id,
        right.permission_id.as_ref(),
        span_id,
        "correlation.permission_id",
    )?;
    merge_optional(
        &mut left.compaction_id,
        right.compaction_id.as_ref(),
        span_id,
        "correlation.compaction_id",
    )
}

fn merge_metrics(
    left: &mut TokenUsage,
    right: TokenUsage,
    span_id: &SpanId,
) -> Result<(), DomainError> {
    macro_rules! field {
        ($name:ident) => {
            merge_optional(
                &mut left.$name,
                right.$name.as_ref(),
                span_id,
                concat!("token_usage.", stringify!($name)),
            )?;
        };
    }
    field!(input);
    field!(output);
    field!(cached_input);
    field!(cache_creation_input);
    field!(reasoning_output);
    field!(total);
    field!(total_input);
    field!(total_output);
    field!(total_cached_input);
    field!(total_reasoning_output);
    field!(total_accumulated);
    field!(context_window);
    field!(input_before);
    field!(input_after);
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TopologyValidation {
    pub unresolved_parent_ids: Vec<SpanId>,
}

/// Validates parent links and reports absent parents as an explicit, non-error result.
///
/// # Errors
///
/// Returns [`DomainError`] for self-parenting, cycles, cross-trace links, or invalid kind links.
pub fn validate_topology(states: &[DomainSpanState]) -> Result<TopologyValidation, DomainError> {
    let by_id: BTreeMap<SpanId, &DomainSpanState> = states
        .iter()
        .map(|state| (state.span_id.clone(), state))
        .collect();
    let mut unresolved = BTreeSet::new();
    for state in states {
        let Some(parent_id) = &state.parent_span_id else {
            continue;
        };
        if parent_id == &state.span_id {
            return Err(DomainError::SelfParent {
                span_id: state.span_id.clone(),
            });
        }
        let Some(parent) = by_id.get(parent_id) else {
            unresolved.insert(parent_id.clone());
            continue;
        };
        if parent.trace_id != state.trace_id {
            return Err(DomainError::CrossTraceParent {
                span_id: state.span_id.clone(),
                parent_span_id: parent_id.clone(),
            });
        }
        detect_cycle(state, &by_id)?;
        if !valid_parent_kind(state.kind, parent.kind) {
            return Err(DomainError::InvalidParentKind {
                span_id: state.span_id.clone(),
                parent_span_id: parent_id.clone(),
            });
        }
    }
    Ok(TopologyValidation {
        unresolved_parent_ids: unresolved.into_iter().collect(),
    })
}

fn detect_cycle(
    state: &DomainSpanState,
    by_id: &BTreeMap<SpanId, &DomainSpanState>,
) -> Result<(), DomainError> {
    let mut seen = BTreeSet::new();
    let mut current = Some(state.span_id.clone());
    while let Some(id) = current {
        if !seen.insert(id.clone()) {
            return Err(DomainError::Cycle { span_id: id });
        }
        current = by_id
            .get(&id)
            .and_then(|value| value.parent_span_id.clone());
    }
    Ok(())
}

fn valid_parent_kind(child: SpanKind, parent: SpanKind) -> bool {
    matches!(
        (child, parent),
        (SpanKind::AgentSession, SpanKind::Workstream)
            | (SpanKind::Turn, SpanKind::AgentSession)
            | (SpanKind::LlmRequest, SpanKind::Turn)
            | (
                SpanKind::ToolExecution | SpanKind::Permission,
                SpanKind::Turn | SpanKind::LlmRequest
            )
            | (
                SpanKind::Compaction,
                SpanKind::AgentSession | SpanKind::Turn
            )
    )
}

#[cfg(test)]
mod tests {
    use super::{
        CorrelationIds, DomainError, DomainSpanState, LifecycleReducer, LifecycleState, SpanId,
        SpanKind, Timing, TokenUsage, TraceId, validate_topology,
    };

    fn state(span: &str, kind: SpanKind, lifecycle: LifecycleState) -> DomainSpanState {
        DomainSpanState {
            trace_id: TraceId::parse("trace").unwrap(),
            span_id: SpanId::parse(span).unwrap(),
            parent_span_id: None,
            kind,
            lifecycle,
            correlation: CorrelationIds::default(),
            timing: Timing::new(10, None).unwrap(),
            token_usage: TokenUsage::default(),
        }
    }

    #[test]
    fn typed_identifier_rejects_empty_values() {
        assert!(SpanId::parse("").is_err());
    }

    #[test]
    fn timing_rejects_end_before_start() {
        assert!(Timing::new(2, Some(1)).is_err());
    }

    #[test]
    fn reducer_is_permutation_invariant_and_fills_optional_fields() {
        let mut first = state("span", SpanKind::Turn, LifecycleState::Observed);
        first.timing = Timing::new(20, Some(30)).unwrap();
        first.correlation.turn_id = Some(super::TurnId::parse("turn").unwrap());
        first.token_usage.input = Some(4);
        first.token_usage.input_before = Some(12);
        let mut second = first.clone();
        second.lifecycle = LifecycleState::Running;
        second.timing = Timing::new(10, None).unwrap();
        second.correlation.session_id = Some(super::SessionId::parse("session").unwrap());
        second.token_usage.output = Some(8);
        second.token_usage.input_after = Some(6);

        let left = LifecycleReducer::reduce([first.clone(), second.clone()]).unwrap();
        let right = LifecycleReducer::reduce([second, first]).unwrap();
        assert_eq!(left, right);
        assert_eq!(left[0].lifecycle, LifecycleState::Running);
        assert_eq!(left[0].timing, Timing::new(10, Some(30)).unwrap());
        assert_eq!(left[0].token_usage.input, Some(4));
        assert_eq!(left[0].token_usage.output, Some(8));
        assert_eq!(left[0].token_usage.input_before, Some(12));
        assert_eq!(left[0].token_usage.input_after, Some(6));
    }

    #[test]
    fn terminal_states_dominate_active_states_but_conflicting_terminals_fail() {
        let observed = state("span", SpanKind::Turn, LifecycleState::Observed);
        let completed = state("span", SpanKind::Turn, LifecycleState::Completed);
        assert_eq!(
            LifecycleReducer::reduce([completed.clone(), observed]).unwrap()[0].lifecycle,
            LifecycleState::Completed
        );
        let failed = state("span", SpanKind::Turn, LifecycleState::Failed);
        assert!(matches!(
            LifecycleReducer::reduce([completed, failed]),
            Err(DomainError::LifecycleConflict { .. })
        ));
    }

    #[test]
    fn every_terminal_state_dominates_active_states_and_conflicts_with_other_terminals() {
        let active = [LifecycleState::Observed, LifecycleState::Running];
        let terminal = [
            LifecycleState::Completed,
            LifecycleState::Failed,
            LifecycleState::Interrupted,
        ];
        for terminal_state in terminal {
            for active_state in active {
                for order in [true, false] {
                    let left = state("span", SpanKind::Turn, terminal_state);
                    let right = state("span", SpanKind::Turn, active_state);
                    let values = if order { [left, right] } else { [right, left] };
                    assert_eq!(
                        LifecycleReducer::reduce(values).unwrap()[0].lifecycle,
                        terminal_state
                    );
                }
            }
        }
        for (left, right) in [
            (LifecycleState::Completed, LifecycleState::Failed),
            (LifecycleState::Completed, LifecycleState::Interrupted),
            (LifecycleState::Failed, LifecycleState::Interrupted),
        ] {
            assert!(matches!(
                LifecycleReducer::reduce([
                    state("span", SpanKind::Turn, left),
                    state("span", SpanKind::Turn, right),
                ]),
                Err(DomainError::LifecycleConflict { .. })
            ));
        }
    }

    #[test]
    fn differing_identity_correlation_and_metrics_fail() {
        let base = state("span", SpanKind::Turn, LifecycleState::Observed);
        let mut different_kind = base.clone();
        different_kind.kind = SpanKind::LlmRequest;
        assert!(matches!(
            LifecycleReducer::reduce([base.clone(), different_kind]),
            Err(DomainError::FieldConflict { field: "kind", .. })
        ));
        let mut different_metric = base.clone();
        different_metric.token_usage.input = Some(1);
        let mut conflicting_metric = base;
        conflicting_metric.token_usage.input = Some(2);
        assert!(matches!(
            LifecycleReducer::reduce([different_metric, conflicting_metric]),
            Err(DomainError::FieldConflict { .. })
        ));

        for field in ["input_before", "input_after"] {
            let mut left = state("span", SpanKind::Compaction, LifecycleState::Completed);
            let mut right = left.clone();
            let expected = if field == "input_before" {
                left.token_usage.input_before = Some(100);
                right.token_usage.input_before = Some(90);
                "token_usage.input_before"
            } else {
                left.token_usage.input_after = Some(60);
                right.token_usage.input_after = Some(50);
                "token_usage.input_after"
            };
            assert!(matches!(
                LifecycleReducer::reduce([left, right]),
                Err(DomainError::FieldConflict { field: conflict, .. }) if conflict == expected
            ));
        }
    }

    #[test]
    fn topology_reports_unresolved_parent_and_rejects_invalid_links() {
        let mut child = state("child", SpanKind::Turn, LifecycleState::Completed);
        child.parent_span_id = Some(SpanId::parse("missing").unwrap());
        let result = validate_topology(&[child]).unwrap();
        assert_eq!(
            result.unresolved_parent_ids,
            vec![SpanId::parse("missing").unwrap()]
        );

        let mut self_parent = state("self", SpanKind::Turn, LifecycleState::Completed);
        self_parent.parent_span_id = Some(self_parent.span_id.clone());
        assert!(matches!(
            validate_topology(&[self_parent]),
            Err(DomainError::SelfParent { .. })
        ));

        let root = state("root", SpanKind::Workstream, LifecycleState::Completed);
        let mut child = state("child", SpanKind::Turn, LifecycleState::Completed);
        child.parent_span_id = Some(root.span_id.clone());
        assert!(matches!(
            validate_topology(&[root, child]),
            Err(DomainError::InvalidParentKind { .. })
        ));
    }

    #[test]
    fn topology_rejects_cross_trace_parent_and_cycle() {
        let mut parent = state("parent", SpanKind::AgentSession, LifecycleState::Completed);
        parent.parent_span_id = Some(SpanId::parse("workstream").unwrap());
        let mut child = state("child", SpanKind::Turn, LifecycleState::Completed);
        child.parent_span_id = Some(parent.span_id.clone());
        child.trace_id = TraceId::parse("other-trace").unwrap();
        assert!(matches!(
            validate_topology(&[parent.clone(), child]),
            Err(DomainError::CrossTraceParent { .. })
        ));

        let mut a = state("a", SpanKind::AgentSession, LifecycleState::Completed);
        let mut b = state("b", SpanKind::Workstream, LifecycleState::Completed);
        a.parent_span_id = Some(b.span_id.clone());
        b.parent_span_id = Some(a.span_id.clone());
        assert!(matches!(
            validate_topology(&[a, b]),
            Err(DomainError::Cycle { .. })
        ));
    }

    #[test]
    fn topology_accepts_every_declared_parent_kind_relation() {
        for (child_kind, parent_kind) in [
            (SpanKind::AgentSession, SpanKind::Workstream),
            (SpanKind::Turn, SpanKind::AgentSession),
            (SpanKind::LlmRequest, SpanKind::Turn),
            (SpanKind::ToolExecution, SpanKind::Turn),
            (SpanKind::ToolExecution, SpanKind::LlmRequest),
            (SpanKind::Permission, SpanKind::Turn),
            (SpanKind::Permission, SpanKind::LlmRequest),
            (SpanKind::Compaction, SpanKind::AgentSession),
            (SpanKind::Compaction, SpanKind::Turn),
        ] {
            let parent = state("parent", parent_kind, LifecycleState::Completed);
            let mut child = state("child", child_kind, LifecycleState::Completed);
            child.parent_span_id = Some(parent.span_id.clone());
            validate_topology(&[child, parent]).unwrap();
        }
    }
}
