use std::process::{Command, Stdio};

#[cfg(unix)]
use std::fs;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_agent-observability"))
}

#[test]
fn contracts_command_succeeds_in_the_real_process() {
    let output = binary().arg("contracts").output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("team_ingest=disabled"));
    assert!(output.stderr.is_empty());
}

#[test]
fn invalid_real_process_command_fails_on_stderr() {
    let output = binary().arg("serve").output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown command"));
}

#[cfg(unix)]
#[test]
fn storage_check_rejects_a_broad_directory_in_the_real_process() {
    use std::os::unix::fs::PermissionsExt;

    let directory = std::env::temp_dir().join(format!(
        "agent-observability-cli-process-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
    let output = binary()
        .arg("storage-check")
        .arg(&directory)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("permissions are too broad"));
    let _ = fs::remove_dir_all(directory);
}

#[cfg(unix)]
#[test]
fn concurrent_real_processes_initialize_one_store() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-concurrent-process-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let directory = root.join("store");
    let children = (0..20)
        .map(|_| {
            binary()
                .arg("storage-check")
                .arg(&directory)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect::<Vec<_>>();
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(directory.join("local-store.sqlite3").is_file());
    assert!(directory.join("observations.jsonl").is_file());
    let _ = fs::remove_dir_all(root);
}
