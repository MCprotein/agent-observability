use agent_observability_adapter_claude_code::{
    AdapterItem as ClaudeAdapterItem, read_handoff_file as read_claude_handoff_file,
};
use agent_observability_adapter_codex::{
    AdapterItem as CodexAdapterItem, parse_handoff_jsonl as parse_codex_handoff_jsonl,
    read_handoff_file as read_codex_handoff_file,
};
use agent_observability_adapter_cursor::{
    AdapterItem as CursorAdapterItem, read_handoff_file as read_cursor_handoff_file,
};
use agent_observability_application::{parse_rate_table_json, project_report};
use agent_observability_contracts::{
    AdapterDispositionCode, AdapterDispositionKind, REPORT_DTO_VERSION, SourceCheckpoint,
    SourceObservation,
};
use agent_observability_contracts::{CONTRACT_MANIFEST, ContractManifest};
use agent_observability_local_runtime::{
    Admission, ConfigMutationGuard, InstalledLayout, LOCAL_RUNTIME_CONFIG_VERSION,
    LocalRuntimeConfigV2, PressureSample, RuntimeControl, Singleton, StorageBudget, install, load,
    save,
};
use agent_observability_local_store::{
    IngestStatus, LOCAL_STORE_SCHEMA_VERSION, LocalStore, RetentionPlan,
};
use agent_observability_local_ui::PreparedUi;
use agent_observability_static_report::write_private;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::Command;
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const USAGE: &str = "Agent Observability (`agentobs`, legacy alias: `agent-observability`)

Quick start:
  agentobs demo [root] [--no-open]       Open the sample monitoring dashboard
  agentobs setup [root] [--no-open]      Initialize a runtime and open monitoring
  agentobs dashboard [root] [--no-open]  Refresh and open the monitoring dashboard
  agentobs ui [root] [--no-open]         Open settings only

Configuration:
  agentobs config show [root]
  agentobs config set [root] <option> <value>

Import and report:
  agentobs <codex|claude-code|cursor>-ingest <root> <handoff-jsonl>
  agentobs report <root> [rate-table-json]

Maintenance:
  agentobs retention-plan <root>
  agentobs retention-apply <root> <plan-id> <private-archive-jsonl>
  agentobs init|runtime-check|storage-check <root>
  agentobs config-check <config-json>
  agentobs contracts|version|help";
const REPORT_FILE_NAME: &str = "agent-observability-report.html";
const MAX_RATE_TABLE_BYTES: u64 = 1_048_576;
const DEFAULT_ROOT_NAME: &str = ".agent-observability";
const DEFAULT_DEMO_ROOT_NAME: &str = ".agent-observability-demo";
const CODEX_DEMO_HANDOFF: &str = include_str!("../../../examples/codex-handoff.v1.jsonl");

enum IngestItem<'a> {
    Observation(&'a SourceObservation),
    Disposition {
        checkpoint: &'a SourceCheckpoint,
        disposition: AdapterDispositionKind,
        code: AdapterDispositionCode,
        payload_hash: Option<&'a str>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IngestBlock {
    CollectionDisabled,
    Policy,
    Pressure,
    Storage,
}

#[derive(Debug, PartialEq, Eq)]
struct IngestResult {
    source: String,
    observations: u64,
    diagnostics: u64,
    duplicates: u64,
    suppressed: u64,
    blocked: Option<IngestBlock>,
}

impl IngestResult {
    fn blocked(source: &str, blocked: IngestBlock) -> Self {
        Self {
            source: source.into(),
            observations: 0,
            diagnostics: 0,
            duplicates: 0,
            suppressed: 0,
            blocked: Some(blocked),
        }
    }

    fn output(&self) -> String {
        format!(
            "source={}\nobservations={}\ndiagnostics={}\nduplicates={}\nsuppressed={}\ncollection_disabled={}\npolicy_blocked={}\npressure_blocked={}\nstorage_blocked={}\nteam_ingest=disabled",
            self.source,
            self.observations,
            self.diagnostics,
            self.duplicates,
            self.suppressed,
            u8::from(self.blocked == Some(IngestBlock::CollectionDisabled)),
            u8::from(self.blocked == Some(IngestBlock::Policy)),
            u8::from(self.blocked == Some(IngestBlock::Pressure)),
            u8::from(self.blocked == Some(IngestBlock::Storage)),
        )
    }
}

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if let Some(result) = run_ui_command(&arguments) {
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("{message}");
                ExitCode::FAILURE
            }
        };
    }
    match run(arguments.into_iter()) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run_ui_command(arguments: &[String]) -> Option<Result<(), String>> {
    let result = match arguments {
        [command] if command == "ui" => default_root().and_then(|root| settings_ui(&root, true)),
        [command, flag] if command == "ui" && flag == "--no-open" => {
            default_root().and_then(|root| settings_ui(&root, false))
        }
        [command, root] if command == "ui" => settings_ui(Path::new(root), true),
        [command, root, flag] if command == "ui" && flag == "--no-open" => {
            settings_ui(Path::new(root), false)
        }
        _ => return None,
    };
    Some(result)
}

