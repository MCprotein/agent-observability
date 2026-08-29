const PRESSURE_WINDOW_MS: u64 = 10_000;
const SUSTAINED_PRESSURE_MS: u64 = 60_000;
const INITIAL_PROBE_BACKOFF_MS: u64 = 5_000;
const MAX_PROBE_BACKOFF_MS: u64 = 60_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Normal,
    Pressured,
    Protected,
    Probe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PressureSample {
    pub resource_percent: u8,
    pub disk_percent: u8,
    pub queue_percent: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schedule {
    pub state: State,
    pub reconcile_interval_ms: u32,
    pub flush_paused: bool,
    pub report_refresh_allowed: bool,
    pub fairness_slot: u64,
    pub next_probe_at_ms: u64,
}

#[derive(Debug)]
pub struct Scheduler {
    state: State,
    pressure_windows: u8,
    recovery_windows: u8,
    pressured_since: Option<u64>,
    last_window_at_ms: Option<u64>,
    next_probe_at_ms: u64,
    probe_backoff_ms: u64,
    fairness_slot: u64,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            state: State::Normal,
            pressure_windows: 0,
            recovery_windows: 0,
            pressured_since: None,
            last_window_at_ms: None,
            next_probe_at_ms: 0,
            probe_backoff_ms: INITIAL_PROBE_BACKOFF_MS,
            fairness_slot: 0,
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn evaluate(
        &mut self,
        now_ms: u64,
        sample: PressureSample,
        base_reconcile_ms: u32,
    ) -> Schedule {
        let over_budget = sample.resource_percent > 100;
        let critical = sample.disk_percent >= 90 || sample.queue_percent >= 90;
        let recovering =
            sample.resource_percent < 70 && sample.disk_percent < 70 && sample.queue_percent < 70;

        match self.state {
            State::Normal => {
                if critical {
                    self.enter_protected(now_ms);
                } else if self.window_due(now_ms) {
                    self.pressure_windows = if over_budget {
                        self.pressure_windows.saturating_add(1)
                    } else {
                        0
                    };
                    if self.pressure_windows >= 2 {
                        self.state = State::Pressured;
                        self.pressured_since = Some(now_ms);
                        self.recovery_windows = 0;
                    }
                }
            }
            State::Pressured => {
                if critical
                    || (over_budget
                        && self
                            .pressured_since
                            .is_some_and(|at| now_ms.saturating_sub(at) >= SUSTAINED_PRESSURE_MS))
                {
                    self.enter_protected(now_ms);
                } else if self.window_due(now_ms) {
                    self.recovery_windows = if recovering {
                        self.recovery_windows.saturating_add(1)
                    } else {
                        0
                    };
                    if self.recovery_windows >= 3 {
                        self.state = State::Normal;
                        self.reset_pressure_tracking();
                    }
                }
            }
            State::Protected => {
                if now_ms >= self.next_probe_at_ms && recovering {
                    self.state = State::Probe;
                    self.recovery_windows = 0;
                    self.last_window_at_ms = None;
                    self.next_probe_at_ms = now_ms.saturating_add(INITIAL_PROBE_BACKOFF_MS);
                }
            }
            State::Probe if now_ms >= self.next_probe_at_ms => {
                if recovering {
                    if self.window_due(now_ms) {
                        self.recovery_windows = self.recovery_windows.saturating_add(1);
                    }
                    self.probe_backoff_ms = INITIAL_PROBE_BACKOFF_MS;
                    self.next_probe_at_ms = now_ms.saturating_add(INITIAL_PROBE_BACKOFF_MS);
                    if self.recovery_windows >= 3 {
                        self.state = State::Pressured;
                        self.pressured_since = Some(now_ms);
                        self.recovery_windows = 0;
                        self.last_window_at_ms = None;
                    }
                } else {
                    self.state = State::Protected;
                    self.recovery_windows = 0;
                    self.last_window_at_ms = None;
                    self.probe_backoff_ms = self
                        .probe_backoff_ms
                        .saturating_mul(2)
                        .min(MAX_PROBE_BACKOFF_MS);
                    self.next_probe_at_ms = now_ms.saturating_add(self.probe_backoff_ms);
                }
            }
            State::Probe => {}
        }

        let multiplier = if self.state == State::Pressured { 2 } else { 1 };
        let schedule = Schedule {
            state: self.state,
            reconcile_interval_ms: base_reconcile_ms.saturating_mul(multiplier).min(60_000),
            flush_paused: self.state == State::Protected,
            report_refresh_allowed: matches!(self.state, State::Normal | State::Probe),
            fairness_slot: self.fairness_slot,
            next_probe_at_ms: self.next_probe_at_ms,
        };
        self.fairness_slot = self.fairness_slot.wrapping_add(1);
        schedule
    }

    fn window_due(&mut self, now_ms: u64) -> bool {
        if self
            .last_window_at_ms
            .is_none_or(|last| now_ms.saturating_sub(last) >= PRESSURE_WINDOW_MS)
        {
            self.last_window_at_ms = Some(now_ms);
            true
        } else {
            false
        }
    }

    fn enter_protected(&mut self, now_ms: u64) {
        self.state = State::Protected;
        self.recovery_windows = 0;
        self.last_window_at_ms = None;
        self.probe_backoff_ms = INITIAL_PROBE_BACKOFF_MS;
        self.next_probe_at_ms = now_ms.saturating_add(INITIAL_PROBE_BACKOFF_MS);
    }

    fn reset_pressure_tracking(&mut self) {
        self.pressure_windows = 0;
        self.recovery_windows = 0;
        self.pressured_since = None;
        self.last_window_at_ms = None;
        self.next_probe_at_ms = 0;
        self.probe_backoff_ms = INITIAL_PROBE_BACKOFF_MS;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(resource: u8, disk: u8, queue: u8) -> PressureSample {
        PressureSample {
            resource_percent: resource,
            disk_percent: disk,
            queue_percent: queue,
        }
    }

    #[test]
    fn two_over_budget_windows_pressure_then_three_low_windows_recover() {
        let mut scheduler = Scheduler::new();
        assert_eq!(
            scheduler.evaluate(0, sample(101, 10, 10), 5_000).state,
            State::Normal
        );
        assert_eq!(
            scheduler.evaluate(10_000, sample(101, 10, 10), 5_000).state,
            State::Pressured
        );
        assert_eq!(
            scheduler.evaluate(20_000, sample(1, 1, 1), 5_000).state,
            State::Pressured
        );
        assert_eq!(
            scheduler.evaluate(30_000, sample(1, 1, 1), 5_000).state,
            State::Pressured
        );
        assert_eq!(
            scheduler.evaluate(40_000, sample(1, 1, 1), 5_000).state,
            State::Normal
        );
    }

    #[test]
    fn critical_pressure_protects_immediately_and_recovers_one_state_at_a_time() {
        let mut scheduler = Scheduler::new();
        let protected = scheduler.evaluate(0, sample(1, 90, 1), 5_000);
        assert_eq!(protected.state, State::Protected);
        assert!(protected.flush_paused);
        assert!(!protected.report_refresh_allowed);

        assert_eq!(
            scheduler.evaluate(5_000, sample(1, 1, 1), 5_000).state,
            State::Probe
        );
        assert_eq!(
            scheduler.evaluate(10_000, sample(1, 1, 1), 5_000).state,
            State::Probe
        );
        assert_eq!(
            scheduler.evaluate(20_000, sample(1, 1, 1), 5_000).state,
            State::Probe
        );
        assert_eq!(
            scheduler.evaluate(30_000, sample(1, 1, 1), 5_000).state,
            State::Pressured
        );
    }

    #[test]
    fn sustained_over_budget_pressure_enters_protected() {
        let mut scheduler = Scheduler::new();
        scheduler.evaluate(0, sample(101, 1, 1), 5_000);
        scheduler.evaluate(10_000, sample(101, 1, 1), 5_000);
        assert_eq!(
            scheduler.evaluate(70_000, sample(101, 1, 1), 5_000).state,
            State::Protected
        );
    }

    #[test]
    fn failed_probe_uses_bounded_exponential_backoff() {
        let mut scheduler = Scheduler::new();
        scheduler.evaluate(0, sample(1, 90, 1), 5_000);
        scheduler.evaluate(5_000, sample(1, 1, 1), 5_000);
        let failed = scheduler.evaluate(10_000, sample(101, 1, 1), 5_000);
        assert_eq!(failed.state, State::Protected);
        assert_eq!(failed.next_probe_at_ms, 20_000);

        scheduler.evaluate(20_000, sample(1, 1, 1), 5_000);
        let failed_again = scheduler.evaluate(25_000, sample(101, 1, 1), 5_000);
        assert_eq!(failed_again.next_probe_at_ms, 45_000);
        assert!(failed_again.next_probe_at_ms - 25_000 <= MAX_PROBE_BACKOFF_MS);

        scheduler.probe_backoff_ms = MAX_PROBE_BACKOFF_MS;
        scheduler.state = State::Probe;
        scheduler.next_probe_at_ms = 45_000;
        let capped = scheduler.evaluate(45_000, sample(101, 1, 1), 5_000);
        assert_eq!(capped.next_probe_at_ms, 105_000);
    }

    #[test]
    fn fairness_slot_advances_without_changing_bounds() {
        let mut scheduler = Scheduler::new();
        let first = scheduler.evaluate(0, sample(1, 1, 1), 60_000);
        let second = scheduler.evaluate(1, sample(1, 1, 1), 60_000);
        assert_eq!(second.fairness_slot, first.fairness_slot + 1);
        assert_eq!(second.reconcile_interval_ms, 60_000);
    }
}
