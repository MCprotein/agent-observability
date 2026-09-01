use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as FmtWrite;
use std::fs::{self, Permissions};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::ops::{Deref, DerefMut};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use agent_observability_contracts::{AgentSource, ObservationEvent, SourceObservation};
use agent_observability_domain::{
    CorrelationIds, LifecycleState, ObservationId, SourceCursor, SourceGeneration, SpanId, Timing,
    TokenUsage, TraceId,
};
use agent_observability_local_collector::{CollectorSettings, TOKEN_HEADER, load_settings};
use agent_observability_local_runtime::{
    Admission, ENQUEUE_DEADLINE_MS, Ingress, IngressMessage, IngressOutcome, LocalRuntimeConfigV2,
    PressureSample, RuntimeControl, StorageBudget,
};
use agent_observability_local_store::LocalStore;
use serde::Deserialize;

const USAGE: &str =
    "usage: cargo run -p xtask -- perf <local|automatic> --profile <release|smoke> --check";
const PROTOCOL: &str = include_str!("../../crates/contracts/performance/local-performance-v1.yaml");
const AUTOMATIC_PROTOCOL: &str =
    include_str!("../../crates/contracts/performance/automatic-local-performance-v1.yaml");
const AUTOMATIC_PROTOCOL_REVISION: &str = "v1.3.0-sustained-primary-otlp-crash-lifecycle-scenarios";
const REQUIRED_PROTOCOL_LINES: [&str; 50] = [
    "schema_version: local_performance.v1",
    "protocol_revision: v1.2.0-supported-rate-saturation-continuous-network",
    "warmup_seconds: 60",
    "idle_seconds: 900",
    "active_seconds: 900",
    "supported_rate_events: 10000",
    "supported_inter_event_ms: 3",
    "saturation_events: 10000",
    "sample_interval_seconds: 1",
    "baseline_runs: 5",
    "enabled_runs: 5",
    "foreground_hook_p95_ms_max: 20",
    "foreground_hook_p99_ms_max: 50",
    "idle_average_percent_max: 0.5",
    "active_average_percent_max: 2",
    "active_any_minute_percent_max: 5",
    "sampling: process CPU time delta divided by sample wall-time delta; lifetime-average percent is forbidden",
    "peak_sampling_interval: profile sample_interval_seconds",
    "supported_rate_integrated_and_sampled_percent_max: 100",
    "saturation_cpu: recorded for diagnosis and never used to dilute supported-rate CPU",
    "supported_rate_durability_barrier: required before saturation",
    "supported_rate_measurement_boundary: first command through durability barrier completion; accepted commit tail is included",
    "p95_mib_max: 96",
    "supported_rate_saturation_and_drain_peak_required: true",
    "total_bytes_max: 1073741824",
    "accounting: allocated filesystem blocks for the full release durable root across all retained run directories, including durable state, final state, projections, crash and sampled temp artifacts",
    "ingest_requests_in_flight_max: 0",
    "required_bytes: 0",
    "mode: platform-specific process-scoped evidence plus bounded static product-surface scan",
    "linux_evidence: point-in-time worker socket-descriptor scan at every resource sample and after drain",
    "macos_evidence: one bounded long-lived nettop monitor per run; only a cycle closed by the next header becomes evidence, latest cumulative bytes attach to each resource sample, and the process-lifetime maximum is retained",
    "freshness: every macOS resource sample requires a completed monitor cycle no older than 3 seconds",
    "sampler_failure: monitor startup has a bounded retry budget; missing samples, parse errors, or unexpected exit fail the run immediately",
    "final_sample: worker remains alive after durable drain until a complete monitor cycle that started after drain completion is observed; Linux performs a final descriptor scan",
    "channel_capacity: 64",
    "normalization_workers: 1",
    "durable_batch_records: 32",
    "durable_handoff_bytes_max: 589824",
    "total_pipeline_payload_bytes_max: 4784128",
    "enqueue_deadline_ms: 10",
    "enabled_rejection_percent_max: 1",
    "rejection_budget_applies_to: each enabled supported-rate and saturation pass",
    "drain_evidence_boundary: worker creates the marker only after the drain command, retains it through drain completion and the parent-confirmed final resource sample, then removes it before drain-complete",
    "resource_sampler_shutdown: stop and result waits are bounded; sampler and worker output readers are joined after completion",
    "required_fields: [protocol_revision, source_revision, machine, os, filesystem, power_mode, cold_warm_cache, logical_cores, source_versions, workload, phase_metrics, all_run_samples, baseline, enabled]",
    "fail_closed: missing or breached required metrics produce non-zero exit",
    "host_metadata: sanitized factual architecture/core, filesystem type, power source and cache/warmup state; placeholder values are forbidden",
    "failure_behavior: stop after the first failed run, bound worker/sampler termination and join waits, join resources whose completion is confirmed, remove durable payloads, and retain the release failure manifest for diagnosis",
    "event_reconciliation: after graceful fixture shutdown enabled supported-rate plus saturation enqueued events must equal durable observations; every rejection remains explicit per pass",
    "output: docs/evidence/local/performance/<run>/manifest.yaml",
];
const SOURCES: [(&str, AgentSource); 3] = [
    ("codex", AgentSource::Codex),
    ("claude-code", AgentSource::ClaudeCode),
    ("cursor", AgentSource::Cursor),
];
const DURABLE_BATCH_COALESCE: Duration = Duration::from_millis(3);
const SUPPORTED_INTER_EVENT_PERIOD: Duration = Duration::from_millis(3);
const DURABLE_BATCH_RECORDS: u16 = 32;
const DURABLE_BATCH_BYTES: u32 = 524_288;
const DURABLE_HANDOFF_BYTES_MAX: u32 = 589_824;
const TOTAL_PIPELINE_PAYLOAD_BYTES_MAX: u32 = 4_784_128;
const WORKER_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const WORKER_PROTOCOL_TIMEOUT: Duration = Duration::from_secs(30);
const WORKER_EXIT_TIMEOUT: Duration = Duration::from_secs(5);
const SAMPLER_STOP_TIMEOUT: Duration = Duration::from_secs(5);
const LOCAL_COMMAND_TIMEOUT: Duration = Duration::from_secs(1);
const AUTOMATIC_BUILD_TIMEOUT: Duration = Duration::from_mins(10);
const AUTOMATIC_START_TIMEOUT: Duration = Duration::from_secs(10);
const AUTOMATIC_ACTIVE_TIMEOUT_RELEASE: Duration = Duration::from_mins(5);
const AUTOMATIC_ACTIVE_TIMEOUT_SMOKE: Duration = Duration::from_secs(15);
const AUTOMATIC_LIFECYCLE_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const AUTOMATIC_LIFECYCLE_RESTART_TIMEOUT: Duration = Duration::from_secs(15);
const AUTOMATIC_LIFECYCLE_RECOVERY_TIMEOUT: Duration = Duration::from_secs(10);
const AUTOMATIC_LIFECYCLE_CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
const AUTOMATIC_PRIMARY_OTLP_P95_MS_MAX: u64 = 20;
const AUTOMATIC_PRIMARY_OTLP_P99_MS_MAX: u64 = 50;
const AUTOMATIC_IDLE_CPU_PERCENT_MAX: f64 = 0.5;
const AUTOMATIC_ACTIVE_CPU_PERCENT_MAX: f64 = 100.0;
const AUTOMATIC_PEAK_RSS_MIB_MAX: f64 = 96.0;
const AUTOMATIC_ALLOCATED_DISK_BYTES_MAX: u64 = 1_073_741_824;
const AUTOMATIC_CONNECT_OUTPUT_MAX_BYTES: u64 = 4_096;
const AUTOMATIC_LIFECYCLE_SEED: &[u8] = b"# exact automatic lifecycle seed\nmodel = \"gpt-test\"\n";
const AUTOMATIC_LIFECYCLE_SEED_MODE: u32 = 0o600;
#[cfg(target_os = "macos")]
const NETWORK_MONITOR_START_ATTEMPTS: usize = 3;
#[cfg(target_os = "macos")]
const NETWORK_MONITOR_START_TIMEOUT: Duration = Duration::from_secs(3);
#[cfg(target_os = "macos")]
const NETWORK_MONITOR_FINAL_TIMEOUT: Duration = Duration::from_secs(4);
#[cfg(target_os = "macos")]
const NETWORK_MONITOR_STALE_AFTER: Duration = Duration::from_secs(3);
#[cfg(target_os = "macos")]
const NETWORK_MONITOR_RETRY_DELAY: Duration = Duration::from_millis(50);
#[cfg(target_os = "macos")]
const NETWORK_MONITOR_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Profile {
    Release,
    Smoke,
}
#[derive(Clone, Copy, Debug)]
struct Config {
    profile: Profile,
    warmup: Duration,
    idle: Duration,
    active: Duration,
    supported_events: usize,
    saturation_events: usize,
    runs: usize,
    sample: Duration,
}

#[derive(Clone, Copy, Debug)]
struct AutomaticConfig {
    profile: Profile,
    warmup: Duration,
    idle: Duration,
    events: usize,
    inter_event: Duration,
    runs: usize,
    sample: Duration,
    active_timeout: Duration,
}