fn settings_ui(root: &Path, open: bool) -> Result<(), String> {
    let layout = install(root).map_err(|error| error.to_string())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("settings runtime failed: {error}"))?;
    runtime.block_on(async {
        let ui = agent_observability_local_ui::prepare(&layout)
            .await
            .map_err(|error| error.to_string())?;
        announce_settings_ui(&ui, open)?;
        ui.serve().await.map_err(|error| error.to_string())
    })
}

fn announce_settings_ui(ui: &PreparedUi, open: bool) -> Result<(), String> {
    let opened = if open {
        match open_settings_url(ui.url()) {
            Ok(()) => true,
            Err(error) => {
                eprintln!("{error}");
                false
            }
        }
    } else {
        false
    };
    println!("status=settings_ready");
    if !opened {
        println!("url={}", ui.url());
    }
    println!("opened={opened}\ncollection=manual_import");
    std::io::stdout()
        .flush()
        .map_err(|error| format!("settings output failed: {error}"))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_settings_url(url: &str) -> Result<(), String> {
    let mut child = Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|error| format!("settings UI open failed: {error}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn open_settings_url(_url: &str) -> Result<(), String> {
    Err("automatic settings UI open is supported on macOS; use ui --no-open and open the reported URL".into())
}

fn run(arguments: impl Iterator<Item = String>) -> Result<String, String> {
    let arguments: Vec<String> = arguments.collect();
    if let Some(result) = run_onboarding(&arguments) {
        return result;
    }
    match arguments.as_slice() {
        [command, root] if command == "init" => {
            let layout = install(Path::new(root)).map_err(|error| error.to_string())?;
            Ok(format!(
                "config_schema={LOCAL_RUNTIME_CONFIG_VERSION}\nroot={}\nconfig={}\nteam_ingest=disabled",
                layout.root.display(),
                layout.config.display()
            ))
        }
        [command, config] if command == "config-check" => {
            let config = load(Path::new(config)).map_err(|error| error.to_string())?;
            Ok(format!(
                "config_schema={LOCAL_RUNTIME_CONFIG_VERSION}\nenabled={}\nlocal_storage_budget_bytes={}\nmax_record_age_days={}\nmax_archive_records={}\nmax_archive_bytes={}\nteam_ingest=disabled",
                config.enabled,
                config.collection.local_storage_budget_bytes,
                config.retention.max_record_age_days,
                config.retention.max_archive_records,
                config.retention.max_archive_bytes
            ))
        }
        [command, root] if command == "runtime-check" => runtime_check(Path::new(root)),
        [command] if command == "contracts" => contracts(),
        [command, root] if command == "storage-check" => storage_check(Path::new(root)),
        [command, root] if command == "retention-plan" => retention(Path::new(root), None),
        [command, root, plan_id, archive] if command == "retention-apply" => retention(
            Path::new(root),
            Some((plan_id.as_str(), Path::new(archive))),
        ),
        [command, directory, handoff] if command == "codex-ingest" => {
            let batch = read_codex_handoff_file(handoff).map_err(|error| error.to_string())?;
            ingest_items(
                directory,
                "codex",
                batch.items.iter().map(|item| match item {
                    CodexAdapterItem::Observation(observation) => {
                        IngestItem::Observation(observation)
                    }
                    CodexAdapterItem::Disposition(diagnostic) => IngestItem::Disposition {
                        checkpoint: &diagnostic.checkpoint,
                        disposition: diagnostic.disposition,
                        code: diagnostic.code,
                        payload_hash: diagnostic.payload_hash.as_deref(),
                    },
                }),
            )
            .map(|result| result.output())
        }
        [command, directory, handoff] if command == "claude-code-ingest" => {
            let batch = read_claude_handoff_file(handoff).map_err(|error| error.to_string())?;
            ingest_items(
                directory,
                "claude-code",
                batch.items.iter().map(|item| match item {
                    ClaudeAdapterItem::Observation(observation) => {
                        IngestItem::Observation(observation)
                    }
                    ClaudeAdapterItem::Disposition(diagnostic) => IngestItem::Disposition {
                        checkpoint: &diagnostic.checkpoint,
                        disposition: diagnostic.disposition,
                        code: diagnostic.code,
                        payload_hash: diagnostic.payload_hash.as_deref(),
                    },
                }),
            )
            .map(|result| result.output())
        }
        [command, directory, handoff] if command == "cursor-ingest" => {
            let batch = read_cursor_handoff_file(handoff).map_err(|error| error.to_string())?;
            ingest_items(
                directory,
                "cursor",
                batch.items.iter().map(|item| match item {
                    CursorAdapterItem::Observation(observation) => {
                        IngestItem::Observation(observation)
                    }
                    CursorAdapterItem::Disposition(diagnostic) => IngestItem::Disposition {
                        checkpoint: &diagnostic.checkpoint,
                        disposition: diagnostic.disposition,
                        code: diagnostic.code,
                        payload_hash: diagnostic.payload_hash.as_deref(),
                    },
                }),
            )
            .map(|result| result.output())
        }
        [command, report_arguments @ ..] if command == "report" => report_command(report_arguments),
        [command] if matches!(command.as_str(), "version" | "--version" | "-V") => {
            Ok(env!("CARGO_PKG_VERSION").into())
        }
        [] => Ok(USAGE.into()),
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => Ok(USAGE.into()),
        [command] => Err(format!("unknown command {command}")),
        _ => Err(USAGE.into()),
    }
}

fn run_onboarding(arguments: &[String]) -> Option<Result<String, String>> {
    let result = match arguments {
        [command] if command == "demo" => default_demo_root().and_then(|root| demo(&root, true)),
        [command, flag] if command == "demo" && flag == "--no-open" => {
            default_demo_root().and_then(|root| demo(&root, false))
        }
        [command, root] if command == "demo" => demo(Path::new(root), true),
        [command, root, flag] if command == "demo" && flag == "--no-open" => {
            demo(Path::new(root), false)
        }
        [command] if command == "setup" => default_root().and_then(|root| setup(&root, true)),
        [command, flag] if command == "setup" && flag == "--no-open" => {
            default_root().and_then(|root| setup(&root, false))
        }
        [command, root] if command == "setup" => setup(Path::new(root), true),
        [command, root, flag] if command == "setup" && flag == "--no-open" => {
            setup(Path::new(root), false)
        }
        [command] if command == "dashboard" => {
            default_root().and_then(|root| dashboard(&root, true))
        }
        [command, flag] if command == "dashboard" && flag == "--no-open" => {
            default_root().and_then(|root| dashboard(&root, false))
        }
        [command, root] if command == "dashboard" => dashboard(Path::new(root), true),
        [command, root, flag] if command == "dashboard" && flag == "--no-open" => {
            dashboard(Path::new(root), false)
        }
        [command] if command == "config" => default_root().and_then(|root| show_config(&root)),
        [command, action] if command == "config" && action == "show" => {
            default_root().and_then(|root| show_config(&root))
        }
        [command, action, root] if command == "config" && action == "show" => {
            show_config(Path::new(root))
        }
        [command, action, key, value] if command == "config" && action == "set" => {
            default_root().and_then(|root| update_config(&root, key, value))
        }
        [command, action, root, key, value] if command == "config" && action == "set" => {
            update_config(Path::new(root), key, value)
        }
        _ => return None,
    };
    Some(result)
}

fn default_root() -> Result<PathBuf, String> {
    default_home_root(DEFAULT_ROOT_NAME)
}

fn default_demo_root() -> Result<PathBuf, String> {
    default_home_root(DEFAULT_DEMO_ROOT_NAME)
}

fn default_home_root(name: &str) -> Result<PathBuf, String> {
    env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(name))
        .ok_or_else(|| "HOME is not set; pass an explicit runtime root".into())
}

fn demo(root: &Path, open: bool) -> Result<String, String> {
    let root_text = root
        .to_str()
        .ok_or_else(|| "demo runtime root must be valid UTF-8".to_string())?;
    let batch = parse_codex_handoff_jsonl(CODEX_DEMO_HANDOFF).map_err(|error| error.to_string())?;
    let ingest = ingest_items(
        root_text,
        "codex",
        batch.items.iter().map(|item| match item {
            CodexAdapterItem::Observation(observation) => IngestItem::Observation(observation),
            CodexAdapterItem::Disposition(diagnostic) => IngestItem::Disposition {
                checkpoint: &diagnostic.checkpoint,
                disposition: diagnostic.disposition,
                code: diagnostic.code,
                payload_hash: diagnostic.payload_hash.as_deref(),
            },
        }),
    )?;
    require_demo_ingest(&ingest)?;
    let dashboard = prepare_dashboard(root, open)?;
    if current_record_count(root)? == 0 {
        return Err("demo completed ingest but the dashboard has no observable data".into());
    }
    Ok(format!(
        "status=demo_ready\nroot={}\n{}\ndashboard={}\nopened={open}",
        root.display(),
        ingest.output(),
        dashboard.display()
    ))
}

fn require_demo_ingest(ingest: &IngestResult) -> Result<(), String> {
    if ingest.blocked.is_some() {
        Err(format!(
            "demo could not create observable data\n{}",
            ingest.output()
        ))
    } else {
        Ok(())
    }
}

fn setup(root: &Path, open: bool) -> Result<String, String> {
    let layout = install(root).map_err(|error| error.to_string())?;
    let dashboard = prepare_dashboard(&layout.root, open)?;
    Ok(format!(
        "status=ready\nroot={}\ndashboard={}\ncollection=manual_import\nopened={open}",
        layout.root.display(),
        dashboard.display()
    ))
}

fn dashboard(root: &Path, open: bool) -> Result<String, String> {
    let dashboard = prepare_dashboard(root, open)?;
    Ok(format!("dashboard={}\nopened={open}", dashboard.display()))
}

fn prepare_dashboard(root: &Path, open: bool) -> Result<PathBuf, String> {
    prepare_dashboard_with(root, open, open_dashboard)
}

fn prepare_dashboard_with(
    root: &Path,
    open: bool,
    opener: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    let layout = install(root).map_err(|error| error.to_string())?;
    let _ = report(&layout.root, None)?;
    let dashboard = layout.logs.join(REPORT_FILE_NAME);
    if open {
        opener(&dashboard)?;
    }
    Ok(dashboard)
}

fn current_record_count(root: &Path) -> Result<usize, String> {
    let layout = install(root).map_err(|error| error.to_string())?;
    let config = load(&layout.config).map_err(|error| error.to_string())?;
    let _singleton = Singleton::acquire(&layout.runtime).map_err(|error| error.to_string())?;
    let store = open_store(&layout, &config)?;
    store
        .current_records()
        .map(|records| records.len())
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn open_dashboard(path: &Path) -> Result<(), String> {
    let status = Command::new("open")
        .arg(path)
        .status()
        .map_err(|error| format!("dashboard open failed: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("dashboard open failed with status {status}"))
    }
}

#[cfg(not(target_os = "macos"))]
fn open_dashboard(_path: &Path) -> Result<(), String> {
    Err(
        "dashboard open is supported on macOS; use setup --no-open and open the reported file"
            .into(),
    )
}

fn show_config(root: &Path) -> Result<String, String> {
    let layout = install(root).map_err(|error| error.to_string())?;
    let config = load(&layout.config).map_err(|error| error.to_string())?;
    Ok(config_output(&layout, &config))
}

fn update_config(root: &Path, key: &str, value: &str) -> Result<String, String> {
    let layout = install(root).map_err(|error| error.to_string())?;
    let mutation = ConfigMutationGuard::acquire(&layout).map_err(|error| error.to_string())?;
    let mut config = load(&layout.config).map_err(|error| error.to_string())?;
    set_config_value(&mut config, key, value)?;
    save(&mutation, &config).map_err(|error| error.to_string())?;
    Ok(format!(
        "updated={key}\n{}",
        config_output(&layout, &config)
    ))
}

fn set_config_value(
    config: &mut LocalRuntimeConfigV2,
    key: &str,
    value: &str,
) -> Result<(), String> {
    macro_rules! parse {
        ($type:ty) => {
            value
                .parse::<$type>()
                .map_err(|_| format!("invalid value for {key}: {value}"))?
        };
    }

    match key {
        "enabled" => config.enabled = parse!(bool),
        "file-reconcile-ms" => config.collection.file_reconcile_interval_ms = parse!(u32),
        "flush-ms" => config.collection.flush_interval_ms = parse!(u32),
        "batch-records" => config.collection.max_batch_records = parse!(u16),
        "batch-bytes" => config.collection.max_batch_bytes = parse!(u32),
        "active-heartbeat-ms" => config.collection.active_heartbeat_interval_ms = parse!(u32),
        "idle-heartbeat-ms" => config.collection.idle_heartbeat_interval_ms = parse!(u32),
        "storage-bytes" => config.collection.local_storage_budget_bytes = parse!(u64),
        "retention-days" => config.retention.max_record_age_days = parse!(u16),
        "archive-records" => config.retention.max_archive_records = parse!(u32),
        "archive-bytes" => config.retention.max_archive_bytes = parse!(u64),
        _ => return Err(format!("unknown config option {key}")),
    }
    config.validate().map_err(|error| error.to_string())
}

fn config_output(layout: &InstalledLayout, config: &LocalRuntimeConfigV2) -> String {
    format!(
        "root={}\nconfig={}\nenabled={}\nfile-reconcile-ms={}\nflush-ms={}\nbatch-records={}\nbatch-bytes={}\nactive-heartbeat-ms={}\nidle-heartbeat-ms={}\nstorage-bytes={}\nretention-days={}\narchive-records={}\narchive-bytes={}",
        layout.root.display(),
        layout.config.display(),
        config.enabled,
        config.collection.file_reconcile_interval_ms,
        config.collection.flush_interval_ms,
        config.collection.max_batch_records,
        config.collection.max_batch_bytes,
        config.collection.active_heartbeat_interval_ms,
        config.collection.idle_heartbeat_interval_ms,
        config.collection.local_storage_budget_bytes,
        config.retention.max_record_age_days,
        config.retention.max_archive_records,
        config.retention.max_archive_bytes
    )
}

fn storage_check(root: &Path) -> Result<String, String> {
    let layout = install(root).map_err(|error| error.to_string())?;
    let config = load(&layout.config).map_err(|error| error.to_string())?;
    let _singleton = Singleton::acquire(&layout.runtime).map_err(|error| error.to_string())?;
    let store = open_store(&layout, &config)?;
    let (observations, records, outcomes) = store.counts().map_err(|error| error.to_string())?;
    Ok(format!(
        "store_schema={LOCAL_STORE_SCHEMA_VERSION}\nobservations={observations}\nrecords={records}\ndelivery_outcomes={outcomes}\nteam_ingest=disabled"
    ))
}

fn open_store(
    layout: &InstalledLayout,
    config: &LocalRuntimeConfigV2,
) -> Result<LocalStore, String> {
    let control = RuntimeControl::new(config).map_err(|error| error.to_string())?;
    let migration_headroom = control
        .migration_headroom(&layout.root)
        .map_err(|error| error.to_string())?;
    LocalStore::open_with_migration_headroom(layout.state.join("store"), migration_headroom)
        .map_err(|error| error.to_string())
}

fn retention(root: &Path, apply: Option<(&str, &Path)>) -> Result<String, String> {
    let layout = install(root).map_err(|error| error.to_string())?;
    let apply = apply
        .map(|(plan_id, path)| normalize_archive_path(&layout.root, plan_id, path))
        .transpose()?;
    let config = load(&layout.config).map_err(|error| error.to_string())?;
    let _singleton = Singleton::acquire(&layout.runtime).map_err(|error| error.to_string())?;
    let store = open_store(&layout, &config)?;
    let now_unix_ms = current_unix_ms()?;
    let retention_ms = u64::from(config.retention.max_record_age_days)
        .checked_mul(86_400_000)
        .ok_or_else(|| "retention cutoff overflow".to_string())?;
    let cutoff_unix_ms = (now_unix_ms / 86_400_000)
        .saturating_mul(86_400_000)
        .saturating_sub(retention_ms);
    if let Some((expected_plan_id, archive_path)) = apply.as_ref() {
        let result = store
            .apply_retention(
                cutoff_unix_ms,
                config.retention.max_archive_records,
                config.retention.max_archive_bytes,
                expected_plan_id,
                archive_path,
            )
            .map_err(|error| error.to_string())?;
        return Ok(retention_output(
            &result.plan,
            result.archive_path.as_deref(),
            true,
        ));
    }
    let plan = store
        .retention_plan(
            cutoff_unix_ms,
            config.retention.max_archive_records,
            config.retention.max_archive_bytes,
        )
        .map_err(|error| error.to_string())?;
    Ok(retention_output(&plan, None, false))
}

fn normalize_archive_path(
    runtime_root: &Path,
    plan_id: &str,
    archive_path: &Path,
) -> Result<(String, PathBuf), String> {
    let name = archive_path
        .file_name()
        .ok_or_else(|| "retention archive path must name a file".to_string())?;
    let parent = archive_path
        .parent()
        .ok_or_else(|| "retention archive path must have a parent".to_string())?
        .canonicalize()
        .map_err(|error| format!("retention archive parent is unavailable: {error}"))?;
    let normalized = parent.join(name);
    if normalized.starts_with(runtime_root) {
        return Err("retention archive must be outside the managed runtime root".into());
    }
    Ok((plan_id.into(), normalized))
}

fn retention_output(plan: &RetentionPlan, archive: Option<&Path>, applied: bool) -> String {
    format!(
        "plan_id={}\ncutoff_unix_ms={}\ntraces={}\nobservations={}\nrecords={}\narchive_bytes={}\ntruncated={}\napplied={}\narchive={}\nteam_ingest=disabled",
        plan.plan_id,
        plan.cutoff_unix_ms,
        plan.traces,
        plan.observations,
        plan.records,
        plan.archive_bytes,
        plan.truncated,
        u8::from(applied && plan.traces > 0),
        archive.map_or_else(|| "none".into(), |path| path.display().to_string())
    )
}

fn contracts() -> Result<String, String> {
    let manifest = ContractManifest::parse(CONTRACT_MANIFEST).map_err(|error| error.to_string())?;
    manifest
        .validate_release_boundary()
        .map_err(|error| error.to_string())?;
    Ok(CONTRACT_MANIFEST.trim_end().into())
}

fn report_command(arguments: &[String]) -> Result<String, String> {
    match arguments {
        [root] => report(Path::new(root), None),
        [root, rate_table] => report(Path::new(root), Some(Path::new(rate_table))),
        _ => Err(USAGE.into()),
    }
}

fn report(root: &Path, rate_table_path: Option<&Path>) -> Result<String, String> {
    let layout = install(root).map_err(|error| error.to_string())?;
    let config = load(&layout.config).map_err(|error| error.to_string())?;
    let _singleton = Singleton::acquire(&layout.runtime).map_err(|error| error.to_string())?;
    let store = open_store(&layout, &config)?;
    let records = store.current_records().map_err(|error| error.to_string())?;
    let rate_table = rate_table_path
        .map(read_private_rate_table)
        .transpose()?
        .map(|body| parse_rate_table_json(&body).map_err(|error| error.to_string()))
        .transpose()?;
    let report = project_report(
        &records,
        current_timestamp()?,
        "Agent Observability Report",
        rate_table.as_ref(),
    )
    .map_err(|error| error.to_string())?;
    let output_path = layout.logs.join(REPORT_FILE_NAME);
    let bytes = write_private(&output_path, &report).map_err(|error| error.to_string())?;
    Ok(format!(
        "report_schema={REPORT_DTO_VERSION}\nrecords={}\ncost_status={}\nreport={}\nbytes={bytes}\nteam_ingest=disabled",
        records.len(),
        report.cost.status,
        output_path.display()
    ))
}

fn read_private_rate_table(path: &Path) -> Result<String, String> {
    let file = open_private_read(path)?;
    let metadata = file.metadata().map_err(|_| "rate table metadata failed")?;
    if metadata.len() > MAX_RATE_TABLE_BYTES {
        return Err("rate table exceeds 1 MiB".into());
    }
    let mut body = String::new();
    file.take(MAX_RATE_TABLE_BYTES + 1)
        .read_to_string(&mut body)
        .map_err(|_| "rate table read failed")?;
    if body.len() as u64 > MAX_RATE_TABLE_BYTES {
        return Err("rate table exceeds 1 MiB".into());
    }
    Ok(body)
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn open_private_read(path: &Path) -> Result<File, String> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut options = OpenOptions::new();
    options.read(true).custom_flags(no_follow_flag());
    let file = options.open(path).map_err(|_| "rate table open failed")?;
    let metadata = file.metadata().map_err(|_| "rate table metadata failed")?;
    if !metadata.is_file() {
        return Err("rate table must be a regular file".into());
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err("rate table permissions must be private".into());
    }
    Ok(file)
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
fn open_private_read(_path: &Path) -> Result<File, String> {
    Err("private rate tables are unsupported on this platform".into())
}

#[cfg(any(target_os = "linux", target_os = "android"))]
const fn no_follow_flag() -> i32 {
    0x20_000
}

#[cfg(target_os = "macos")]
const fn no_follow_flag() -> i32 {
    0x100
}

fn current_timestamp() -> Result<String, String> {
    let duration = current_duration()?;
    timestamp_from_duration(duration)
}

fn current_unix_ms() -> Result<u64, String> {
    let duration = current_duration()?;
    u64::try_from(duration.as_millis()).map_err(|_| "system clock is out of range".into())
}

fn current_duration() -> Result<Duration, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before the Unix epoch".into())
}

fn timestamp_from_duration(duration: Duration) -> Result<String, String> {
    let seconds = i64::try_from(duration.as_secs()).map_err(|_| "system clock is out of range")?;
    let days = seconds / 86_400;
    let seconds_in_day = seconds % 86_400;
    let (year, month, day) = civil_date_from_days(days);
    let hour = seconds_in_day / 3_600;
    let minute = (seconds_in_day % 3_600) / 60;
    let second = seconds_in_day % 60;
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        duration.subsec_millis()
    ))
}

fn civil_date_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let days = days_since_epoch + 719_468;
    let era = days / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn runtime_check(root: &Path) -> Result<String, String> {
    let layout = install(root).map_err(|error| error.to_string())?;
    let config = load(&layout.config).map_err(|error| error.to_string())?;
    let _singleton = Singleton::acquire(&layout.runtime).map_err(|error| error.to_string())?;
    let store = open_store(&layout, &config)?;
    drop(store);
    let allocated =
        StorageBudget::allocated_tree_bytes(&layout.root).map_err(|error| error.to_string())?;
    let mut control = RuntimeControl::new(&config).map_err(|error| error.to_string())?;
    let admission = match control
        .storage_budget()
        .admit(allocated, u64::from(config.collection.max_batch_bytes))
    {
        Admission::Allowed { .. } => "allowed",
        Admission::Denied => "denied",
    };
    let schedule = control.evaluate(
        0,
        PressureSample {
            resource_percent: 0,
            disk_percent: control.storage_percent(allocated),
            queue_percent: 0,
        },
    );
    Ok(format!(
        "config_schema={LOCAL_RUNTIME_CONFIG_VERSION}\nstore_schema={LOCAL_STORE_SCHEMA_VERSION}\nallocated_bytes={allocated}\nstorage_admission={admission}\nruntime_state={:?}\nsingleton=held\nteam_ingest=disabled",
        schedule.state
    ))
}

fn ingest_items<'a>(
    directory: &str,
    source: &str,
    items: impl Iterator<Item = IngestItem<'a>>,
) -> Result<IngestResult, String> {
    let paths = ingest_paths(Path::new(directory))?;
    let _singleton =
        Singleton::acquire(&paths.runtime_directory).map_err(|error| error.to_string())?;
    let mut control = RuntimeControl::new(&paths.config).map_err(|error| error.to_string())?;
    let items = items.collect::<Vec<_>>();
    if !paths.config.enabled {
        return Ok(IngestResult::blocked(
            source,
            IngestBlock::CollectionDisabled,
        ));
    }
    if items.len() > usize::from(paths.config.collection.max_batch_records) {
        return Ok(IngestResult::blocked(source, IngestBlock::Policy));
    }
    let allocated = StorageBudget::allocated_tree_bytes(&paths.accounting_root)
        .map_err(|error| error.to_string())?;
    let schedule = control.evaluate(
        0,
        PressureSample {
            resource_percent: 0,
            disk_percent: control.storage_percent(allocated),
            queue_percent: 0,
        },
    );
    if schedule.flush_paused {
        return Ok(IngestResult::blocked(source, IngestBlock::Pressure));
    }
    if control
        .admit(
            &paths.accounting_root,
            ingest_reservation_bytes(
                &paths.store_directory,
                u64::from(paths.config.collection.max_batch_bytes),
            )?,
        )
        .map_err(|error| error.to_string())?
        == Admission::Denied
    {
        return Ok(IngestResult::blocked(source, IngestBlock::Storage));
    }
    let migration_headroom = control
        .migration_headroom(&paths.accounting_root)
        .map_err(|error| error.to_string())?;
    let mut store =
        LocalStore::open_with_migration_headroom(&paths.store_directory, migration_headroom)
            .map_err(|error| error.to_string())?;
    let mut observations = 0_u64;
    let mut diagnostics = 0_u64;
    let mut duplicates = 0_u64;
    let mut suppressed = 0_u64;
    for item in items {
        match item {
            IngestItem::Observation(observation) => {
                let status = store
                    .ingest_deferred_projection(observation)
                    .map_err(|error| error.to_string())?;
                match status {
                    IngestStatus::Committed => observations += 1,
                    IngestStatus::Duplicate => duplicates += 1,
                    IngestStatus::Suppressed => suppressed += 1,
                }
            }
            IngestItem::Disposition {
                checkpoint,
                disposition,
                code,
                payload_hash,
            } => {
                let status = store
                    .ingest_disposition_with_payload(checkpoint, disposition, code, payload_hash)
                    .map_err(|error| error.to_string())?;
                match status {
                    IngestStatus::Committed => match disposition {
                        AdapterDispositionKind::Diagnostic => diagnostics += 1,
                        AdapterDispositionKind::Suppressed => suppressed += 1,
                    },
                    IngestStatus::Duplicate => duplicates += 1,
                    IngestStatus::Suppressed => suppressed += 1,
                }
            }
        }
    }
    store
        .rebuild_projection()
        .map_err(|error| error.to_string())?;
    Ok(IngestResult {
        source: source.into(),
        observations,
        diagnostics,
        duplicates,
        suppressed,
        blocked: None,
    })
}

