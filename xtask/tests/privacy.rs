use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn evidence_process_never_prints_manifest_content_or_paths() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("AUTOMATIC_RAW_PROMPT_SENTINEL-private-key-{stamp}"));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let manifest = root.join("AUTOMATIC_CODEX_E2E_RAW_RESPONSE_SENTINEL.yaml");
    fs::write(
        &manifest,
        "status: failed\nmessage: /Users/private raw-child-stderr token=secret\n",
    )
    .unwrap();
    fs::set_permissions(&manifest, fs::Permissions::from_mode(0o600)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args([
            "evidence",
            "validate-automatic",
            manifest.to_str().unwrap(),
            "--source-revision",
            "0123456789abcdef0123456789abcdef01234567",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "automatic_evidence_validation_failed\n"
    );
    fs::remove_dir_all(root).unwrap();
}
