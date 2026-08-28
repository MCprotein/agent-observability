use agent_observability_contracts::{CONTRACT_MANIFEST, ContractManifest};
use std::env;
use std::process::ExitCode;

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

fn run(mut arguments: impl Iterator<Item = String>) -> Result<String, String> {
    let command = arguments.next().unwrap_or_else(|| "help".into());
    if arguments.next().is_some() {
        return Err("usage: agent-observability [contracts|version|help]".into());
    }

    match command.as_str() {
        "contracts" => {
            let manifest =
                ContractManifest::parse(CONTRACT_MANIFEST).map_err(|error| error.to_string())?;
            manifest
                .validate_release_boundary()
                .map_err(|error| error.to_string())?;
            Ok(CONTRACT_MANIFEST.trim_end().into())
        }
        "version" | "--version" | "-V" => Ok(env!("CARGO_PKG_VERSION").into()),
        "help" | "--help" | "-h" => {
            Ok("usage: agent-observability [contracts|version|help]".into())
        }
        _ => Err(format!("unknown command {command}")),
    }
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn contracts_command_exposes_disabled_team_boundary() {
        let output = run(["contracts".into()].into_iter()).expect("contracts command succeeds");
        assert!(output.contains("durable_record=agent_observability.v1"));
        assert!(output.contains("team_ingest=disabled"));
    }

    #[test]
    fn unknown_command_fails_closed() {
        assert!(run(["serve".into()].into_iter()).is_err());
    }
}