fn ingest_reservation_bytes(store_directory: &Path, batch_bytes: u64) -> Result<u64, String> {
    if !store_directory.exists() {
        return Ok(batch_bytes);
    }
    let existing =
        StorageBudget::allocated_tree_bytes(store_directory).map_err(|error| error.to_string())?;
    batch_bytes
        .checked_add(existing)
        .ok_or_else(|| "ingest storage reservation overflow".to_string())
}

struct IngestPaths {
    config: LocalRuntimeConfigV2,
    accounting_root: PathBuf,
    runtime_directory: PathBuf,
    store_directory: PathBuf,
}

fn ingest_paths(path: &Path) -> Result<IngestPaths, String> {
    let layout = install(path).map_err(|error| error.to_string())?;
    let config = load(&layout.config).map_err(|error| error.to_string())?;
    Ok(IngestPaths {
        config,
        accounting_root: layout.root,
        runtime_directory: layout.runtime,
        store_directory: layout.state.join("store"),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        IngestBlock, IngestResult, LOCAL_STORE_SCHEMA_VERSION, REPORT_FILE_NAME,
        prepare_dashboard_with, require_demo_ingest, run, timestamp_from_duration,
    };
    use std::fs;
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn contracts_command_exposes_disabled_team_boundary() {
        let output = run(["contracts".into()].into_iter()).expect("contracts command succeeds");
        assert!(output.contains("durable_record=agent_observability.v1"));
        assert!(output.contains("team_ingest=disabled"));
    }

    #[test]
    fn unknown_command_fails_closed() {
        assert!(run(["serve".into()].into_iter()).is_err());
        assert!(run(["storage-check".into()].into_iter()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn init_and_config_check_use_private_strict_config() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-cli-init-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let init = run(["init".into(), root.to_string_lossy().into_owned()].into_iter()).unwrap();
        assert!(init.contains("config_schema=local_runtime.v2"));
        let config = root.join("config.json");
        let check = run(["config-check".into(), config.to_string_lossy().into_owned()].into_iter())
            .unwrap();
        assert!(check.contains("enabled=true"));
        assert!(check.contains("local_storage_budget_bytes=1073741824"));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn setup_creates_a_ready_private_dashboard_in_one_command() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "agent-observability-cli-setup-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let output = run([
            "setup".into(),
            root.to_string_lossy().into_owned(),
            "--no-open".into(),
        ]
        .into_iter())
        .unwrap();

        assert!(output.contains("status=ready"));
        assert!(output.contains("collection=manual_import"));
        assert!(output.contains("opened=false"));
        let dashboard = root.join("logs").join(REPORT_FILE_NAME);
        assert!(dashboard.is_file());
        assert_eq!(
            fs::metadata(dashboard).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn demo_is_idempotent_and_opens_a_populated_isolated_runtime() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-cli-demo-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        for _ in 0..2 {
            let output = run([
                "demo".into(),
                root.to_string_lossy().into_owned(),
                "--no-open".into(),
            ]
            .into_iter())
            .unwrap();
            assert!(output.contains("status=demo_ready"));
            assert!(output.contains("source=codex"));
            assert!(output.contains("opened=false"));
        }
        let dashboard = fs::read_to_string(root.join("logs").join(REPORT_FILE_NAME)).unwrap();
        assert!(dashboard.contains(r#""generatedSpans":1"#));
        assert!(!dashboard.contains("example-conversation"));
        assert!(!dashboard.contains("prompt"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn demo_rejects_every_blocked_ingest_outcome() {
        for blocked in [
            IngestBlock::CollectionDisabled,
            IngestBlock::Policy,
            IngestBlock::Pressure,
            IngestBlock::Storage,
        ] {
            let result = IngestResult::blocked("codex", blocked);
            let error = require_demo_ingest(&result).unwrap_err();
            assert!(error.contains("demo could not create observable data"));
            assert!(error.contains("=1"));
        }
        let result = IngestResult {
            source: "codex".into(),
            observations: 1,
            diagnostics: 2,
            duplicates: 0,
            suppressed: 0,
            blocked: None,
        };
        require_demo_ingest(&result).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn dashboard_propagates_opener_failure_after_writing_report() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-cli-dashboard-open-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let error =
            prepare_dashboard_with(&root, true, |_| Err("opener failed".into())).unwrap_err();
        assert_eq!(error, "opener failed");
        assert!(root.join("logs").join(REPORT_FILE_NAME).is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn dashboard_invokes_opener_with_the_generated_report_path() {
        use std::cell::RefCell;

        let root = std::env::temp_dir().join(format!(
            "agent-observability-cli-dashboard-open-success-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let opened = RefCell::new(None);
        let dashboard = prepare_dashboard_with(&root, true, |path| {
            assert!(path.ends_with(Path::new("logs").join(REPORT_FILE_NAME)));
            *opened.borrow_mut() = Some(path.to_owned());
            Ok(())
        })
        .unwrap();
        assert_eq!(opened.into_inner().as_deref(), Some(dashboard.as_path()));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn config_set_updates_every_supported_option_and_rejects_invalid_values() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-cli-config-set-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let root_arg = root.to_string_lossy().into_owned();
        let options = [
            ("enabled", "false"),
            ("file-reconcile-ms", "1000"),
            ("flush-ms", "2000"),
            ("batch-records", "50"),
            ("batch-bytes", "65536"),
            ("active-heartbeat-ms", "30000"),
            ("idle-heartbeat-ms", "120000"),
            ("storage-bytes", "536870912"),
            ("retention-days", "90"),
            ("archive-records", "5000"),
            ("archive-bytes", "8388608"),
        ];
        for (key, value) in options {
            let output = run([
                "config".into(),
                "set".into(),
                root_arg.clone(),
                key.into(),
                value.into(),
            ]
            .into_iter())
            .unwrap();
            assert!(output.contains(&format!("updated={key}")));
        }

        let output = run(["config".into(), "show".into(), root_arg.clone()].into_iter()).unwrap();
        for (key, value) in options {
            assert!(output.contains(&format!("{key}={value}")));
        }
        assert!(
            run([
                "config".into(),
                "set".into(),
                root_arg.clone(),
                "retention-days".into(),
                "0".into(),
            ]
            .into_iter())
            .is_err()
        );
        assert!(
            run([
                "config".into(),
                "set".into(),
                root_arg,
                "unknown".into(),
                "1".into(),
            ]
            .into_iter())
            .is_err()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn runtime_check_composes_config_lock_store_and_budget() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-cli-runtime-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let output =
            run(["runtime-check".into(), root.to_string_lossy().into_owned()].into_iter()).unwrap();
        assert!(output.contains("singleton=held"));
        assert!(output.contains("storage_admission=allowed"));
        assert!(output.contains("team_ingest=disabled"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn storage_check_opens_private_local_authority() {
        let directory = std::env::temp_dir().join(format!(
            "agent-observability-cli-storage-check-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        let output = run([
            "storage-check".into(),
            directory.to_string_lossy().into_owned(),
        ]
        .into_iter())
        .expect("storage check succeeds");
        assert!(output.contains(&format!("store_schema={LOCAL_STORE_SCHEMA_VERSION}")));
        assert!(output.contains("observations=0"));
        assert!(output.contains("team_ingest=disabled"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn utc_timestamp_is_stable_at_known_boundaries() {
        assert_eq!(
            timestamp_from_duration(Duration::from_millis(0)).unwrap(),
            "1970-01-01T00:00:00.000Z"
        );
        assert_eq!(
            timestamp_from_duration(Duration::from_millis(946_684_800_123)).unwrap(),
            "2000-01-01T00:00:00.123Z"
        );
    }

    #[cfg(unix)]
    #[test]
    fn report_writes_a_private_self_contained_artifact() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "agent-observability-cli-report-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let output = run(["report".into(), root.to_string_lossy().into_owned()].into_iter())
            .expect("report command succeeds");
        assert!(output.contains("report_schema=agent_observability.report.v1"));
        assert!(output.contains("records=0"));
        assert!(output.contains("cost_status=unknown"));
        let report = root.join("logs").join(REPORT_FILE_NAME);
        let metadata = fs::metadata(&report).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        let html = fs::read_to_string(report).unwrap();
        assert!(html.contains("Agent Observability Report"));
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn report_accepts_only_private_versioned_rate_tables() {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let root = std::env::temp_dir().join(format!(
            "agent-observability-cli-rate-table-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let runtime = root.join("runtime");
        let handoff = root.join("claude-handoff.jsonl");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../adapter-claude-code/tests/fixtures/claude-handoff.jsonl"),
            &handoff,
        )
        .unwrap();
        fs::set_permissions(&handoff, fs::Permissions::from_mode(0o600)).unwrap();
        run([
            "claude-code-ingest".into(),
            runtime.to_string_lossy().into_owned(),
            handoff.to_string_lossy().into_owned(),
        ]
        .into_iter())
        .expect("Claude Code fixture ingests");
        let rate_table = root.join("rates.json");
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&rate_table)
            .unwrap();
        file.write_all(
            br#"{"schema_version":"agent_observability.rate_table.v1","version":"cli-test","models":{"claude-sonnet-5":{"input_tokens":3,"output_tokens":15,"cached_input_tokens":0.3,"cache_creation_input_tokens":3.75}}}"#,
        )
        .unwrap();
        drop(file);

        let output = run([
            "report".into(),
            runtime.to_string_lossy().into_owned(),
            rate_table.to_string_lossy().into_owned(),
        ]
        .into_iter())
        .expect("private rate table succeeds");
        assert!(!output.contains("cost_status=unknown"));

        fs::set_permissions(&rate_table, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            run([
                "report".into(),
                runtime.to_string_lossy().into_owned(),
                rate_table.to_string_lossy().into_owned(),
            ]
            .into_iter(),)
            .unwrap_err()
            .contains("permissions must be private")
        );
        let _ = fs::remove_dir_all(root);
    }
}
