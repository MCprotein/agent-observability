use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as FmtWrite;
use std::fs::{self, Permissions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::ops::{Deref, DerefMut};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use agent_observability_contracts::{AgentSource, ObservationEvent, SourceObservation};
use agent_observability_domain::{
    CorrelationIds, LifecycleState, ObservationId, SourceCursor, SourceGeneration, SpanId, Timing,
    TokenUsage, TraceId,
};
use agent_observability_local_runtime::{
    Admission, ENQUEUE_DEADLINE_MS, Ingress, IngressMessage, IngressOutcome, LocalRuntimeConfigV2,
    PressureSample, RuntimeControl, StorageBudget,
};
use agent_observability_local_store::LocalStore;

const USAGE: &str = "usage: cargo run -p xtask -- perf local --profile <release|smoke> --check";
const PROTOCOL: &str = include_str!("../../crates/contracts/performance/local-performance-v1.yaml");
const REQUIRED_PROTOCOL_LINES: [&str; 36] = [
    "schema_version: local_performance.v1",
    "warmup_seconds: 60",
    "idle_seconds: 900",
    "active_seconds: 900",
    "burst_events: 10000",
    "sample_interval_seconds: 1",
    "baseline_runs: 5",
    "enabled_runs: 5",
    "foreground_hook_p95_ms_max: 20",
    "foreground_hook_p99_ms_max: 50",
    "idle_average_percent_max: 0.5",
    "active_average_percent_max: 2",
    "active_any_minute_percent_max: 5",
    "sampling: process CPU time delta divided by sample wall-time delta; lifetime-average percent is forbidden",
    "burst_integrated_and_sampled_percent_max: 100",
    "p95_mib_max: 96",
    "burst_and_drain_peak_required: true",
    "total_bytes_max: 1073741824",
    "accounting: allocated filesystem blocks for the full release durable root across all retained run directories, including durable state, final state, projections, crash and sampled temp artifacts",
    "ingest_requests_in_flight_max: 0",
    "required_bytes: 0",
    "channel_capacity: 64",
    "normalization_workers: 1",
    "durable_batch_records: 500",
    "durable_batch_queue_capacity: 12",
    "durable_batch_queue_bytes_max: 6291456",
    "durable_handoff_bytes_max: 7340032",
    "total_pipeline_payload_bytes_max: 11534336",
    "enqueue_deadline_ms: 10",
    "enabled_rejection_percent_max: 1",
    "required_fields: [machine, os, filesystem, power_mode, cold_warm_cache, logical_cores, source_versions, workload, phase_metrics, all_run_samples, baseline, enabled]",
    "fail_closed: missing or breached required metrics produce non-zero exit",
    "host_metadata: sanitized factual architecture/core, filesystem type, power source and cache/warmup state; placeholder values are forbidden",
    "failure_behavior: stop after the first failed run, terminate and join worker/sampler resources, remove durable payloads, and retain the release failure manifest for diagnosis",
    "event_reconciliation: after graceful fixture shutdown enabled enqueued events must equal durable observations; every rejection remains explicit",
    "output: docs/evidence/local/performance/<run>/manifest.yaml",
];
const SOURCES: [(&str, AgentSource); 3] = [
    ("codex", AgentSource::Codex),
    ("claude-code", AgentSource::ClaudeCode),
    ("cursor", AgentSource::Cursor),
];
const DURABLE_BATCH_COALESCE: Duration = Duration::from_millis(3);
const DURABLE_BATCH_RECORDS: u16 = 500;
const DURABLE_BATCH_BYTES: u32 = 524_288;
const DURABLE_BATCH_QUEUE_CAPACITY: usize = 12;
const DURABLE_BATCH_QUEUE_BYTES_MAX: u32 = 6_291_456;
const DURABLE_HANDOFF_BYTES_MAX: u32 = 7_340_032;
const TOTAL_PIPELINE_PAYLOAD_BYTES_MAX: u32 = 11_534_336;

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
    burst: usize,
    runs: usize,
    sample: Duration,
}
impl Config {
    fn for_profile(profile: Profile) -> Self {
        match profile {
            Profile::Release => Self {
                profile,
                warmup: Duration::from_mins(1),
                idle: Duration::from_mins(15),
                active: Duration::from_mins(15),
                burst: 10_000,
                runs: 5,
                sample: Duration::from_secs(1),
            },
            Profile::Smoke => Self {
                profile,
                warmup: Duration::from_millis(100),
                idle: Duration::from_millis(250),
                active: Duration::from_millis(500),
                burst: 100,
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
    durable_events: u64,
    latencies_us: Vec<u128>,
    samples: Vec<Sample>,
    durable_bytes: u64,
    burst_cpu_percent: f64,
    peak_rss_kib: f64,
    peak_disk_bytes: u64,
}

#[derive(Clone, Debug, Default)]
struct PressurePeaks {
    cpu_percent: f64,
    rss_kib: f64,
    disk_bytes: u64,
    drain_sample_count: usize,
    process_exit_observed: bool,
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
        self.0
            .wait()
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
        || args[1] != "local"
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
    run(Config::for_profile(profile))
}

fn run(config: Config) -> Result<(), String> {
    validate_protocol_contract()?;
    validate_network_surface()?;
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
    let completion = (|| -> Result<(), String> {
        fs::write(
            &manifest_path,
            render_manifest(config, &host, &results, &errors),
        )
        .map_err(|e| format!("write manifest: {e}"))?;
        let pending_manifest = fs::read_to_string(&manifest_path)
            .map_err(|e| format!("read pending manifest: {e}"))?;
        validate_manifest_shape(&pending_manifest)?;
        validate_results(config, &results, &errors).map_err(|error| {
            format!(
                "performance check failed; manifest: {}: {error}",
                manifest_path.display()
            )
        })?;
        let finalized = pending_manifest.replace("status: pending-validation", "status: pass");
        fs::write(&manifest_path, finalized).map_err(|e| format!("finalize manifest: {e}"))
    })();
    let durable_cleanup = durable_cleanup.cleanup();
    let smoke_cleanup = smoke_root_cleanup
        .as_mut()
        .map_or(Ok(()), DirectoryCleanup::cleanup);
    combine_cleanup(combine_cleanup(completion, durable_cleanup), smoke_cleanup)?;
    println!(
        "manifest={}\nprofile={}\nstatus=pass",
        manifest_path.display(),
        profile_name(config.profile)
    );
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
    let result = execute_run_with_child(config, enabled, run, durable_dir, &path, &mut child);
    if let Err(error) = result {
        child
            .terminate()
            .map_err(|cleanup| format!("{error}; worker cleanup failed: {cleanup}"))?;
        return Err(error);
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
) -> Result<RunResult, String> {
    let network_baseline = network_bytes(child.id()).ok();
    let mut writer = BufWriter::new(child.stdin.take().ok_or("worker stdin unavailable")?);
    let stdout = child.stdout.take().ok_or("worker stdout unavailable")?;
    let mut reader = BufReader::new(stdout);
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
        network_baseline,
    )?;
    sample_phase(
        &mut samples,
        "idle",
        config.idle,
        config.sample,
        durable_dir,
        child.id(),
        started,
        network_baseline,
    )?;
    let cpu_before = process_cpu_seconds(child.id())?;
    let burst_started = Instant::now();
    let mut burst_sampler =
        start_pressure_sampler(durable_dir.to_path_buf(), child.id(), false, None)?;
    let mut latencies_us = Vec::with_capacity(config.burst);
    let mut rejected_events = 0_usize;
    let burst_result = (|| -> Result<(), String> {
        for event in 0..config.burst {
            let (name, _) = SOURCES[event % SOURCES.len()];
            let command = format!("{name}|{}", event / SOURCES.len());
            let before = Instant::now();
            writeln!(writer, "{command}").map_err(|e| format!("write workload command: {e}"))?;
            writer
                .flush()
                .map_err(|e| format!("flush workload command: {e}"))?;
            let mut response = String::new();
            reader
                .read_line(&mut response)
                .map_err(|e| format!("read worker response: {e}"))?;
            match response.trim() {
                "ok" => {}
                "full" | "oversized" | "unavailable" => {
                    rejected_events = rejected_events.saturating_add(1);
                }
                _ => return Err("worker returned an invalid response".into()),
            }
            latencies_us.push(before.elapsed().as_micros());
        }
        Ok(())
    })();
    let burst_elapsed = burst_started.elapsed();
    let burst_peaks = burst_sampler.stop()?;
    burst_result?;
    let cpu_after = process_cpu_seconds(child.id())?;
    let burst_cpu_percent = if burst_elapsed.is_zero() {
        0.0
    } else {
        (cpu_after - cpu_before).max(0.0) / burst_elapsed.as_secs_f64() * 100.0
    };
    samples.push(sample(
        durable_dir,
        child.id(),
        "active",
        started.elapsed(),
        Some(burst_cpu_percent),
        network_baseline,
    )?);
    sample_phase(
        &mut samples,
        "active",
        config.active,
        config.sample,
        durable_dir,
        child.id(),
        started,
        network_baseline,
    )?;
    drop(writer);
    let drain_marker = enabled.then(|| path.join(".drain-active"));
    let mut drain_sampler =
        start_pressure_sampler(durable_dir.to_path_buf(), child.id(), true, drain_marker)?;
    read_worker_marker(&mut reader, "drain-start")?;
    read_worker_marker(&mut reader, "drain-complete")?;
    let status_result = child.wait().map_err(|e| format!("wait for worker: {e}"));
    let drain_result = drain_sampler.stop();
    let status = status_result?;
    let drain_peaks = drain_result?;
    if !status.success() {
        return Err(format!("worker exited with {status}"));
    }
    if enabled && drain_peaks.drain_sample_count == 0 {
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
    if latencies_us.len() != config.burst || samples.is_empty() {
        return Err("required subprocess latency or resource samples are missing".into());
    }
    Ok(RunResult {
        enabled,
        run,
        events: latencies_us.len(),
        rejected_events,
        durable_events,
        latencies_us,
        samples,
        durable_bytes,
        burst_cpu_percent: burst_cpu_percent.max(burst_peaks.cpu_percent),
        peak_rss_kib: burst_peaks.rss_kib.max(drain_peaks.rss_kib),
        peak_disk_bytes: burst_peaks.disk_bytes.max(drain_peaks.disk_bytes),
    })
}

struct PressureSampler {
    stop: Option<mpsc::Sender<()>>,
    handle: Option<thread::JoinHandle<Result<PressurePeaks, String>>>,
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
    fn stop(&mut self) -> Result<PressurePeaks, String> {
        let signal_failed = self.stop.take().is_some_and(|stop| stop.send(()).is_err());
        let peaks = self
            .handle
            .take()
            .ok_or_else(|| "pressure sampler handle is missing".to_string())?
            .join()
            .map_err(|_| "pressure sampler panicked".to_string())??;
        if signal_failed && !peaks.process_exit_observed {
            return Err("signal pressure sampler cleanup failed".into());
        }
        Ok(peaks)
    }
}

fn start_pressure_sampler(
    path: PathBuf,
    pid: u32,
    allow_process_exit: bool,
    drain_marker: Option<PathBuf>,
) -> Result<PressureSampler, String> {
    let (stop_tx, stop_rx) = mpsc::channel();
    let previous_cpu = process_cpu_seconds(pid)?;
    let previous_at = Instant::now();
    let initial_rss = ps_metric(pid, "rss")?;
    let initial_disk = StorageBudget::allocated_tree_bytes(&path)
        .map_err(|e| format!("measure initial pressure disk: {e}"))?;
    let handle = thread::spawn(move || {
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
            let current_cpu = match process_cpu_seconds(pid) {
                Ok(value) => value,
                Err(_) if allow_process_exit && !process_exists(pid) => {
                    peaks.process_exit_observed = true;
                    break;
                }
                Err(error) => return Err(error),
            };
            let current_at = Instant::now();
            peaks.cpu_percent = peaks.cpu_percent.max(interval_cpu_percent(
                previous_cpu,
                current_cpu,
                current_at.duration_since(previous_at),
            ));
            previous_cpu = current_cpu;
            previous_at = current_at;
            peaks.rss_kib = peaks.rss_kib.max(ps_metric(pid, "rss")?);
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
            }
            if stop_rx.recv_timeout(Duration::from_millis(10)).is_ok() {
                break;
            }
        }
        Ok(peaks)
    });
    Ok(PressureSampler {
        stop: Some(stop_tx),
        handle: Some(handle),
    })
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
    let (minutes, seconds) = value
        .rsplit_once(':')
        .ok_or("process CPU time has an invalid format")?;
    let minutes = minutes
        .parse::<u32>()
        .map_err(|_| "process CPU minutes are invalid")?;
    let seconds = seconds
        .parse::<f64>()
        .map_err(|_| "process CPU seconds are invalid")?;
    let total = f64::from(minutes) * 60.0 + seconds;
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

fn process_exists(pid: u32) -> bool {
    ps_value(pid, "pid").is_some()
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
    network_baseline: Option<u64>,
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
            network_baseline,
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
            network_baseline,
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

fn read_worker_marker(reader: &mut impl BufRead, expected: &str) -> Result<(), String> {
    let mut marker = String::new();
    reader
        .read_line(&mut marker)
        .map_err(|e| format!("read worker {expected} marker: {e}"))?;
    if marker.trim() != expected {
        return Err(format!("worker omitted {expected} marker"));
    }
    Ok(())
}

fn worker() -> Result<(), String> {
    let mut args = env::args().skip(2);
    let path = args.next().ok_or("worker durable path missing")?;
    let enabled = args.next().is_some_and(|mode| mode == "enabled");
    let mut output = BufWriter::new(io::stdout().lock());
    if !enabled {
        for line in io::stdin().lock().lines() {
            line.map_err(|e| format!("read worker command: {e}"))?;
            writeln!(output, "ok").map_err(|e| format!("write worker response: {e}"))?;
            output
                .flush()
                .map_err(|e| format!("flush worker response: {e}"))?;
        }
        writeln!(output, "drain-start").map_err(|e| format!("write drain marker: {e}"))?;
        writeln!(output, "drain-complete").map_err(|e| format!("write drain marker: {e}"))?;
        output
            .flush()
            .map_err(|e| format!("flush drain marker: {e}"))?;
        return Ok(());
    }
    let (ingress, receiver) = Ingress::new();
    let (tx, rx) = mpsc::channel();
    let drain_path = PathBuf::from(&path);
    thread::spawn(move || {
        let _ = tx.send(drain(&receiver, &drain_path));
    });
    for line in io::stdin().lock().lines() {
        let command = line.map_err(|e| format!("read worker command: {e}"))?;
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
    let drain_marker = PathBuf::from(&path).join(".drain-active");
    fs::write(&drain_marker, []).map_err(|e| format!("create drain boundary: {e}"))?;
    fs::set_permissions(&drain_marker, Permissions::from_mode(0o600))
        .map_err(|e| format!("protect drain boundary: {e}"))?;
    let drain_result = rx
        .recv()
        .map_err(|_| "local runtime drain stopped".to_string())?;
    let marker_cleanup =
        fs::remove_file(&drain_marker).map_err(|e| format!("remove drain boundary: {e}"));
    drain_result?;
    marker_cleanup?;
    writeln!(output, "drain-complete").map_err(|e| format!("write drain marker: {e}"))?;
    output
        .flush()
        .map_err(|e| format!("flush drain marker: {e}"))?;
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

fn drain(receiver: &std::sync::mpsc::Receiver<IngressMessage>, path: &Path) -> Result<(), String> {
    let mut config = LocalRuntimeConfigV2::default();
    config.collection.max_batch_records = DURABLE_BATCH_RECORDS;
    config.collection.max_batch_bytes = DURABLE_BATCH_BYTES;
    let max_batch_records = usize::from(config.collection.max_batch_records);
    let max_batch_bytes = usize::try_from(config.collection.max_batch_bytes).unwrap_or(usize::MAX);
    let (batch_sender, batch_receiver) = mpsc::sync_channel(DURABLE_BATCH_QUEUE_CAPACITY);
    let durable_path = path.to_path_buf();
    let writer = thread::spawn(move || persist_batches(&batch_receiver, &durable_path, &config));
    let mut pending = None;
    let receive_result = pump_batches(
        receiver,
        &batch_sender,
        &mut pending,
        max_batch_records,
        max_batch_bytes,
    );
    drop(batch_sender);
    let write_result = writer
        .join()
        .map_err(|_| "local durable batch writer panicked".to_string())?;
    match (receive_result, write_result) {
        (_, Err(error)) | (Err(error), Ok(())) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn pump_batches(
    receiver: &std::sync::mpsc::Receiver<IngressMessage>,
    batch_sender: &std::sync::mpsc::SyncSender<Vec<IngressMessage>>,
    pending: &mut Option<IngressMessage>,
    max_batch_records: usize,
    max_batch_bytes: usize,
) -> Result<(), String> {
    while let Some(messages) = receive_batch(receiver, pending, max_batch_records, max_batch_bytes)?
    {
        batch_sender
            .send(messages)
            .map_err(|_| "local durable batch writer stopped".to_string())?;
    }
    Ok(())
}

fn persist_batches(
    receiver: &std::sync::mpsc::Receiver<Vec<IngressMessage>>,
    path: &Path,
    config: &LocalRuntimeConfigV2,
) -> Result<(), String> {
    let mut store = LocalStore::open(path).map_err(|e| format!("open local durable store: {e}"))?;
    let mut control = RuntimeControl::new(config).map_err(|e| e.to_string())?;
    let mut previous = BTreeMap::new();
    while let Ok(messages) = receiver.recv() {
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
    network_baseline: Option<u64>,
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
        network_bytes: network_bytes(pid)
            .ok()
            .zip(network_baseline)
            .map(|(current, baseline)| current.saturating_sub(baseline)),
    })
}
fn network_bytes(pid: u32) -> Result<u64, String> {
    #[cfg(target_os = "linux")]
    {
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
    {
        let output = Command::new("nettop")
            .args([
                "-P",
                "-x",
                "-J",
                "bytes_in,bytes_out",
                "-p",
                &pid.to_string(),
                "-l",
                "1",
            ])
            .output()
            .map_err(|e| format!("run process-scoped network sampler: {e}"))?;
        if !output.status.success() {
            return Err("process-scoped network sampler failed".into());
        }
        let body = String::from_utf8(output.stdout)
            .map_err(|_| "process-scoped network sampler returned non-UTF8".to_string())?;
        let mut total = 0_u64;
        for line in body.lines().skip(1).filter(|line| !line.trim().is_empty()) {
            let columns = line.split_whitespace().collect::<Vec<_>>();
            if columns.len() < 3 {
                return Err("process-scoped network sampler returned an invalid row".into());
            }
            let bytes_in = columns[columns.len() - 2]
                .parse::<u64>()
                .map_err(|_| "process-scoped network bytes_in is invalid")?;
            let bytes_out = columns[columns.len() - 1]
                .parse::<u64>()
                .map_err(|_| "process-scoped network bytes_out is invalid")?;
            total = total
                .checked_add(bytes_in)
                .and_then(|value| value.checked_add(bytes_out))
                .ok_or("process-scoped network byte counter overflow")?;
        }
        Ok(total)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        Err("process-scoped network evidence is unsupported on this host".into())
    }
}

fn validate_network_surface() -> Result<(), String> {
    const FORBIDDEN: [&str; 8] = [
        "std::net",
        "TcpStream",
        "UdpSocket",
        "reqwest",
        "hyper",
        "tokio",
        "collector_endpoint",
        "TeamIngestEnvelope",
    ];
    let mut pending = vec![PathBuf::from("crates")];
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
            .is_some_and(|extension| matches!(extension.to_str(), Some("rs" | "toml" | "lock")))
        {
            let body =
                fs::read_to_string(&path).map_err(|e| format!("read network surface: {e}"))?;
            if let Some(token) = FORBIDDEN.iter().find(|token| body.contains(**token)) {
                return Err(format!(
                    "network surface token {token} found in {}",
                    path.display()
                ));
            }
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
    let output = Command::new("ps")
        .arg("-o")
        .arg(format!("{field}="))
        .args(["-p", &pid.to_string()])
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .find(|line| !line.trim().is_empty())?
        .trim()
        .to_owned();
    (!value.is_empty()).then_some(value)
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
        .flat_map(|r| r.latencies_us.iter().copied())
        .collect::<Vec<_>>();
    let mut out = format!(
        "schema_version: local_performance.v1\nprofile: {}\nprotocol: crates/contracts/performance/local-performance-v1.yaml\nstatus: pending-validation\nmachine: {}\nos: {}\nfilesystem: {}\npower_mode: {}\ncold_warm_cache: warm-after-build-and-per-run-warmup\nlogical_cores: {}\nsource_versions:\n  product: {}\n  runtime_config: local_runtime.v2\n  durable_store: local_state.v4\nbaseline:\n  runs: {}\nenabled:\n  runs: {}\nworkload:\n  warmup_seconds: {}\n  idle_seconds: {}\n  active_seconds: {}\n  burst_events: {}\n  sample_interval_seconds: {}\n  adapters: [codex, claude-code, cursor]\n  schedule: round-robin-codex-claude-code-cursor\n  channel_capacity: 64\n  normalization_workers: 1\n  durable_batch_records: {DURABLE_BATCH_RECORDS}\n  durable_batch_queue_capacity: {DURABLE_BATCH_QUEUE_CAPACITY}\n  durable_batch_queue_bytes_max: {DURABLE_BATCH_QUEUE_BYTES_MAX}\n  durable_handoff_bytes_max: {DURABLE_HANDOFF_BYTES_MAX}\n  total_pipeline_payload_bytes_max: {TOTAL_PIPELINE_PAYLOAD_BYTES_MAX}\n  enqueue_deadline_ms: 10\n  command_boundary: fixed-capacity-local-runtime-ingress\n  worker_boundary: bounded-batch-pump-and-asynchronous-local-store-writer\n  foreground_response: bounded-enqueue-acceptance\n  durable_path: run-relative/durable\n  durable_path_lifecycle: removed-after-measurement\nall_run_samples:\n",
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
        config.burst,
        config.sample.as_secs_f64()
    );
    for result in results {
        let _ = writeln!(
            out,
            "  - mode: {}\n    run: {}\n    attempted_events: {}\n    enqueued_events: {}\n    rejected_events: {}\n    durable_events: {}\n    durable_bytes: {}\n    burst_cpu_percent: {}\n    peak_rss_kib: {}\n    peak_disk_bytes: {}\n    hook_latency_us: {:?}\n    samples:",
            if result.enabled {
                "enabled"
            } else {
                "baseline"
            },
            result.run,
            result.events,
            result.events.saturating_sub(result.rejected_events),
            result.rejected_events,
            result.durable_events,
            result.durable_bytes,
            result.burst_cpu_percent,
            result.peak_rss_kib,
            result.peak_disk_bytes,
            result.latencies_us
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
    let summary = metric_summary(results);
    let _ = writeln!(
        out,
        "phase_metrics:\n  idle_average_cpu_delta_percent: {}\n  active_average_cpu_delta_percent: {}\n  active_any_minute_cpu_delta_percent: {}\nmetrics:\n  hook_latency_p95_us: {}\n  hook_latency_p99_us: {}\n  idle_average_cpu_delta_percent: {}\n  active_average_cpu_delta_percent: {}\n  active_any_minute_cpu_delta_percent: {}\n  enabled_rss_p95_kib: {}\n  total_allocated_disk_bytes: {}\n  network_bytes: {}\n  network_static_surface: pass\n  required: [hook_latency_p95_us, hook_latency_p99_us, idle_average_cpu_delta_percent, active_average_cpu_delta_percent, active_any_minute_cpu_delta_percent, enabled_rss_p95_kib, total_allocated_disk_bytes, network_bytes, network_static_surface]\n  network_mode: process-scoped-samples-plus-static-product-surface\n  evidence_scope: subprocess-plus-fixed-capacity-ingress-plus-asynchronous-local-store-drain",
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
        summary.map_or_else(|| "null".into(), |value| value.network_bytes.to_string())
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
    let network_bytes = results
        .iter()
        .flat_map(|result| {
            result
                .samples
                .iter()
                .filter_map(|sample| sample.network_bytes)
        })
        .max()?;
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
    if results
        .iter()
        .any(|r| r.events != config.burst || r.latencies_us.is_empty() || r.samples.is_empty())
    {
        return Err("incomplete event, latency, or sample evidence".into());
    }
    for result in results {
        if result.enabled {
            let enqueued = result.events.saturating_sub(result.rejected_events);
            if result.durable_events != u64::try_from(enqueued).unwrap_or(u64::MAX)
                || result.rejected_events.saturating_mul(100) > result.events
            {
                return Err("enabled event reconciliation or rejection budget failed".into());
            }
        } else if result.rejected_events != 0 || result.durable_events != 0 {
            return Err("baseline event accounting is invalid".into());
        }
        for phase in ["warmup", "idle", "active"] {
            if !result.samples.iter().any(|s| s.phase == phase) {
                return Err(format!("missing {phase} samples"));
            }
        }
        if result.samples.iter().any(|s| {
            s.cpu_percent.is_none()
                || s.rss_kib.is_none()
                || s.cpu_percent
                    .is_some_and(|value| !value.is_finite() || value.is_sign_negative())
                || s.rss_kib
                    .is_some_and(|value| !value.is_finite() || value.is_sign_negative())
                || s.disk_bytes.is_none()
                || (config.profile == Profile::Release && s.network_bytes.is_none())
        }) {
            return Err("required resource metric is missing".into());
        }
        if !result.burst_cpu_percent.is_finite()
            || result.burst_cpu_percent.is_sign_negative()
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
            .any(|result| result.burst_cpu_percent > 100.0)
        {
            return Err("burst CPU exceeded one logical core".into());
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
            .flat_map(|result| result.latencies_us.iter().copied())
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
        RunResult {
            enabled,
            run: 1,
            events: 1,
            rejected_events: 0,
            durable_events: u64::from(enabled),
            latencies_us: vec![1],
            samples,
            durable_bytes: 1,
            burst_cpu_percent: 0.1,
            peak_rss_kib: 1.0,
            peak_disk_bytes: 1,
        }
    }
    fn c() -> Config {
        Config {
            profile: Profile::Release,
            warmup: Duration::ZERO,
            idle: Duration::ZERO,
            active: Duration::ZERO,
            burst: 1,
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
            machine: "sanitized-test-4-logical-core".into(),
            logical_cores: 4,
            filesystem: "testfs".into(),
            power_mode: "ac".into(),
        }
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
        assert_eq!(Config::for_profile(Profile::Release).burst, 10_000);
        assert_eq!(DURABLE_BATCH_RECORDS, 500);
        assert_eq!(DURABLE_BATCH_BYTES, 524_288);
        assert_eq!(DURABLE_BATCH_QUEUE_CAPACITY, 12);
        assert_eq!(DURABLE_BATCH_QUEUE_BYTES_MAX, 6_291_456);
        assert_eq!(DURABLE_HANDOFF_BYTES_MAX, 7_340_032);
        assert_eq!(TOTAL_PIPELINE_PAYLOAD_BYTES_MAX, 11_534_336);
        assert!(PROTOCOL.contains("active_any_minute_percent_max"));
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
        config.burst = 100;
        let mut results = pair(enabled(0.1, 0.1, 1.0, 1, 0));
        for result in &mut results {
            result.events = 100;
            result.latencies_us = vec![1; 100];
        }
        results[1].rejected_events = 1;
        results[1].durable_events = 99;
        assert!(validate_results(config, &results, &[]).is_ok());
        results[1].rejected_events = 2;
        results[1].durable_events = 98;
        assert!(validate_results(config, &results, &[]).is_err());
        results[1].rejected_events = 1;
        assert!(validate_results(config, &results, &[]).is_err());
    }

    #[test]
    fn manifest_contains_computed_phase_and_network_evidence() {
        let manifest = render_manifest(c(), &host(), &pair(enabled(0.1, 0.1, 1.0, 1, 0)), &[]);
        assert!(manifest.contains("machine: sanitized-test-4-logical-core"));
        assert!(manifest.contains("filesystem: testfs"));
        assert!(manifest.contains("power_mode: ac"));
        assert!(manifest.contains("durable_path: run-relative/durable"));
        assert!(manifest.contains("idle_average_cpu_delta_percent: 0"));
        assert!(manifest.contains("active_any_minute_cpu_delta_percent: 0"));
        assert!(manifest.contains("durable_batch_queue_capacity: 12"));
        assert!(manifest.contains("durable_batch_queue_bytes_max: 6291456"));
        assert!(manifest.contains("durable_handoff_bytes_max: 7340032"));
        assert!(manifest.contains("total_pipeline_payload_bytes_max: 11534336"));
        assert!(manifest.contains("network_static_surface: pass"));
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
        validate_manifest_shape(&hostile_error).unwrap();
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
    fn durable_batch_handoff_blocks_at_capacity_and_resumes_without_loss() {
        let (input_sender, input_receiver) = mpsc::channel();
        for index in 0..DURABLE_BATCH_QUEUE_CAPACITY + 2 {
            input_sender
                .send(IngressMessage(vec![u8::try_from(index).unwrap()]))
                .unwrap();
        }
        drop(input_sender);
        let (batch_sender, batch_receiver) = mpsc::sync_channel(DURABLE_BATCH_QUEUE_CAPACITY);
        let pump = thread::spawn(move || {
            pump_batches(&input_receiver, &batch_sender, &mut None, 1, usize::MAX)
        });
        thread::sleep(Duration::from_millis(20));
        assert!(!pump.is_finished());
        let mut delivered = vec![batch_receiver.recv().unwrap()];
        thread::sleep(Duration::from_millis(20));
        assert!(!pump.is_finished());
        delivered.push(batch_receiver.recv().unwrap());
        pump.join().unwrap().unwrap();
        delivered.extend(batch_receiver);
        assert_eq!(delivered.len(), DURABLE_BATCH_QUEUE_CAPACITY + 2);
        assert_eq!(
            delivered
                .into_iter()
                .map(|batch| batch[0].0[0])
                .collect::<Vec<_>>(),
            (0..u8::try_from(DURABLE_BATCH_QUEUE_CAPACITY + 2).unwrap()).collect::<Vec<_>>()
        );
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