impl AutomaticConfig {
    fn for_profile(profile: Profile) -> Self {
        match profile {
            Profile::Release => Self {
                profile,
                warmup: Duration::from_mins(1),
                idle: Duration::from_mins(15),
                events: 10_000,
                inter_event: Duration::from_millis(3),
                runs: 5,
                sample: Duration::from_secs(1),
                active_timeout: AUTOMATIC_ACTIVE_TIMEOUT_RELEASE,
            },
            Profile::Smoke => Self {
                profile,
                warmup: Duration::from_millis(100),
                idle: Duration::from_millis(250),
                events: 25,
                inter_event: Duration::from_millis(1),
                runs: 1,
                sample: Duration::from_millis(100),
                active_timeout: AUTOMATIC_ACTIVE_TIMEOUT_SMOKE,
            },
        }
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AutomaticProtocol {
    schema_version: String,
    version: String,
    protocol_revision: String,
    purpose: String,
    profiles: AutomaticProtocolProfiles,
    workload: AutomaticProtocolWorkload,
    metrics: AutomaticProtocolMetrics,
    execution: AutomaticProtocolExecution,
    evidence: AutomaticProtocolEvidence,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AutomaticProtocolProfiles {
    release: AutomaticReleaseProfile,
    smoke: AutomaticSmokeProfile,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AutomaticReleaseProfile {
    build: String,
    required_os: String,
    warmup_seconds: u64,
    idle_seconds: u64,
    primary_otlp_requests: usize,
    primary_otlp_inter_request_ms: u64,
    runs: usize,
    sample_interval_seconds: u64,
    active_timeout_seconds: u64,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AutomaticSmokeProfile {
    build: String,
    normative: bool,
    release_check: bool,
    primary_otlp_requests: usize,
    runs: usize,
    active_timeout_seconds: u64,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AutomaticProtocolWorkload {
    lifecycle_preflight: String,
    lifecycle_seed: String,
    lifecycle_assertions: String,
    lifecycle_cleanup: String,
    primary_boundary: String,
    notify_boundary: String,
    collector_boundary: String,
    payload: String,
    readiness: String,
    collector_shutdown: String,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AutomaticProtocolMetrics {
    primary_otlp: AutomaticPrimaryOtlpMetrics,
    collector_cpu: AutomaticCpuMetrics,
    collector_rss: AutomaticRssMetrics,
    disk: AutomaticDiskMetrics,
    network: AutomaticNetworkMetrics,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AutomaticPrimaryOtlpMetrics {
    required_quantiles: Vec<String>,
    validation_scope: String,
    p95_ms_max: u64,
    p99_ms_max: u64,
    command_timeout_seconds: u64,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AutomaticCpuMetrics {
    idle_average_percent_max: f64,
    active_integrated_percent_max: f64,
    normalization: String,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AutomaticRssMetrics {
    peak_mib_max: f64,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AutomaticDiskMetrics {
    allocated_tree_bytes_max: u64,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AutomaticNetworkMetrics {
    allowed_transport: String,
    bytes: String,
    endpoints: String,
    evidence: String,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AutomaticProtocolExecution {
    build_timeout_seconds: u64,
    startup_timeout_seconds: u64,
    cleanup_timeout_seconds: u64,
    fail_closed: String,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AutomaticProtocolEvidence {
    output: String,
    exact_source: String,
    sanitized_paths: String,
}

#[derive(Debug)]
struct AutomaticRunResult {
    run: usize,
    primary_otlp_latencies_us: Vec<u128>,
    idle_samples: Vec<Sample>,
    idle_cpu_percent: f64,
    active_cpu_percent: f64,
    peak_rss_kib: f64,
    peak_disk_bytes: u64,
    collector_network_bytes: u64,
    network_monitor_samples: u64,
    accepted_primary_requests: usize,
    rejected_primary_requests: usize,
    notify_supplement_accepted: bool,
}
impl Config {
    fn for_profile(profile: Profile) -> Self {
        match profile {
            Profile::Release => Self {
                profile,
                warmup: Duration::from_mins(1),
                idle: Duration::from_mins(15),
                active: Duration::from_mins(15),
                supported_events: 10_000,
                saturation_events: 10_000,
                runs: 5,
                sample: Duration::from_secs(1),
            },
            Profile::Smoke => Self {
                profile,
                warmup: Duration::from_millis(100),
                idle: Duration::from_millis(250),
                active: Duration::from_millis(500),
                supported_events: 100,
                saturation_events: 100,
                runs: 1,
                sample: Duration::from_millis(100),
            },
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Sample {
    phase: String,
    elapsed_ms: u128,
    cpu_percent: Option<f64>,
    rss_kib: Option<f64>,
    disk_bytes: Option<u64>,
    network_bytes: Option<u64>,
}
#[derive(Debug)]
struct RunResult {
    enabled: bool,
    run: usize,
    events: usize,
    rejected_events: usize,
    saturation_events: usize,
    saturation_rejected_events: usize,
    durable_events: u64,
    latencies_us: Vec<u128>,
    saturation_latencies_us: Vec<u128>,
    samples: Vec<Sample>,
    durable_bytes: u64,
    supported_rate_cpu_percent: f64,
    saturation_cpu_percent: f64,
    peak_rss_kib: f64,
    peak_disk_bytes: u64,
    network_bytes: u64,
    network_monitor_samples: u64,
}

#[derive(Clone, Debug, Default)]
struct PressurePeaks {
    cpu_percent: f64,
    rss_kib: f64,
    disk_bytes: u64,
    drain_sample_count: usize,
    process_exit_observed: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct NetworkEvidence {
    latest_bytes: u64,
    max_bytes: u64,
    samples: u64,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Default)]
struct MacNetworkMonitorState {
    evidence: NetworkEvidence,
    pending_bytes: Option<u64>,
    pending_started_at: Option<Instant>,
    last_completed_at: Option<Instant>,
    last_completed_started_at: Option<Instant>,
    error: Option<String>,
}

#[cfg(target_os = "macos")]
struct NetworkMonitor {
    process: Option<Child>,
    reader: Option<thread::JoinHandle<()>>,
    reader_done: mpsc::Receiver<()>,
    state: Arc<Mutex<MacNetworkMonitorState>>,
    stopping: Arc<AtomicBool>,
}

#[cfg(target_os = "macos")]
impl NetworkMonitor {
    fn start(pid: u32) -> Result<Self, String> {
        let mut errors = Vec::with_capacity(NETWORK_MONITOR_START_ATTEMPTS);
        for attempt in 1..=NETWORK_MONITOR_START_ATTEMPTS {
            match Self::start_once(pid) {
                Ok(monitor) => return Ok(monitor),
                Err(error) => errors.push(format!("attempt {attempt}: {error}")),
            }
            if attempt < NETWORK_MONITOR_START_ATTEMPTS {
                sleep(NETWORK_MONITOR_RETRY_DELAY);
            }
        }
        Err(format!(
            "process-scoped network monitor exhausted its startup retry budget: {}",
            errors.join("; ")
        ))
    }

    fn start_once(pid: u32) -> Result<Self, String> {
        let mut process = Command::new("/usr/bin/script")
            .args([
                "-q",
                "/dev/null",
                "/usr/bin/nettop",
                "-P",
                "-n",
                "-x",
                "-l",
                "0",
                "-s",
                "1",
                "-J",
                "bytes_in,bytes_out",
                "-p",
                &pid.to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("start process-scoped network monitor: {error}"))?;
        let stdout = process
            .stdout
            .take()
            .ok_or("process-scoped network monitor stdout is unavailable")?;
        let state = Arc::new(Mutex::new(MacNetworkMonitorState::default()));
        let reader_state = Arc::clone(&state);
        let stopping = Arc::new(AtomicBool::new(false));
        let reader_stopping = Arc::clone(&stopping);
        let (reader_done_tx, reader_done) = mpsc::channel();
        let reader = thread::spawn(move || {
            let result = read_macos_network_monitor(BufReader::new(stdout), &reader_state);
            if let Err(error) = result
                && let Ok(mut state) = reader_state.lock()
            {
                state.error = Some(error);
            }
            if !reader_stopping.load(Ordering::Acquire)
                && let Ok(mut state) = reader_state.lock()
                && state.error.is_none()
            {
                state.error = Some("process-scoped network monitor ended unexpectedly".into());
            }
            let _ = reader_done_tx.send(());
        });
        let mut monitor = Self {
            process: Some(process),
            reader: Some(reader),
            reader_done,
            state,
            stopping,
        };
        if let Err(error) = monitor.wait_until_started() {
            let _ = monitor.shutdown();
            return Err(error);
        }
        Ok(monitor)
    }

    fn wait_until_started(&mut self) -> Result<(), String> {
        let started = Instant::now();
        loop {
            let state = self.state()?;
            if let Some(error) = state.error {
                return Err(error);
            }
            if state.evidence.samples > 0 {
                return Ok(());
            }
            if self
                .process
                .as_mut()
                .ok_or("process-scoped network monitor process is missing")?
                .try_wait()
                .map_err(|error| format!("poll process-scoped network monitor: {error}"))?
                .is_some()
            {
                return Err("process-scoped network monitor exited during startup".into());
            }
            if started.elapsed() >= NETWORK_MONITOR_START_TIMEOUT {
                return Err(format!(
                    "process-scoped network monitor startup timed out after {} ms",
                    NETWORK_MONITOR_START_TIMEOUT.as_millis()
                ));
            }
            sleep(NETWORK_MONITOR_POLL_INTERVAL);
        }
    }

    fn sample(&mut self) -> Result<u64, String> {
        if self
            .process
            .as_mut()
            .ok_or("process-scoped network monitor process is missing")?
            .try_wait()
            .map_err(|error| format!("poll process-scoped network monitor: {error}"))?
            .is_some()
        {
            return Err("process-scoped network monitor exited unexpectedly".into());
        }
        macos_network_sample(&self.state()?, Instant::now())
    }

    fn final_sample(&mut self) -> Result<u64, String> {
        let drain_completed_at = Instant::now();
        let started = drain_completed_at;
        loop {
            let latest = self.sample()?;
            if macos_cycle_started_after(&self.state()?, drain_completed_at) {
                return Ok(latest);
            }
            if started.elapsed() >= NETWORK_MONITOR_FINAL_TIMEOUT {
                return Err(format!(
                    "post-drain network monitor sample timed out after {} ms",
                    NETWORK_MONITOR_FINAL_TIMEOUT.as_millis()
                ));
            }
            sleep(NETWORK_MONITOR_POLL_INTERVAL);
        }
    }

    fn finish(mut self) -> Result<NetworkEvidence, String> {
        let shutdown = self.shutdown();
        let evidence = self.state().and_then(finalize_macos_network_evidence);
        match (evidence, shutdown) {
            (Ok(evidence), Ok(())) => Ok(evidence),
            (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
            (Err(evidence_error), Err(shutdown_error)) => {
                Err(format!("{evidence_error}; {shutdown_error}"))
            }
        }
    }

    fn state(&self) -> Result<MacNetworkMonitorState, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "process-scoped network monitor state is poisoned")?;
        Ok(MacNetworkMonitorState {
            evidence: state.evidence,
            pending_bytes: state.pending_bytes,
            pending_started_at: state.pending_started_at,
            last_completed_at: state.last_completed_at,
            last_completed_started_at: state.last_completed_started_at,
            error: state.error.clone(),
        })
    }

    fn shutdown(&mut self) -> Result<(), String> {
        if self.process.is_none() && self.reader.is_none() {
            return Ok(());
        }
        self.stopping.store(true, Ordering::Release);
        let process_cleanup = if let Some(mut process) = self.process.take() {
            let mut cleanup_errors = Vec::new();
            let running = match process.try_wait() {
                Ok(status) => status.is_none(),
                Err(error) => {
                    cleanup_errors.push(format!("inspect process-scoped network monitor: {error}"));
                    true
                }
            };
            if running {
                if let Err(error) = terminate_monitor_descendants(process.id()) {
                    cleanup_errors.push(error);
                }
                if let Err(error) = process.kill() {
                    cleanup_errors.push(format!("stop process-scoped network monitor: {error}"));
                }
            }
            if let Err(error) = wait_for_child(&mut process, WORKER_EXIT_TIMEOUT) {
                cleanup_errors.push(format!("join process-scoped network monitor: {error}"));
            }
            errors_result(&cleanup_errors)
        } else {
            Ok(())
        };
        let (reader_finished, reader_cleanup) =
            match self.reader_done.recv_timeout(WORKER_EXIT_TIMEOUT) {
                Ok(()) => (true, Ok(())),
                Err(mpsc::RecvTimeoutError::Timeout) => (
                    false,
                    Err(format!(
                        "process-scoped network monitor reader timed out after {} ms",
                        WORKER_EXIT_TIMEOUT.as_millis()
                    )),
                ),
                Err(mpsc::RecvTimeoutError::Disconnected) => (
                    true,
                    Err("process-scoped network monitor reader ended without completion".into()),
                ),
            };
        let reader_join = if reader_finished {
            self.reader.take().map_or(Ok(()), |reader| {
                reader
                    .join()
                    .map_err(|_| "process-scoped network monitor reader panicked".into())
            })
        } else {
            Ok(())
        };
        combine_cleanup(
            combine_cleanup(process_cleanup, reader_cleanup),
            reader_join,
        )
    }
}

#[cfg(target_os = "macos")]
fn finalize_macos_network_evidence(
    state: MacNetworkMonitorState,
) -> Result<NetworkEvidence, String> {
    if let Some(error) = state.error {
        return Err(error);
    }
    if state.evidence.samples == 0 {
        return Err("process-scoped network monitor produced no samples".into());
    }
    Ok(state.evidence)
}

#[cfg(target_os = "macos")]
fn terminate_monitor_descendants(pid: u32) -> Result<(), String> {
    let mut process = Command::new("/usr/bin/pkill")
        .args(["-KILL", "-P", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("start monitor descendant cleanup: {error}"))?;
    let status = wait_for_child(&mut process, LOCAL_COMMAND_TIMEOUT).inspect_err(|_| {
        let _ = process.kill();
        let _ = wait_for_child(&mut process, LOCAL_COMMAND_TIMEOUT);
    })?;
    if status.success() || status.code() == Some(1) {
        Ok(())
    } else {
        Err(format!(
            "stop process-scoped network monitor child: {status}"
        ))
    }
}

fn errors_result(errors: &[String]) -> Result<(), String> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[cfg(target_os = "macos")]
fn macos_network_sample(
    state: &MacNetworkMonitorState,
    observed_at: Instant,
) -> Result<u64, String> {
    if let Some(error) = &state.error {
        return Err(error.clone());
    }
    if state.evidence.samples == 0 {
        return Err("process-scoped network monitor produced no samples".into());
    }
    let completed_at = state
        .last_completed_at
        .ok_or("process-scoped network monitor completion time is missing")?;
    if observed_at.saturating_duration_since(completed_at) > NETWORK_MONITOR_STALE_AFTER {
        return Err("process-scoped network monitor evidence is stale".into());
    }
    Ok(state.evidence.latest_bytes)
}

#[cfg(target_os = "macos")]
fn macos_cycle_started_after(state: &MacNetworkMonitorState, boundary: Instant) -> bool {
    state
        .last_completed_started_at
        .is_some_and(|cycle_started_at| cycle_started_at >= boundary)
}

#[cfg(target_os = "macos")]
impl Drop for NetworkMonitor {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            eprintln!("fallback network monitor cleanup failed: {error}");
        }
    }
}

#[cfg(target_os = "linux")]
struct NetworkMonitor {
    pid: u32,
    evidence: NetworkEvidence,
}

#[cfg(target_os = "linux")]
impl NetworkMonitor {
    fn start(pid: u32) -> Result<Self, String> {
        let mut monitor = Self {
            pid,
            evidence: NetworkEvidence::default(),
        };
        monitor.sample()?;
        Ok(monitor)
    }

    fn sample(&mut self) -> Result<u64, String> {
        let bytes = linux_network_bytes(self.pid)?;
        self.evidence.latest_bytes = bytes;
        self.evidence.max_bytes = self.evidence.max_bytes.max(bytes);
        self.evidence.samples = self.evidence.samples.saturating_add(1);
        Ok(bytes)
    }

    fn final_sample(&mut self) -> Result<u64, String> {
        self.sample()
    }

    fn finish(self) -> Result<NetworkEvidence, String> {
        if self.evidence.samples == 0 {
            return Err("process-scoped network monitor produced no samples".into());
        }
        Ok(self.evidence)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
struct NetworkMonitor;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl NetworkMonitor {
    fn start(_pid: u32) -> Result<Self, String> {
        Err("process-scoped network evidence is unsupported on this host".into())
    }

    fn sample(&mut self) -> Result<u64, String> {
        Err("process-scoped network evidence is unsupported on this host".into())
    }

    fn final_sample(&mut self) -> Result<u64, String> {
        Err("process-scoped network evidence is unsupported on this host".into())
    }

    fn finish(self) -> Result<NetworkEvidence, String> {
        Err("process-scoped network evidence is unsupported on this host".into())
    }
}

struct WorkloadPass {
    latencies_us: Vec<u128>,
    rejected_events: usize,
}

struct WorkerOutput {
    receiver: mpsc::Receiver<Result<String, String>>,
    reader_done: mpsc::Receiver<()>,
    reader: Option<thread::JoinHandle<()>>,
}

impl WorkerOutput {
    fn new(stdout: ChildStdout) -> Self {
        let (sender, receiver) = mpsc::channel();
        let (reader_done_tx, reader_done) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.map_err(|error| format!("read worker output: {error}"));
                if sender.send(line).is_err() {
                    break;
                }
            }
            let _ = reader_done_tx.send(());
        });
        Self {
            receiver,
            reader_done,
            reader: Some(reader),
        }
    }

    fn read(&self, timeout: Duration, label: &str) -> Result<String, String> {
        read_worker_output(&self.receiver, timeout, label)
    }

    fn join(&mut self) -> Result<(), String> {
        self.join_with_timeout(WORKER_EXIT_TIMEOUT)
    }

    fn join_with_timeout(&mut self, timeout: Duration) -> Result<(), String> {
        let finished = match self.reader_done.recv_timeout(timeout) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => true,
            Err(mpsc::RecvTimeoutError::Timeout) => false,
        };
        if !finished {
            return Err(format!(
                "worker output reader timed out after {} ms",
                timeout.as_millis()
            ));
        }
        if let Some(reader) = self.reader.take() {
            reader.join().map_err(|_| "worker output reader panicked")?;
        }
        Ok(())
    }
}

fn read_worker_output(
    receiver: &mpsc::Receiver<Result<String, String>>,
    timeout: Duration,
    label: &str,
) -> Result<String, String> {
    match receiver.recv_timeout(timeout) {
        Ok(line) => line,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
            "worker {label} timed out after {} ms",
            timeout.as_millis()
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(format!("worker output ended before {label}"))
        }
    }
}

struct DirectoryCleanup {
    path: PathBuf,
    label: &'static str,
    complete: bool,
}

impl DirectoryCleanup {
    fn new(path: PathBuf, label: &'static str) -> Self {
        Self {
            path,
            label,
            complete: false,
        }
    }

    fn cleanup(&mut self) -> Result<(), String> {
        match fs::remove_dir_all(&self.path) {
            Ok(()) => {
                self.complete = true;
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.complete = true;
                Ok(())
            }
            Err(error) => Err(format!("remove {}: {error}", self.label)),
        }
    }
}

impl Drop for DirectoryCleanup {
    fn drop(&mut self) {
        if !self.complete
            && let Err(error) = self.cleanup()
        {
            eprintln!("fallback directory cleanup failed: {error}");
        }
    }
}

struct ChildGuard(Child);

impl ChildGuard {
    fn terminate(&mut self) -> Result<(), String> {
        if self
            .0
            .try_wait()
            .map_err(|e| format!("inspect worker during cleanup: {e}"))?
            .is_some()
        {
            return Ok(());
        }
        self.0
            .kill()
            .map_err(|e| format!("terminate worker during cleanup: {e}"))?;
        wait_for_child(&mut self.0, WORKER_EXIT_TIMEOUT)
            .map_err(|e| format!("join worker during cleanup: {e}"))?;
        Ok(())
    }
}

impl Deref for ChildGuard {
    type Target = Child;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ChildGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Err(error) = self.terminate() {
            eprintln!("fallback worker cleanup failed: {error}");
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct MetricSummary {
    idle_cpu_delta_percent: f64,
    active_cpu_delta_percent: f64,
    active_any_minute_cpu_delta_percent: f64,
    enabled_rss_p95_kib: f64,
    total_allocated_disk_bytes: u64,
    network_bytes: u64,
}

#[derive(Clone, Debug)]
struct HostEvidence {
    source_revision: String,
    machine: String,
    logical_cores: usize,
    filesystem: String,
    power_mode: String,
}

impl HostEvidence {
    fn collect(path: &Path) -> Result<Self, String> {
        let logical_cores = thread::available_parallelism()
            .map_err(|e| format!("inspect logical core count: {e}"))?
            .get();
        Ok(Self {
            source_revision: source_revision()?,
            machine: format!(
                "sanitized-{}-{logical_cores}-logical-core",
                env::consts::ARCH
            ),
            logical_cores,
            filesystem: filesystem_type(path)?,
            power_mode: power_mode()?,
        })
    }
}

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let result = if args.first().map(String::as_str) == Some("--workload-worker") {
        worker()
    } else {
        command(&args)
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn command(args: &[String]) -> Result<(), String> {
    if args.is_empty() || args == ["help"] || args == ["--help"] || args == ["-h"] {
        println!(
            "{USAGE}\n\nRuns a local file-backed subprocess workload and writes sanitized evidence."
        );
        return Ok(());
    }
    if args.len() != 5
        || args[0] != "perf"
        || !matches!(args[1].as_str(), "local" | "automatic")
        || args[2] != "--profile"
        || args[4] != "--check"
    {
        return Err(USAGE.into());
    }
    let profile = match args[3].as_str() {
        "release" => Profile::Release,
        "smoke" => Profile::Smoke,
        _ => return Err("--profile must be release or smoke".into()),
    };
    if args[1] == "automatic" {
        run_automatic(AutomaticConfig::for_profile(profile))
    } else {
        run(Config::for_profile(profile))
    }
}

fn run_automatic(config: AutomaticConfig) -> Result<(), String> {
    validate_automatic_protocol_contract()?;
    validate_automatic_profile_host(config.profile, env::consts::OS)?;
    validate_network_surface()?;
    if config.profile == Profile::Release {
        require_clean_worktree()?;
    }
    let source_revision = source_revision()?;
    let binary = build_automatic_binary(config.profile)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "clock before epoch")?
        .as_nanos();
    let evidence_root =
        PathBuf::from("docs/evidence/local/performance").join(format!("automatic-{stamp}"));
    fs::create_dir_all(&evidence_root)
        .map_err(|error| format!("create automatic evidence directory: {error}"))?;
    fs::set_permissions(&evidence_root, Permissions::from_mode(0o700))
        .map_err(|error| format!("protect automatic evidence directory: {error}"))?;
    let mut smoke_cleanup = (config.profile == Profile::Smoke)
        .then(|| DirectoryCleanup::new(evidence_root.clone(), "automatic smoke evidence path"));
    let runtime_root = env::temp_dir().join(format!("agent-observability-automatic-{stamp}"));
    let mut runtime_cleanup = DirectoryCleanup::new(runtime_root.clone(), "automatic runtime path");
    fs::create_dir(&runtime_root)
        .map_err(|error| format!("create automatic runtime parent: {error}"))?;
    fs::set_permissions(&runtime_root, Permissions::from_mode(0o700))
        .map_err(|error| format!("protect automatic runtime parent: {error}"))?;
    run_automatic_lifecycle_smoke(&binary, &runtime_root)?;
    let host = HostEvidence::collect(&evidence_root)?;
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for run_number in 1..=config.runs {
        match execute_automatic_run(config, run_number, &binary, &runtime_root) {
            Ok(result) => results.push(result),
            Err(error) => {
                errors.push(format!("run {run_number}: {error}"));
                break;
            }
        }
    }
    let validation = validate_automatic_results(config, &results, &errors);
    if let Err(error) = &validation {
        errors.push(format!("validation: {error}"));
    }
    let manifest_path = evidence_root.join("manifest.yaml");
    let status = if validation.is_ok() {
        "pending-validation"
    } else {
        "failed"
    };
    let manifest =
        render_automatic_manifest(config, &host, &source_revision, &results, &errors, status);
    validate_automatic_manifest_shape(&manifest)?;
    fs::write(&manifest_path, &manifest)
        .map_err(|error| format!("write automatic manifest: {error}"))?;
    let runtime_result = runtime_cleanup.cleanup();
    let smoke_result = smoke_cleanup
        .as_mut()
        .map_or(Ok(()), DirectoryCleanup::cleanup);
    if let Err(cleanup_error) = combine_cleanup(runtime_result, smoke_result) {
        errors.push(format!("cleanup: {cleanup_error}"));
        if config.profile == Profile::Release {
            let failed = render_automatic_manifest(
                config,
                &host,
                &source_revision,
                &results,
                &errors,
                "failed",
            );
            fs::write(&manifest_path, failed)
                .map_err(|error| format!("finalize automatic cleanup failure: {error}"))?;
        }
        return Err(format!(
            "automatic performance cleanup failed: {cleanup_error}"
        ));
    }
    validation.map_err(|error| {
        format!(
            "automatic performance check failed; manifest: {}: {error}",
            manifest_path.display()
        )
    })?;
    if config.profile == Profile::Release {
        fs::write(
            &manifest_path,
            manifest.replace("status: pending-validation", "status: pass"),
        )
        .map_err(|error| format!("finalize automatic manifest: {error}"))?;
    }
    println!(
        "manifest={}\nprofile={}\nstatus=pass",
        manifest_path.display(),
        profile_name(config.profile)
    );
    Ok(())
}

fn validate_automatic_protocol_contract() -> Result<(), String> {
    validate_automatic_protocol(AUTOMATIC_PROTOCOL)
}

fn validate_automatic_protocol(protocol: &str) -> Result<(), String> {
    let parsed = serde_saphyr::from_str::<AutomaticProtocol>(protocol)
        .map_err(|error| format!("parse embedded automatic performance protocol: {error}"))?;
    let expected = expected_automatic_protocol();
    if parsed != expected {
        return Err(format!(
            "embedded automatic performance protocol drifted from executable configuration: {parsed:#?}"
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn expected_automatic_protocol() -> AutomaticProtocol {
    let release = AutomaticConfig::for_profile(Profile::Release);
    let smoke = AutomaticConfig::for_profile(Profile::Smoke);
    AutomaticProtocol {
        schema_version: "automatic_local_performance.v1".into(),
        version: "v1.3.0".into(),
        protocol_revision: AUTOMATIC_PROTOCOL_REVISION.into(),
        purpose: "normative performance evidence for the shipped Codex automatic local path".into(),
        profiles: AutomaticProtocolProfiles {
            release: AutomaticReleaseProfile {
                build: "cargo build --locked -p agent-observability-cli --release".into(),
                required_os: "macos".into(),
                warmup_seconds: release.warmup.as_secs(),
                idle_seconds: release.idle.as_secs(),
                primary_otlp_requests: release.events,
                primary_otlp_inter_request_ms: u64::try_from(release.inter_event.as_millis())
                    .expect("release inter-request duration fits u64"),
                runs: release.runs,
                sample_interval_seconds: release.sample.as_secs(),
                active_timeout_seconds: release.active_timeout.as_secs(),
            },
            smoke: AutomaticSmokeProfile {
                build: "cargo build --locked -p agent-observability-cli".into(),
                normative: false,
                release_check: false,
                primary_otlp_requests: smoke.events,
                runs: smoke.runs,
                active_timeout_seconds: smoke.active_timeout.as_secs(),
            },
        },
        workload: AutomaticProtocolWorkload {
            lifecycle_preflight: "bounded built-binary no-argument setup --no-open under isolated HOME and CODEX_HOME".into(),
            lifecycle_seed: "exact private Codex config bytes and permission mode".into(),
            lifecycle_assertions: "connected config, ready or degraded collector, installed LaunchAgent plist, pre-failure authenticated primary OTLP and notify durability, unexpected SIGKILL service termination and bounded launchd kickstart recovery, post-recovery authenticated primary OTLP and notify durability, occupied persisted port explicit reconnect, concurrent explicit connect commands, missing-settings disconnect, exact config restoration, inherited loaded and unloaded plist restoration, and bounded removal of the isolated service".into(),
            lifecycle_cleanup: "best-effort bounded disconnect, bootout, exact seed restoration, plist removal, and isolated directory removal on success or error".into(),
            primary_boundary: "sustained authenticated Codex OTLP/HTTP /v1/logs requests through the built collector and durable report authority".into(),
            notify_boundary: "separately verified built agent-observability codex-notify supplement through authenticated loopback response".into(),
            collector_boundary: "built agent-observability collector-serve subprocess".into(),
            payload: "bounded valid Codex OTLP log pairs with synthetic opaque identifiers; one bounded notify supplement per measured run".into(),
            readiness: "accepted authenticated primary OTLP request within a bounded startup deadline".into(),
            collector_shutdown: "bounded child termination and wait".into(),
        },
        metrics: AutomaticProtocolMetrics {
            primary_otlp: AutomaticPrimaryOtlpMetrics {
                required_quantiles: vec!["p95".into(), "p99".into()],
                validation_scope: "every run independently".into(),
                p95_ms_max: AUTOMATIC_PRIMARY_OTLP_P95_MS_MAX,
                p99_ms_max: AUTOMATIC_PRIMARY_OTLP_P99_MS_MAX,
                command_timeout_seconds: LOCAL_COMMAND_TIMEOUT.as_secs(),
            },
            collector_cpu: AutomaticCpuMetrics {
                idle_average_percent_max: AUTOMATIC_IDLE_CPU_PERCENT_MAX,
                active_integrated_percent_max: AUTOMATIC_ACTIVE_CPU_PERCENT_MAX,
                normalization: "percent of one logical core".into(),
            },
            collector_rss: AutomaticRssMetrics {
                peak_mib_max: AUTOMATIC_PEAK_RSS_MIB_MAX,
            },
            disk: AutomaticDiskMetrics {
                allocated_tree_bytes_max: AUTOMATIC_ALLOCATED_DISK_BYTES_MAX,
            },
            network: AutomaticNetworkMetrics {
                allowed_transport: "authenticated IPv4 loopback only".into(),
                bytes: "measured collector process bytes from the existing platform NetworkMonitor and reported without classifying loopback traffic as external".into(),
                endpoints: "every observed collector endpoint must be IPv4 loopback".into(),
                evidence: "process NetworkMonitor plus independent socket endpoint scans plus static product surface validation".into(),
            },
        },
        execution: AutomaticProtocolExecution {
            build_timeout_seconds: AUTOMATIC_BUILD_TIMEOUT.as_secs(),
            startup_timeout_seconds: AUTOMATIC_START_TIMEOUT.as_secs(),
            cleanup_timeout_seconds: AUTOMATIC_LIFECYCLE_CLEANUP_TIMEOUT.as_secs(),
            fail_closed: "missing or invalid metrics, rejected primary OTLP requests, missing notify supplement evidence, non-loopback endpoints, timeout, or threshold breach produce non-zero exit".into(),
        },
        evidence: AutomaticProtocolEvidence {
            output: "docs/evidence/local/performance/automatic-<run>/manifest.yaml".into(),
            exact_source: "full git commit, locked build command, package version, profile, and protocol revision".into(),
            sanitized_paths: "run-relative runtime and protocol paths only; token, port, host path, payload, and raw collector output are forbidden".into(),
        },
    }
}

fn validate_automatic_profile_host(profile: Profile, os: &str) -> Result<(), String> {
    if profile == Profile::Release && os != "macos" {
        return Err("automatic release performance evidence requires macOS".into());
    }
    Ok(())
}

fn build_automatic_binary(profile: Profile) -> Result<PathBuf, String> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask manifest directory has no workspace parent")?;
    let mut command = Command::new("cargo");
    command
        .current_dir(workspace)
        .args(["build", "--locked", "-p", "agent-observability-cli"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    if profile == Profile::Release {
        command.arg("--release");
    }
    let mut child = ChildGuard(
        command
            .spawn()
            .map_err(|error| format!("start automatic product build: {error}"))?,
    );
    let status = wait_for_child(&mut child, AUTOMATIC_BUILD_TIMEOUT)?;
    if !status.success() {
        return Err(format!("automatic product build failed: {status}"));
    }
    let target =
        env::var_os("CARGO_TARGET_DIR").map_or_else(|| workspace.join("target"), PathBuf::from);
    let binary = target
        .join(if profile == Profile::Release {
            "release"
        } else {
            "debug"
        })
        .join("agent-observability");
    if !binary.is_file() {
        return Err("built automatic collector binary is missing".into());
    }
    Ok(binary)
}

struct AutomaticLifecycleCleanup<'a> {
    binary: &'a Path,
    home: PathBuf,
    codex_home: PathBuf,
    config: PathBuf,
    seed: &'static [u8],
    seed_mode: u32,
    target: Option<String>,
    plist: Option<PathBuf>,
    connection_may_exist: bool,
    complete: bool,
}

impl AutomaticLifecycleCleanup<'_> {
    fn environment(&self) -> [(&str, &Path); 2] {
        [("HOME", &self.home), ("CODEX_HOME", &self.codex_home)]
    }

    fn cleanup(&mut self) -> Result<(), String> {
        if self.complete {
            return Ok(());
        }
        let mut errors = Vec::new();
        if self.connection_may_exist
            && let Err(error) = retry_automatic_lifecycle_disconnect(self)
        {
            errors.push(format!(
                "best-effort automatic lifecycle disconnect: {error}"
            ));
        }
        let mut artifacts = automatic_lifecycle_service_artifacts(
            &self.home,
            self.target.as_deref(),
            self.plist.as_deref(),
        )
        .unwrap_or_else(|error| {
            errors.push(error);
            Vec::new()
        });
        for (target, plist) in artifacts.drain(..) {
            if let Err(error) = run_bounded_status_command(
                "/bin/launchctl",
                &["bootout", &target],
                WORKER_EXIT_TIMEOUT,
                &[0, 3, 113],
            ) {
                errors.push(format!("best-effort automatic lifecycle bootout: {error}"));
            }
            if let Err(error) = fs::remove_file(&plist)
                && error.kind() != io::ErrorKind::NotFound
            {
                errors.push(format!(
                    "best-effort automatic lifecycle plist removal: {error}"
                ));
            }
        }
        if let Some(parent) = self.config.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            errors.push(format!(
                "best-effort automatic lifecycle config parent: {error}"
            ));
        }
        if let Err(error) = fs::write(&self.config, self.seed) {
            errors.push(format!(
                "best-effort automatic lifecycle config restoration: {error}"
            ));
        } else if let Err(error) =
            fs::set_permissions(&self.config, Permissions::from_mode(self.seed_mode))
        {
            errors.push(format!(
                "best-effort automatic lifecycle config mode restoration: {error}"
            ));
        }
        self.complete = true;
        errors_result(&errors)
    }
}

fn automatic_lifecycle_service_artifacts(
    home: &Path,
    known_target: Option<&str>,
    known_plist: Option<&Path>,
) -> Result<Vec<(String, PathBuf)>, String> {
    let mut plists = known_plist
        .into_iter()
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    let directory = home.join("Library/LaunchAgents");
    match fs::read_dir(&directory) {
        Ok(entries) => {
            for entry in entries {
                let path = entry
                    .map_err(|error| format!("inspect automatic lifecycle plist entry: {error}"))?
                    .path();
                let matching =
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            name.starts_with("io.agent-observability.collector.")
                                && path.extension().is_some_and(|extension| {
                                    extension
                                        .to_str()
                                        .is_some_and(|value| value.eq_ignore_ascii_case("plist"))
                                })
                        });
                if matching && !plists.contains(&path) {
                    plists.push(path);
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "inspect automatic lifecycle LaunchAgent directory: {error}"
            ));
        }
    }
    let uid = run_bounded_status_command("/usr/bin/id", &["-u"], LOCAL_COMMAND_TIMEOUT, &[0])?;
    let uid = uid.trim();
    plists
        .into_iter()
        .map(|plist| {
            let label = plist
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or("automatic lifecycle plist has an invalid label")?;
            let target = known_target
                .filter(|target| target.ends_with(label))
                .map_or_else(|| format!("gui/{uid}/{label}"), str::to_owned);
            Ok((target, plist))
        })
        .collect()
}

impl Drop for AutomaticLifecycleCleanup<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            eprintln!("fallback automatic lifecycle cleanup failed: {error}");
        }
    }
}

#[allow(clippy::too_many_lines)]
fn run_automatic_lifecycle_smoke(binary: &Path, runtime_root: &Path) -> Result<(), String> {
    if env::consts::OS != "macos" {
        return Err("automatic lifecycle smoke requires macOS".into());
    }
    let root = runtime_root.join("lifecycle-smoke/home/.agent-observability");
    let home = runtime_root.join("lifecycle-smoke/home");
    let codex_home = runtime_root.join("lifecycle-smoke/codex-home");
    fs::create_dir_all(&home)
        .map_err(|error| format!("create automatic lifecycle home: {error}"))?;
    fs::set_permissions(&home, Permissions::from_mode(0o700))
        .map_err(|error| format!("protect automatic lifecycle home: {error}"))?;
    fs::create_dir_all(&codex_home)
        .map_err(|error| format!("create automatic lifecycle Codex home: {error}"))?;
    fs::set_permissions(&codex_home, Permissions::from_mode(0o700))
        .map_err(|error| format!("protect automatic lifecycle Codex home: {error}"))?;
    let config = codex_home.join("config.toml");
    fs::write(&config, AUTOMATIC_LIFECYCLE_SEED)
        .map_err(|error| format!("seed automatic lifecycle Codex config: {error}"))?;
    fs::set_permissions(
        &config,
        Permissions::from_mode(AUTOMATIC_LIFECYCLE_SEED_MODE),
    )
    .map_err(|error| format!("protect automatic lifecycle Codex config: {error}"))?;
    let mut cleanup = AutomaticLifecycleCleanup {
        binary,
        home,
        codex_home,
        config,
        seed: AUTOMATIC_LIFECYCLE_SEED,
        seed_mode: AUTOMATIC_LIFECYCLE_SEED_MODE,
        target: None,
        plist: None,
        connection_may_exist: true,
        complete: false,
    };
    let smoke = (|| {
        let setup = run_bounded_product_command_with_env(
            binary,
            &["setup", "--no-open"],
            AUTOMATIC_LIFECYCLE_COMMAND_TIMEOUT,
            &cleanup.environment(),
        )?;
        require_output_line(&setup, "status", "ready")?;
        require_output_line(&setup, "config", "connected")?;
        require_collector_ready_or_degraded(&setup)?;
        let service =
            output_value(&setup, "service").ok_or("automatic lifecycle setup omitted service")?;
        if !service.starts_with("io.agent-observability.collector.") {
            return Err("automatic lifecycle setup returned an invalid service label".into());
        }
        let plist = cleanup
            .home
            .join("Library/LaunchAgents")
            .join(format!("{service}.plist"));
        let uid = run_bounded_status_command("/usr/bin/id", &["-u"], LOCAL_COMMAND_TIMEOUT, &[0])?;
        let target = format!("gui/{}/{service}", uid.trim());
        cleanup.plist = Some(plist.clone());
        cleanup.target = Some(target.clone());
        verify_automatic_launch_agent_plist(&plist, binary, &root)?;

        let status = run_bounded_product_command_with_env(
            binary,
            &["status", "codex"],
            AUTOMATIC_LIFECYCLE_COMMAND_TIMEOUT,
            &cleanup.environment(),
        )?;
        require_output_line(&status, "config", "connected")?;
        require_collector_ready_or_degraded(&status)?;
        let initial_records = automatic_report_record_count(binary, &root, &cleanup)?;
        submit_automatic_primary_otlp(&root, 0, 0)?;
        let pre_failure_otlp_records = require_automatic_record_growth(
            binary,
            &root,
            &cleanup,
            initial_records,
            "pre-failure primary OTLP",
        )?;
        submit_automatic_notify(binary, &root, &cleanup, 0)?;
        require_automatic_record_growth(
            binary,
            &root,
            &cleanup,
            pre_failure_otlp_records,
            "pre-failure notify",
        )?;

        run_bounded_status_command(
            "/bin/launchctl",
            &["kill", "SIGKILL", &target],
            AUTOMATIC_LIFECYCLE_RESTART_TIMEOUT,
            &[0],
        )?;
        run_bounded_status_command(
            "/bin/launchctl",
            &["kickstart", &target],
            AUTOMATIC_LIFECYCLE_RESTART_TIMEOUT,
            &[0],
        )?;
        wait_for_automatic_lifecycle_recovery(binary, &cleanup)?;

        let recovered_records = automatic_report_record_count(binary, &root, &cleanup)?;
        submit_automatic_primary_otlp(&root, 0, 1)?;
        let post_recovery_otlp_records = require_automatic_record_growth(
            binary,
            &root,
            &cleanup,
            recovered_records,
            "post-recovery primary OTLP",
        )?;
        submit_automatic_notify(binary, &root, &cleanup, 1)?;
        require_automatic_record_growth(
            binary,
            &root,
            &cleanup,
            post_recovery_otlp_records,
            "post-recovery notify",
        )?;

        let occupied_port = load_settings(&root)
            .map_err(|error| error.to_string())?
            .port;
        run_bounded_status_command(
            "/bin/launchctl",
            &["bootout", &target],
            AUTOMATIC_LIFECYCLE_RESTART_TIMEOUT,
            &[0],
        )?;
        let occupied = occupy_automatic_lifecycle_port(occupied_port)?;
        let reconnected = run_bounded_product_command_with_env(
            binary,
            &["connect", "codex", path_text(&root)?],
            AUTOMATIC_LIFECYCLE_COMMAND_TIMEOUT,
            &cleanup.environment(),
        )?;
        require_output_line(&reconnected, "config", "connected")?;
        require_collector_ready_or_degraded(&reconnected)?;
        let recovered_port = load_settings(&root)
            .map_err(|error| error.to_string())?
            .port;
        if recovered_port == occupied_port {
            return Err("automatic lifecycle occupied port was not recovered".into());
        }
        drop(occupied);

        run_concurrent_automatic_connects(binary, &root, &cleanup)?;
        fs::remove_file(root.join("runtime/collector.json"))
            .map_err(|error| format!("remove automatic lifecycle settings: {error}"))?;

        let disconnected = run_bounded_product_command_with_env(
            binary,
            &["disconnect", "codex", path_text(&root)?],
            AUTOMATIC_LIFECYCLE_COMMAND_TIMEOUT,
            &cleanup.environment(),
        )?;
        require_output_line(&disconnected, "config", "disconnected")?;
        require_output_line(&disconnected, "collector", "stopped")?;
        cleanup.connection_may_exist = false;
        verify_exact_file(
            &cleanup.config,
            AUTOMATIC_LIFECYCLE_SEED,
            AUTOMATIC_LIFECYCLE_SEED_MODE,
            "Codex config",
        )?;
        if plist.exists() {
            return Err("automatic lifecycle disconnect left the LaunchAgent plist".into());
        }
        if launch_agent_is_loaded(&target)? {
            return Err("automatic lifecycle disconnect left the LaunchAgent loaded".into());
        }
        verify_inherited_automatic_plist(binary, &root, &mut cleanup, &plist, &target, false)?;
        verify_inherited_automatic_plist(binary, &root, &mut cleanup, &plist, &target, true)?;
        Ok(())
    })();
    combine_cleanup(smoke, cleanup.cleanup())
}

fn automatic_report_record_count(
    binary: &Path,
    root: &Path,
    cleanup: &AutomaticLifecycleCleanup<'_>,
) -> Result<u64, String> {
    let report = run_bounded_product_command_with_env(
        binary,
        &["report", path_text(root)?],
        AUTOMATIC_LIFECYCLE_COMMAND_TIMEOUT,
        &cleanup.environment(),
    )?;
    output_value(&report, "records")
        .ok_or_else(|| String::from("automatic lifecycle report omitted records"))?
        .parse::<u64>()
        .map_err(|_| "automatic lifecycle report returned invalid records".into())
}

fn require_automatic_record_growth(
    binary: &Path,
    root: &Path,
    cleanup: &AutomaticLifecycleCleanup<'_>,
    previous: u64,
    boundary: &str,
) -> Result<u64, String> {
    let started = Instant::now();
    loop {
        let records = automatic_report_record_count(binary, root, cleanup)?;
        if records > previous {
            return Ok(records);
        }
        if started.elapsed() >= AUTOMATIC_LIFECYCLE_RECOVERY_TIMEOUT {
            return Err(format!(
                "automatic lifecycle {boundary} produced no new durable report record"
            ));
        }
        sleep(Duration::from_millis(20));
    }
}

fn occupy_automatic_lifecycle_port(port: u16) -> Result<TcpListener, String> {
    let started = Instant::now();
    loop {
        match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
            Ok(listener) => return Ok(listener),
            Err(_) if started.elapsed() < AUTOMATIC_LIFECYCLE_RECOVERY_TIMEOUT => {
                sleep(Duration::from_millis(20));
            }
            Err(error) => {
                return Err(format!(
                    "occupy automatic lifecycle persisted port after bootout: {error}"
                ));
            }
        }
    }
}

fn submit_automatic_notify(
    binary: &Path,
    root: &Path,
    cleanup: &AutomaticLifecycleCleanup<'_>,
    event: usize,
) -> Result<(), String> {
    let payload = automatic_notify_payload(0, event);
    let root = path_text(root)?;
    let started = Instant::now();
    loop {
        if let Ok(notify) = run_bounded_product_command_with_env(
            binary,
            &["codex-notify", root, &payload],
            LOCAL_COMMAND_TIMEOUT,
            &cleanup.environment(),
        ) && notify.trim() == "notify=accepted"
        {
            return Ok(());
        }
        if started.elapsed() >= AUTOMATIC_LIFECYCLE_RECOVERY_TIMEOUT {
            return Err("automatic lifecycle notify was not accepted".into());
        }
        sleep(Duration::from_millis(50));
    }
}

fn retry_automatic_lifecycle_disconnect(
    cleanup: &AutomaticLifecycleCleanup<'_>,
) -> Result<(), String> {
    let started = Instant::now();
    loop {
        match run_bounded_product_command_with_env(
            cleanup.binary,
            &["disconnect", "codex"],
            AUTOMATIC_LIFECYCLE_CLEANUP_TIMEOUT,
            &cleanup.environment(),
        ) {
            Ok(_) => return Ok(()),
            Err(error)
                if error.contains("lifecycle is busy")
                    && started.elapsed() < AUTOMATIC_LIFECYCLE_CLEANUP_TIMEOUT =>
            {
                sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(error),
        }
    }
}

fn run_concurrent_automatic_connects(
    binary: &Path,
    root: &Path,
    cleanup: &AutomaticLifecycleCleanup<'_>,
) -> Result<(), String> {
    let ownership = AutomaticLifecycleOwnership::capture(root, cleanup)?;
    let root_text = path_text(root)?;
    let mut commands = Vec::with_capacity(2);
    for _ in 0..2 {
        let mut command = Command::new(binary);
        command
            .args(["connect", "codex", root_text])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in cleanup.environment() {
            command.env(name, value);
        }
        commands.push(ChildGuard(command.spawn().map_err(|error| {
            format!("spawn concurrent automatic lifecycle connect: {error}")
        })?));
    }

    let deadline = Instant::now() + AUTOMATIC_LIFECYCLE_COMMAND_TIMEOUT;
    let mut outcomes = Vec::with_capacity(commands.len());
    for command in &mut commands {
        let remaining = deadline.saturating_duration_since(Instant::now());
        outcomes.push(collect_automatic_connect_output(command, remaining));
    }
    let mut completed = Vec::with_capacity(outcomes.len());
    for outcome in outcomes {
        completed.push(outcome?);
    }
    let successes = completed
        .iter()
        .filter(|outcome| outcome.status.success())
        .count();
    let busy_failures = completed
        .iter()
        .filter(|outcome| {
            !outcome.status.success()
                && outcome
                    .stderr
                    .contains("Codex integration lifecycle is busy")
        })
        .count();
    if !((successes == 1 && busy_failures == 1) || successes == 2) {
        return Err(format!(
            "automatic lifecycle concurrent connect outcomes violated the product contract: {}",
            format_automatic_connect_outcomes(&completed)
        ));
    }
    for outcome in completed.iter().filter(|outcome| outcome.status.success()) {
        require_output_line(&outcome.stdout, "integration", "codex")?;
        require_output_line(&outcome.stdout, "config", "connected")?;
        require_collector_ready_or_degraded(&outcome.stdout)?;
    }

    let status = run_bounded_product_command_with_env(
        binary,
        &["status", "codex", root_text],
        AUTOMATIC_LIFECYCLE_COMMAND_TIMEOUT,
        &cleanup.environment(),
    )?;
    require_output_line(&status, "integration", "codex")?;
    require_output_line(&status, "config", "connected")?;
    require_collector_ready_or_degraded(&status)?;
    ownership.verify(binary, root, cleanup)
}

#[derive(Debug)]
struct AutomaticConnectOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

#[derive(Debug)]
struct AutomaticLifecycleOwnership {
    settings: CollectorSettings,
    settings_bytes: Vec<u8>,
    config_bytes: Vec<u8>,
    config_mode: u32,
    plist: PathBuf,
    plist_bytes: Vec<u8>,
    plist_mode: u32,
    target: String,
}

impl AutomaticLifecycleOwnership {
    fn capture(root: &Path, cleanup: &AutomaticLifecycleCleanup<'_>) -> Result<Self, String> {
        let settings_path = root.join("runtime/collector.json");
        let plist = cleanup
            .plist
            .clone()
            .ok_or("automatic lifecycle plist ownership is unavailable")?;
        Ok(Self {
            settings: load_settings(root).map_err(|error| error.to_string())?,
            settings_bytes: fs::read(settings_path).map_err(|error| {
                format!("read automatic lifecycle settings before concurrency: {error}")
            })?,
            config_bytes: fs::read(&cleanup.config).map_err(|error| {
                format!("read automatic lifecycle config before concurrency: {error}")
            })?,
            config_mode: file_mode(&cleanup.config, "automatic lifecycle config")?,
            plist_bytes: fs::read(&plist).map_err(|error| {
                format!("read automatic lifecycle plist before concurrency: {error}")
            })?,
            plist_mode: file_mode(&plist, "automatic lifecycle plist")?,
            plist,
            target: cleanup
                .target
                .clone()
                .ok_or("automatic lifecycle target ownership is unavailable")?,
        })
    }

    fn verify(
        &self,
        binary: &Path,
        root: &Path,
        cleanup: &AutomaticLifecycleCleanup<'_>,
    ) -> Result<(), String> {
        let settings_path = root.join("runtime/collector.json");
        let settings_after = load_settings(root).map_err(|error| error.to_string())?;
        let settings_bytes_after = fs::read(settings_path).map_err(|error| {
            format!("read automatic lifecycle settings after concurrency: {error}")
        })?;
        if settings_after != self.settings || settings_bytes_after != self.settings_bytes {
            return Err("automatic lifecycle concurrent connect changed collector settings".into());
        }
        verify_exact_file(
            &cleanup.config,
            &self.config_bytes,
            self.config_mode,
            "owned Codex config",
        )?;
        verify_exact_file(
            &self.plist,
            &self.plist_bytes,
            self.plist_mode,
            "owned LaunchAgent plist",
        )?;
        verify_automatic_launch_agent_plist(&self.plist, binary, root)?;
        if !launch_agent_is_loaded(&self.target)? {
            return Err("automatic lifecycle concurrent connect lost LaunchAgent ownership".into());
        }
        Ok(())
    }
}

fn collect_automatic_connect_output(
    child: &mut ChildGuard,
    timeout: Duration,
) -> Result<AutomaticConnectOutput, String> {
    let status = match wait_for_child(child, timeout) {
        Ok(status) => status,
        Err(error) => {
            child.terminate()?;
            return Err(format!(
                "concurrent automatic lifecycle connect did not terminate boundedly: {error}"
            ));
        }
    };
    let stdout = read_bounded_child_stream(
        child
            .stdout
            .take()
            .ok_or("concurrent automatic lifecycle connect stdout is unavailable")?,
        "stdout",
    )?;
    let stderr = read_bounded_child_stream(
        child
            .stderr
            .take()
            .ok_or("concurrent automatic lifecycle connect stderr is unavailable")?,
        "stderr",
    )?;
    Ok(AutomaticConnectOutput {
        status,
        stdout,
        stderr,
    })
}

fn read_bounded_child_stream(stream: impl Read, label: &str) -> Result<String, String> {
    let mut bytes = Vec::new();
    stream
        .take(AUTOMATIC_CONNECT_OUTPUT_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read concurrent automatic lifecycle connect {label}: {error}"))?;
    if bytes.len() as u64 > AUTOMATIC_CONNECT_OUTPUT_MAX_BYTES {
        return Err(format!(
            "concurrent automatic lifecycle connect {label} exceeded {AUTOMATIC_CONNECT_OUTPUT_MAX_BYTES} bytes"
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| format!("concurrent automatic lifecycle connect {label} is not UTF-8"))
}

fn format_automatic_connect_outcomes(outcomes: &[AutomaticConnectOutput]) -> String {
    outcomes
        .iter()
        .enumerate()
        .map(|(index, outcome)| {
            format!(
                "child {} status={} stdout={:?} stderr={:?}",
                index + 1,
                outcome.status,
                outcome.stdout.trim(),
                outcome.stderr.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn file_mode(path: &Path, label: &str) -> Result<u32, String> {
    Ok(fs::metadata(path)
        .map_err(|error| format!("inspect {label}: {error}"))?
        .permissions()
        .mode()
        & 0o777)
}

fn verify_inherited_automatic_plist(
    binary: &Path,
    root: &Path,
    cleanup: &mut AutomaticLifecycleCleanup<'_>,
    plist: &Path,
    target: &str,
    loaded: bool,
) -> Result<(), String> {
    let label = plist
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or("automatic lifecycle inherited plist label is invalid")?;
    let inherited = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>{}</string>\n<key>ProgramArguments</key><array><string>/usr/bin/true</string></array>\n</dict></plist>\n",
        xml_escape(label)
    );
    if let Some(parent) = plist.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create inherited LaunchAgent directory: {error}"))?;
    }
    fs::write(plist, inherited.as_bytes())
        .map_err(|error| format!("write inherited automatic lifecycle plist: {error}"))?;
    fs::set_permissions(plist, Permissions::from_mode(0o600))
        .map_err(|error| format!("protect inherited automatic lifecycle plist: {error}"))?;
    if loaded {
        let domain = target
            .rsplit_once('/')
            .map(|(domain, _)| domain)
            .ok_or("automatic lifecycle inherited LaunchAgent target is invalid")?;
        run_bounded_status_command(
            "/bin/launchctl",
            &["bootstrap", domain, path_text(plist)?],
            AUTOMATIC_LIFECYCLE_RESTART_TIMEOUT,
            &[0],
        )?;
    }

    cleanup.connection_may_exist = true;
    let connected = run_bounded_product_command_with_env(
        binary,
        &["connect", "codex", path_text(root)?],
        AUTOMATIC_LIFECYCLE_COMMAND_TIMEOUT,
        &cleanup.environment(),
    )?;
    require_output_line(&connected, "config", "connected")?;
    require_collector_ready_or_degraded(&connected)?;
    let disconnected = run_bounded_product_command_with_env(
        binary,
        &["disconnect", "codex", path_text(root)?],
        AUTOMATIC_LIFECYCLE_COMMAND_TIMEOUT,
        &cleanup.environment(),
    )?;
    require_output_line(&disconnected, "config", "disconnected")?;
    cleanup.connection_may_exist = false;
    verify_exact_file(
        plist,
        inherited.as_bytes(),
        0o600,
        "inherited LaunchAgent plist",
    )?;
    if launch_agent_is_loaded(target)? != loaded {
        return Err(
            "automatic lifecycle did not restore inherited LaunchAgent loaded state".into(),
        );
    }
    verify_exact_file(
        &cleanup.config,
        AUTOMATIC_LIFECYCLE_SEED,
        AUTOMATIC_LIFECYCLE_SEED_MODE,
        "Codex config",
    )?;
    if loaded {
        run_bounded_status_command(
            "/bin/launchctl",
            &["bootout", target],
            AUTOMATIC_LIFECYCLE_RESTART_TIMEOUT,
            &[0],
        )?;
    }
    fs::remove_file(plist)
        .map_err(|error| format!("remove inherited automatic lifecycle plist: {error}"))?;
    Ok(())
}

fn submit_automatic_primary_otlp(root: &Path, run: usize, event: usize) -> Result<(), String> {
    let settings = load_settings(root).map_err(|error| error.to_string())?;
    let body = r#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[
      {"attributes":[
        {"key":"event.name","value":{"stringValue":"codex.api_request"}},
        {"key":"conversation.id","value":{"stringValue":"automatic-perf-$RUN"}},
        {"key":"model","value":{"stringValue":"gpt-test"}},
        {"key":"auth.request_id","value":{"stringValue":"automatic-request-$EVENT"}},
        {"key":"success","value":{"boolValue":true}}
      ]},
      {"attributes":[
        {"key":"event.name","value":{"stringValue":"codex.sse_event"}},
        {"key":"conversation.id","value":{"stringValue":"automatic-perf-$RUN"}},
        {"key":"model","value":{"stringValue":"gpt-test"}},
        {"key":"event.kind","value":{"stringValue":"response.completed"}},
        {"key":"input_token_count","value":{"intValue":"10"}},
        {"key":"output_token_count","value":{"intValue":"5"}}
      ]}
    ]}]}]}"#
        .replace("$RUN", &run.to_string())
        .replace("$EVENT", &event.to_string());
    let mut stream = TcpStream::connect_timeout(
        &(Ipv4Addr::LOCALHOST, settings.port).into(),
        LOCAL_COMMAND_TIMEOUT,
    )
    .map_err(|error| format!("connect automatic lifecycle OTLP collector: {error}"))?;
    stream
        .set_read_timeout(Some(LOCAL_COMMAND_TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(LOCAL_COMMAND_TIMEOUT)))
        .map_err(|error| format!("set automatic lifecycle OTLP timeout: {error}"))?;
    write!(
        stream,
        "POST /v1/logs HTTP/1.1\r\nHost: 127.0.0.1\r\n{TOKEN_HEADER}: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        settings.token,
        body.len()
    )
    .and_then(|()| stream.write_all(body.as_bytes()))
    .and_then(|()| stream.flush())
    .map_err(|error| format!("submit automatic lifecycle OTLP request: {error}"))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("read automatic lifecycle OTLP response: {error}"))?;
    if !response.starts_with("HTTP/1.1 200") {
        return Err("automatic lifecycle primary OTLP request was rejected".into());
    }
    Ok(())
}

fn wait_for_automatic_lifecycle_recovery(
    binary: &Path,
    cleanup: &AutomaticLifecycleCleanup<'_>,
) -> Result<(), String> {
    let started = Instant::now();
    loop {
        if let Ok(status) = run_bounded_product_command_with_env(
            binary,
            &["status", "codex"],
            AUTOMATIC_LIFECYCLE_COMMAND_TIMEOUT,
            &cleanup.environment(),
        ) && require_output_line(&status, "config", "connected").is_ok()
            && require_collector_ready_or_degraded(&status).is_ok()
        {
            return Ok(());
        }
        if started.elapsed() >= AUTOMATIC_LIFECYCLE_RECOVERY_TIMEOUT {
            return Err("automatic lifecycle collector recovery timed out".into());
        }
        sleep(Duration::from_millis(50));
    }
}

fn verify_automatic_launch_agent_plist(
    plist: &Path,
    binary: &Path,
    root: &Path,
) -> Result<(), String> {
    let bytes = fs::read(plist)
        .map_err(|error| format!("read automatic lifecycle LaunchAgent plist: {error}"))?;
    let body = std::str::from_utf8(&bytes)
        .map_err(|_| "automatic lifecycle LaunchAgent plist is not UTF-8")?;
    if !body.contains(&xml_escape(path_text(binary)?))
        || !body.contains(&xml_escape(path_text(root)?))
        || !body.contains("<string>collector-serve</string>")
    {
        return Err("automatic lifecycle LaunchAgent plist has unexpected arguments".into());
    }
    Ok(())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn require_output_line(output: &str, key: &str, expected: &str) -> Result<(), String> {
    if output_value(output, key).as_deref() == Some(expected) {
        Ok(())
    } else {
        Err(format!("automatic lifecycle expected {key}={expected}"))
    }
}

fn require_collector_ready_or_degraded(output: &str) -> Result<(), String> {
    if output_value(output, "collector")
        .is_some_and(|value| matches!(value.as_str(), "ready" | "degraded"))
    {
        Ok(())
    } else {
        Err("automatic lifecycle collector was neither ready nor degraded".into())
    }
}

fn output_value(output: &str, key: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (candidate, value) = line.split_once('=')?;
        (candidate == key).then(|| value.to_owned())
    })
}

fn verify_exact_file(
    path: &Path,
    expected_bytes: &[u8],
    expected_mode: u32,
    label: &str,
) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|error| format!("read restored {label}: {error}"))?;
    let mode = fs::metadata(path)
        .map_err(|error| format!("inspect restored {label}: {error}"))?
        .permissions()
        .mode()
        & 0o777;
    if bytes != expected_bytes || mode != expected_mode {
        return Err(format!(
            "automatic lifecycle did not exactly restore {label}"
        ));
    }
    Ok(())
}

fn launch_agent_is_loaded(target: &str) -> Result<bool, String> {
    let mut child = Command::new("/bin/launchctl")
        .args(["print", target])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("inspect automatic lifecycle LaunchAgent: {error}"))?;
    Ok(wait_for_child(&mut child, WORKER_EXIT_TIMEOUT)
        .map_err(|error| format!("wait for automatic lifecycle LaunchAgent inspection: {error}"))?
        .success())
}

fn run(config: Config) -> Result<(), String> {
    validate_protocol_contract()?;
    validate_network_surface()?;
    if config.profile == Profile::Release {
        require_clean_worktree()?;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "clock before epoch")?
        .as_nanos();
    let root = PathBuf::from("docs/evidence/local/performance").join(stamp.to_string());
    fs::create_dir_all(&root).map_err(|e| format!("create manifest directory: {e}"))?;
    let mut smoke_root_cleanup = (config.profile == Profile::Smoke)
        .then(|| DirectoryCleanup::new(root.clone(), "smoke manifest path"));
    fs::set_permissions(&root, Permissions::from_mode(0o700))
        .map_err(|e| format!("protect manifest directory: {e}"))?;
    let durable_root = if config.profile == Profile::Smoke {
        env::temp_dir().join(format!("agent-observability-perf-{stamp}"))
    } else {
        root.join("durable")
    };
    let mut durable_cleanup = DirectoryCleanup::new(durable_root.clone(), "durable evidence path");
    fs::create_dir_all(&durable_root).map_err(|e| format!("create evidence directory: {e}"))?;
    fs::set_permissions(&durable_root, Permissions::from_mode(0o700))
        .map_err(|e| format!("protect durable evidence directory: {e}"))?;
    let host = HostEvidence::collect(&root)?;
    let mut results = Vec::new();
    let mut errors = Vec::new();
    'workload: for enabled in [false, true] {
        for run_number in 1..=config.runs {
            match execute_run(config, enabled, run_number, &durable_root) {
                Ok(result) => results.push(result),
                Err(error) => {
                    errors.push(format!(
                        "{} run {run_number}: {error}",
                        if enabled { "enabled" } else { "baseline" }
                    ));
                    break 'workload;
                }
            }
        }
    }
    let manifest_path = root.join("manifest.yaml");
    let validation = validate_results(config, &results, &errors);
    let mut manifest_errors = errors.clone();
    if let Err(error) = &validation {
        manifest_errors.push(format!("validation: {error}"));
    }
    let preparation = prepare_manifest(
        &manifest_path,
        config,
        &host,
        &results,
        &manifest_errors,
        validation,
    );
    let durable_cleanup = durable_cleanup.cleanup();
    let smoke_cleanup = smoke_root_cleanup
        .as_mut()
        .map_or(Ok(()), DirectoryCleanup::cleanup);
    let cleanup = combine_cleanup(durable_cleanup, smoke_cleanup);
    complete_manifest_lifecycle(
        &manifest_path,
        config,
        &host,
        &results,
        &manifest_errors,
        preparation,
        cleanup,
    )?;
    println!(
        "manifest={}\nprofile={}\nstatus=pass",
        manifest_path.display(),
        profile_name(config.profile)
    );
    Ok(())
}

fn prepare_manifest(
    manifest_path: &Path,
    config: Config,
    host: &HostEvidence,
    results: &[RunResult],
    manifest_errors: &[String],
    validation: Result<(), String>,
) -> Result<String, String> {
    let pending_manifest = render_manifest(config, host, results, manifest_errors);
    fs::write(manifest_path, &pending_manifest)
        .map_err(|error| format!("write manifest: {error}"))?;
    if let Err(shape_error) = validate_manifest_shape(&pending_manifest) {
        let failed_manifest = render_failed_manifest(
            config,
            host,
            results,
            manifest_errors,
            &format!("manifest shape: {shape_error}"),
        );
        fs::write(manifest_path, failed_manifest)
            .map_err(|error| format!("finalize malformed manifest: {error}"))?;
        return Err(format!(
            "performance manifest validation failed; manifest: {}: {shape_error}",
            manifest_path.display()
        ));
    }
    if let Err(error) = validation {
        let failed_manifest =
            pending_manifest.replace("status: pending-validation", "status: failed");
        fs::write(manifest_path, failed_manifest)
            .map_err(|write_error| format!("finalize failed manifest: {write_error}"))?;
        return Err(format!(
            "performance check failed; manifest: {}: {error}",
            manifest_path.display()
        ));
    }
    Ok(pending_manifest)
}

fn complete_manifest_lifecycle(
    manifest_path: &Path,
    config: Config,
    host: &HostEvidence,
    results: &[RunResult],
    manifest_errors: &[String],
    preparation: Result<String, String>,
    cleanup: Result<(), String>,
) -> Result<(), String> {
    match preparation {
        Ok(pending_manifest) => {
            let cleanup_failed_manifest = cleanup.as_ref().err().map(|error| {
                render_failed_manifest(
                    config,
                    host,
                    results,
                    manifest_errors,
                    &format!("cleanup: {error}"),
                )
            });
            finalize_manifest_after_cleanup(
                config.profile,
                manifest_path,
                &pending_manifest,
                cleanup,
                cleanup_failed_manifest.as_deref(),
            )
        }
        Err(preparation_error) => match cleanup {
            Ok(()) => Err(preparation_error),
            Err(cleanup_error) => {
                let failed_manifest = render_failed_manifest(
                    config,
                    host,
                    results,
                    manifest_errors,
                    &format!("preparation: {preparation_error}; cleanup: {cleanup_error}"),
                );
                let manifest_write = if manifest_path.exists() {
                    fs::write(manifest_path, failed_manifest)
                        .map_err(|error| format!("finalize combined failure manifest: {error}"))
                } else {
                    Ok(())
                };
                combine_cleanup(
                    combine_cleanup(Err(preparation_error), Err(cleanup_error)),
                    manifest_write,
                )
            }
        },
    }
}

fn finalize_manifest_after_cleanup(
    profile: Profile,
    manifest_path: &Path,
    pending_manifest: &str,
    cleanup: Result<(), String>,
    cleanup_failed_manifest: Option<&str>,
) -> Result<(), String> {
    if let Err(error) = cleanup {
        let manifest_write = if manifest_path.exists() {
            let failed_manifest =
                cleanup_failed_manifest.ok_or("cleanup failure manifest is unavailable")?;
            fs::write(manifest_path, failed_manifest)
                .map_err(|write_error| format!("finalize cleanup failure manifest: {write_error}"))
        } else {
            Ok(())
        };
        return combine_cleanup(
            Err(format!("performance cleanup failed: {error}")),
            manifest_write,
        );
    }
    if profile == Profile::Release {
        let finalized = pending_manifest.replace("status: pending-validation", "status: pass");
        fs::write(manifest_path, finalized)
            .map_err(|error| format!("finalize manifest: {error}"))?;
    }
    Ok(())
}

fn render_failed_manifest(
    config: Config,
    host: &HostEvidence,
    results: &[RunResult],
    errors: &[String],
    final_error: &str,
) -> String {
    let mut all_errors = errors.to_vec();
    all_errors.push(final_error.into());
    render_manifest(config, host, results, &all_errors)
        .replace("status: pending-validation", "status: failed")
}

fn source_revision() -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| format!("read source revision: {error}"))?;
    if !output.status.success() {
        return Err("source revision command failed".into());
    }
    let revision = String::from_utf8(output.stdout)
        .map_err(|_| "source revision is not UTF-8".to_string())?
        .trim()
        .to_owned();
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("source revision is not a full commit SHA".into());
    }
    Ok(revision)
}

fn require_clean_worktree() -> Result<(), String> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .output()
        .map_err(|error| format!("inspect release worktree: {error}"))?;
    if !output.status.success() {
        return Err("release worktree inspection failed".into());
    }
    if !output.stdout.is_empty() {
        return Err("release performance evidence requires a clean worktree".into());
    }
    Ok(())
}

fn combine_cleanup(primary: Result<(), String>, cleanup: Result<(), String>) -> Result<(), String> {
    match (primary, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(cleanup_error)) => {
            Err(format!("{error}; cleanup failed: {cleanup_error}"))
        }
    }
}

