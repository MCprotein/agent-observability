use agent_observability_local_runtime::{ConfigMutationGuard, StorageBudget, install, load, save};
use agent_observability_local_store::LocalStore;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[cfg(unix)]
use std::fs;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_agent-observability"))
}

#[cfg(unix)]
fn private_codex_handoff(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let handoff = root.join("codex-handoff.jsonl");
    fs::write(
        &handoff,
        include_str!("../../adapter-codex/tests/fixtures/codex-handoff.jsonl"),
    )
    .unwrap();
    fs::set_permissions(&handoff, fs::Permissions::from_mode(0o600)).unwrap();
    handoff
}

#[cfg(unix)]
fn spawn_codex_ingest(root: &Path, handoff: &Path) -> Child {
    binary()
        .args([
            "codex-ingest",
            root.to_str().unwrap(),
            handoff.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

#[cfg(unix)]
fn assert_ingest_waits(child: &mut Child) {
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(child.try_wait().unwrap().is_none());
}

#[cfg(unix)]
fn inflate_allocated_accounting(root: &Path) {
    let source = root.join("allocated-budget-fixture");
    fs::write(&source, vec![0_u8; 1024 * 1024]).unwrap();
    let reduced = StorageBudget::calculate(256 * 1024 * 1024, false).unwrap();
    for index in 0..300 {
        let allocated = StorageBudget::allocated_tree_bytes(root).unwrap();
        if allocated + 512 * 1024 > reduced.writable_limit() {
            return;
        }
        fs::hard_link(&source, root.join(format!("allocated-budget-link-{index}"))).unwrap();
    }
    panic!("failed to inflate storage accounting above the reduced budget");
}

#[test]
fn version_flags_report_the_package_version() {
    for flag in ["version", "--version", "-V"] {
        let output = binary().arg(flag).output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            env!("CARGO_PKG_VERSION")
        );
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn help_distinguishes_monitoring_from_settings() {
    let output = binary().arg("help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("agentobs dashboard"));
    assert!(stdout.contains("Serve monitoring on private localhost"));
    assert!(stdout.contains("agentobs settings"));
    assert!(stdout.contains("agentobs ui"));
    assert!(stdout.contains("Compatibility alias for settings"));
    assert!(stdout.contains("legacy alias: `agent-observability`"));
}

#[test]
fn nested_settings_help_and_ui_alias_have_no_side_effect() {
    let working = std::env::temp_dir().join(format!(
        "agent-observability-cli-ui-help-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&working);
    std::fs::create_dir(&working).unwrap();

    for command in ["settings", "ui"] {
        let output = binary()
            .args([command, "--help"])
            .current_dir(&working)
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("agentobs settings"));
        assert!(stdout.contains("agentobs ui"));
        assert!(output.stderr.is_empty());
    }
    assert_eq!(std::fs::read_dir(&working).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(working);
}

#[cfg(unix)]
#[test]
fn settings_and_ui_alias_start_the_same_private_surface() {
    for command in ["settings", "ui"] {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-{command}-alias-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut child = binary()
            .args([command, root.to_str().unwrap(), "--no-open"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        assert_eq!(lines.next().unwrap().unwrap(), "status=settings_ready");
        let url = lines.next().unwrap().unwrap();
        assert!(url.starts_with("url=http://127.0.0.1:"));
        assert!(url.contains("/#session="));
        child.kill().unwrap();
        child.wait().unwrap();
        let _ = fs::remove_dir_all(root);
    }
}

fn installed_store(root: &Path) -> PathBuf {
    root.join("state/store")
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

#[test]
fn codex_notify_real_process_rejects_before_io_with_zero_exit() {
    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-notify-missing-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let output = binary()
        .args([
            "codex-notify",
            root.to_str().unwrap(),
            r#"{"type":"agent-turn-complete"}"#,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "notify=rejected"
    );
    assert!(output.stderr.is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn codex_notify_real_process_is_unavailable_after_valid_projection() {
    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-notify-unavailable-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let output = binary()
        .args([
            "codex-notify",
            root.to_str().unwrap(),
            r#"{"type":"agent-turn-complete","thread-id":"thread-1","turn-id":"turn-1","cwd":"/RAW_CWD_SENTINEL","input-messages":["RAW_PROMPT_SENTINEL"],"last-assistant-message":"RAW_ASSISTANT_SENTINEL"}"#,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "notify=unavailable"
    );
    assert!(output.stderr.is_empty());
    assert!(!root.join("runtime/collector.json").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn init_and_runtime_check_create_only_private_local_paths() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-install-process-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let init = binary()
        .args(["init", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    assert!(String::from_utf8_lossy(&init.stdout).contains("config_schema=local_runtime.v2"));
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(root.join("config.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    for directory in ["logs", "queue", "state", "runtime"] {
        assert_eq!(
            fs::metadata(root.join(directory))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    let check = binary()
        .args(["runtime-check", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("singleton=held"));
    assert!(stdout.contains("storage_admission=allowed"));
    assert!(stdout.contains("team_ingest=disabled"));
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn setup_and_config_set_work_end_to_end_in_the_real_process() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-onboarding-process-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let setup = binary()
        .args(["setup", root.to_str().unwrap(), "--no-open"])
        .output()
        .unwrap();
    assert!(
        setup.status.success(),
        "{}",
        String::from_utf8_lossy(&setup.stderr)
    );
    let setup_output = String::from_utf8_lossy(&setup.stdout);
    assert!(setup_output.contains("status=ready"));
    assert!(setup_output.contains("collection=manual_import"));
    assert!(setup_output.contains("opened=false"));
    let dashboard = root.join("logs/agent-observability-report.html");
    assert_eq!(
        fs::metadata(&dashboard).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let update = binary()
        .args([
            "config",
            "set",
            root.to_str().unwrap(),
            "retention-days",
            "90",
        ])
        .output()
        .unwrap();
    assert!(
        update.status.success(),
        "{}",
        String::from_utf8_lossy(&update.stderr)
    );
    assert!(String::from_utf8_lossy(&update.stdout).contains("retention-days=90"));
    let show = binary()
        .args(["config", "show", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(show.status.success());
    assert!(String::from_utf8_lossy(&show.stdout).contains("retention-days=90"));
    assert_eq!(
        fs::metadata(root.join("config.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );

    fs::remove_file(&dashboard).unwrap();
    let dashboard_command = binary()
        .args(["report", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        dashboard_command.status.success(),
        "{}",
        String::from_utf8_lossy(&dashboard_command.stderr)
    );
    assert!(String::from_utf8_lossy(&dashboard_command.stdout).contains("report="));
    assert_eq!(
        fs::metadata(&dashboard).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn demo_produces_first_observable_value_without_an_external_file() {
    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-demo-process-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let output = binary()
        .args(["demo", root.to_str().unwrap(), "--no-open"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("status=demo_ready"));
    assert!(stdout.contains("observations=1"));
    assert!(stdout.contains("diagnostics=2"));
    let dashboard = fs::read_to_string(root.join("logs/agent-observability-report.html")).unwrap();
    assert!(dashboard.contains(r#""generatedSpans":1"#));
    assert!(!dashboard.contains("example-conversation"));
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn demo_fails_when_collection_policy_blocks_first_value() {
    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-demo-blocked-process-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let setup = binary()
        .args(["setup", root.to_str().unwrap(), "--no-open"])
        .output()
        .unwrap();
    assert!(setup.status.success());
    let disable = binary()
        .args(["config", "set", root.to_str().unwrap(), "enabled", "false"])
        .output()
        .unwrap();
    assert!(disable.status.success());

    let demo = binary()
        .args(["demo", root.to_str().unwrap(), "--no-open"])
        .output()
        .unwrap();
    assert!(!demo.status.success());
    assert!(demo.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&demo.stderr);
    assert!(stderr.contains("demo could not create observable data"));
    assert!(stderr.contains("collection_disabled=1"));
    assert!(!stderr.contains("status=demo_ready"));
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
#[allow(clippy::too_many_lines)]
fn retention_plan_is_read_only_and_apply_writes_one_private_archive() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-retention-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let handoff = root.join("old-cursor-handoff.jsonl");
    let old = include_str!("../../adapter-cursor/tests/fixtures/cursor-handoff.jsonl")
        .replace("178787520", "100000000");
    fs::write(&handoff, old).unwrap();
    fs::set_permissions(&handoff, fs::Permissions::from_mode(0o600)).unwrap();
    let runtime = root.join("runtime-root");
    let ingest = binary()
        .args([
            "cursor-ingest",
            runtime.to_str().unwrap(),
            handoff.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        ingest.status.success(),
        "{}",
        String::from_utf8_lossy(&ingest.stderr)
    );
    let initial_report = binary()
        .args(["report", runtime.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(initial_report.status.success());
    let dashboard = runtime.join("logs/agent-observability-report.html");
    assert!(
        !fs::read_to_string(&dashboard)
            .unwrap()
            .contains(r#""generatedSpans":0"#)
    );
    let store = LocalStore::open(installed_store(&runtime)).unwrap();
    assert!(!store.report_status().unwrap().pending());
    drop(store);
    let projection = installed_store(&runtime).join("observations.jsonl");
    let before = fs::read(&projection).unwrap();

    let plan = binary()
        .args(["retention-plan", runtime.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let plan_output = String::from_utf8_lossy(&plan.stdout);
    assert!(plan_output.contains("applied=0"));
    assert!(!plan_output.contains("traces=0"));
    let plan_id = plan_output
        .lines()
        .find_map(|line| line.strip_prefix("plan_id="))
        .unwrap();
    assert_eq!(fs::read(&projection).unwrap(), before);
    let archive = root.join("expired.jsonl");
    assert!(!archive.exists());

    let inside_archive = runtime.join("inside.jsonl");
    let rejected = binary()
        .current_dir(&root)
        .args([
            "retention-apply",
            "runtime-root",
            plan_id,
            inside_archive.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr)
            .contains("retention archive must be outside the managed runtime root")
    );
    assert!(!inside_archive.exists());

    let apply = binary()
        .args([
            "retention-apply",
            runtime.to_str().unwrap(),
            plan_id,
            archive.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        apply.status.success(),
        "{}",
        String::from_utf8_lossy(&apply.stderr)
    );
    assert!(String::from_utf8_lossy(&apply.stdout).contains("applied=1"));
    assert_eq!(
        fs::metadata(&archive).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(fs::read_to_string(&projection).unwrap().is_empty());
    let store = LocalStore::open(installed_store(&runtime)).unwrap();
    assert!(store.report_status().unwrap().pending());
    drop(store);
    assert!(
        !fs::read_to_string(&dashboard)
            .unwrap()
            .contains(r#""generatedSpans":0"#)
    );

    let refresh = binary()
        .args(["report", runtime.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        refresh.status.success(),
        "{}",
        String::from_utf8_lossy(&refresh.stderr)
    );
    assert!(
        fs::read_to_string(&dashboard)
            .unwrap()
            .contains(r#""generatedSpans":0"#)
    );
    let store = LocalStore::open(installed_store(&runtime)).unwrap();
    assert!(!store.report_status().unwrap().pending());
    drop(store);

    let stale_replay = binary()
        .args([
            "cursor-ingest",
            runtime.to_str().unwrap(),
            handoff.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!stale_replay.status.success());
    assert!(String::from_utf8_lossy(&stale_replay.stderr).contains("source cursor conflict"));
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn config_check_rejects_unknown_fields_in_the_real_process() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-config-process-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let config = root.join("config.json");
    fs::write(
        &config,
        br#"{"schema_version":"local_runtime.v1","endpoint":"forbidden"}"#,
    )
    .unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    let output = binary()
        .args(["config-check", config.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn installed_runtime_config_disables_ingest_without_creating_a_store() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-disabled-process-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let init = binary()
        .args(["init", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(init.status.success());
    let config = root.join("config.json");
    let body = fs::read_to_string(&config)
        .unwrap()
        .replace("\"enabled\": true", "\"enabled\": false");
    fs::write(&config, body).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    let handoff = root.join("codex-handoff.jsonl");
    fs::write(
        &handoff,
        include_str!("../../adapter-codex/tests/fixtures/codex-handoff.jsonl"),
    )
    .unwrap();
    fs::set_permissions(&handoff, fs::Permissions::from_mode(0o600)).unwrap();

    let output = binary()
        .args([
            "codex-ingest",
            root.to_str().unwrap(),
            handoff.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("collection_disabled=1"));
    assert!(!root.join("state/store/local-store.sqlite3").exists());
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn concurrent_disable_blocks_manual_ingest_before_admission() {
    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-concurrent-disable-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let layout = install(&root).unwrap();
    let handoff = private_codex_handoff(&root);
    let guard = ConfigMutationGuard::acquire(&layout).unwrap();
    let mut ingest = spawn_codex_ingest(&root, &handoff);
    assert_ingest_waits(&mut ingest);

    let mut config = load(&layout.config).unwrap();
    config.enabled = false;
    save(&guard, &config).unwrap();
    drop(guard);

    let output = ingest.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("collection_disabled=1"));
    assert!(!root.join("state/store/local-store.sqlite3").exists());
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn concurrent_budget_reduction_blocks_manual_ingest_before_commit() {
    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-concurrent-budget-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let layout = install(&root).unwrap();
    let handoff = private_codex_handoff(&root);
    inflate_allocated_accounting(&root);
    let guard = ConfigMutationGuard::acquire(&layout).unwrap();
    let mut ingest = spawn_codex_ingest(&root, &handoff);
    assert_ingest_waits(&mut ingest);

    let mut config = load(&layout.config).unwrap();
    config.collection.local_storage_budget_bytes = 256 * 1024 * 1024;
    save(&guard, &config).unwrap();
    drop(guard);

    let output = ingest.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("storage_blocked=1"));
    assert!(!root.join("state/store/local-store.sqlite3").exists());
    let _ = fs::remove_dir_all(root);
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
    assert!(String::from_utf8_lossy(&output.stderr).contains("not private"));
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
    let mut successes = 0;
    for child in children {
        let output = child.wait_with_output().unwrap();
        if output.status.success() {
            successes += 1;
        } else {
            assert!(String::from_utf8_lossy(&output.stderr).contains("already running"));
        }
    }
    assert!(successes >= 1);
    assert!(
        installed_store(&directory)
            .join("local-store.sqlite3")
            .is_file()
    );
    assert!(
        installed_store(&directory)
            .join("observations.jsonl")
            .is_file()
    );
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
    assert!(stdout.contains("observations=6"));
    assert!(stdout.contains("diagnostics=2"));
    assert!(stdout.contains("duplicates=0"));
    assert!(stdout.contains("suppressed=0"));

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
    let projection =
        fs::read_to_string(installed_store(&store).join("observations.jsonl")).unwrap();
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

    for run in 0..2 {
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
        if run == 0 {
            assert!(stdout.contains("observations=7"));
            assert!(stdout.contains("diagnostics=2"));
            assert!(stdout.contains("duplicates=0"));
            assert!(stdout.contains("suppressed=1"));
        } else {
            assert!(stdout.contains("observations=0"));
            assert!(stdout.contains("diagnostics=0"));
            assert!(stdout.contains("duplicates=10"));
            assert!(stdout.contains("suppressed=0"));
        }
    }

    let projection =
        fs::read_to_string(installed_store(&store).join("observations.jsonl")).unwrap();
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
fn claude_code_ingest_restarts_from_a_failed_prefix_and_tail() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-claude-tail-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let fixture = include_str!("../../adapter-claude-code/tests/fixtures/claude-handoff.jsonl");
    let prefix = root.join("prefix.jsonl");
    let tail = root.join("tail.jsonl");
    fs::write(
        &prefix,
        fixture.lines().take(3).collect::<Vec<_>>().join("\n"),
    )
    .unwrap();
    fs::write(
        &tail,
        fixture.lines().skip(3).collect::<Vec<_>>().join("\n"),
    )
    .unwrap();
    fs::set_permissions(&prefix, fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(&tail, fs::Permissions::from_mode(0o600)).unwrap();
    let split_store = root.join("split-store");

    for handoff in [&prefix, &tail] {
        let output = binary()
            .args([
                "claude-code-ingest",
                split_store.to_str().unwrap(),
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

    let full_handoff = root.join("full.jsonl");
    fs::write(&full_handoff, fixture).unwrap();
    fs::set_permissions(&full_handoff, fs::Permissions::from_mode(0o600)).unwrap();
    let full_store = root.join("full-store");
    let full = binary()
        .args([
            "claude-code-ingest",
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
        fs::read_to_string(installed_store(&split_store).join("observations.jsonl")).unwrap(),
        fs::read_to_string(installed_store(&full_store).join("observations.jsonl")).unwrap()
    );
    let split_state =
        agent_observability_local_store::LocalStore::open(installed_store(&split_store)).unwrap();
    let full_state =
        agent_observability_local_store::LocalStore::open(installed_store(&full_store)).unwrap();
    assert_eq!(split_state.observation_count().unwrap(), 7);
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

    let projection =
        fs::read_to_string(installed_store(&store).join("observations.jsonl")).unwrap();
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
        fs::read_to_string(installed_store(&full_store).join("observations.jsonl")).unwrap()
    );
    let split_state =
        agent_observability_local_store::LocalStore::open(installed_store(&store)).unwrap();
    let full_state =
        agent_observability_local_store::LocalStore::open(installed_store(&full_store)).unwrap();
    assert_eq!(split_state.observation_count().unwrap(), 6);
    assert_eq!(split_state.disposition_count().unwrap(), 2);
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

#[cfg(unix)]
#[test]
#[allow(clippy::too_many_lines)]
fn cursor_ingest_process_is_private_idempotent_and_restartable() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "agent-observability-cli-cursor-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let fixture = include_str!("../../adapter-cursor/tests/fixtures/cursor-handoff.jsonl");
    let prefix = root.join("prefix.jsonl");
    let tail = root.join("tail.jsonl");
    let full = root.join("full.jsonl");
    fs::write(
        &prefix,
        fixture.lines().take(4).collect::<Vec<_>>().join("\n"),
    )
    .unwrap();
    fs::write(
        &tail,
        fixture.lines().skip(4).collect::<Vec<_>>().join("\n"),
    )
    .unwrap();
    fs::write(&full, fixture).unwrap();
    for path in [&prefix, &tail, &full] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    let split_store = root.join("split-store");
    for handoff in [&prefix, &tail] {
        let output = binary()
            .args([
                "cursor-ingest",
                split_store.to_str().unwrap(),
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
    let replay = binary()
        .args([
            "cursor-ingest",
            split_store.to_str().unwrap(),
            full.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(replay.status.success());

    let full_store = root.join("full-store");
    let output = binary()
        .args([
            "cursor-ingest",
            full_store.to_str().unwrap(),
            full.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("source=cursor"));

    let split_projection =
        fs::read_to_string(installed_store(&split_store).join("observations.jsonl")).unwrap();
    let full_projection =
        fs::read_to_string(installed_store(&full_store).join("observations.jsonl")).unwrap();
    assert_eq!(split_projection, full_projection);
    let tool_executions = split_projection
        .lines()
        .filter(|line| line.contains("\"span_kind\":\"tool.execution\""))
        .count();
    assert_eq!(tool_executions, 2);
    let shell_record = split_projection
        .lines()
        .find(|line| line.contains("\"tool_name\":\"shell\""))
        .unwrap();
    assert!(shell_record.contains("\"phase\":\"failure\""));
    assert!(shell_record.contains("\"status\":{\"code\":\"error\"}"));
    assert!(shell_record.contains("\"start_time_unix_ms\":1787875200200.0"));
    assert!(shell_record.contains("\"end_time_unix_ms\":1787875200400.0"));
    assert!(shell_record.contains(
        "\"call_id\":\"id:sha256:7998d275087ee3f171d53721137f16ed6632d7f7d3a07f9663fbf5ad6108742f\""
    ));
    for secret in [
        "RAW_EMAIL",
        "RAW_PATH",
        "RAW_COMMAND",
        "RAW_PROMPT",
        "RAW_OUTPUT",
        "RAW_MCP",
    ] {
        assert!(!split_projection.contains(secret));
    }
    let split_state =
        agent_observability_local_store::LocalStore::open(installed_store(&split_store)).unwrap();
    assert_eq!(split_state.observation_count().unwrap(), 7);
    assert_eq!(split_state.disposition_count().unwrap(), 6);
    let _ = fs::remove_dir_all(root);
}
