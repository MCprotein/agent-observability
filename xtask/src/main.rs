use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as FmtWrite;
use std::fs::{self, Permissions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
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
    Admission, ENQUEUE_DEADLINE_MS, Ingress, IngressMessage, IngressOutcome, LocalRuntimeConfigV1,
    PressureSample, RuntimeControl, StorageBudget,
};
use agent_observability_local_store::LocalStore;

const USAGE: &str = "usage: cargo run -p xtask -- perf local --profile <release|smoke> --check";
const PROTOCOL: &str = include_str!("../../crates/contracts/performance/local-performance-v1.yaml");
const REQUIRED_PROTOCOL_LINES: [&str; 27] = [
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
    "burst_integrated_and_sampled_percent_max: 100",
    "p95_mib_max: 96",
    "burst_and_drain_peak_required: true",
    "total_bytes_max: 1073741824",
    "ingest_requests_in_flight_max: 0",
    "required_bytes: 0",
    "channel_capacity: 64",
    "normalization_workers: 1",
    "enqueue_deadline_ms: 10",
    "enabled_rejection_percent_max: 1",
    "required_fields: [machine, os, filesystem, power_mode, cold_warm_cache, logical_cores, source_versions, workload, phase_metrics, all_run_samples, baseline, enabled]",
    "fail_closed: missing or breached required metrics produce non-zero exit",
    "event_reconciliation: after graceful fixture shutdown enabled enqueued events must equal durable observations; every rejection remains explicit",
    "output: docs/evidence/local/performance/<run>/manifest.yaml",
];
const SOURCES: [(&str, AgentSource); 3] = [
    ("codex", AgentSource::Codex),
    ("claude-code", AgentSource::ClaudeCode),
    ("cursor", AgentSource::Cursor),
];

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

#[derive(Clone, Copy, Debug, Default)]
struct PressurePeaks {
    cpu_percent: f64,
    rss_kib: f64,
    disk_bytes: u64,
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
    fs::set_permissions(&root, Permissions::from_mode(0o700))
        .map_err(|e| format!("protect manifest directory: {e}"))?;
    let durable_root = if config.profile == Profile::Smoke {
        env::temp_dir().join(format!("agent-observability-perf-{stamp}"))
    } else {
        root.join("durable")
    };
    fs::create_dir_all(&durable_root).map_err(|e| format!("create evidence directory: {e}"))?;
    fs::set_permissions(&durable_root, Permissions::from_mode(0o700))
        .map_err(|e| format!("protect durable evidence directory: {e}"))?;
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for enabled in [false, true] {
        for run_number in 1..=config.runs {
            match execute_run(config, enabled, run_number, &durable_root) {
                Ok(result) => results.push(result),
                Err(error) => errors.push(format!(
                    "{} run {run_number}: {error}",
                    if enabled { "enabled" } else { "baseline" }
                )),
            }
        }
    }
    let manifest_path = root.join("manifest.yaml");
    fs::write(&manifest_path, render_manifest(config, &results, &errors))
        .map_err(|e| format!("write manifest: {e}"))?;
    let pending_manifest =
        fs::read_to_string(&manifest_path).map_err(|e| format!("read pending manifest: {e}"))?;
    validate_manifest_shape(&pending_manifest)?;
    if let Err(error) = validate_results(config, &results, &errors) {
        if config.profile == Profile::Smoke {
            let _ = fs::remove_dir_all(&durable_root);
            let _ = fs::remove_dir_all(&root);
        }
        return Err(format!(
            "performance check failed; manifest: {}: {error}",
            manifest_path.display()
        ));
    }
    let finalized = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("read manifest: {e}"))?
        .replace("status: pending-validation", "status: pass");
    fs::write(&manifest_path, finalized).map_err(|e| format!("finalize manifest: {e}"))?;
    fs::remove_dir_all(&durable_root).map_err(|e| format!("remove durable evidence path: {e}"))?;
    if config.profile == Profile::Smoke {
        fs::remove_dir_all(&root).map_err(|e| format!("remove smoke manifest: {e}"))?;
    }
    println!(
        "manifest={}\nprofile={}\nstatus=pass",
        manifest_path.display(),
        profile_name(config.profile)
    );
    Ok(())
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
    let mut child = spawn_worker(&path, enabled)?;
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
        &path,
        child.id(),
        started,
        network_baseline,
    )?;
    sample_phase(
        &mut samples,
        "idle",
        config.idle,
        config.sample,
        &path,
        child.id(),
        started,
        network_baseline,
    )?;
    let cpu_before = process_cpu_seconds(child.id())?;
    let burst_started = Instant::now();
    let burst_sampler = start_pressure_sampler(path.clone(), child.id());
    let mut latencies_us = Vec::with_capacity(config.burst);
    let mut rejected_events = 0_usize;
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
    let burst_elapsed = burst_started.elapsed();
    let burst_peaks = stop_pressure_sampler(burst_sampler)?;
    let cpu_after = process_cpu_seconds(child.id())?;
    let burst_cpu_percent = if burst_elapsed.is_zero() {
        0.0
    } else {
        (cpu_after - cpu_before).max(0.0) / burst_elapsed.as_secs_f64() * 100.0
    };
    samples.push(sample(
        &path,
        child.id(),
        "active",
        started.elapsed(),
        network_baseline,
    )?);
    sample_phase(
        &mut samples,
        "active",
        config.active,
        config.sample,
        &path,
        child.id(),
        started,
        network_baseline,
    )?;
    let drain_sampler = start_pressure_sampler(path.clone(), child.id());
    drop(writer);
    let status = child.wait().map_err(|e| format!("wait for worker: {e}"))?;
    let drain_peaks = stop_pressure_sampler(drain_sampler)?;
    if !status.success() {
        return Err(format!("worker exited with {status}"));
    }
    let durable_bytes = StorageBudget::allocated_tree_bytes(&path)
        .map_err(|e| format!("measure allocated durable bytes: {e}"))?;
    let durable_events = if enabled {
        LocalStore::open(&path)
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

type PressureSampler = (
    mpsc::Sender<()>,
    thread::JoinHandle<Result<PressurePeaks, String>>,
);

fn start_pressure_sampler(path: PathBuf, pid: u32) -> PressureSampler {
    let (stop_tx, stop_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let mut peaks = PressurePeaks::default();
        loop {
            peaks.cpu_percent = peaks.cpu_percent.max(
                ps_value(pid, "%cpu")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0.0),
            );
            peaks.rss_kib = peaks.rss_kib.max(
                ps_value(pid, "rss")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0.0),
            );
            peaks.disk_bytes = peaks.disk_bytes.max(
                StorageBudget::allocated_tree_bytes(&path)
                    .map_err(|e| format!("measure pressure disk peak: {e}"))?,
            );
            if stop_rx.recv_timeout(Duration::from_millis(10)).is_ok() {
                break;
            }
        }
        Ok(peaks)
    });
    (stop_tx, handle)
}