fn validate_protocol_contract() -> Result<(), String> {
    if let Some(line) = REQUIRED_PROTOCOL_LINES
        .iter()
        .find(|line| !PROTOCOL.lines().any(|candidate| candidate.trim() == **line))
    {
        return Err(format!("embedded performance protocol is missing {line}"));
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn execute_automatic_run(
    config: AutomaticConfig,
    run: usize,
    binary: &Path,
    runtime_root: &Path,
) -> Result<AutomaticRunResult, String> {
    let root = runtime_root.join(format!("run-{run}"));
    run_bounded_product_command(binary, &["init", path_text(&root)?], LOCAL_COMMAND_TIMEOUT)?;
    let port = available_loopback_port()?;
    let settings = format!(
        "{{\n  \"schema_version\": \"local_collector.v1\",\n  \"port\": {port},\n  \"token\": \"{}\",\n  \"source_generation\": \"automatic-perf-v1\"\n}}\n",
        "a".repeat(64)
    );
    let settings_path = root.join("runtime/collector.json");
    fs::write(&settings_path, settings)
        .map_err(|error| format!("write automatic collector settings: {error}"))?;
    fs::set_permissions(&settings_path, Permissions::from_mode(0o600))
        .map_err(|error| format!("protect automatic collector settings: {error}"))?;
    let mut collector = ChildGuard(
        Command::new(binary)
            .args(["collector-serve", path_text(&root)?])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("spawn built local collector: {error}"))?,
    );
    let mut network_monitor = NetworkMonitor::start(collector.id())?;
    wait_for_automatic_ready(&root, &mut collector, run)?;
    assert_automatic_network_local(collector.id())?;

    let started = Instant::now();
    automatic_sample_phase(
        config.warmup,
        config.sample,
        &root,
        collector.id(),
        &mut network_monitor,
        started,
        "warmup",
    )?;
    let idle_samples = automatic_sample_phase(
        config.idle,
        config.sample,
        &root,
        collector.id(),
        &mut network_monitor,
        started,
        "idle",
    )?;
    let idle_cpu_percent = average_sample_cpu(&idle_samples)?;
    let active_cpu_before = process_cpu_seconds(collector.id())?;
    let active_started = Instant::now();
    let mut active_sampler =
        start_pressure_sampler(root.clone(), collector.id(), false, None, config.sample)?;
    let mut primary_otlp_latencies_us = Vec::with_capacity(config.events);
    let mut accepted_primary_requests = 0_usize;
    for event in 0..config.events {
        if active_started.elapsed() >= config.active_timeout {
            return Err(format!(
                "automatic active workload exceeded {} ms",
                config.active_timeout.as_millis()
            ));
        }
        let event_offset = u32::try_from(event).map_err(|_| "automatic event count overflow")?;
        let scheduled = active_started + config.inter_event.saturating_mul(event_offset);
        sleep(scheduled.saturating_duration_since(Instant::now()));
        let primary_started = Instant::now();
        submit_automatic_primary_otlp(&root, run, event)?;
        primary_otlp_latencies_us.push(primary_started.elapsed().as_micros());
        accepted_primary_requests = accepted_primary_requests.saturating_add(1);
        if event % 100 == 0 {
            assert_automatic_network_local(collector.id())?;
            network_monitor.sample()?;
        }
    }
    let active_elapsed = active_started.elapsed();
    let active_peaks = active_sampler.stop()?;
    let active_cpu_after = process_cpu_seconds(collector.id())?;
    let active_cpu_percent =
        interval_cpu_percent(active_cpu_before, active_cpu_after, active_elapsed);
    let notify_payload = automatic_notify_payload(run, config.events);
    let notify = run_bounded_product_command(
        binary,
        &["codex-notify", path_text(&root)?, &notify_payload],
        LOCAL_COMMAND_TIMEOUT,
    )?;
    let notify_supplement_accepted = notify.trim() == "notify=accepted";
    if !notify_supplement_accepted {
        return Err(format!(
            "notify supplement was not accepted: {}",
            notify.trim()
        ));
    }
    sleep(Duration::from_millis(250));
    assert_automatic_network_local(collector.id())?;
    network_monitor.final_sample()?;
    let peak_disk_bytes = active_peaks.disk_bytes.max(
        StorageBudget::allocated_tree_bytes(&root)
            .map_err(|error| format!("measure automatic allocated disk: {error}"))?,
    );
    let peak_rss_kib = active_peaks.rss_kib.max(ps_metric(collector.id(), "rss")?);
    let network_evidence = network_monitor.finish()?;
    collector.terminate()?;
    Ok(AutomaticRunResult {
        run,
        primary_otlp_latencies_us,
        idle_samples,
        idle_cpu_percent,
        active_cpu_percent,
        peak_rss_kib,
        peak_disk_bytes,
        collector_network_bytes: network_evidence.max_bytes,
        network_monitor_samples: network_evidence.samples,
        accepted_primary_requests,
        rejected_primary_requests: 0,
        notify_supplement_accepted,
    })
}

fn run_bounded_product_command(
    binary: &Path,
    arguments: &[&str],
    timeout: Duration,
) -> Result<String, String> {
    run_bounded_product_command_with_env(binary, arguments, timeout, &[])
}

fn run_bounded_product_command_with_env(
    binary: &Path,
    arguments: &[&str],
    timeout: Duration,
    environment: &[(&str, &Path)],
) -> Result<String, String> {
    let mut command = Command::new(binary);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut child = ChildGuard(
        command
            .spawn()
            .map_err(|error| format!("spawn built product command: {error}"))?,
    );
    let status = wait_for_child(&mut child, timeout)
        .map_err(|error| format!("wait for built product command: {error}"))?;
    let mut output = String::new();
    child
        .stdout
        .take()
        .ok_or("built product command stdout is unavailable")?
        .take(4_096)
        .read_to_string(&mut output)
        .map_err(|error| format!("read built product command output: {error}"))?;
    if !status.success() {
        let mut error = String::new();
        child
            .stderr
            .take()
            .ok_or("built product command stderr is unavailable")?
            .take(4_096)
            .read_to_string(&mut error)
            .map_err(|read_error| format!("read built product command error: {read_error}"))?;
        return Err(format!(
            "built product command failed: {status}: {}",
            error.trim()
        ));
    }
    Ok(output)
}

fn run_bounded_status_command(
    program: &str,
    arguments: &[&str],
    timeout: Duration,
    accepted_codes: &[i32],
) -> Result<String, String> {
    let mut child = ChildGuard(
        Command::new(program)
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("spawn bounded command {program}: {error}"))?,
    );
    let status = wait_for_child(&mut child, timeout)
        .map_err(|error| format!("wait for bounded command {program}: {error}"))?;
    let mut output = String::new();
    child
        .stdout
        .take()
        .ok_or("bounded command stdout is unavailable")?
        .take(4_096)
        .read_to_string(&mut output)
        .map_err(|error| format!("read bounded command output: {error}"))?;
    if !status
        .code()
        .is_some_and(|code| accepted_codes.contains(&code))
    {
        return Err(format!("bounded command {program} failed: {status}"));
    }
    Ok(output)
}

