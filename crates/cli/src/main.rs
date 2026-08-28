use agent_observability_adapter_claude_code::{
    AdapterItem as ClaudeAdapterItem, read_handoff_file as read_claude_handoff_file,
};
use agent_observability_adapter_codex::{
    AdapterItem as CodexAdapterItem, read_handoff_file as read_codex_handoff_file,
};
use agent_observability_adapter_cursor::{
    AdapterItem as CursorAdapterItem, read_handoff_file as read_cursor_handoff_file,
};
use agent_observability_contracts::{
    AdapterDispositionCode, AdapterDispositionKind, SourceCheckpoint, SourceObservation,
};
use agent_observability_contracts::{CONTRACT_MANIFEST, ContractManifest};
use agent_observability_local_runtime::{
    Admission, LOCAL_RUNTIME_CONFIG_VERSION, LocalRuntimeConfigV1, PressureSample, RuntimeControl,
    Singleton, StorageBudget, install, load,
};
use agent_observability_local_store::{IngestStatus, LOCAL_STORE_SCHEMA_VERSION, LocalStore};
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: agent-observability [init <root>|config-check <config-json>|runtime-check <root>|contracts|storage-check <runtime-root>|codex-ingest <runtime-root> <handoff-jsonl>|claude-code-ingest <runtime-root> <handoff-jsonl>|cursor-ingest <runtime-root> <handoff-jsonl>|version|help]";

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
                "config_schema={LOCAL_RUNTIME_CONFIG_VERSION}\nenabled={}\nlocal_storage_budget_bytes={}\nteam_ingest=disabled",
                config.enabled, config.collection.local_storage_budget_bytes
            ))
        }
        [command, root] if command == "runtime-check" => runtime_check(Path::new(root)),
        [command] if command == "contracts" => {
            let manifest =
                ContractManifest::parse(CONTRACT_MANIFEST).map_err(|error| error.to_string())?;
            manifest
                .validate_release_boundary()
                .map_err(|error| error.to_string())?;
            Ok(CONTRACT_MANIFEST.trim_end().into())
        }
        [command, root] if command == "storage-check" => {
            let layout = install(Path::new(root)).map_err(|error| error.to_string())?;
            let _singleton =
                Singleton::acquire(&layout.runtime).map_err(|error| error.to_string())?;
            let store =
                LocalStore::open(layout.state.join("store")).map_err(|error| error.to_string())?;
            let (observations, records, outcomes) =
                store.counts().map_err(|error| error.to_string())?;
            Ok(format!(
                "store_schema={LOCAL_STORE_SCHEMA_VERSION}\nobservations={observations}\nrecords={records}\ndelivery_outcomes={outcomes}\nteam_ingest=disabled"
            ))
        }
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
        [command] if matches!(command.as_str(), "version" | "--version" | "-V") => {
            Ok(env!("CARGO_PKG_VERSION").into())
        }
        [] => Ok(USAGE.into()),
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => Ok(USAGE.into()),
        [command] => Err(format!("unknown command {command}")),
        _ => Err(USAGE.into()),
    }
}

fn runtime_check(root: &Path) -> Result<String, String> {
    let layout = install(root).map_err(|error| error.to_string())?;
    let config = load(&layout.config).map_err(|error| error.to_string())?;
    let _singleton = Singleton::acquire(&layout.runtime).map_err(|error| error.to_string())?;
    let store_directory = layout.state.join("store");
    let store = LocalStore::open(&store_directory).map_err(|error| error.to_string())?;
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
    let mut store = LocalStore::open(&paths.store_directory).map_err(|error| error.to_string())?;
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
    config: LocalRuntimeConfigV1,
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
    use super::{LOCAL_STORE_SCHEMA_VERSION, run};
    use std::fs;

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
        assert!(init.contains("config_schema=local_runtime.v1"));
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
}