fn stop_pressure_sampler(sampler: PressureSampler) -> Result<PressurePeaks, String> {
    let (stop, handle) = sampler;
    let _ = stop.send(());
    handle
        .join()
        .map_err(|_| "pressure sampler panicked".to_string())?
}

fn process_cpu_seconds(pid: u32) -> Result<f64, String> {
    let value = ps_value(pid, "time").ok_or("process CPU time is unavailable")?;
    let (minutes, seconds) = value
        .rsplit_once(':')
        .ok_or("process CPU time has an invalid format")?;
    let minutes = minutes
        .parse::<u32>()
        .map_err(|_| "process CPU minutes are invalid")?;
    let seconds = seconds
        .parse::<f64>()
        .map_err(|_| "process CPU seconds are invalid")?;
    Ok(f64::from(minutes) * 60.0 + seconds)
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
    let mut next_sample = phase_started + interval;
    while phase_started.elapsed() < duration {
        sleep(next_sample.saturating_duration_since(Instant::now()));
        samples.push(sample(
            path,
            pid,
            phase,
            started.elapsed(),
            network_baseline,
        )?);
        next_sample += interval;
    }
    if !samples.iter().any(|s| s.phase == phase) {
        samples.push(sample(
            path,
            pid,
            phase,
            started.elapsed(),
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
        return Ok(());
    }
    let (ingress, receiver) = Ingress::new();
    let (tx, rx) = mpsc::channel();
    let drain_path = PathBuf::from(path);
    thread::spawn(move || {
        let _ = tx.send(drain(&receiver, &drain_path));
    });
    for line in io::stdin().lock().lines() {
        let command = line.map_err(|e| format!("read worker command: {e}"))?;
        let deadline = Instant::now() + Duration::from_millis(ENQUEUE_DEADLINE_MS);
        let outcome = loop {
            match ingress.try_send_projected(command.len(), command.as_bytes()) {
                IngressOutcome::Accepted => break "ok",
                IngressOutcome::Full if Instant::now() < deadline => thread::yield_now(),
                IngressOutcome::Full => break "full",
                IngressOutcome::Oversized => break "oversized",
                IngressOutcome::Unavailable => break "unavailable",
            }
        };
        writeln!(output, "{outcome}").map_err(|e| format!("write worker response: {e}"))?;
        output
            .flush()
            .map_err(|e| format!("flush worker response: {e}"))?;
    }
    drop(ingress);
    rx.recv()
        .map_err(|_| "local runtime drain stopped".to_string())??;
    Ok(())
}
fn drain(receiver: &std::sync::mpsc::Receiver<IngressMessage>, path: &Path) -> Result<(), String> {
    let mut store = LocalStore::open(path).map_err(|e| format!("open local durable store: {e}"))?;
    let config = LocalRuntimeConfigV1::default();
    let mut control = RuntimeControl::new(&config).map_err(|e| e.to_string())?;
    let mut previous = BTreeMap::new();
    let mut pending = None;
    while let Some(messages) = receive_batch(
        receiver,
        &mut pending,
        usize::from(config.collection.max_batch_records),
        usize::try_from(config.collection.max_batch_bytes).unwrap_or(usize::MAX),
    )? {
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
        let message = match receiver.try_recv() {
            Ok(message) => message,
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => break,
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
    network_baseline: Option<u64>,
) -> Result<Sample, String> {
    Ok(Sample {
        phase: phase.into(),
        elapsed_ms: elapsed.as_millis(),
        cpu_percent: ps_value(pid, "%cpu").and_then(|v| v.parse().ok()),
        rss_kib: ps_value(pid, "rss").and_then(|v| v.parse().ok()),
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
        return Ok(0);
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
fn ps_value(pid: u32, field: &str) -> Option<String> {
    let output = Command::new("ps")
        .args(["-o", field, "-p", &pid.to_string()])
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .nth(1)?
        .trim()
        .to_owned();
    (!value.is_empty()).then_some(value)
}

#[allow(clippy::too_many_lines)]
fn render_manifest(config: Config, results: &[RunResult], errors: &[String]) -> String {
    let all = results
        .iter()
        .filter(|result| result.enabled)
        .flat_map(|r| r.latencies_us.iter().copied())
        .collect::<Vec<_>>();
    let mut out = format!(
        "schema_version: local_performance.v1\nprofile: {}\nprotocol: crates/contracts/performance/local-performance-v1.yaml\nstatus: pending-validation\nmachine: sanitized-local-host\nos: {}\nfilesystem: local-filesystem\npower_mode: unspecified\ncold_warm_cache: warm\nlogical_cores: {}\nsource_versions:\n  product: {}\n  runtime_config: local_runtime.v1\n  durable_store: local_state.v3\nbaseline:\n  runs: {}\nenabled:\n  runs: {}\nworkload:\n  warmup_seconds: {}\n  idle_seconds: {}\n  active_seconds: {}\n  burst_events: {}\n  sample_interval_seconds: {}\n  adapters: [codex, claude-code, cursor]\n  schedule: round-robin-codex-claude-code-cursor\n  channel_capacity: 64\n  normalization_workers: 1\n  enqueue_deadline_ms: 10\n  command_boundary: fixed-capacity-local-runtime-ingress\n  worker_boundary: asynchronous-local-store-drain\n  foreground_response: bounded-enqueue-acceptance\n  durable_path: removed-after-measurement\nall_run_samples:\n",
        profile_name(config.profile),
        env::consts::OS,
        thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
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
        "phase_metrics:\n  idle_average_cpu_delta_percent: {}\n  active_average_cpu_delta_percent: {}\n  active_any_minute_cpu_delta_percent: {}\nmetrics:\n  hook_latency_p95_us: {}\n  hook_latency_p99_us: {}\n  idle_average_cpu_delta_percent: {}\n  active_average_cpu_delta_percent: {}\n  active_any_minute_cpu_delta_percent: {}\n  enabled_rss_p95_kib: {}\n  total_allocated_disk_bytes: {}\n  network_bytes: {}\n  network_static_surface: pass\n  required: [hook_latency_p95_us, hook_latency_p99_us, idle_average_cpu_delta_percent, active_average_cpu_delta_percent, active_any_minute_cpu_delta_percent, enabled_rss_p95_kib, total_allocated_disk_bytes, network_bytes, network_static_surface]\n  network_mode: process-scoped-samples-plus-static-product-surface\n  evidence_scope: subprocess-plus-fixed-capacity-ingress-plus-asynchronous-local-store-drain\nerrors: {:?}",
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
        summary.map_or_else(|| "null".into(), |value| value.network_bytes.to_string()),
        errors
    );
    out
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
    if !errors.is_empty() || results.len() != config.runs * 2 {
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
                || s.disk_bytes.is_none()
                || (config.profile == Profile::Release && s.network_bytes.is_none())
        }) {
            return Err("required resource metric is missing".into());
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
        let manifest = render_manifest(c(), &pair(enabled(0.1, 0.1, 1.0, 1, 0)), &[]);
        assert!(manifest.contains("idle_average_cpu_delta_percent: 0"));
        assert!(manifest.contains("active_any_minute_cpu_delta_percent: 0"));
        assert!(manifest.contains("network_static_surface: pass"));
        assert!(manifest.contains("network_bytes: 0"));
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
}