fn wait_for_automatic_ready(
    root: &Path,
    collector: &mut ChildGuard,
    run: usize,
) -> Result<(), String> {
    let started = Instant::now();
    loop {
        if collector
            .try_wait()
            .map_err(|error| format!("inspect automatic collector startup: {error}"))?
            .is_some()
        {
            return Err("built automatic collector exited during startup".into());
        }
        if submit_automatic_primary_otlp(root, run, usize::MAX).is_ok() {
            return Ok(());
        }
        if started.elapsed() >= AUTOMATIC_START_TIMEOUT {
            return Err("built automatic collector readiness timed out".into());
        }
        sleep(Duration::from_millis(20));
    }
}

fn automatic_notify_payload(run: usize, event: usize) -> String {
    format!(
        "{{\"type\":\"agent-turn-complete\",\"thread-id\":\"automatic-perf-{run}\",\"turn-id\":\"turn-{event}\"}}"
    )
}

fn path_text(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| "non-UTF8 automatic path".into())
}

fn available_loopback_port() -> Result<u16, String> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| format!("reserve automatic loopback port: {error}"))
}

fn automatic_sample_phase(
    duration: Duration,
    interval: Duration,
    root: &Path,
    pid: u32,
    network_monitor: &mut NetworkMonitor,
    started: Instant,
    phase: &str,
) -> Result<Vec<Sample>, String> {
    let phase_started = Instant::now();
    let mut previous_cpu = process_cpu_seconds(pid)?;
    let mut previous_at = Instant::now();
    let mut samples = Vec::new();
    loop {
        let remaining = duration.saturating_sub(phase_started.elapsed());
        if remaining.is_zero() && !samples.is_empty() {
            break;
        }
        sleep(interval.min(remaining));
        let current_cpu = process_cpu_seconds(pid)?;
        let current_at = Instant::now();
        assert_automatic_network_local(pid)?;
        let network_bytes = network_monitor.sample()?;
        samples.push(Sample {
            phase: phase.into(),
            elapsed_ms: started.elapsed().as_millis(),
            cpu_percent: Some(interval_cpu_percent(
                previous_cpu,
                current_cpu,
                current_at.duration_since(previous_at),
            )),
            rss_kib: Some(ps_metric(pid, "rss")?),
            disk_bytes: Some(
                StorageBudget::allocated_tree_bytes(root)
                    .map_err(|error| format!("measure automatic allocated disk: {error}"))?,
            ),
            network_bytes: Some(network_bytes),
        });
        previous_cpu = current_cpu;
        previous_at = current_at;
        if phase_started.elapsed() >= duration {
            break;
        }
    }
    Ok(samples)
}

fn average_sample_cpu(samples: &[Sample]) -> Result<f64, String> {
    let values = samples
        .iter()
        .filter_map(|sample| sample.cpu_percent)
        .collect::<Vec<_>>();
    let count = u32::try_from(values.len()).map_err(|_| "too many automatic CPU samples")?;
    if count == 0 {
        return Err("automatic CPU samples are missing".into());
    }
    Ok(values.iter().sum::<f64>() / f64::from(count))
}

