use agent_observability_adapter_claude_code::{
    AdapterItem as ClaudeAdapterItem, read_handoff_file as read_claude_handoff_file,
};
use agent_observability_adapter_codex::{
    AdapterItem as CodexAdapterItem, read_handoff_file as read_codex_handoff_file,
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
    Admission, InstalledLayout, LOCAL_RUNTIME_CONFIG_VERSION, LocalRuntimeConfigV2, PressureSample,
    RuntimeControl, Singleton, StorageBudget, install, load,
};
use agent_observability_local_store::{
    IngestStatus, LOCAL_STORE_SCHEMA_VERSION, LocalStore, RetentionPlan,
};
use agent_observability_static_report::write_private;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const USAGE: &str = "usage: agent-observability [init <root>|config-check <config-json>|runtime-check <root>|contracts|storage-check <runtime-root>|retention-plan <runtime-root>|retention-apply <runtime-root> <plan-id> <private-archive-jsonl>|codex-ingest <runtime-root> <handoff-jsonl>|claude-code-ingest <runtime-root> <handoff-jsonl>|cursor-ingest <runtime-root> <handoff-jsonl>|report <runtime-root> [rate-table-json]|version|help]";
const REPORT_FILE_NAME: &str = "agent-observability-report.html";
const MAX_RATE_TABLE_BYTES: u64 = 1_048_576;

enum IngestItem<'a> {
    Observation(&'a SourceObservation),
    Disposition {
        checkpoint: &'a SourceCheckpoint,
        disposition: AdapterDispositionKind,
        code: AdapterDispositionCode,
        payload_hash: Option<&'a str>,
    },
}

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
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

fn run(arguments: impl Iterator<Item = String>) -> Result<String, String> {
    let arguments: Vec<String> = arguments.collect();
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
) -> Result<String, String> {
    let paths = ingest_paths(Path::new(directory))?;
    let _singleton =
        Singleton::acquire(&paths.runtime_directory).map_err(|error| error.to_string())?;
    let mut control = RuntimeControl::new(&paths.config).map_err(|error| error.to_string())?;
    let items = items.collect::<Vec<_>>();
    if !paths.config.enabled {
        return Ok(format!(
            "source={source}\nobservations=0\ndiagnostics=0\nduplicates=0\nsuppressed=0\ncollection_disabled=1\nstorage_blocked=0\nteam_ingest=disabled"
        ));
    }
    if items.len() > usize::from(paths.config.collection.max_batch_records) {
        return Ok(format!(
            "source={source}\nobservations=0\ndiagnostics=0\nduplicates=0\nsuppressed=0\ncollection_disabled=0\npolicy_blocked=1\nstorage_blocked=0\nteam_ingest=disabled"
        ));
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
        return Ok(format!(
            "source={source}\nobservations=0\ndiagnostics=0\nduplicates=0\nsuppressed=0\ncollection_disabled=0\npressure_blocked=1\nstorage_blocked=0\nteam_ingest=disabled"
        ));
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
        return Ok(format!(
            "source={source}\nobservations=0\ndiagnostics=0\nduplicates=0\nsuppressed=0\ncollection_disabled=0\nstorage_blocked=1\nteam_ingest=disabled"
        ));
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
    Ok(format!(
        "source={source}\nobservations={observations}\ndiagnostics={diagnostics}\nduplicates={duplicates}\nsuppressed={suppressed}\ncollection_disabled=0\nstorage_blocked=0\nteam_ingest=disabled"
    ))
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
    use super::{LOCAL_STORE_SCHEMA_VERSION, REPORT_FILE_NAME, run, timestamp_from_duration};
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
