use agent_observability_adapter_claude_code::{
    AdapterItem as ClaudeAdapterItem, read_handoff_file as read_claude_handoff_file,
};
use agent_observability_adapter_codex::{
    AdapterItem as CodexAdapterItem, read_handoff_file as read_codex_handoff_file,
};
use agent_observability_contracts::{
    AdapterDispositionCode, AdapterDispositionKind, SourceCheckpoint, SourceObservation,
};
use agent_observability_contracts::{CONTRACT_MANIFEST, ContractManifest};
use agent_observability_local_store::{IngestStatus, LOCAL_STORE_SCHEMA_VERSION, LocalStore};
use std::env;
use std::process::ExitCode;

const USAGE: &str = "usage: agent-observability [contracts|storage-check <directory>|codex-ingest <store-directory> <handoff-jsonl>|claude-code-ingest <store-directory> <handoff-jsonl>|version|help]";

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
        [command] if command == "contracts" => {
            let manifest =
                ContractManifest::parse(CONTRACT_MANIFEST).map_err(|error| error.to_string())?;
            manifest
                .validate_release_boundary()
                .map_err(|error| error.to_string())?;
            Ok(CONTRACT_MANIFEST.trim_end().into())
        }
        [command, directory] if command == "storage-check" => {
            let store = LocalStore::open(directory).map_err(|error| error.to_string())?;
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
        [command] if matches!(command.as_str(), "version" | "--version" | "-V") => {
            Ok(env!("CARGO_PKG_VERSION").into())
        }
        [] => Ok(USAGE.into()),
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => Ok(USAGE.into()),
        [command] => Err(format!("unknown command {command}")),
        _ => Err(USAGE.into()),
    }
}

fn ingest_items<'a>(
    directory: &str,
    source: &str,
    items: impl Iterator<Item = IngestItem<'a>>,
) -> Result<String, String> {
    let mut store = LocalStore::open(directory).map_err(|error| error.to_string())?;
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
        "source={source}\nobservations={observations}\ndiagnostics={diagnostics}\nduplicates={duplicates}\nsuppressed={suppressed}\nteam_ingest=disabled"
    ))
}

#[cfg(test)]
mod tests {
    use super::run;
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
        assert!(output.contains("store_schema=local_state.v2"));
        assert!(output.contains("observations=0"));
        assert!(output.contains("team_ingest=disabled"));
        let _ = fs::remove_dir_all(directory);
    }
}