#[cfg(target_os = "macos")]
fn assert_automatic_network_local(pid: u32) -> Result<(), String> {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-nP", "-a", "-p", &pid.to_string(), "-iTCP"])
        .output()
        .map_err(|error| format!("inspect automatic collector sockets: {error}"))?;
    if !output.status.success() && output.status.code() != Some(1) {
        return Err("automatic collector socket inspection failed".into());
    }
    let body = String::from_utf8(output.stdout)
        .map_err(|_| "automatic collector socket inspection is not UTF-8")?;
    for line in body.lines().skip(1) {
        let Some((_, endpoints)) = line.split_once("TCP ") else {
            continue;
        };
        let endpoints = endpoints.split_whitespace().next().unwrap_or_default();
        for endpoint in endpoints.split("->") {
            if endpoint != "*:*" && !endpoint.starts_with("127.0.0.1:") {
                return Err("automatic collector opened a non-loopback network endpoint".into());
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn assert_automatic_network_local(pid: u32) -> Result<(), String> {
    let mut sockets = Vec::new();
    for entry in fs::read_dir(format!("/proc/{pid}/fd"))
        .map_err(|error| format!("inspect automatic collector descriptors: {error}"))?
    {
        let target = fs::read_link(
            entry
                .map_err(|error| format!("inspect automatic collector descriptor: {error}"))?
                .path(),
        )
        .map_err(|error| format!("inspect automatic collector descriptor target: {error}"))?;
        let target = target.to_string_lossy();
        if let Some(inode) = target
            .strip_prefix("socket:[")
            .and_then(|value| value.strip_suffix(']'))
        {
            sockets.push(inode.to_owned());
        }
    }
    let tcp = fs::read_to_string(format!("/proc/{pid}/net/tcp"))
        .map_err(|error| format!("inspect automatic collector TCP endpoints: {error}"))?;
    for line in tcp.lines().skip(1) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 10 || !sockets.iter().any(|inode| inode == fields[9]) {
            continue;
        }
        let local = fields[1]
            .split_once(':')
            .map(|pair| pair.0)
            .unwrap_or_default();
        let remote = fields[2]
            .split_once(':')
            .map(|pair| pair.0)
            .unwrap_or_default();
        if local != "0100007F" || !matches!(remote, "00000000" | "0100007F") {
            return Err("automatic collector opened a non-loopback network endpoint".into());
        }
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn assert_automatic_network_local(_pid: u32) -> Result<(), String> {
    Err("automatic collector network evidence is unsupported on this host".into())
}

#[allow(clippy::too_many_lines)]
fn execute_run(
    config: Config,
    enabled: bool,
    run: usize,
    durable_dir: &Path,
) -> Result<RunResult, String> {
    let path = durable_dir.join(format!(
        "{}-{run}",
        if enabled { "enabled" } else { "baseline" }
    ));
    fs::create_dir_all(&path).map_err(|e| format!("create run artifact directory: {e}"))?;
    fs::set_permissions(&path, Permissions::from_mode(0o700))
        .map_err(|e| format!("protect run artifact directory: {e}"))?;
    let mut child = ChildGuard(spawn_worker(&path, enabled)?);
    let stdout = child.stdout.take().ok_or("worker stdout unavailable")?;
    let mut output = WorkerOutput::new(stdout);
    let result = execute_run_with_child(
        config,
        enabled,
        run,
        durable_dir,
        &path,
        &mut child,
        &mut output,
    );
    if let Err(error) = result {
        let worker_cleanup = child
            .terminate()
            .map_err(|cleanup| format!("worker cleanup failed: {cleanup}"));
        let output_cleanup = output.join();
        return Err(
            combine_cleanup(combine_cleanup(Err(error), worker_cleanup), output_cleanup)
                .unwrap_err(),
        );
    }
    result
}

#[allow(clippy::too_many_lines)]
fn execute_run_with_child(
    config: Config,
    enabled: bool,
    run: usize,
    durable_dir: &Path,
    path: &Path,
    child: &mut ChildGuard,
    output: &mut WorkerOutput,
) -> Result<RunResult, String> {
    let mut network_monitor = NetworkMonitor::start(child.id())?;
    let mut writer = BufWriter::new(child.stdin.take().ok_or("worker stdin unavailable")?);
    writeln!(writer, "__network_monitor_start__")
        .map_err(|e| format!("start network-monitored worker: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("flush network monitor start: {e}"))?;
    read_worker_marker(output, "worker-ready", WORKER_PROTOCOL_TIMEOUT)?;
    let started = Instant::now();
    let mut samples = Vec::new();
    sample_phase(
        &mut samples,
        "warmup",
        config.warmup,
        config.sample,
        durable_dir,
        child.id(),
        started,
        &mut network_monitor,
    )?;
    sample_phase(
        &mut samples,
        "idle",
        config.idle,
        config.sample,
        durable_dir,
        child.id(),
        started,
        &mut network_monitor,
    )?;
    let drain_marker = enabled.then(|| path.join(".drain-active"));
    let supported_cpu_before = process_cpu_seconds(child.id())?;
    let supported_started = Instant::now();
    let mut supported_sampler = start_pressure_sampler(
        durable_dir.to_path_buf(),
        child.id(),
        false,
        drain_marker.clone(),
        config.sample,
    )?;
    let supported = execute_commands(
        &mut writer,
        output,
        0,
        config.supported_events,
        SUPPORTED_INTER_EVENT_PERIOD,
    )?;
    durability_barrier(&mut writer, output)?;
    let supported_elapsed = supported_started.elapsed();
    let supported_peaks = supported_sampler.stop()?;
    let supported_cpu_after = process_cpu_seconds(child.id())?;
    let supported_rate_cpu_percent = if supported_elapsed.is_zero() {
        0.0
    } else {
        (supported_cpu_after - supported_cpu_before).max(0.0) / supported_elapsed.as_secs_f64()
            * 100.0
    };
    let (saturation, saturation_cpu_percent, saturation_peaks) = if enabled {
        let cpu_before = process_cpu_seconds(child.id())?;
        let saturation_started = Instant::now();
        let mut saturation_sampler = start_pressure_sampler(
            durable_dir.to_path_buf(),
            child.id(),
            false,
            drain_marker.clone(),
            config.sample,
        )?;
        let saturation = execute_commands(
            &mut writer,
            output,
            config.supported_events,
            config.saturation_events,
            Duration::ZERO,
        );
        let elapsed = saturation_started.elapsed();
        let peaks = saturation_sampler.stop()?;
        let saturation = saturation?;
        let cpu_after = process_cpu_seconds(child.id())?;
        let cpu_percent = if elapsed.is_zero() {
            0.0
        } else {
            (cpu_after - cpu_before).max(0.0) / elapsed.as_secs_f64() * 100.0
        };
        (saturation, cpu_percent, peaks)
    } else {
        (
            WorkloadPass {
                latencies_us: Vec::new(),
                rejected_events: 0,
            },
            0.0,
            PressurePeaks::default(),
        )
    };
    samples.push(sample(
        durable_dir,
        child.id(),
        "active",
        started.elapsed(),
        Some(supported_rate_cpu_percent),
        &mut network_monitor,
    )?);
    sample_phase(
        &mut samples,
        "active",
        config.active,
        config.sample,
        durable_dir,
        child.id(),
        started,
        &mut network_monitor,
    )?;
    let mut drain_sampler = start_pressure_sampler(
        durable_dir.to_path_buf(),
        child.id(),
        true,
        drain_marker,
        config.sample,
    )?;
    writeln!(writer, "__drain__").map_err(|e| format!("request worker drain: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("flush worker drain request: {e}"))?;
    read_worker_marker(output, "drain-start", WORKER_PROTOCOL_TIMEOUT)?;
    read_worker_marker(output, "drain-finished", WORKER_PROTOCOL_TIMEOUT)?;
    if enabled {
        drain_sampler.wait_for_drain_sample(WORKER_PROTOCOL_TIMEOUT)?;
    }
    writeln!(writer, "__drain_sampled__")
        .map_err(|e| format!("acknowledge worker drain sample: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("flush worker drain sample acknowledgement: {e}"))?;
    read_worker_marker(output, "drain-complete", WORKER_PROTOCOL_TIMEOUT)?;
    network_monitor.final_sample()?;
    writeln!(writer, "__network_monitor_release__")
        .map_err(|e| format!("release network-monitored worker: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("flush network monitor release: {e}"))?;
    drop(writer);
    let status_result = wait_for_child(child, WORKER_EXIT_TIMEOUT);
    if status_result.is_ok() {
        drain_sampler.confirm_process_exit();
    }
    let drain_result = drain_sampler.stop();
    let status = status_result?;
    output.join()?;
    let drain_peaks = drain_result?;
    if !status.success() {
        return Err(format!("worker exited with {status}"));
    }
    let network_evidence = network_monitor.finish()?;
    if enabled
        && supported_peaks.drain_sample_count
            + saturation_peaks.drain_sample_count
            + drain_peaks.drain_sample_count
            == 0
    {
        return Err("drain pressure sampler produced no sample inside the drain boundary".into());
    }
    let durable_bytes = StorageBudget::allocated_tree_bytes(durable_dir)
        .map_err(|e| format!("measure allocated durable bytes: {e}"))?;
    let durable_events = if enabled {
        LocalStore::open(path)
            .map_err(|e| format!("reopen durable store for reconciliation: {e}"))?
            .observation_count()
            .map_err(|e| format!("count durable observations: {e}"))?
    } else {
        0
    };
    if supported.latencies_us.len() != config.supported_events
        || (enabled && saturation.latencies_us.len() != config.saturation_events)
        || samples.is_empty()
    {
        return Err("required subprocess latency or resource samples are missing".into());
    }
    Ok(RunResult {
        enabled,
        run,
        events: supported.latencies_us.len(),
        rejected_events: supported.rejected_events,
        saturation_events: saturation.latencies_us.len(),
        saturation_rejected_events: saturation.rejected_events,
        durable_events,
        latencies_us: supported.latencies_us,
        saturation_latencies_us: saturation.latencies_us,
        samples,
        durable_bytes,
        supported_rate_cpu_percent: supported_rate_cpu_percent.max(supported_peaks.cpu_percent),
        saturation_cpu_percent: saturation_cpu_percent.max(saturation_peaks.cpu_percent),
        peak_rss_kib: supported_peaks
            .rss_kib
            .max(saturation_peaks.rss_kib)
            .max(drain_peaks.rss_kib),
        peak_disk_bytes: supported_peaks
            .disk_bytes
            .max(saturation_peaks.disk_bytes)
            .max(drain_peaks.disk_bytes),
        network_bytes: network_evidence.max_bytes,
        network_monitor_samples: network_evidence.samples,
    })
}

fn execute_commands(
    writer: &mut impl Write,
    output: &WorkerOutput,
    event_offset: usize,
    event_count: usize,
    inter_event_period: Duration,
) -> Result<WorkloadPass, String> {
    let mut latencies_us = Vec::with_capacity(event_count);
    let mut rejected_events = 0_usize;
    let mut next_event_at = Instant::now();
    for event in event_offset..event_offset.saturating_add(event_count) {
        sleep(next_event_at.saturating_duration_since(Instant::now()));
        next_event_at += inter_event_period;
        let (name, _) = SOURCES[event % SOURCES.len()];
        let command = format!("{name}|{}", event / SOURCES.len());
        let before = Instant::now();
        writeln!(writer, "{command}").map_err(|e| format!("write workload command: {e}"))?;
        writer
            .flush()
            .map_err(|e| format!("flush workload command: {e}"))?;
        let response = output.read(WORKER_COMMAND_TIMEOUT, "command response")?;
        match response.trim() {
            "ok" => {}
            "full" | "oversized" | "unavailable" => {
                rejected_events = rejected_events.saturating_add(1);
            }
            _ => return Err("worker returned an invalid response".into()),
        }
        latencies_us.push(before.elapsed().as_micros());
    }
    Ok(WorkloadPass {
        latencies_us,
        rejected_events,
    })
}

fn durability_barrier(writer: &mut impl Write, output: &WorkerOutput) -> Result<(), String> {
    writeln!(writer, "__durability_barrier__")
        .map_err(|e| format!("write durability barrier: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("flush durability barrier: {e}"))?;
    let response = output.read(WORKER_PROTOCOL_TIMEOUT, "durability barrier")?;
    if response.trim() != "barrier-complete" {
        return Err("worker omitted durability barrier completion".into());
    }
    Ok(())
}

struct PressureSampler {
    stop: Option<mpsc::Sender<()>>,
    result: mpsc::Receiver<Result<PressurePeaks, String>>,
    handle: Option<thread::JoinHandle<()>>,
    process_exit_confirmed: Arc<AtomicBool>,
    drain_sample_count: Arc<AtomicU64>,
}

impl Drop for PressureSampler {
    fn drop(&mut self) {
        if self.handle.is_some()
            && let Err(error) = self.stop()
        {
            eprintln!("fallback pressure sampler cleanup failed: {error}");
        }
    }
}

impl PressureSampler {
    fn confirm_process_exit(&self) {
        self.process_exit_confirmed.store(true, Ordering::Release);
    }

    fn wait_for_drain_sample(&mut self, timeout: Duration) -> Result<(), String> {
        let started = Instant::now();
        loop {
            if self.drain_sample_count.load(Ordering::Acquire) > 0 {
                return Ok(());
            }
            match self.result.try_recv() {
                Ok(result) => {
                    self.join_completed()?;
                    return match result {
                        Ok(_) => Err("pressure sampler ended before a drain sample".into()),
                        Err(error) => Err(error),
                    };
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.join_completed()?;
                    return Err("pressure sampler ended without a result".into());
                }
            }
            if started.elapsed() >= timeout {
                return Err(format!(
                    "drain pressure sample timed out after {} ms",
                    timeout.as_millis()
                ));
            }
            sleep(Duration::from_millis(5));
        }
    }

    fn stop(&mut self) -> Result<PressurePeaks, String> {
        let signal_failed = self.stop.take().is_some_and(|stop| stop.send(()).is_err());
        let result = match self.result.recv_timeout(SAMPLER_STOP_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(format!(
                    "pressure sampler stop timed out after {} ms",
                    SAMPLER_STOP_TIMEOUT.as_millis()
                ));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.join_completed()?;
                return Err("pressure sampler ended without a result".into());
            }
        };
        self.join_completed()?;
        let peaks = result?;
        if signal_failed && !peaks.process_exit_observed {
            return Err("signal pressure sampler cleanup failed".into());
        }
        Ok(peaks)
    }

    fn join_completed(&mut self) -> Result<(), String> {
        self.handle
            .take()
            .ok_or_else(|| "pressure sampler handle is missing".to_string())?
            .join()
            .map_err(|_| "pressure sampler panicked".to_string())
    }
}

fn start_pressure_sampler(
    path: PathBuf,
    pid: u32,
    allow_process_exit: bool,
    drain_marker: Option<PathBuf>,
    interval: Duration,
) -> Result<PressureSampler, String> {
    let (stop_tx, stop_rx) = mpsc::channel();
    let previous_cpu = process_cpu_seconds(pid)?;
    let previous_at = Instant::now();
    let initial_rss = ps_metric(pid, "rss")?;
    let initial_disk = StorageBudget::allocated_tree_bytes(&path)
        .map_err(|e| format!("measure initial pressure disk: {e}"))?;
    let process_exit_confirmed = Arc::new(AtomicBool::new(false));
    let sampler_exit_confirmed = Arc::clone(&process_exit_confirmed);
    let drain_sample_count = Arc::new(AtomicU64::new(0));
    let sampler_drain_sample_count = Arc::clone(&drain_sample_count);
    let (result_tx, result_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let result = (|| {
            let mut peaks = PressurePeaks {
                rss_kib: initial_rss,
                disk_bytes: initial_disk,
                ..PressurePeaks::default()
            };
            let mut previous_cpu = previous_cpu;
            let mut previous_at = previous_at;
            loop {
                let drain_active_before = drain_marker
                    .as_deref()
                    .map(marker_is_active)
                    .transpose()?
                    .unwrap_or(false);
                let Some(current_cpu) = metric_or_confirmed_exit(
                    process_cpu_seconds(pid),
                    allow_process_exit,
                    &sampler_exit_confirmed,
                )?
                else {
                    peaks.process_exit_observed = true;
                    break;
                };
                let current_at = Instant::now();
                peaks.cpu_percent = peaks.cpu_percent.max(interval_cpu_percent(
                    previous_cpu,
                    current_cpu,
                    current_at.duration_since(previous_at),
                ));
                previous_cpu = current_cpu;
                previous_at = current_at;
                let Some(current_rss) = metric_or_confirmed_exit(
                    ps_metric(pid, "rss"),
                    allow_process_exit,
                    &sampler_exit_confirmed,
                )?
                else {
                    peaks.process_exit_observed = true;
                    break;
                };
                peaks.rss_kib = peaks.rss_kib.max(current_rss);
                peaks.disk_bytes = peaks.disk_bytes.max(
                    StorageBudget::allocated_tree_bytes(&path)
                        .map_err(|e| format!("measure pressure disk peak: {e}"))?,
                );
                if drain_active_before
                    && drain_marker
                        .as_deref()
                        .map(marker_is_active)
                        .transpose()?
                        .unwrap_or(false)
                {
                    peaks.drain_sample_count = peaks.drain_sample_count.saturating_add(1);
                    sampler_drain_sample_count.fetch_add(1, Ordering::Release);
                }
                if stop_rx.recv_timeout(interval).is_ok() {
                    break;
                }
            }
            Ok(peaks)
        })();
        let _ = result_tx.send(result);
    });
    Ok(PressureSampler {
        stop: Some(stop_tx),
        result: result_rx,
        handle: Some(handle),
        process_exit_confirmed,
        drain_sample_count,
    })
}

fn write_private_marker(path: &Path, label: &str) -> Result<(), String> {
    fs::write(path, []).map_err(|e| format!("create {label}: {e}"))?;
    fs::set_permissions(path, Permissions::from_mode(0o600))
        .map_err(|e| format!("protect {label}: {e}"))
}

fn marker_is_active(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err("drain marker is not a regular file".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("inspect drain marker: {error}")),
    }
}

fn interval_cpu_percent(previous: f64, current: f64, elapsed: Duration) -> f64 {
    if elapsed.is_zero() {
        return 0.0;
    }
    (current - previous).max(0.0) / elapsed.as_secs_f64() * 100.0
}

fn process_cpu_seconds(pid: u32) -> Result<f64, String> {
    let mut value = None;
    for _ in 0..5 {
        value = ps_value(pid, "time");
        if value.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let value = value.ok_or("process CPU time is unavailable")?;
    parse_process_cpu_seconds(&value)
}

fn parse_process_cpu_seconds(value: &str) -> Result<f64, String> {
    let mut parts = value.rsplit(':');
    let seconds = parts
        .next()
        .ok_or("process CPU time has an invalid format")?
        .parse::<f64>()
        .map_err(|_| "process CPU seconds are invalid")?;
    let minutes = parts
        .next()
        .ok_or("process CPU time has an invalid format")?
        .parse::<u32>()
        .map_err(|_| "process CPU minutes are invalid")?;
    let hours_and_days = parts.next();
    let has_hours = hours_and_days.is_some();
    if parts.next().is_some() {
        return Err("process CPU time has an invalid format".into());
    }
    let (days, hours) = match hours_and_days {
        None => (0, 0),
        Some(value) => match value.split_once('-') {
            Some((days, hours)) => (
                days.parse::<u32>()
                    .map_err(|_| "process CPU days are invalid")?,
                hours
                    .parse::<u32>()
                    .map_err(|_| "process CPU hours are invalid")?,
            ),
            None => (
                0,
                value
                    .parse::<u32>()
                    .map_err(|_| "process CPU hours are invalid")?,
            ),
        },
    };
    if (has_hours && minutes >= 60) || hours >= 24 || !(0.0..60.0).contains(&seconds) {
        return Err("process CPU time components are out of range".into());
    }
    let total = f64::from(days) * 86_400.0
        + f64::from(hours) * 3_600.0
        + f64::from(minutes) * 60.0
        + seconds;
    if !total.is_finite() || total.is_sign_negative() {
        return Err("process CPU time is non-finite or negative".into());
    }
    Ok(total)
}

fn ps_metric(pid: u32, field: &str) -> Result<f64, String> {
    let value = ps_value(pid, field).ok_or_else(|| format!("process {field} is unavailable"))?;
    let value = value
        .parse::<f64>()
        .map_err(|_| format!("process {field} is invalid"))?;
    if !value.is_finite() || value.is_sign_negative() {
        return Err(format!("process {field} is non-finite or negative"));
    }
    Ok(value)
}

fn metric_or_confirmed_exit(
    metric: Result<f64, String>,
    allow_process_exit: bool,
    process_exit_confirmed: &AtomicBool,
) -> Result<Option<f64>, String> {
    match metric {
        Ok(value) => Ok(Some(value)),
        Err(error) if allow_process_exit => {
            for _ in 0..20 {
                if process_exit_confirmed.load(Ordering::Acquire) {
                    return Ok(None);
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(error)
        }
        Err(error) => Err(error),
    }
}
#[allow(clippy::too_many_arguments)]
fn sample_phase(
    samples: &mut Vec<Sample>,
    phase: &str,
    duration: Duration,
    interval: Duration,
    path: &Path,
    pid: u32,
    started: Instant,
    network_monitor: &mut NetworkMonitor,
) -> Result<(), String> {
    let phase_started = Instant::now();
    let mut previous_cpu = process_cpu_seconds(pid)?;
    let mut previous_at = Instant::now();
    let mut next_sample = previous_at + interval;
    while phase_started.elapsed() < duration {
        sleep(next_sample.saturating_duration_since(Instant::now()));
        let current_cpu = process_cpu_seconds(pid)?;
        let current_at = Instant::now();
        samples.push(sample(
            path,
            pid,
            phase,
            started.elapsed(),
            Some(interval_cpu_percent(
                previous_cpu,
                current_cpu,
                current_at.duration_since(previous_at),
            )),
            network_monitor,
        )?);
        previous_cpu = current_cpu;
        previous_at = current_at;
        next_sample += interval;
    }
    if !samples.iter().any(|s| s.phase == phase) {
        samples.push(sample(
            path,
            pid,
            phase,
            started.elapsed(),
            Some(0.0),
            network_monitor,
        )?);
    }
    Ok(())
}
fn spawn_worker(path: &Path, enabled: bool) -> Result<Child, String> {
    let exe = env::current_exe().map_err(|e| format!("locate xtask executable: {e}"))?;
    Command::new(exe)
        .args([
            "--workload-worker",
            path.to_str().ok_or("non-UTF8 durable path")?,
            if enabled { "enabled" } else { "baseline" },
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn local workload worker: {e}"))
}

fn read_worker_marker(
    output: &WorkerOutput,
    expected: &str,
    timeout: Duration,
) -> Result<(), String> {
    let marker = output.read(timeout, expected)?;
    if marker.trim() != expected {
        return Err(format!("worker omitted {expected} marker"));
    }
    Ok(())
}

fn wait_for_child(child: &mut Child, timeout: Duration) -> Result<ExitStatus, String> {
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("inspect worker exit: {error}"))?
        {
            return Ok(status);
        }
        if started.elapsed() >= timeout {
            return Err(format!(
                "worker exit timed out after {} ms",
                timeout.as_millis()
            ));
        }
        sleep(Duration::from_millis(5));
    }
}

fn worker() -> Result<(), String> {
    let mut args = env::args().skip(2);
    let path = args.next().ok_or("worker durable path missing")?;
    let enabled = args.next().is_some_and(|mode| mode == "enabled");
    let mut output = BufWriter::new(io::stdout().lock());
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    read_network_monitor_start(&mut lines)?;
    if !enabled {
        return baseline_worker(&mut lines, &mut output);
    }
    let (ingress, receiver) = Ingress::new();
    let (tx, rx) = mpsc::channel();
    let (progress_tx, progress_rx) = mpsc::channel();
    let drain_path = PathBuf::from(&path);
    let drain_marker = drain_path.join(".drain-active");
    let cpu_token = Arc::new(Mutex::new(()));
    let drain_cpu_token = Arc::clone(&cpu_token);
    thread::spawn(move || {
        let _ = tx.send(drain_with_cpu_token(
            &receiver,
            &drain_path,
            Some(&drain_cpu_token),
            Some(&progress_tx),
        ));
    });
    writeln!(output, "worker-ready").map_err(|e| format!("write worker ready: {e}"))?;
    output
        .flush()
        .map_err(|e| format!("flush worker ready: {e}"))?;
    let mut durable_events = 0_usize;
    for line in &mut lines {
        let command = line.map_err(|e| format!("read worker command: {e}"))?;
        if begin_drain_boundary(&command, &drain_marker)? {
            break;
        }
        if command == "__durability_barrier__" {
            let accepted = usize::try_from(ingress.counters.snapshot().0)
                .map_err(|_| "accepted ingress count overflow")?;
            await_durable_count(&progress_rx, &mut durable_events, accepted)?;
            writeln!(output, "barrier-complete")
                .map_err(|e| format!("write durability barrier response: {e}"))?;
            output
                .flush()
                .map_err(|e| format!("flush durability barrier response: {e}"))?;
            continue;
        }
        let _cpu_guard = cpu_token
            .lock()
            .map_err(|_| "local CPU execution token poisoned")?;
        let deadline = Instant::now() + Duration::from_millis(ENQUEUE_DEADLINE_MS);
        let outcome = enqueue_until(deadline, || {
            ingress.try_send_projected(command.len(), command.as_bytes())
        });
        let response = match outcome {
            IngressOutcome::Accepted => "ok",
            IngressOutcome::Full => "full",
            IngressOutcome::Oversized => "oversized",
            IngressOutcome::Unavailable => "unavailable",
        };
        writeln!(output, "{response}").map_err(|e| format!("write worker response: {e}"))?;
        output
            .flush()
            .map_err(|e| format!("flush worker response: {e}"))?;
    }
    drop(ingress);
    writeln!(output, "drain-start").map_err(|e| format!("write drain marker: {e}"))?;
    output
        .flush()
        .map_err(|e| format!("flush drain marker: {e}"))?;
    let drain_result = rx
        .recv()
        .map_err(|_| "local runtime drain stopped".to_string())?;
    drain_result?;
    writeln!(output, "drain-finished").map_err(|e| format!("write drain marker: {e}"))?;
    output
        .flush()
        .map_err(|e| format!("flush drain marker: {e}"))?;
    read_drain_sample_ack(&mut lines)?;
    finish_drain_boundary(&drain_marker)?;
    writeln!(output, "drain-complete").map_err(|e| format!("write drain marker: {e}"))?;
    output
        .flush()
        .map_err(|e| format!("flush drain marker: {e}"))?;
    read_network_monitor_release(&mut lines)?;
    Ok(())
}

fn baseline_worker(
    lines: &mut impl Iterator<Item = io::Result<String>>,
    output: &mut impl Write,
) -> Result<(), String> {
    writeln!(output, "worker-ready").map_err(|e| format!("write worker ready: {e}"))?;
    output
        .flush()
        .map_err(|e| format!("flush worker ready: {e}"))?;
    for line in &mut *lines {
        let command = line.map_err(|e| format!("read worker command: {e}"))?;
        if command == "__drain__" {
            break;
        }
        let response = if command == "__durability_barrier__" {
            "barrier-complete"
        } else {
            "ok"
        };
        writeln!(output, "{response}").map_err(|e| format!("write worker response: {e}"))?;
        output
            .flush()
            .map_err(|e| format!("flush worker response: {e}"))?;
    }
    writeln!(output, "drain-start").map_err(|e| format!("write drain marker: {e}"))?;
    writeln!(output, "drain-finished").map_err(|e| format!("write drain marker: {e}"))?;
    output
        .flush()
        .map_err(|e| format!("flush drain marker: {e}"))?;
    read_drain_sample_ack(lines)?;
    writeln!(output, "drain-complete").map_err(|e| format!("write drain marker: {e}"))?;
    output
        .flush()
        .map_err(|e| format!("flush drain marker: {e}"))?;
    read_network_monitor_release(lines)
}

fn read_drain_sample_ack(
    lines: &mut impl Iterator<Item = io::Result<String>>,
) -> Result<(), String> {
    let acknowledgement = lines
        .next()
        .ok_or("drain sample acknowledgement is missing")?
        .map_err(|error| format!("read drain sample acknowledgement: {error}"))?;
    if acknowledgement != "__drain_sampled__" {
        return Err("drain sample acknowledgement is invalid".into());
    }
    Ok(())
}

fn begin_drain_boundary(command: &str, marker: &Path) -> Result<bool, String> {
    if command != "__drain__" {
        return Ok(false);
    }
    write_private_marker(marker, "drain boundary")?;
    Ok(true)
}

fn finish_drain_boundary(marker: &Path) -> Result<(), String> {
    fs::remove_file(marker).map_err(|e| format!("remove drain boundary: {e}"))
}

fn read_network_monitor_start(
    lines: &mut impl Iterator<Item = io::Result<String>>,
) -> Result<(), String> {
    let start = lines
        .next()
        .ok_or("network monitor start is missing")?
        .map_err(|error| format!("read network monitor start: {error}"))?;
    if start != "__network_monitor_start__" {
        return Err("network monitor start is invalid".into());
    }
    Ok(())
}

fn read_network_monitor_release(
    lines: &mut impl Iterator<Item = io::Result<String>>,
) -> Result<(), String> {
    let release = lines
        .next()
        .ok_or("network monitor release is missing")?
        .map_err(|error| format!("read network monitor release: {error}"))?;
    if release != "__network_monitor_release__" {
        return Err("network monitor release is invalid".into());
    }
    Ok(())
}

fn enqueue_until(deadline: Instant, mut attempt: impl FnMut() -> IngressOutcome) -> IngressOutcome {
    let mut retrying = false;
    loop {
        if retrying && Instant::now() >= deadline {
            return IngressOutcome::Full;
        }
        match attempt() {
            IngressOutcome::Full => {
                retrying = true;
                sleep(retry_sleep(Instant::now(), deadline));
            }
            outcome => return outcome,
        }
    }
}

fn retry_sleep(now: Instant, deadline: Instant) -> Duration {
    deadline
        .saturating_duration_since(now)
        .min(Duration::from_micros(100))
}

#[cfg(test)]
fn drain(receiver: &std::sync::mpsc::Receiver<IngressMessage>, path: &Path) -> Result<(), String> {
    drain_with_cpu_token(receiver, path, None, None)
}

fn drain_with_cpu_token(
    receiver: &std::sync::mpsc::Receiver<IngressMessage>,
    path: &Path,
    cpu_token: Option<&Mutex<()>>,
    progress: Option<&mpsc::Sender<usize>>,
) -> Result<(), String> {
    let mut store = LocalStore::open(path).map_err(|e| format!("open local durable store: {e}"))?;
    let mut config = LocalRuntimeConfigV2::default();
    config.collection.max_batch_records = DURABLE_BATCH_RECORDS;
    config.collection.max_batch_bytes = DURABLE_BATCH_BYTES;
    let mut control = RuntimeControl::new(&config).map_err(|e| e.to_string())?;
    let mut previous = BTreeMap::new();
    let mut pending = None;
    while let Some(messages) = receive_batch(
        receiver,
        &mut pending,
        usize::from(config.collection.max_batch_records),
        usize::try_from(config.collection.max_batch_bytes).unwrap_or(usize::MAX),
    )? {
        let _cpu_guard = cpu_token
            .map(|token| {
                token
                    .lock()
                    .map_err(|_| "local CPU execution token poisoned")
            })
            .transpose()?;
        if control
            .admit(path, u64::from(config.collection.max_batch_bytes))
            .map_err(|e| e.to_string())?
            == Admission::Denied
        {
            return Err("local storage admission denied the durable batch".into());
        }
        let mut observations = Vec::with_capacity(messages.len());
        for message in messages {
            let observation = observation(
                std::str::from_utf8(&message.0).map_err(|_| "ingress payload is not UTF-8")?,
                &previous,
            )?;
            previous.insert(
                observation.source.as_str(),
                observation.source_cursor.clone(),
            );
            observations.push(observation);
        }
        store
            .ingest_batch_deferred_projection(&observations)
            .map_err(|e| format!("local durable batch commit: {e}"))?;
        if let Some(progress) = progress {
            progress
                .send(observations.len())
                .map_err(|_| "durability progress receiver stopped".to_string())?;
        }
    }
    let allocated = StorageBudget::allocated_tree_bytes(path)
        .map_err(|e| format!("measure local state: {e}"))?;
    let schedule = control.evaluate(
        0,
        PressureSample {
            resource_percent: 0,
            disk_percent: control.storage_percent(allocated),
            queue_percent: 0,
        },
    );
    if !schedule.flush_paused {
        store
            .rebuild_projection()
            .map_err(|e| format!("rebuild local projection: {e}"))?;
    }
    Ok(())
}

fn await_durable_count(
    progress: &mpsc::Receiver<usize>,
    durable_events: &mut usize,
    target: usize,
) -> Result<(), String> {
    while *durable_events < target {
        *durable_events = durable_events.saturating_add(
            progress
                .recv()
                .map_err(|_| "local runtime drain stopped before durability barrier")?,
        );
    }
    Ok(())
}

fn receive_batch(
    receiver: &std::sync::mpsc::Receiver<IngressMessage>,
    pending: &mut Option<IngressMessage>,
    max_records: usize,
    max_bytes: usize,
) -> Result<Option<Vec<IngressMessage>>, String> {
    let first = match pending.take() {
        Some(message) => message,
        None => match receiver.recv() {
            Ok(message) => message,
            Err(_) => return Ok(None),
        },
    };
    if first.0.len() > max_bytes || max_records == 0 {
        return Err("ingress message exceeds the durable batch policy".into());
    }
    let mut bytes = first.0.len();
    let mut batch = Vec::with_capacity(max_records);
    batch.push(first);
    while batch.len() < max_records {
        let message = match receiver.recv_timeout(DURABLE_BATCH_COALESCE) {
            Ok(message) => message,
            Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if bytes.saturating_add(message.0.len()) > max_bytes {
            *pending = Some(message);
            break;
        }
        bytes = bytes.saturating_add(message.0.len());
        batch.push(message);
    }
    Ok(Some(batch))
}
fn observation(
    command: &str,
    previous: &BTreeMap<&'static str, SourceCursor>,
) -> Result<SourceObservation, String> {
    let (name, cursor) = command
        .split_once('|')
        .ok_or("invalid source schedule command")?;
    let source = SOURCES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, source)| *source)
        .ok_or("unknown source schedule command")?;
    let generation = SourceGeneration::parse("performance").map_err(|e| e.to_string())?;
    let source_cursor = SourceCursor::parse(cursor).map_err(|e| e.to_string())?;
    Ok(SourceObservation {
        source,
        source_generation: generation,
        previous_source_cursor: previous.get(name).cloned(),
        source_cursor,
        observation_id: ObservationId::parse(format!("performance-{name}-{cursor}"))
            .map_err(|e| e.to_string())?,
        trace_id: TraceId::parse(format!("performance-{name}")).map_err(|e| e.to_string())?,
        span_id: SpanId::parse(format!("span-{name}-{cursor}")).map_err(|e| e.to_string())?,
        parent_span_id: None,
        correlation: CorrelationIds::default(),
        event: ObservationEvent::Turn,
        lifecycle: LifecycleState::Completed,
        timing: Timing::new(1, Some(2)).map_err(|e| e.to_string())?,
        token_usage: TokenUsage::default(),
    })
}
fn sample(
    path: &Path,
    pid: u32,
    phase: &str,
    elapsed: Duration,
    cpu_percent: Option<f64>,
    network_monitor: &mut NetworkMonitor,
) -> Result<Sample, String> {
    Ok(Sample {
        phase: phase.into(),
        elapsed_ms: elapsed.as_millis(),
        cpu_percent,
        rss_kib: Some(ps_metric(pid, "rss")?),
        disk_bytes: Some(
            StorageBudget::allocated_tree_bytes(path)
                .map_err(|e| format!("measure allocated disk: {e}"))?,
        ),
        network_bytes: Some(network_monitor.sample()?),
    })
}

#[cfg(target_os = "linux")]
fn linux_network_bytes(pid: u32) -> Result<u64, String> {
    let descriptors = fs::read_dir(format!("/proc/{pid}/fd"))
        .map_err(|e| format!("inspect process descriptors: {e}"))?;
    for descriptor in descriptors {
        let target = fs::read_link(
            descriptor
                .map_err(|e| format!("inspect process descriptor: {e}"))?
                .path(),
        )
        .map_err(|e| format!("inspect process descriptor target: {e}"))?;
        if target.to_string_lossy().starts_with("socket:[") {
            return Err(
                "worker owns a network socket; zero-byte evidence cannot be established".into(),
            );
        }
    }
    Ok(0)
}

#[cfg(target_os = "macos")]
fn read_macos_network_monitor(
    reader: impl BufRead,
    state: &Mutex<MacNetworkMonitorState>,
) -> Result<(), String> {
    for line in reader.lines() {
        let line = line.map_err(|error| format!("read process-scoped network monitor: {error}"))?;
        let mut state = state
            .lock()
            .map_err(|_| "process-scoped network monitor state is poisoned")?;
        record_macos_network_line(&mut state, &line, Instant::now())?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn record_macos_network_line(
    state: &mut MacNetworkMonitorState,
    line: &str,
    observed_at: Instant,
) -> Result<(), String> {
    let sanitized = line
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    let fields = sanitized.split_whitespace().collect::<Vec<_>>();
    if fields.len() >= 2
        && fields[fields.len() - 2] == "bytes_in"
        && fields[fields.len() - 1] == "bytes_out"
    {
        if let Some(completed_bytes) = state.pending_bytes.take() {
            state.evidence.latest_bytes = completed_bytes;
            state.evidence.max_bytes = state.evidence.max_bytes.max(completed_bytes);
            state.evidence.samples = state.evidence.samples.saturating_add(1);
            state.last_completed_at = Some(observed_at);
            state.last_completed_started_at = state.pending_started_at.take();
        }
        state.pending_bytes = Some(0);
        state.pending_started_at = Some(observed_at);
        return Ok(());
    }
    if fields.len() < 3 {
        return Err("process-scoped network monitor returned an invalid row".into());
    }
    let bytes_in = fields[fields.len() - 2]
        .parse::<u64>()
        .map_err(|_| "process-scoped network bytes_in is invalid")?;
    let bytes_out = fields[fields.len() - 1]
        .parse::<u64>()
        .map_err(|_| "process-scoped network bytes_out is invalid")?;
    let pending_bytes = state
        .pending_bytes
        .as_mut()
        .ok_or("process-scoped network monitor row arrived before its header")?;
    *pending_bytes = pending_bytes
        .checked_add(bytes_in)
        .and_then(|value| value.checked_add(bytes_out))
        .ok_or("process-scoped network byte counter overflow")?;
    Ok(())
}

#[derive(Clone, Copy)]
struct NetworkSurfacePolicy {
    path: &'static str,
    allowed_tokens: &'static [&'static str],
    requires_ipv4_loopback: bool,
}

const NETWORK_SURFACE_POLICIES: &[NetworkSurfacePolicy] = &[
    NetworkSurfacePolicy {
        path: "crates/local-ui/Cargo.toml",
        allowed_tokens: &["hyper", "tokio"],
        requires_ipv4_loopback: false,
    },
    NetworkSurfacePolicy {
        path: "crates/local-ui/src/lib.rs",
        allowed_tokens: &["std::net", "TcpListener", "TcpStream", "hyper", "tokio"],
        requires_ipv4_loopback: true,
    },
    NetworkSurfacePolicy {
        path: "crates/local-collector/Cargo.toml",
        allowed_tokens: &["tokio"],
        requires_ipv4_loopback: false,
    },
    NetworkSurfacePolicy {
        path: "crates/local-collector/src/lib.rs",
        allowed_tokens: &["std::net", "TcpListener", "TcpStream", "tokio"],
        requires_ipv4_loopback: true,
    },
    NetworkSurfacePolicy {
        path: "crates/codex-integration/src/lib.rs",
        allowed_tokens: &["std::net", "TcpListener", "TcpStream"],
        requires_ipv4_loopback: true,
    },
    NetworkSurfacePolicy {
        path: "crates/cli/Cargo.toml",
        allowed_tokens: &["tokio"],
        requires_ipv4_loopback: false,
    },
    NetworkSurfacePolicy {
        path: "crates/cli/src/main.rs",
        allowed_tokens: &["tokio"],
        requires_ipv4_loopback: false,
    },
];

const NETWORK_TOKENS: &[&str] = &[
    "std::net",
    "TcpListener",
    "TcpStream",
    "UdpSocket",
    "reqwest",
    "hyper",
    "tokio",
];

const GLOBALLY_FORBIDDEN_NETWORK_TOKENS: &[&str] = &[
    "UdpSocket",
    "reqwest",
    "hyper::client",
    "ToSocketAddrs",
    "collector_endpoint",
    "TeamIngestEnvelope",
];

fn validate_network_file(path: &Path, body: &str) -> Result<(), String> {
    let relative = path.to_string_lossy().replace('\\', "/");
    if let Some(token) = GLOBALLY_FORBIDDEN_NETWORK_TOKENS
        .iter()
        .find(|token| body.contains(**token))
    {
        return Err(format!("network surface token {token} found in {relative}"));
    }

    validate_network_urls(&relative, body)?;

    let policy = NETWORK_SURFACE_POLICIES
        .iter()
        .find(|policy| policy.path == relative);
    for token in NETWORK_TOKENS.iter().filter(|token| body.contains(**token)) {
        if !policy.is_some_and(|policy| policy.allowed_tokens.contains(token)) {
            return Err(format!("network surface token {token} found in {relative}"));
        }
    }

    if policy.is_some_and(|policy| policy.requires_ipv4_loopback)
        && !body.contains("Ipv4Addr::LOCALHOST")
    {
        return Err(format!(
            "approved network surface {relative} lacks an IPv4 loopback boundary"
        ));
    }
    for token in [
        "0.0.0.0",
        "[::]",
        "Ipv4Addr::UNSPECIFIED",
        "Ipv6Addr::UNSPECIFIED",
    ] {
        if body.contains(token) {
            return Err(format!(
                "external network destination token {token} found in {relative}"
            ));
        }
    }
    Ok(())
}

fn validate_network_urls(path: &str, body: &str) -> Result<(), String> {
    for scheme in ["http://", "https://"] {
        for (offset, _) in body.match_indices(scheme) {
            let suffix = &body[offset..];
            let end = suffix
                .find(|character: char| {
                    character.is_ascii_whitespace()
                        || matches!(character, '"' | '\'' | '<' | '>' | ')' | ']')
                })
                .unwrap_or(suffix.len());
            let url = &suffix[..end];
            let allowed_loopback = url.starts_with("http://127.0.0.1:");
            let allowed_non_destination = matches!(
                (path, url),
                (
                    "crates/local-ui/src/lib.rs",
                    "http://{host}" | "http://example.invalid"
                )
            ) || (path == "crates/codex-integration/src/lib.rs"
                && url.starts_with("http://www.apple.com/DTDs/PropertyList-1.0.dtd"))
                || (url == scheme
                    && matches!(
                        path,
                        "crates/static-report/src/lib.rs" | "crates/cli/src/main.rs"
                    ));
            if !allowed_loopback && !allowed_non_destination {
                return Err(format!(
                    "external network destination {url} found in {path}"
                ));
            }
        }
    }
    Ok(())
}

fn validate_network_surface() -> Result<(), String> {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask manifest directory has no workspace parent")?;
    let mut pending = vec![workspace_root.join("crates")];
    let mut inspected = 1_usize;
    while let Some(path) = pending.pop() {
        let metadata =
            fs::symlink_metadata(&path).map_err(|e| format!("inspect network surface: {e}"))?;
        if metadata.file_type().is_symlink() {
            return Err("network surface scan refuses symlinks".into());
        }
        if metadata.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            for entry in fs::read_dir(&path).map_err(|e| format!("scan network surface: {e}"))? {
                inspected = inspected.saturating_add(1);
                if inspected > 4_096 {
                    return Err("network surface scan exceeded its entry bound".into());
                }
                pending.push(
                    entry
                        .map_err(|e| format!("scan network surface entry: {e}"))?
                        .path(),
                );
            }
        } else if path
            .extension()
            .is_some_and(|extension| matches!(extension.to_str(), Some("rs" | "toml")))
        {
            let body =
                fs::read_to_string(&path).map_err(|e| format!("read network surface: {e}"))?;
            let relative = path
                .strip_prefix(workspace_root)
                .map_err(|_| "network surface escaped the workspace root")?;
            validate_network_file(relative, &body)?;
        }
    }
    Ok(())
}

fn filesystem_type(path: &Path) -> Result<String, String> {
    filesystem_type_platform(path)
}

#[cfg(target_os = "macos")]
fn filesystem_type_platform(path: &Path) -> Result<String, String> {
    let df = Command::new("df")
        .arg("-P")
        .arg(path)
        .output()
        .map_err(|e| format!("locate evidence filesystem device: {e}"))?;
    if !df.status.success() {
        return Err("filesystem device command failed".into());
    }
    let body = String::from_utf8(df.stdout)
        .map_err(|_| "filesystem device evidence is not UTF-8".to_string())?;
    let device = body
        .lines()
        .nth(1)
        .and_then(|line| line.split_whitespace().next())
        .ok_or("filesystem device evidence is missing")?;
    let info = Command::new("diskutil")
        .args(["info", device])
        .output()
        .map_err(|e| format!("inspect evidence filesystem: {e}"))?;
    if !info.status.success() {
        return Err("filesystem evidence command failed".into());
    }
    let body = String::from_utf8(info.stdout)
        .map_err(|_| "filesystem evidence is not UTF-8".to_string())?;
    let value = body
        .lines()
        .find_map(|line| line.trim().strip_prefix("Type (Bundle):"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("filesystem type evidence is missing")?;
    validated_evidence_token(value, "filesystem")
}

#[cfg(target_os = "linux")]
fn filesystem_type_platform(path: &Path) -> Result<String, String> {
    let output = Command::new("stat")
        .args(["-f", "-c", "%T"])
        .arg(path)
        .output()
        .map_err(|e| format!("inspect evidence filesystem: {e}"))?;
    if !output.status.success() {
        return Err("filesystem evidence command failed".into());
    }
    let value = String::from_utf8(output.stdout)
        .map_err(|_| "filesystem evidence is not UTF-8".to_string())?;
    validated_evidence_token(value.trim(), "filesystem")
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn filesystem_type_platform(_path: &Path) -> Result<String, String> {
    Err("filesystem evidence is unsupported on this host".into())
}

fn validated_evidence_token(value: &str, field: &str) -> Result<String, String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!("{field} evidence is empty or unsafe"));
    }
    Ok(value.to_owned())
}

fn power_mode() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("pmset")
            .args(["-g", "batt"])
            .output()
            .map_err(|e| format!("inspect power mode: {e}"))?;
        if !output.status.success() {
            return Err("power mode command failed".into());
        }
        let value = String::from_utf8(output.stdout)
            .map_err(|_| "power mode evidence is not UTF-8".to_string())?;
        if value.contains("AC Power") {
            return Ok("ac".into());
        }
        if value.contains("Battery Power") {
            return Ok("battery".into());
        }
        Err("power mode evidence is unavailable".into())
    }
    #[cfg(target_os = "linux")]
    {
        let root = Path::new("/sys/class/power_supply");
        match fs::metadata(root) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => return Err("power supply interface is not a directory".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok("not-applicable-no-power-supply-interface".into());
            }
            Err(error) => return Err(format!("inspect power supply interface: {error}")),
        }
        let mut has_battery = false;
        for entry in fs::read_dir(root).map_err(|e| format!("inspect power supplies: {e}"))? {
            let path = entry
                .map_err(|e| format!("inspect power supply entry: {e}"))?
                .path();
            let supply_type = fs::read_to_string(path.join("type"))
                .map_err(|e| format!("read power supply type: {e}"))?;
            if supply_type.trim() == "Battery" {
                has_battery = true;
            } else {
                let online = fs::read_to_string(path.join("online"))
                    .map_err(|e| format!("read power supply online state: {e}"))?;
                match online.trim() {
                    "1" => return Ok("ac".into()),
                    "0" => {}
                    _ => return Err("power supply online state is invalid".into()),
                }
            }
        }
        if has_battery {
            Ok("battery".into())
        } else {
            Ok("not-applicable-no-power-supply".into())
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    Err("power mode evidence is unsupported on this host".into())
}

fn ps_value(pid: u32, field: &str) -> Option<String> {
    let mut process = Command::new("ps")
        .arg("-o")
        .arg(format!("{field}="))
        .args(["-p", &pid.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let Ok(status) = wait_for_child(&mut process, LOCAL_COMMAND_TIMEOUT) else {
        let _ = process.kill();
        let _ = wait_for_child(&mut process, LOCAL_COMMAND_TIMEOUT);
        return None;
    };
    if !status.success() {
        return None;
    }
    let mut stdout = String::new();
    process.stdout.take()?.read_to_string(&mut stdout).ok()?;
    let value = stdout
        .lines()
        .find(|line| !line.trim().is_empty())?
        .trim()
        .to_owned();
    (!value.is_empty()).then_some(value)
}

#[allow(clippy::too_many_lines)]
fn validate_automatic_results(
    config: AutomaticConfig,
    results: &[AutomaticRunResult],
    errors: &[String],
) -> Result<(), String> {
    if !errors.is_empty() {
        return Err(format!("automatic workload errors: {}", errors.join("; ")));
    }
    if results.len() != config.runs {
        return Err("incomplete automatic run evidence".into());
    }
    for result in results {
        if result.accepted_primary_requests != config.events
            || result.primary_otlp_latencies_us.len() != config.events
            || result.idle_samples.is_empty()
        {
            return Err("incomplete automatic primary OTLP or idle evidence".into());
        }
        if result.rejected_primary_requests != 0 {
            return Err("automatic primary OTLP requests were rejected".into());
        }
        if !result.notify_supplement_accepted {
            return Err("automatic notify supplement evidence is missing".into());
        }
        if !result.idle_cpu_percent.is_finite()
            || result.idle_cpu_percent.is_sign_negative()
            || !result.active_cpu_percent.is_finite()
            || result.active_cpu_percent.is_sign_negative()
            || !result.peak_rss_kib.is_finite()
            || result.peak_rss_kib.is_sign_negative()
        {
            return Err("automatic resource evidence is non-finite or negative".into());
        }
        if result.network_monitor_samples == 0 {
            return Err("automatic collector network monitor samples are missing".into());
        }
        if config.profile == Profile::Release {
            let mut latencies = result.primary_otlp_latencies_us.clone();
            latencies.sort_unstable();
            if percentile(&latencies, 95) > u128::from(AUTOMATIC_PRIMARY_OTLP_P95_MS_MAX) * 1_000 {
                return Err(format!(
                    "automatic run {} primary OTLP p95 latency budget exceeded",
                    result.run
                ));
            }
            if percentile(&latencies, 99) > u128::from(AUTOMATIC_PRIMARY_OTLP_P99_MS_MAX) * 1_000 {
                return Err(format!(
                    "automatic run {} primary OTLP p99 latency budget exceeded",
                    result.run
                ));
            }
        }
    }
    if config.profile == Profile::Release {
        if results
            .iter()
            .any(|result| result.idle_cpu_percent > AUTOMATIC_IDLE_CPU_PERCENT_MAX)
        {
            return Err("automatic collector idle CPU budget exceeded".into());
        }
        if results
            .iter()
            .any(|result| result.active_cpu_percent > AUTOMATIC_ACTIVE_CPU_PERCENT_MAX)
        {
            return Err("automatic collector active CPU exceeded one logical core".into());
        }
        if results
            .iter()
            .any(|result| result.peak_rss_kib > AUTOMATIC_PEAK_RSS_MIB_MAX * 1024.0)
        {
            return Err("automatic collector RSS budget exceeded".into());
        }
        if results
            .iter()
            .any(|result| result.peak_disk_bytes > AUTOMATIC_ALLOCATED_DISK_BYTES_MAX)
        {
            return Err("automatic allocated disk budget exceeded".into());
        }
    }
    Ok(())
}

fn render_automatic_manifest(
    config: AutomaticConfig,
    host: &HostEvidence,
    source_revision: &str,
    results: &[AutomaticRunResult],
    errors: &[String],
    status: &str,
) -> String {
    let mut latencies = results
        .iter()
        .flat_map(|result| result.primary_otlp_latencies_us.iter().copied())
        .collect::<Vec<_>>();
    latencies.sort_unstable();
    let p95 = (!latencies.is_empty()).then(|| percentile(&latencies, 95));
    let p99 = (!latencies.is_empty()).then(|| percentile(&latencies, 99));
    let idle_cpu = results
        .iter()
        .map(|result| result.idle_cpu_percent)
        .max_by(f64::total_cmp);
    let active_cpu = results
        .iter()
        .map(|result| result.active_cpu_percent)
        .max_by(f64::total_cmp);
    let peak_rss = results
        .iter()
        .map(|result| result.peak_rss_kib)
        .max_by(f64::total_cmp);
    let peak_disk = results.iter().map(|result| result.peak_disk_bytes).max();
    let collector_network = results
        .iter()
        .map(|result| result.collector_network_bytes)
        .max();
    let network_samples = results
        .iter()
        .map(|result| result.network_monitor_samples)
        .sum::<u64>();
    let mut manifest = format!(
        "schema_version: automatic_local_performance.v1\nprotocol_revision: {AUTOMATIC_PROTOCOL_REVISION}\nstatus: {status}\nsource_revision: {source_revision}\nprofile: {}\nprotocol: crates/contracts/performance/automatic-local-performance-v1.yaml\ncommand: cargo run -p xtask -- perf automatic --profile {} --check\nbuild:\n  package: agent-observability-cli\n  package_version: {}\n  cargo_locked: true\n  cargo_profile: {}\nhost:\n  machine: {}\n  os: {}\n  filesystem: {}\n  power_mode: {}\n  logical_cores: {}\nworkload:\n  lifecycle_preflight: built-binary-isolated-home-codex-home-setup-sigkill-recovery-reconnect-concurrency-missing-settings-inherited-plist-disconnect\n  collector_boundary: built-agent-observability-collector-serve-subprocess\n  primary_boundary: sustained-authenticated-codex-otlp-http-v1-logs-through-built-collector-and-durable-report\n  notify_boundary: separately-verified-built-agent-observability-codex-notify-supplement\n  runtime_path: run-relative/runtime\n  warmup_ms: {}\n  idle_ms: {}\n  primary_otlp_requests_per_run: {}\n  inter_request_ms: {}\n  runs: {}\n  sample_interval_ms: {}\n  active_timeout_ms: {}\n  startup_timeout_ms: {}\n  command_timeout_ms: {}\n  cleanup_timeout_ms: {}\nmetrics:\n  primary_otlp_p95_us: {}\n  primary_otlp_p99_us: {}\n  collector_idle_cpu_percent_max: {}\n  collector_active_cpu_percent_max: {}\n  collector_peak_rss_kib: {}\n  allocated_disk_bytes_max: {}\n  collector_network_bytes_max: {}\n  network_monitor_samples: {}\n  all_observed_endpoints_loopback: true\n  network_evidence: process-network-monitor-plus-independent-socket-endpoint-scan-plus-static-product-surface\nruns:\n",
        profile_name(config.profile),
        profile_name(config.profile),
        env!("CARGO_PKG_VERSION"),
        if config.profile == Profile::Release {
            "release"
        } else {
            "dev"
        },
        host.machine,
        env::consts::OS,
        host.filesystem,
        host.power_mode,
        host.logical_cores,
        config.warmup.as_millis(),
        config.idle.as_millis(),
        config.events,
        config.inter_event.as_millis(),
        config.runs,
        config.sample.as_millis(),
        config.active_timeout.as_millis(),
        AUTOMATIC_START_TIMEOUT.as_millis(),
        LOCAL_COMMAND_TIMEOUT.as_millis(),
        WORKER_EXIT_TIMEOUT.as_millis(),
        optional_u128(p95),
        optional_u128(p99),
        optional_f64(idle_cpu),
        optional_f64(active_cpu),
        optional_f64(peak_rss),
        optional_u64(peak_disk),
        optional_u64(collector_network),
        network_samples,
    );
    for result in results {
        let mut run_latencies = result.primary_otlp_latencies_us.clone();
        run_latencies.sort_unstable();
        let _ = writeln!(
            manifest,
            "  - run: {}\n    accepted_primary_otlp_requests: {}\n    rejected_primary_otlp_requests: {}\n    notify_supplement_accepted: {}\n    primary_otlp_p95_us: {}\n    primary_otlp_p99_us: {}\n    idle_cpu_percent: {}\n    active_cpu_percent: {}\n    peak_rss_kib: {}\n    allocated_disk_bytes: {}\n    collector_network_bytes: {}\n    network_monitor_samples: {}\n    all_observed_endpoints_loopback: true",
            result.run,
            result.accepted_primary_requests,
            result.rejected_primary_requests,
            result.notify_supplement_accepted,
            percentile(&run_latencies, 95),
            percentile(&run_latencies, 99),
            result.idle_cpu_percent,
            result.active_cpu_percent,
            result.peak_rss_kib,
            result.peak_disk_bytes,
            result.collector_network_bytes,
            result.network_monitor_samples,
        );
    }
    if errors.is_empty() {
        manifest.push_str("errors: []\n");
    } else {
        manifest.push_str("errors:\n");
        for error in errors {
            let _ = writeln!(manifest, "  - {}", yaml_single_quoted(error));
        }
    }
    manifest
}

fn validate_automatic_manifest_shape(manifest: &str) -> Result<(), String> {
    for field in [
        "schema_version: automatic_local_performance.v1",
        "protocol_revision: v1.3.0-sustained-primary-otlp-crash-lifecycle-scenarios",
        "source_revision:",
        "cargo_locked: true",
        "collector_boundary: built-agent-observability-collector-serve-subprocess",
        "primary_boundary: sustained-authenticated-codex-otlp-http-v1-logs-through-built-collector-and-durable-report",
        "notify_boundary: separately-verified-built-agent-observability-codex-notify-supplement",
        "primary_otlp_p95_us:",
        "primary_otlp_p99_us:",
        "collector_idle_cpu_percent_max:",
        "collector_active_cpu_percent_max:",
        "collector_peak_rss_kib:",
        "allocated_disk_bytes_max:",
        "collector_network_bytes_max:",
        "network_monitor_samples:",
        "all_observed_endpoints_loopback: true",
        "lifecycle_preflight: built-binary-isolated-home-codex-home-setup-sigkill-recovery-reconnect-concurrency-missing-settings-inherited-plist-disconnect",
        "runtime_path: run-relative/runtime",
    ] {
        if !manifest.contains(field) {
            return Err(format!("automatic performance manifest is missing {field}"));
        }
    }
    for forbidden in ["/tmp/", "/private/tmp/", "collector.json", "\"token\""] {
        if manifest.contains(forbidden) {
            return Err(format!(
                "automatic performance manifest contains unsanitized field {forbidden}"
            ));
        }
    }
    let revision = manifest
        .lines()
        .find_map(|line| line.strip_prefix("source_revision: "))
        .ok_or("automatic performance manifest source revision is missing")?;
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("automatic performance manifest source revision is invalid".into());
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn render_manifest(
    config: Config,
    host: &HostEvidence,
    results: &[RunResult],
    errors: &[String],
) -> String {
    let all = results
        .iter()
        .filter(|result| result.enabled)
        .flat_map(|result| {
            result
                .latencies_us
                .iter()
                .chain(&result.saturation_latencies_us)
                .copied()
        })
        .collect::<Vec<_>>();
    let mut out = format!(
        "schema_version: local_performance.v1\nprotocol_revision: v1.2.0-supported-rate-saturation-continuous-network\nsource_revision: {}\nprofile: {}\nprotocol: crates/contracts/performance/local-performance-v1.yaml\nstatus: pending-validation\nmachine: {}\nos: {}\nfilesystem: {}\npower_mode: {}\ncold_warm_cache: warm-after-build-and-per-run-warmup\nlogical_cores: {}\nsource_versions:\n  product: {}\n  runtime_config: local_runtime.v2\n  durable_store: local_state.v4\nbaseline:\n  runs: {}\nenabled:\n  runs: {}\nworkload:\n  warmup_seconds: {}\n  idle_seconds: {}\n  active_seconds: {}\n  supported_rate_events: {}\n  supported_inter_event_ms: {}\n  saturation_events: {}\n  sample_interval_seconds: {}\n  adapters: [codex, claude-code, cursor]\n  schedule: round-robin-codex-claude-code-cursor\n  supported_rate_schedule: symmetric-driver-paced\n  supported_rate_durability_barrier: required-before-saturation\n  supported_rate_measurement_boundary: first-command-through-barrier-completion\n  saturation_schedule: enabled-unpaced\n  channel_capacity: 64\n  normalization_workers: 1\n  durable_batch_records: {DURABLE_BATCH_RECORDS}\n  durable_handoff_bytes_max: {DURABLE_HANDOFF_BYTES_MAX}\n  total_pipeline_payload_bytes_max: {TOTAL_PIPELINE_PAYLOAD_BYTES_MAX}\n  enqueue_deadline_ms: 10\n  command_boundary: fixed-capacity-local-runtime-ingress\n  worker_boundary: one-bounded-batch-local-store-drain-actor\n  foreground_response: bounded-enqueue-acceptance\n  durable_path: run-relative/durable\n  durable_path_lifecycle: removed-after-measurement\nall_run_samples:\n",
        host.source_revision,
        profile_name(config.profile),
        host.machine,
        env::consts::OS,
        host.filesystem,
        host.power_mode,
        host.logical_cores,
        env!("CARGO_PKG_VERSION"),
        config.runs,
        config.runs,
        config.warmup.as_secs_f64(),
        config.idle.as_secs_f64(),
        config.active.as_secs_f64(),
        config.supported_events,
        SUPPORTED_INTER_EVENT_PERIOD.as_millis(),
        config.saturation_events,
        config.sample.as_secs_f64(),
    );
    for result in results {
        let _ = writeln!(
            out,
            "  - mode: {}\n    run: {}\n    supported_rate_attempted_events: {}\n    supported_rate_enqueued_events: {}\n    supported_rate_rejected_events: {}\n    saturation_attempted_events: {}\n    saturation_enqueued_events: {}\n    saturation_rejected_events: {}\n    durable_events: {}\n    durable_bytes: {}\n    supported_rate_cpu_percent: {}\n    saturation_cpu_percent: {}\n    peak_rss_kib: {}\n    peak_disk_bytes: {}\n    network_bytes: {}\n    network_monitor_samples: {}\n    hook_latency_us: {:?}\n    saturation_hook_latency_us: {:?}\n    samples:",
            if result.enabled {
                "enabled"
            } else {
                "baseline"
            },
            result.run,
            result.events,
            result.events.saturating_sub(result.rejected_events),
            result.rejected_events,
            result.saturation_events,
            result
                .saturation_events
                .saturating_sub(result.saturation_rejected_events),
            result.saturation_rejected_events,
            result.durable_events,
            result.durable_bytes,
            result.supported_rate_cpu_percent,
            result.saturation_cpu_percent,
            result.peak_rss_kib,
            result.peak_disk_bytes,
            result.network_bytes,
            result.network_monitor_samples,
            result.latencies_us,
            result.saturation_latencies_us
        );
        for sample in &result.samples {
            let _ = writeln!(
                out,
                "      - phase: {}\n        elapsed_ms: {}\n        cpu_percent: {}\n        rss_kib: {}\n        disk_bytes: {}\n        network_bytes: {}",
                sample.phase,
                sample.elapsed_ms,
                optional_f64(sample.cpu_percent),
                optional_f64(sample.rss_kib),
                optional_u64(sample.disk_bytes),
                optional_u64(sample.network_bytes)
            );
        }
    }
    let p95 = if all.is_empty() {
        None
    } else {
        let mut values = all.clone();
        values.sort_unstable();
        Some(percentile(&values, 95))
    };
    let p99 = if all.is_empty() {
        None
    } else {
        let mut values = all;
        values.sort_unstable();
        Some(percentile(&values, 99))
    };
    let summary = if errors.is_empty() {
        metric_summary(results)
    } else {
        None
    };
    let supported_rate_cpu_percent_max = results
        .iter()
        .filter(|result| result.enabled)
        .map(|result| result.supported_rate_cpu_percent)
        .max_by(f64::total_cmp);
    let saturation_cpu_percent_max = results
        .iter()
        .filter(|result| result.enabled)
        .map(|result| result.saturation_cpu_percent)
        .max_by(f64::total_cmp);
    let _ = writeln!(
        out,
        "phase_metrics:\n  idle_average_cpu_delta_percent: {}\n  active_average_cpu_delta_percent: {}\n  active_any_minute_cpu_delta_percent: {}\nmetrics:\n  hook_latency_p95_us: {}\n  hook_latency_p99_us: {}\n  supported_rate_cpu_percent_max: {}\n  saturation_cpu_percent_max: {}\n  idle_average_cpu_delta_percent: {}\n  active_average_cpu_delta_percent: {}\n  active_any_minute_cpu_delta_percent: {}\n  enabled_rss_p95_kib: {}\n  total_allocated_disk_bytes: {}\n  network_bytes: {}\n  network_static_surface: pass\n  required: [hook_latency_p95_us, hook_latency_p99_us, supported_rate_cpu_percent_max, saturation_cpu_percent_max, idle_average_cpu_delta_percent, active_average_cpu_delta_percent, active_any_minute_cpu_delta_percent, enabled_rss_p95_kib, total_allocated_disk_bytes, network_bytes, network_static_surface]\n  network_mode: {}\n  evidence_scope: subprocess-plus-fixed-capacity-ingress-plus-one-token-local-store-drain",
        summary.map_or_else(
            || "null".into(),
            |value| value.idle_cpu_delta_percent.to_string()
        ),
        summary.map_or_else(
            || "null".into(),
            |value| value.active_cpu_delta_percent.to_string()
        ),
        summary.map_or_else(
            || "null".into(),
            |value| value.active_any_minute_cpu_delta_percent.to_string()
        ),
        optional_u128(p95),
        optional_u128(p99),
        optional_f64(supported_rate_cpu_percent_max),
        optional_f64(saturation_cpu_percent_max),
        summary.map_or_else(
            || "null".into(),
            |value| value.idle_cpu_delta_percent.to_string()
        ),
        summary.map_or_else(
            || "null".into(),
            |value| value.active_cpu_delta_percent.to_string()
        ),
        summary.map_or_else(
            || "null".into(),
            |value| value.active_any_minute_cpu_delta_percent.to_string()
        ),
        summary.map_or_else(
            || "null".into(),
            |value| value.enabled_rss_p95_kib.to_string()
        ),
        summary.map_or_else(
            || "null".into(),
            |value| value.total_allocated_disk_bytes.to_string()
        ),
        summary.map_or_else(|| "null".into(), |value| value.network_bytes.to_string()),
        network_evidence_mode(env::consts::OS)
    );
    if errors.is_empty() {
        out.push_str("errors: []\n");
    } else {
        out.push_str("errors:\n");
        for error in errors {
            let _ = writeln!(out, "  - {}", yaml_single_quoted(error));
        }
    }
    out
}

fn network_evidence_mode(os: &str) -> &'static str {
    match os {
        "macos" => "continuous-process-lifetime-monitor-plus-static-product-surface",
        "linux" => "point-in-time-socket-descriptor-scan-plus-static-product-surface",
        _ => "unsupported-process-scoped-evidence-plus-static-product-surface",
    }
}

fn yaml_single_quoted(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .replace('\'', "''");
    format!("'{sanitized}'")
}

fn validate_manifest_shape(manifest: &str) -> Result<(), String> {
    for field in [
        "protocol_revision:",
        "source_revision:",
        "machine:",
        "os:",
        "filesystem:",
        "power_mode:",
        "cold_warm_cache:",
        "logical_cores:",
        "source_versions:",
        "workload:",
        "phase_metrics:",
        "all_run_samples:",
        "baseline:",
        "enabled:",
    ] {
        if !manifest.lines().any(|line| line.starts_with(field)) {
            return Err(format!(
                "performance manifest is missing required field {field}"
            ));
        }
    }
    let source_revision = manifest
        .lines()
        .find_map(|line| line.strip_prefix("source_revision: "))
        .ok_or("performance manifest is missing source revision")?;
    if source_revision.len() != 40 || !source_revision.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("performance manifest source revision is invalid".into());
    }
    for invalid in [
        "machine: sanitized-local-host",
        "filesystem: local-filesystem",
        "power_mode: unspecified",
        "durable_path: removed-after-measurement",
    ] {
        if manifest.lines().any(|line| line.trim() == invalid) {
            return Err(format!(
                "performance manifest contains placeholder evidence {invalid}"
            ));
        }
    }
    if !manifest
        .lines()
        .any(|line| line.trim() == "durable_path: run-relative/durable")
    {
        return Err("performance manifest is missing sanitized durable path evidence".into());
    }
    let mut lines = manifest.lines();
    let errors = lines
        .find(|line| line.starts_with("errors:"))
        .ok_or("performance manifest is missing errors evidence")?;
    if errors != "errors: []" {
        for line in lines {
            let value = line
                .strip_prefix("  - ")
                .ok_or("performance manifest error entry has invalid indentation")?;
            validate_yaml_single_quoted(value)?;
        }
    }
    Ok(())
}

fn validate_yaml_single_quoted(value: &str) -> Result<(), String> {
    let inner = value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
        .ok_or("performance manifest error entry is not single-quoted")?;
    let mut characters = inner.chars().peekable();
    while let Some(character) = characters.next() {
        if character.is_control() {
            return Err("performance manifest error entry contains a control character".into());
        }
        if character == '\'' && characters.next() != Some('\'') {
            return Err("performance manifest error entry has an unescaped quote".into());
        }
    }
    Ok(())
}
fn optional_f64(value: Option<f64>) -> String {
    value.map_or_else(|| "null".into(), |v| v.to_string())
}
fn optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "null".into(), |v| v.to_string())
}
fn optional_u128(value: Option<u128>) -> String {
    value.map_or_else(|| "null".into(), |v| v.to_string())
}

fn metric_summary(results: &[RunResult]) -> Option<MetricSummary> {
    let baseline = results
        .iter()
        .filter(|result| !result.enabled)
        .collect::<Vec<_>>();
    let enabled = results
        .iter()
        .filter(|result| result.enabled)
        .collect::<Vec<_>>();
    if baseline.is_empty() || enabled.is_empty() {
        return None;
    }
    let baseline_idle = average_phase_cpu(&baseline, "idle")?;
    let baseline_active = average_phase_cpu(&baseline, "active")?;
    let enabled_idle = average_phase_cpu(&enabled, "idle")?;
    let enabled_active = average_phase_cpu(&enabled, "active")?;
    let active_any_minute = enabled
        .iter()
        .filter_map(|result| max_minute_cpu(result, "active"))
        .max_by(f64::total_cmp)?;
    let rss = enabled
        .iter()
        .flat_map(|result| result.samples.iter().filter_map(|sample| sample.rss_kib))
        .collect::<Vec<_>>();
    let enabled_rss_p95_kib = (!rss.is_empty()).then(|| percentile_f64(&rss, 95))?;
    let sampled_disk_bytes = enabled
        .iter()
        .flat_map(|result| result.samples.iter().filter_map(|sample| sample.disk_bytes))
        .max()?;
    let final_or_peak_disk_bytes = enabled
        .iter()
        .map(|result| result.durable_bytes.max(result.peak_disk_bytes))
        .max()?;
    let total_allocated_disk_bytes = sampled_disk_bytes.max(final_or_peak_disk_bytes);
    let network_bytes = results.iter().map(|result| result.network_bytes).max()?;
    Some(MetricSummary {
        idle_cpu_delta_percent: enabled_idle - baseline_idle,
        active_cpu_delta_percent: enabled_active - baseline_active,
        active_any_minute_cpu_delta_percent: active_any_minute - baseline_active,
        enabled_rss_p95_kib: enabled_rss_p95_kib.max(
            enabled
                .iter()
                .map(|result| result.peak_rss_kib)
                .max_by(f64::total_cmp)?,
        ),
        total_allocated_disk_bytes,
        network_bytes,
    })
}

fn average_phase_cpu(results: &[&RunResult], phase: &str) -> Option<f64> {
    let values = results
        .iter()
        .flat_map(|result| {
            result
                .samples
                .iter()
                .filter(|sample| sample.phase == phase)
                .filter_map(|sample| sample.cpu_percent)
        })
        .collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    let count = u32::try_from(values.len()).ok()?;
    Some(values.iter().sum::<f64>() / f64::from(count))
}

fn max_minute_cpu(result: &RunResult, phase: &str) -> Option<f64> {
    let samples = result
        .samples
        .iter()
        .filter(|sample| sample.phase == phase)
        .collect::<Vec<_>>();
    let first = samples.first()?.elapsed_ms;
    let mut buckets = BTreeMap::<u128, (f64, u32)>::new();
    for sample in samples {
        let cpu = sample.cpu_percent?;
        let bucket = sample.elapsed_ms.saturating_sub(first) / 60_000;
        let entry = buckets.entry(bucket).or_default();
        entry.0 += cpu;
        entry.1 = entry.1.saturating_add(1);
    }
    buckets
        .values()
        .map(|(sum, count)| *sum / f64::from(*count))
        .max_by(f64::total_cmp)
}

#[allow(clippy::too_many_lines)]
fn validate_results(
    config: Config,
    results: &[RunResult],
    errors: &[String],
) -> Result<(), String> {
    if !errors.is_empty() {
        return Err(format!("workload errors: {}", errors.join("; ")));
    }
    if results.len() != config.runs * 2 {
        return Err("incomplete baseline/enabled evidence".into());
    }
    if results.iter().any(|result| {
        result.events != config.supported_events
            || result.latencies_us.is_empty()
            || result.samples.is_empty()
            || (result.enabled
                && (result.saturation_events != config.saturation_events
                    || result.saturation_latencies_us.is_empty()))
            || (!result.enabled
                && (result.saturation_events != 0 || !result.saturation_latencies_us.is_empty()))
    }) {
        return Err("incomplete event, latency, or sample evidence".into());
    }
    for result in results {
        if result.enabled {
            let supported_enqueued = result.events.saturating_sub(result.rejected_events);
            let saturation_enqueued = result
                .saturation_events
                .saturating_sub(result.saturation_rejected_events);
            let enqueued = supported_enqueued.saturating_add(saturation_enqueued);
            if result.durable_events != u64::try_from(enqueued).unwrap_or(u64::MAX)
                || result.rejected_events.saturating_mul(100) > result.events
                || result.saturation_rejected_events.saturating_mul(100) > result.saturation_events
            {
                return Err("enabled event reconciliation or rejection budget failed".into());
            }
        } else if result.rejected_events != 0
            || result.saturation_rejected_events != 0
            || result.durable_events != 0
        {
            return Err("baseline event accounting is invalid".into());
        }
        for phase in ["warmup", "idle", "active"] {
            if !result.samples.iter().any(|s| s.phase == phase) {
                return Err(format!("missing {phase} samples"));
            }
        }
        if result.network_monitor_samples == 0
            || result.samples.iter().any(|s| {
                s.cpu_percent.is_none()
                    || s.rss_kib.is_none()
                    || s.cpu_percent
                        .is_some_and(|value| !value.is_finite() || value.is_sign_negative())
                    || s.rss_kib
                        .is_some_and(|value| !value.is_finite() || value.is_sign_negative())
                    || s.disk_bytes.is_none()
                    || (config.profile == Profile::Release && s.network_bytes.is_none())
            })
        {
            return Err("required resource metric is missing".into());
        }
        if !result.supported_rate_cpu_percent.is_finite()
            || result.supported_rate_cpu_percent.is_sign_negative()
            || !result.saturation_cpu_percent.is_finite()
            || result.saturation_cpu_percent.is_sign_negative()
            || !result.peak_rss_kib.is_finite()
            || result.peak_rss_kib.is_sign_negative()
        {
            return Err("required peak metric is non-finite or negative".into());
        }
    }
    if config.profile == Profile::Release {
        let summary = metric_summary(results)
            .ok_or_else(|| "required phase metrics are missing".to_string())?;
        if summary.idle_cpu_delta_percent > 0.5 {
            return Err("idle average CPU delta budget exceeded".into());
        }
        if summary.active_cpu_delta_percent > 2.0 {
            return Err("active average CPU delta budget exceeded".into());
        }
        if summary.active_any_minute_cpu_delta_percent > 5.0 {
            return Err("active any-minute CPU delta budget exceeded".into());
        }
        if results
            .iter()
            .filter(|result| result.enabled)
            .any(|result| result.supported_rate_cpu_percent > 100.0)
        {
            return Err("supported-rate CPU exceeded one logical core".into());
        }
        if summary.enabled_rss_p95_kib > 96.0 * 1024.0 {
            return Err("enabled RSS p95 budget exceeded".into());
        }
        if summary.total_allocated_disk_bytes > 1_073_741_824 {
            return Err("allocated disk budget exceeded".into());
        }
        let latencies = results
            .iter()
            .filter(|result| result.enabled)
            .flat_map(|result| {
                result
                    .latencies_us
                    .iter()
                    .chain(&result.saturation_latencies_us)
                    .copied()
            })
            .collect::<Vec<_>>();
        if percentile(&latencies, 95) > 20_000 || percentile(&latencies, 99) > 50_000 {
            return Err("foreground latency budget exceeded".into());
        }
        if summary.network_bytes != 0 {
            return Err("network evidence is not zero".into());
        }
    }
    Ok(())
}
fn percentile(values: &[u128], p: usize) -> u128 {
    values[((values.len() - 1) * p) / 100]
}
fn percentile_f64(values: &[f64], p: usize) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() - 1) * p) / 100]
}
fn profile_name(profile: Profile) -> &'static str {
    match profile {
        Profile::Release => "release",
        Profile::Smoke => "smoke",
    }
}
fn sleep(duration: Duration) {
    if !duration.is_zero() {
        thread::sleep(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(
        phase: &str,
        cpu: Option<f64>,
        rss: Option<f64>,
        disk: Option<u64>,
        net: Option<u64>,
    ) -> Sample {
        Sample {
            phase: phase.into(),
            elapsed_ms: 1,
            cpu_percent: cpu,
            rss_kib: rss,
            disk_bytes: disk,
            network_bytes: net,
        }
    }
    fn r(enabled: bool, samples: Vec<Sample>) -> RunResult {
        let network_bytes = samples
            .iter()
            .filter_map(|sample| sample.network_bytes)
            .max()
            .unwrap_or_default();
        RunResult {
            enabled,
            run: 1,
            events: 1,
            rejected_events: 0,
            saturation_events: usize::from(enabled),
            saturation_rejected_events: 0,
            durable_events: u64::from(enabled) * 2,
            latencies_us: vec![1],
            saturation_latencies_us: enabled.then_some(1).into_iter().collect(),
            samples,
            durable_bytes: 1,
            supported_rate_cpu_percent: 0.1,
            saturation_cpu_percent: 0.1,
            peak_rss_kib: 1.0,
            peak_disk_bytes: 1,
            network_bytes,
            network_monitor_samples: 1,
        }
    }
    fn c() -> Config {
        Config {
            profile: Profile::Release,
            warmup: Duration::ZERO,
            idle: Duration::ZERO,
            active: Duration::ZERO,
            supported_events: 1,
            saturation_events: 1,
            runs: 1,
            sample: Duration::ZERO,
        }
    }
    fn pair(enabled: Vec<Sample>) -> Vec<RunResult> {
        vec![
            r(
                false,
                vec![
                    s("warmup", Some(0.1), Some(1.0), Some(1), Some(0)),
                    s("idle", Some(0.1), Some(1.0), Some(1), Some(0)),
                    s("active", Some(0.1), Some(1.0), Some(1), Some(0)),
                ],
            ),
            r(true, enabled),
        ]
    }
    fn host() -> HostEvidence {
        HostEvidence {
            source_revision: "0123456789abcdef0123456789abcdef01234567".into(),
            machine: "sanitized-test-4-logical-core".into(),
            logical_cores: 4,
            filesystem: "testfs".into(),
            power_mode: "ac".into(),
        }
    }
    fn automatic_config() -> AutomaticConfig {
        AutomaticConfig {
            profile: Profile::Release,
            warmup: Duration::ZERO,
            idle: Duration::ZERO,
            events: 100,
            inter_event: Duration::ZERO,
            runs: 2,
            sample: Duration::ZERO,
            active_timeout: Duration::from_secs(1),
        }
    }
    fn automatic_result(run: usize, latencies_us: Vec<u128>) -> AutomaticRunResult {
        AutomaticRunResult {
            run,
            accepted_primary_requests: latencies_us.len(),
            primary_otlp_latencies_us: latencies_us,
            idle_samples: vec![s("idle", Some(0.1), Some(1.0), Some(1), Some(321))],
            idle_cpu_percent: 0.1,
            active_cpu_percent: 0.1,
            peak_rss_kib: 1.0,
            peak_disk_bytes: 1,
            collector_network_bytes: 321,
            network_monitor_samples: 2,
            rejected_primary_requests: 0,
            notify_supplement_accepted: true,
        }
    }
    fn validate_single_automatic(result: AutomaticRunResult) -> Result<(), String> {
        let mut config = automatic_config();
        config.runs = 1;
        validate_automatic_results(config, &[result], &[])
    }
    fn enabled(idle_cpu: f64, active_cpu: f64, rss: f64, disk: u64, net: u64) -> Vec<Sample> {
        vec![
            s("warmup", Some(0.1), Some(rss), Some(disk), Some(net)),
            s("idle", Some(idle_cpu), Some(rss), Some(disk), Some(net)),
            s("active", Some(active_cpu), Some(rss), Some(disk), Some(net)),
        ]
    }
    #[test]
    fn release_protocol_is_normative() {
        validate_protocol_contract().unwrap();
        assert_eq!(
            Config::for_profile(Profile::Release).supported_events,
            10_000
        );
        assert_eq!(
            Config::for_profile(Profile::Release).saturation_events,
            10_000
        );
        assert_eq!(DURABLE_BATCH_RECORDS, 32);
        assert_eq!(DURABLE_BATCH_BYTES, 524_288);
        assert_eq!(SUPPORTED_INTER_EVENT_PERIOD, Duration::from_millis(3));
        assert_eq!(DURABLE_HANDOFF_BYTES_MAX, 589_824);
        assert_eq!(TOTAL_PIPELINE_PAYLOAD_BYTES_MAX, 4_784_128);
        assert!(PROTOCOL.contains("active_any_minute_percent_max"));
    }

    #[test]
    fn automatic_protocol_requires_exact_lifecycle_and_honest_network_evidence() {
        validate_automatic_protocol_contract().unwrap();
        assert!(AUTOMATIC_PROTOCOL.contains("required_os: macos"));
        assert!(AUTOMATIC_PROTOCOL.contains("validation_scope: every run independently"));
        assert!(AUTOMATIC_PROTOCOL.contains("NetworkMonitor"));
        assert!(AUTOMATIC_PROTOCOL.contains("unexpected SIGKILL service termination"));
        assert!(AUTOMATIC_PROTOCOL.contains("sustained authenticated Codex OTLP/HTTP /v1/logs"));
        assert!(
            AUTOMATIC_PROTOCOL
                .contains("separately verified built agent-observability codex-notify")
        );
        assert!(!AUTOMATIC_PROTOCOL.contains("external_network_bytes_required"));
    }

    #[test]
    fn automatic_protocol_rejects_profile_and_threshold_mutations() {
        for (name, original, replacement) in [
            ("release runs", "    runs: 5\n", "    runs: 6\n"),
            ("smoke runs", "    runs: 1\n", "    runs: 2\n"),
            (
                "release warmup",
                "    warmup_seconds: 60\n",
                "    warmup_seconds: 61\n",
            ),
            (
                "release idle",
                "    idle_seconds: 900\n",
                "    idle_seconds: 901\n",
            ),
            (
                "release request count",
                "    primary_otlp_requests: 10000\n",
                "    primary_otlp_requests: 9999\n",
            ),
            (
                "smoke request count",
                "    primary_otlp_requests: 25\n",
                "    primary_otlp_requests: 24\n",
            ),
            (
                "release request cadence",
                "    primary_otlp_inter_request_ms: 3\n",
                "    primary_otlp_inter_request_ms: 4\n",
            ),
            (
                "release sample interval",
                "    sample_interval_seconds: 1\n",
                "    sample_interval_seconds: 2\n",
            ),
            (
                "build timeout",
                "  build_timeout_seconds: 600\n",
                "  build_timeout_seconds: 601\n",
            ),
            (
                "release active timeout",
                "    active_timeout_seconds: 300\n",
                "    active_timeout_seconds: 301\n",
            ),
            (
                "smoke active timeout",
                "    active_timeout_seconds: 15\n",
                "    active_timeout_seconds: 16\n",
            ),
            (
                "request timeout",
                "    command_timeout_seconds: 1\n",
                "    command_timeout_seconds: 2\n",
            ),
            (
                "startup timeout",
                "  startup_timeout_seconds: 10\n",
                "  startup_timeout_seconds: 11\n",
            ),
            (
                "cleanup timeout",
                "  cleanup_timeout_seconds: 5\n",
                "  cleanup_timeout_seconds: 6\n",
            ),
            ("p95", "    p95_ms_max: 20\n", "    p95_ms_max: 21\n"),
            ("p99", "    p99_ms_max: 50\n", "    p99_ms_max: 51\n"),
            (
                "idle CPU",
                "    idle_average_percent_max: 0.5\n",
                "    idle_average_percent_max: 0.6\n",
            ),
            (
                "active CPU",
                "    active_integrated_percent_max: 100\n",
                "    active_integrated_percent_max: 99\n",
            ),
            ("RSS", "    peak_mib_max: 96\n", "    peak_mib_max: 95\n"),
            (
                "disk",
                "    allocated_tree_bytes_max: 1073741824\n",
                "    allocated_tree_bytes_max: 1073741823\n",
            ),
        ] {
            let mutated = AUTOMATIC_PROTOCOL.replacen(original, replacement, 1);
            assert_ne!(
                mutated, AUTOMATIC_PROTOCOL,
                "missing mutation fixture for {name}"
            );
            assert!(
                validate_automatic_protocol(&mutated).is_err(),
                "automatic protocol accepted mutated {name}"
            );
        }
    }

    #[test]
    fn automatic_protocol_rejects_unknown_structure() {
        let mutated = AUTOMATIC_PROTOCOL.replacen(
            "execution:\n",
            "execution:\n  undocumented_timeout_seconds: 1\n",
            1,
        );

        assert!(validate_automatic_protocol(&mutated).is_err());
        assert!(
            validate_automatic_protocol(&format!(
                "{AUTOMATIC_PROTOCOL}\n---\nschema_version: shadow\n"
            ))
            .is_err()
        );
    }

    #[test]
    fn automatic_release_profile_requires_macos() {
        validate_automatic_profile_host(Profile::Release, "macos").unwrap();
        assert!(validate_automatic_profile_host(Profile::Release, "linux").is_err());
        validate_automatic_profile_host(Profile::Smoke, "linux").unwrap();
    }

    #[test]
    fn automatic_p95_is_validated_per_run() {
        let mut bad = vec![1; 94];
        bad.extend([21_000; 6]);
        let results = vec![automatic_result(1, bad), automatic_result(2, vec![1; 100])];

        let error = validate_automatic_results(automatic_config(), &results, &[]).unwrap_err();

        assert!(error.contains("run 1 primary OTLP p95"));
    }

    #[test]
    fn automatic_p99_is_validated_per_run() {
        let mut bad = vec![1; 98];
        bad.extend([51_000; 2]);
        let results = vec![automatic_result(1, bad), automatic_result(2, vec![1; 100])];

        let error = validate_automatic_results(automatic_config(), &results, &[]).unwrap_err();

        assert!(error.contains("run 1 primary OTLP p99"));
    }

    #[test]
    fn automatic_cpu_limits_are_fail_closed() {
        let mut idle = automatic_result(1, vec![1; 100]);
        idle.idle_cpu_percent = 0.6;
        assert!(
            validate_single_automatic(idle)
                .unwrap_err()
                .contains("idle CPU")
        );

        let mut active = automatic_result(1, vec![1; 100]);
        active.active_cpu_percent = 100.1;
        assert!(
            validate_single_automatic(active)
                .unwrap_err()
                .contains("active CPU")
        );
    }

    #[test]
    fn automatic_rss_limit_is_fail_closed() {
        let mut result = automatic_result(1, vec![1; 100]);
        result.peak_rss_kib = 96.0 * 1024.0 + 1.0;
        assert!(
            validate_single_automatic(result)
                .unwrap_err()
                .contains("RSS")
        );
    }

    #[test]
    fn automatic_disk_limit_is_fail_closed() {
        let mut result = automatic_result(1, vec![1; 100]);
        result.peak_disk_bytes = 1_073_741_825;
        assert!(
            validate_single_automatic(result)
                .unwrap_err()
                .contains("disk")
        );
    }

    #[test]
    fn automatic_rejections_are_fail_closed() {
        let mut result = automatic_result(1, vec![1; 100]);
        result.rejected_primary_requests = 1;
        assert!(
            validate_single_automatic(result)
                .unwrap_err()
                .contains("rejected")
        );
    }

    #[test]
    fn automatic_incomplete_evidence_is_fail_closed() {
        let mut result = automatic_result(1, vec![1; 100]);
        result.accepted_primary_requests = 99;
        assert!(
            validate_single_automatic(result)
                .unwrap_err()
                .contains("incomplete")
        );
    }

    #[test]
    fn automatic_invalid_metrics_are_fail_closed() {
        let mut result = automatic_result(1, vec![1; 100]);
        result.peak_rss_kib = f64::NAN;
        assert!(
            validate_single_automatic(result)
                .unwrap_err()
                .contains("non-finite or negative")
        );
    }

    #[test]
    fn automatic_manifest_reports_measured_bytes_and_loopback_separately() {
        let results = vec![automatic_result(1, vec![1; 100])];
        let mut config = automatic_config();
        config.runs = 1;
        let manifest = render_automatic_manifest(
            config,
            &host(),
            "0123456789abcdef0123456789abcdef01234567",
            &results,
            &[],
            "pending-validation",
        );

        assert!(manifest.contains("collector_network_bytes_max: 321"));
        assert!(manifest.contains("collector_network_bytes: 321"));
        assert!(manifest.contains("network_monitor_samples: 2"));
        assert!(manifest.contains("all_observed_endpoints_loopback: true"));
        assert!(!manifest.contains("external_network_bytes"));
        validate_automatic_manifest_shape(&manifest).unwrap();

        let missing_lifecycle = manifest.replace(
            "  lifecycle_preflight: built-binary-isolated-home-codex-home-setup-sigkill-recovery-reconnect-concurrency-missing-settings-inherited-plist-disconnect\n",
            "",
        );
        assert!(validate_automatic_manifest_shape(&missing_lifecycle).is_err());
    }

    #[test]
    fn approved_ipv4_loopback_network_surfaces_pass() {
        validate_network_file(
            Path::new("crates/local-collector/src/lib.rs"),
            "use std::net::{Ipv4Addr, TcpStream}; use tokio::net::TcpListener; \
             TcpListener::bind(Ipv4Addr::LOCALHOST); http://127.0.0.1:4318/v1/logs",
        )
        .unwrap();
        validate_network_file(
            Path::new("crates/codex-integration/src/lib.rs"),
            "use std::net::{Ipv4Addr, TcpStream}; \
             TcpStream::connect((Ipv4Addr::LOCALHOST, 4318));",
        )
        .unwrap();
        validate_network_file(
            Path::new("crates/local-ui/src/lib.rs"),
            "use hyper::server; use std::net::Ipv4Addr; \
             use tokio::net::TcpListener; TcpListener::bind(Ipv4Addr::LOCALHOST);",
        )
        .unwrap();
        validate_network_surface().unwrap();
    }

    #[test]
    fn network_surface_rejects_external_destinations_and_clients() {
        let collector = Path::new("crates/local-collector/src/lib.rs");
        for body in [
            "use reqwest::Client; Ipv4Addr::LOCALHOST;",
            "use std::net::{Ipv4Addr, TcpStream}; Ipv4Addr::LOCALHOST; \
             https://collector.example.com/v1/logs",
            "use std::net::{Ipv4Addr, TcpStream}; Ipv4Addr::LOCALHOST; \
             TeamIngestEnvelope",
            "use std::net::{Ipv4Addr, TcpStream}; Ipv4Addr::UNSPECIFIED;",
        ] {
            assert!(validate_network_file(collector, body).is_err(), "{body}");
        }
        let unapproved = Path::new("crates/application/src/lib.rs");
        for body in [
            "use std::net::TcpStream;",
            "use hyper::server;",
            "use tokio::runtime;",
        ] {
            assert!(validate_network_file(unapproved, body).is_err(), "{body}");
        }
    }

    #[test]
    fn missing_metric_fails_closed() {
        assert!(
            validate_results(
                c(),
                &pair(vec![
                    s("warmup", None, Some(1.0), Some(1), Some(0)),
                    s("idle", Some(0.1), Some(1.0), Some(1), Some(0)),
                    s("active", Some(0.1), Some(1.0), Some(1), Some(0))
                ]),
                &[]
            )
            .is_err()
        );
    }

    #[test]
    fn missing_network_monitor_samples_fail_closed() {
        let mut results = pair(enabled(0.1, 0.1, 1.0, 1, 0));
        results[1].network_monitor_samples = 0;
        assert!(validate_results(c(), &results, &[]).is_err());
    }
    #[test]
    fn non_finite_metric_fails_closed() {
        assert!(validate_results(c(), &pair(enabled(f64::NAN, 0.1, 1.0, 1, 0)), &[]).is_err());
        assert!(validate_results(c(), &pair(enabled(0.1, 0.1, f64::INFINITY, 1, 0)), &[]).is_err());
    }
    #[test]
    fn idle_threshold() {
        assert!(validate_results(c(), &pair(enabled(0.7, 0.1, 1.0, 1, 0)), &[]).is_err());
    }
    #[test]
    fn active_average_threshold() {
        assert!(validate_results(c(), &pair(enabled(0.1, 2.2, 1.0, 1, 0)), &[]).is_err());
    }
    #[test]
    fn active_any_minute_threshold() {
        let mut samples = enabled(0.1, 5.2, 1.0, 1, 0);
        for (elapsed_ms, cpu) in [(60_001, 0.1), (120_001, 0.1), (180_001, 0.1)] {
            let mut sample = s("active", Some(cpu), Some(1.0), Some(1), Some(0));
            sample.elapsed_ms = elapsed_ms;
            samples.push(sample);
        }
        assert!(validate_results(c(), &pair(samples), &[]).is_err());
    }
    #[test]
    fn rss_p95_threshold() {
        assert!(validate_results(c(), &pair(enabled(0.1, 0.1, 100_000.0, 1, 0)), &[]).is_err());
    }
    #[test]
    fn disk_threshold() {
        assert!(
            validate_results(c(), &pair(enabled(0.1, 0.1, 1.0, 1_073_741_825, 0)), &[]).is_err()
        );
    }
    #[test]
    fn latency_threshold() {
        let mut results = pair(enabled(0.1, 0.1, 1.0, 1, 0));
        results[1].latencies_us = vec![30_000];
        assert!(validate_results(c(), &results, &[]).is_err());
    }
    #[test]
    fn network_threshold() {
        assert!(validate_results(c(), &pair(enabled(0.1, 0.1, 1.0, 1, 1)), &[]).is_err());
    }

    #[test]
    fn complete_metrics_within_bounds_pass() {
        assert!(validate_results(c(), &pair(enabled(0.1, 0.1, 1.0, 1, 0)), &[]).is_ok());
    }

    #[test]
    fn enabled_rejections_are_bounded_and_reconciled_with_durable_events() {
        let mut config = c();
        config.supported_events = 100;
        config.saturation_events = 100;
        let mut results = pair(enabled(0.1, 0.1, 1.0, 1, 0));
        for result in &mut results {
            result.events = 100;
            result.latencies_us = vec![1; 100];
        }
        results[1].saturation_events = 100;
        results[1].saturation_latencies_us = vec![1; 100];
        results[1].rejected_events = 1;
        results[1].saturation_rejected_events = 1;
        results[1].durable_events = 198;
        assert!(validate_results(config, &results, &[]).is_ok());
        results[1].saturation_rejected_events = 2;
        results[1].durable_events = 197;
        assert!(validate_results(config, &results, &[]).is_err());
        results[1].saturation_rejected_events = 1;
        assert!(validate_results(config, &results, &[]).is_err());
    }

    #[test]
    fn manifest_contains_computed_phase_and_network_evidence() {
        let manifest = render_manifest(c(), &host(), &pair(enabled(0.1, 0.1, 1.0, 1, 0)), &[]);
        assert!(manifest.contains("machine: sanitized-test-4-logical-core"));
        assert!(manifest.contains("filesystem: testfs"));
        assert!(manifest.contains("power_mode: ac"));
        assert!(
            manifest
                .contains("protocol_revision: v1.2.0-supported-rate-saturation-continuous-network")
        );
        assert!(manifest.contains("source_revision: 0123456789abcdef0123456789abcdef01234567"));
        assert!(manifest.contains("supported_inter_event_ms: 3"));
        assert!(manifest.contains(
            "supported_rate_measurement_boundary: first-command-through-barrier-completion"
        ));
        assert!(manifest.contains("saturation_schedule: enabled-unpaced"));
        assert!(manifest.contains("supported_rate_attempted_events: 1"));
        assert!(manifest.contains("saturation_attempted_events: 1"));
        assert!(manifest.contains("durable_path: run-relative/durable"));
        assert!(manifest.contains("idle_average_cpu_delta_percent: 0"));
        assert!(manifest.contains("active_any_minute_cpu_delta_percent: 0"));
        assert!(manifest.contains("durable_handoff_bytes_max: 589824"));
        assert!(manifest.contains("total_pipeline_payload_bytes_max: 4784128"));
        assert!(manifest.contains("network_static_surface: pass"));
        assert!(manifest.contains("network_monitor_samples: 1"));
        assert!(manifest.contains("network_bytes: 0"));
        validate_manifest_shape(&manifest).unwrap();

        let placeholder = manifest.replace(
            "machine: sanitized-test-4-logical-core",
            "machine: sanitized-local-host",
        );
        assert!(validate_manifest_shape(&placeholder).is_err());

        let hostile_error = render_manifest(
            c(),
            &host(),
            &pair(enabled(0.1, 0.1, 1.0, 1, 0)),
            &["failed 'quote'\ncontrol \u{1f}".into()],
        );
        assert!(hostile_error.contains("failed ''quote'' control  "));
        assert!(hostile_error.contains(
            "metrics:\n  hook_latency_p95_us: 1\n  hook_latency_p99_us: 1\n  supported_rate_cpu_percent_max: 0.1\n  saturation_cpu_percent_max: 0.1\n  idle_average_cpu_delta_percent: null"
        ));
        assert!(hostile_error.contains("  network_bytes: null\n  network_static_surface: pass"));
        validate_manifest_shape(&hostile_error).unwrap();

        let malformed = render_failed_manifest(
            c(),
            &host(),
            &pair(enabled(0.1, 0.1, 1.0, 1, 0)),
            &[],
            "manifest shape: required field missing",
        );
        assert!(malformed.contains("status: failed"));
        assert!(malformed.contains("manifest shape: required field missing"));
    }

    #[test]
    fn cleanup_failure_finalizes_release_manifest_as_failed() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-xtask-cleanup-manifest-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let manifest_path = root.join("manifest.yaml");
        let pending = "status: pending-validation\nerrors: []\n";
        let failed = "status: failed\nerrors:\n  - cleanup: injected failure\n";
        fs::write(&manifest_path, pending).unwrap();

        let error = finalize_manifest_after_cleanup(
            Profile::Release,
            &manifest_path,
            pending,
            Err("injected failure".into()),
            Some(failed),
        )
        .unwrap_err();

        assert!(error.contains("performance cleanup failed: injected failure"));
        assert_eq!(fs::read_to_string(&manifest_path).unwrap(), failed);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validation_failure_finalizes_written_manifest_as_failed() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-xtask-validation-manifest-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let manifest_path = root.join("manifest.yaml");

        let error = prepare_manifest(
            &manifest_path,
            c(),
            &host(),
            &pair(enabled(0.1, 0.1, 1.0, 1, 0)),
            &[],
            Err("injected validation failure".into()),
        )
        .unwrap_err();
        let manifest = fs::read_to_string(&manifest_path).unwrap();

        assert!(error.contains("injected validation failure"));
        assert!(manifest.contains("status: failed"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preparation_and_cleanup_failures_are_both_persisted() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-xtask-combined-manifest-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let manifest_path = root.join("manifest.yaml");
        fs::write(&manifest_path, "status: pending-validation\n").unwrap();

        let error = complete_manifest_lifecycle(
            &manifest_path,
            c(),
            &host(),
            &pair(enabled(0.1, 0.1, 1.0, 1, 0)),
            &[],
            Err("injected preparation failure".into()),
            Err("injected cleanup failure".into()),
        )
        .unwrap_err();
        let manifest = fs::read_to_string(&manifest_path).unwrap();

        assert!(error.contains("injected preparation failure"));
        assert!(error.contains("injected cleanup failure"));
        assert!(manifest.contains("status: failed"));
        assert!(manifest.contains("preparation: injected preparation failure"));
        assert!(manifest.contains("cleanup: injected cleanup failure"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn network_manifest_mode_is_platform_specific() {
        assert_eq!(
            network_evidence_mode("macos"),
            "continuous-process-lifetime-monitor-plus-static-product-surface"
        );
        assert_eq!(
            network_evidence_mode("linux"),
            "point-in-time-socket-descriptor-scan-plus-static-product-surface"
        );
        assert_eq!(
            network_evidence_mode("other"),
            "unsupported-process-scoped-evidence-plus-static-product-surface"
        );
    }

    #[test]
    fn worker_output_timeout_is_bounded() {
        let (_sender, receiver) = mpsc::channel::<Result<String, String>>();
        let timeout = Duration::from_millis(20);
        let started = Instant::now();
        let error = read_worker_output(&receiver, timeout, "test marker").unwrap_err();
        assert!(error.contains("timed out after 20 ms"));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn worker_output_reader_join_timeout_is_bounded() {
        let (_line_sender, receiver) = mpsc::channel::<Result<String, String>>();
        let (reader_done_sender, reader_done) = mpsc::channel();
        let reader = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            let _ = reader_done_sender.send(());
        });
        let mut output = WorkerOutput {
            receiver,
            reader_done,
            reader: Some(reader),
        };
        let started = Instant::now();
        let error = output
            .join_with_timeout(Duration::from_millis(5))
            .unwrap_err();
        assert!(error.contains("timed out after 5 ms"));
        assert!(started.elapsed() < Duration::from_secs(1));
        output.join_with_timeout(Duration::from_secs(1)).unwrap();
    }

    #[test]
    fn pressure_sampler_result_timeout_is_bounded() {
        let (_sender, receiver) = mpsc::channel::<Result<PressurePeaks, String>>();
        let timeout = Duration::from_millis(20);
        let started = Instant::now();
        let error = receiver.recv_timeout(timeout).unwrap_err();
        assert_eq!(error, mpsc::RecvTimeoutError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn pressure_sampler_joins_after_result_channel_disconnects() {
        let (stop, _stop_receiver) = mpsc::channel();
        let (result_sender, result) = mpsc::channel::<Result<PressurePeaks, String>>();
        drop(result_sender);
        let mut sampler = PressureSampler {
            stop: Some(stop),
            result,
            handle: Some(thread::spawn(|| {})),
            process_exit_confirmed: Arc::new(AtomicBool::new(false)),
            drain_sample_count: Arc::new(AtomicU64::new(0)),
        };

        let error = sampler.stop().unwrap_err();

        assert!(error.contains("pressure sampler ended without a result"));
        assert!(sampler.handle.is_none());
    }

    #[test]
    fn worker_drain_marker_starts_on_request_and_ends_after_sample_ack() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-xtask-drain-protocol-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let marker = root.join(".drain-active");
        assert!(!begin_drain_boundary("codex|0", &marker).unwrap());
        assert!(!marker.exists());
        assert!(begin_drain_boundary("__drain__", &marker).unwrap());
        assert!(marker.exists());
        let mut valid_ack = [Ok("__drain_sampled__".into())].into_iter();
        read_drain_sample_ack(&mut valid_ack).unwrap();
        assert!(marker.exists());
        finish_drain_boundary(&marker).unwrap();
        assert!(!marker.exists());
        let mut invalid_ack = [Ok("wrong".into())].into_iter();
        assert!(read_drain_sample_ack(&mut invalid_ack).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn worker_exit_timeout_is_bounded() {
        let mut child = Command::new("sleep")
            .arg("1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let timeout = Duration::from_millis(20);
        let started = Instant::now();
        let error = wait_for_child(&mut child, timeout).unwrap_err();
        assert!(error.contains("worker exit timed out after 20 ms"));
        assert!(started.elapsed() < Duration::from_secs(1));
        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[test]
    fn interval_cpu_uses_process_delta_over_wall_time() {
        assert!(
            (interval_cpu_percent(1.0, 1.25, Duration::from_secs(1)) - 25.0).abs() < f64::EPSILON
        );
        assert!(interval_cpu_percent(2.0, 1.0, Duration::from_secs(1)).abs() < f64::EPSILON);
        assert!(interval_cpu_percent(1.0, 2.0, Duration::ZERO).abs() < f64::EPSILON);
    }

    #[test]
    fn process_cpu_time_accepts_macos_and_linux_formats() {
        assert!((parse_process_cpu_seconds("2:03.50").unwrap() - 123.5).abs() < f64::EPSILON);
        assert!((parse_process_cpu_seconds("61:00.00").unwrap() - 3_660.0).abs() < f64::EPSILON);
        assert!((parse_process_cpu_seconds("01:02:03").unwrap() - 3_723.0).abs() < f64::EPSILON);
        assert!(
            (parse_process_cpu_seconds("2-01:02:03").unwrap() - 176_523.0).abs() < f64::EPSILON
        );
        assert!(parse_process_cpu_seconds("01:60:00").is_err());
        assert!(parse_process_cpu_seconds("-1:00").is_err());
        assert!(parse_process_cpu_seconds("1:NaN").is_err());
        assert!(parse_process_cpu_seconds("1:inf").is_err());
        assert!(parse_process_cpu_seconds("4294967296:00").is_err());
        assert!(parse_process_cpu_seconds("not-a-time").is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_network_monitor_preserves_peak_across_closed_socket_samples() {
        let mut state = MacNetworkMonitorState::default();
        let started = Instant::now();
        record_macos_network_line(&mut state, "^D\u{8}\u{8} bytes_in bytes_out\r", started)
            .unwrap();
        assert_eq!(state.evidence.samples, 0);
        record_macos_network_line(&mut state, "worker.1 10 20", started).unwrap();
        record_macos_network_line(&mut state, "worker.1 3 4", started).unwrap();
        assert_eq!(state.evidence.samples, 0);

        let first_completed = started + Duration::from_secs(1);
        record_macos_network_line(&mut state, "bytes_in bytes_out", first_completed).unwrap();
        assert_eq!(state.evidence.latest_bytes, 37);
        assert_eq!(state.evidence.max_bytes, 37);
        assert_eq!(state.evidence.samples, 1);

        record_macos_network_line(
            &mut state,
            "bytes_in bytes_out",
            first_completed + Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(state.evidence.latest_bytes, 0);
        assert_eq!(state.evidence.max_bytes, 37);
        assert_eq!(state.evidence.samples, 2);
        assert!(record_macos_network_line(&mut state, "invalid", started).is_err());
        assert!(record_macos_network_line(&mut state, "worker nope 1", started).is_err());

        assert_eq!(
            macos_network_sample(&state, first_completed + Duration::from_secs(2)).unwrap(),
            0
        );
        assert!(
            macos_network_sample(&state, first_completed + Duration::from_secs(5))
                .unwrap_err()
                .contains("stale")
        );
        let post_drain_boundary = first_completed + Duration::from_millis(500);
        assert!(!macos_cycle_started_after(&state, post_drain_boundary));
        state.last_completed_started_at = Some(post_drain_boundary);
        assert!(macos_cycle_started_after(&state, post_drain_boundary));
        state.error = Some("monitor parse failed".into());
        assert_eq!(
            macos_network_sample(&state, first_completed + Duration::from_secs(2)).unwrap_err(),
            "monitor parse failed"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_network_finish_includes_terminal_buffered_cycle() {
        let state = Mutex::new(MacNetworkMonitorState::default());
        let buffered = b"bytes_in bytes_out\nworker.1 40 2\nbytes_in bytes_out\n";

        read_macos_network_monitor(std::io::Cursor::new(buffered), &state).unwrap();
        let state = state.into_inner().unwrap();
        let evidence = finalize_macos_network_evidence(state).unwrap();

        assert_eq!(evidence.latest_bytes, 42);
        assert_eq!(evidence.max_bytes, 42);
        assert_eq!(evidence.samples, 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_network_finish_propagates_terminal_reader_error() {
        let state = MacNetworkMonitorState {
            evidence: NetworkEvidence {
                latest_bytes: 42,
                max_bytes: 42,
                samples: 1,
            },
            error: Some("terminal parse failure".into()),
            ..MacNetworkMonitorState::default()
        };

        let error = finalize_macos_network_evidence(state).unwrap_err();

        assert_eq!(error, "terminal parse failure");
    }

    #[test]
    fn drain_metric_allows_only_a_parent_confirmed_process_exit() {
        let process_exit_confirmed = AtomicBool::new(false);
        assert!(
            metric_or_confirmed_exit(
                Err("process rss is unavailable".into()),
                true,
                &process_exit_confirmed,
            )
            .is_err()
        );
        process_exit_confirmed.store(true, Ordering::Release);
        assert_eq!(
            metric_or_confirmed_exit(
                Err("process rss is unavailable".into()),
                true,
                &process_exit_confirmed,
            )
            .unwrap(),
            None
        );
        assert!(
            metric_or_confirmed_exit(
                Err("process rss is unavailable".into()),
                false,
                &process_exit_confirmed,
            )
            .is_err()
        );
    }

    #[test]
    fn drain_sampler_accepts_exit_only_after_child_wait_confirms_it() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-xtask-sampler-exit-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let mut child = Command::new("sleep")
            .arg("0.2")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut sampler = start_pressure_sampler(
            root.clone(),
            child.id(),
            true,
            None,
            Duration::from_millis(10),
        )
        .unwrap();
        assert!(child.wait().unwrap().success());
        sampler.confirm_process_exit();
        thread::sleep(Duration::from_millis(30));
        let peaks = sampler.stop().unwrap();
        assert!(peaks.process_exit_observed);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn durable_drain_preserves_all_three_source_schedules() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-xtask-three-source-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let (sender, receiver) = mpsc::channel();
        for source in ["codex", "claude-code", "cursor"] {
            sender
                .send(agent_observability_local_runtime::IngressMessage(
                    format!("{source}|0").into_bytes(),
                ))
                .unwrap();
        }
        drop(sender);
        drain(&receiver, &root).unwrap();
        assert_eq!(
            LocalStore::open(&root)
                .unwrap()
                .observation_count()
                .unwrap(),
            3
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn durable_batches_enforce_record_and_byte_bounds_without_dropping_overflow() {
        let (sender, receiver) = mpsc::channel();
        for payload in [b"1234".to_vec(), b"5678".to_vec(), b"90".to_vec()] {
            sender.send(IngressMessage(payload)).unwrap();
        }
        drop(sender);
        let mut pending = None;
        let first = receive_batch(&receiver, &mut pending, 3, 8)
            .unwrap()
            .unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(first.iter().map(|item| item.0.len()).sum::<usize>(), 8);
        let second = receive_batch(&receiver, &mut pending, 3, 8)
            .unwrap()
            .unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].0, b"90");
        assert!(
            receive_batch(&receiver, &mut pending, 3, 8)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn durability_barrier_accumulates_committed_batches_to_target() {
        let (sender, receiver) = mpsc::channel();
        sender.send(2).unwrap();
        sender.send(3).unwrap();
        let mut durable_events = 0;
        await_durable_count(&receiver, &mut durable_events, 5).unwrap();
        assert_eq!(durable_events, 5);
    }

    #[test]
    fn durability_progress_is_emitted_only_after_store_commit() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-xtask-barrier-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let (sender, receiver) = mpsc::channel();
        let (progress_sender, progress_receiver) = mpsc::channel();
        let drain_root = root.clone();
        let drain = thread::spawn(move || {
            drain_with_cpu_token(&receiver, &drain_root, None, Some(&progress_sender))
        });
        sender.send(IngressMessage(b"codex|0".to_vec())).unwrap();
        let mut durable_events = 0;
        await_durable_count(&progress_receiver, &mut durable_events, 1).unwrap();
        assert_eq!(
            LocalStore::open(&root)
                .unwrap()
                .observation_count()
                .unwrap(),
            1
        );
        drop(sender);
        drain.join().unwrap().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn durable_drain_propagates_writer_startup_failure() {
        let (sender, receiver) = mpsc::channel();
        drop(sender);
        let invalid = std::env::temp_dir().join(format!(
            "agent-observability-xtask-file-root-{}",
            std::process::id()
        ));
        let _ = fs::remove_file(&invalid);
        fs::write(&invalid, b"not a directory").unwrap();
        assert!(
            drain(&receiver, &invalid)
                .unwrap_err()
                .contains("open local durable store")
        );
        fs::remove_file(invalid).unwrap();
    }

    #[test]
    fn enqueue_retry_never_attempts_acceptance_after_deadline() {
        let mut attempts = 0;
        let outcome = enqueue_until(Instant::now(), || {
            attempts += 1;
            if attempts == 1 {
                IngressOutcome::Full
            } else {
                IngressOutcome::Accepted
            }
        });
        assert_eq!(outcome, IngressOutcome::Full);
        assert_eq!(attempts, 1);
    }

    #[test]
    fn enqueue_retry_sleep_is_capped_and_saturates_at_deadline() {
        let now = Instant::now();
        assert_eq!(
            retry_sleep(now, now + Duration::from_secs(1)),
            Duration::from_micros(100)
        );
        assert_eq!(retry_sleep(now, now), Duration::ZERO);
    }
}
