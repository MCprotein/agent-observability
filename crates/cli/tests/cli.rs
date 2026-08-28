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

#[cfg(unix)]
#[test]
fn codex_ingest_process_commits_observations_and_bounded_diagnostics() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-codex-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let handoff = root.join("codex-handoff.jsonl");
    fs::write(
        &handoff,
        include_str!("../../adapter-codex/tests/fixtures/codex-handoff.jsonl"),
    )
    .unwrap();
    fs::set_permissions(&handoff, fs::Permissions::from_mode(0o600)).unwrap();
    let store = root.join("store");

    let first = binary()
        .args([
            "codex-ingest",
            store.to_str().unwrap(),
            handoff.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let stdout = String::from_utf8_lossy(&first.stdout);
    assert!(stdout.contains("observations=5"));
    assert!(stdout.contains("diagnostics=2"));
    assert!(stdout.contains("suppressed=1"));

    let second = binary()
        .args([
            "codex-ingest",
            store.to_str().unwrap(),
            handoff.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let projection = fs::read_to_string(store.join("observations.jsonl")).unwrap();
    assert_eq!(projection.lines().count(), 5);
    for secret in [
        "RAW_PROMPT_SECRET",
        "RAW_TOOL_OUTPUT_SECRET",
        "RAW_ASSISTANT_SECRET",
    ] {
        assert!(!projection.contains(secret));
    }
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn claude_code_ingest_process_is_private_and_idempotent() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-claude-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let handoff = root.join("claude-handoff.jsonl");
    fs::write(
        &handoff,
        include_str!("../../adapter-claude-code/tests/fixtures/claude-handoff.jsonl"),
    )
    .unwrap();
    fs::set_permissions(&handoff, fs::Permissions::from_mode(0o600)).unwrap();
    let store = root.join("store");

    for _ in 0..2 {
        let output = binary()
            .args([
                "claude-code-ingest",
                store.to_str().unwrap(),
                handoff.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("source=claude-code"));
        assert!(stdout.contains("observations=7"));
    }

    let projection = fs::read_to_string(store.join("observations.jsonl")).unwrap();
    assert_eq!(projection.lines().count(), 6);
    for secret in [
        "RAW_PROMPT_SECRET",
        "RAW_RESPONSE_SECRET",
        "RAW_TOOL_INPUT_SECRET",
        "RAW_ASSISTANT_SECRET",
        "raw@example.invalid",
    ] {
        assert!(!projection.contains(secret));
    }
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn codex_ingest_process_restarts_from_an_appended_tail() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-codex-tail-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let fixture = include_str!("../../adapter-codex/tests/fixtures/codex-handoff.jsonl");
    let prefix = root.join("prefix.jsonl");
    let tail = root.join("tail.jsonl");
    fs::write(
        &prefix,
        fixture.lines().take(7).collect::<Vec<_>>().join("\n"),
    )
    .unwrap();
    fs::write(
        &tail,
        fixture.lines().skip(7).collect::<Vec<_>>().join("\n"),
    )
    .unwrap();
    fs::set_permissions(&prefix, fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(&tail, fs::Permissions::from_mode(0o600)).unwrap();
    let store = root.join("store");

    for handoff in [&prefix, &tail] {
        let output = binary()
            .args([
                "codex-ingest",
                store.to_str().unwrap(),
                handoff.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let projection = fs::read_to_string(store.join("observations.jsonl")).unwrap();
    let full_store = root.join("full-store");
    let full_handoff = root.join("full.jsonl");
    fs::write(&full_handoff, fixture).unwrap();
    fs::set_permissions(&full_handoff, fs::Permissions::from_mode(0o600)).unwrap();
    let full = binary()
        .args([
            "codex-ingest",
            full_store.to_str().unwrap(),
            full_handoff.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        full.status.success(),
        "{}",
        String::from_utf8_lossy(&full.stderr)
    );
    assert_eq!(
        projection,
        fs::read_to_string(full_store.join("observations.jsonl")).unwrap()
    );
    let split_state = agent_observability_local_store::LocalStore::open(&store).unwrap();
    let full_state = agent_observability_local_store::LocalStore::open(&full_store).unwrap();
    assert_eq!(split_state.observation_count().unwrap(), 5);
    assert_eq!(split_state.disposition_count().unwrap(), 3);
    assert_eq!(
        split_state.observation_count().unwrap(),
        full_state.observation_count().unwrap()
    );
    assert_eq!(
        split_state.disposition_count().unwrap(),
        full_state.disposition_count().unwrap()
    );
    let _ = fs::remove_dir_all(root);
}
