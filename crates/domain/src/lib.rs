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
    IdentifierTooLong { actual: usize, maximum: usize },
    EndBeforeStart,
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
        }
    }
}

impl Error for DomainError {}

#[cfg(test)]
mod tests {
    use super::{SpanId, Timing};

    #[test]
    fn typed_identifier_rejects_empty_values() {
        assert!(SpanId::parse("").is_err());
    }

    #[test]
    fn timing_rejects_end_before_start() {
        assert!(Timing::new(2, Some(1)).is_err());
    }
}
