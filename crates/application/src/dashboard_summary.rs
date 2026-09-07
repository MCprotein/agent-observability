//! Constant-space summary contributions for an immutable dashboard scan.
//!
//! Distinct session and turn counts are supplied only after separate ordered identity scans.
//! This accumulator never retains spans or identity sets and never exposes partial totals.

use agent_observability_contracts::dashboard::{
    DASHBOARD_ESTIMATED_COST_MAX, DASHBOARD_SAFE_INTEGER_MAX, DashboardCostStatusV1,
    DashboardKpisV1, DashboardTokenStatusV1,
};
use agent_observability_contracts::{AvailabilityStateV2, ReportMetricsV1, ReportSpanV2};
use agent_observability_domain::{SpanKind, StatusCode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SummaryCapacityExceeded;

/// Scalar state only. Clone-before-update makes rejected contributions atomic.
#[derive(Clone, Debug, Default)]
// Independent evidence flags can coexist; they are not mutually exclusive lifecycle states.
#[allow(clippy::struct_excessive_bools)]
pub struct DashboardSummaryAccumulator {
    llm: u64,
    tools: u64,
    errors: u64,
    input: u64,
    output: u64,
    incomplete_input: bool,
    incomplete_output: bool,
    tokens: u64,
    complete_tokens: bool,
    incomplete_tokens: bool,
    estimated: bool,
    incomplete_cost: bool,
    unknown_cost: bool,
    cost: f64,
    currency: Option<String>,
    mixed_currency: bool,
}

/// Matches the static browser's direct, total, cumulative token precedence.
#[must_use]
pub fn dashboard_token_total(metrics: &ReportMetricsV1) -> Option<f64> {
    metrics
        .input_tokens
        .zip(metrics.output_tokens)
        .map(|(a, b)| a + b)
        .or(metrics.total_tokens)
        .or_else(|| {
            metrics
                .total_input_tokens
                .zip(metrics.total_output_tokens)
                .map(|(a, b)| a + b)
        })
        .or(metrics.total_accumulated_tokens)
}

impl DashboardSummaryAccumulator {
    /// Adds one privacy-projected span without retaining it.
    ///
    /// # Errors
    /// Rejects non-integral/unsafe token counts, overflow, or invalid currency/cost values.
    pub fn push(&mut self, span: &ReportSpanV2) -> Result<(), SummaryCapacityExceeded> {
        let mut next = self.clone();
        next.push_checked(span)?;
        *self = next;
        Ok(())
    }

    fn push_checked(&mut self, span: &ReportSpanV2) -> Result<(), SummaryCapacityExceeded> {
        if span.kind == SpanKind::LlmRequest {
            increment(&mut self.llm, 1)?;
        }
        if span.kind == SpanKind::ToolExecution {
            increment(&mut self.tools, 1)?;
        }
        if span.status == StatusCode::Error {
            increment(&mut self.errors, 1)?;
        }
        if span.availability.tokens.state != AvailabilityStateV2::NotApplicable {
            accumulate_component(
                &mut self.input,
                &mut self.incomplete_input,
                span.metrics.input_tokens,
            )?;
            accumulate_component(
                &mut self.output,
                &mut self.incomplete_output,
                span.metrics.output_tokens,
            )?;
        }
        let total = dashboard_token_total(&span.metrics);
        if let Some(total) =
            total.filter(|_| span.availability.tokens.state == AvailabilityStateV2::Available)
        {
            increment(&mut self.tokens, token_integer(total)?)?;
            self.complete_tokens = true;
        } else if span.availability.tokens.state != AvailabilityStateV2::NotApplicable {
            self.incomplete_tokens = true;
        }
        let billable = total.is_some()
            || span.metrics.cached_input_tokens.is_some()
            || span.metrics.cache_creation_input_tokens.is_some()
            || span.metrics.reasoning_output_tokens.is_some();
        if billable {
            match span.cost.status.as_str() {
                "estimated" => self.estimated = true,
                "incomplete" => self.incomplete_cost = true,
                _ => self.unknown_cost = true,
            }
        }
        if let Some(cost) = span.estimated_cost {
            if !cost.is_finite() || cost < 0.0 {
                return Err(SummaryCapacityExceeded);
            }
            self.cost += cost;
            if !self.cost.is_finite() || self.cost > DASHBOARD_ESTIMATED_COST_MAX {
                return Err(SummaryCapacityExceeded);
            }
            if let Some(currency) = &span.cost.currency {
                if currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_uppercase()) {
                    return Err(SummaryCapacityExceeded);
                }
                match &self.currency {
                    Some(previous) if previous != currency => self.mixed_currency = true,
                    None => self.currency = Some(currency.clone()),
                    _ => {}
                }
            } else {
                self.mixed_currency = true;
            }
        }
        Ok(())
    }

    /// Call only after the entire filtered scan and both distinct-identity passes finish.
    ///
    /// # Errors
    /// Rejects distinct counts outside the browser's exact integer range.
    pub fn finish(
        self,
        sessions: u64,
        turns: u64,
    ) -> Result<DashboardKpisV1, SummaryCapacityExceeded> {
        if sessions > DASHBOARD_SAFE_INTEGER_MAX || turns > DASHBOARD_SAFE_INTEGER_MAX {
            return Err(SummaryCapacityExceeded);
        }
        let token_status = if self.incomplete_tokens {
            DashboardTokenStatusV1::Incomplete
        } else if self.complete_tokens {
            DashboardTokenStatusV1::Complete
        } else {
            DashboardTokenStatusV1::Unavailable
        };
        let cost_status = if !self.estimated && !self.incomplete_cost {
            DashboardCostStatusV1::Unknown
        } else if self.incomplete_cost || self.unknown_cost || self.mixed_currency {
            DashboardCostStatusV1::Incomplete
        } else {
            DashboardCostStatusV1::Estimated
        };
        let known_cost = cost_status != DashboardCostStatusV1::Unknown && !self.mixed_currency;
        Ok(DashboardKpisV1 {
            sessions,
            turns,
            llm: self.llm,
            tools: self.tools,
            errors: self.errors,
            input_tokens: (token_status != DashboardTokenStatusV1::Unavailable
                && !self.incomplete_input)
                .then_some(self.input),
            output_tokens: (token_status != DashboardTokenStatusV1::Unavailable
                && !self.incomplete_output)
                .then_some(self.output),
            total_tokens: (token_status == DashboardTokenStatusV1::Complete).then_some(self.tokens),
            token_status,
            estimated_cost: known_cost.then_some(self.cost),
            cost_status,
            currency: if known_cost { self.currency } else { None },
        })
    }
}

fn accumulate_component(
    sum: &mut u64,
    incomplete: &mut bool,
    value: Option<f64>,
) -> Result<(), SummaryCapacityExceeded> {
    if let Some(value) = value {
        increment(sum, token_integer(value)?)?;
    } else {
        *incomplete = true;
    }
    Ok(())
}

fn increment(value: &mut u64, amount: u64) -> Result<(), SummaryCapacityExceeded> {
    *value = value
        .checked_add(amount)
        .filter(|sum| *sum <= DASHBOARD_SAFE_INTEGER_MAX)
        .ok_or(SummaryCapacityExceeded)?;
    Ok(())
}

fn token_integer(value: f64) -> Result<u64, SummaryCapacityExceeded> {
    // The bound is exactly representable and checked before the narrowing conversion.
    if !value.is_finite()
        || !(0.0..=9_007_199_254_740_991.0).contains(&value)
        || value.fract() != 0.0
    {
        return Err(SummaryCapacityExceeded);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(value as u64)
}
