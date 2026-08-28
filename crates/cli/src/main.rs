use agent_observability_contracts::{CONTRACT_MANIFEST, ContractManifest};
use agent_observability_local_store::LocalStore;
use std::env;
use std::process::ExitCode;

const USAGE: &str = "usage: agent-observability [contracts|storage-check <directory>|version|help]";

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
                "store_schema=local_state.v1\nobservations={observations}\nrecords={records}\ndelivery_outcomes={outcomes}\nteam_ingest=disabled"
            ))
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
        assert!(output.contains("store_schema=local_state.v1"));
        assert!(output.contains("observations=0"));
        assert!(output.contains("team_ingest=disabled"));
        let _ = fs::remove_dir_all(directory);
    }
}
