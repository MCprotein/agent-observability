#![cfg(unix)]

use agent_observability_local_runtime::{
    InstalledLayout, MutationGuard, install, load, storage_coherence::StorageBarrier,
};
use agent_observability_local_store::LocalStore;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
    process::{Command, Output},
};

struct Fixture {
    base: PathBuf,
    layout: InstalledLayout,
}

impl Fixture {
    fn populated(case: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "agentobs-runtime-ownership-{case}-{}",
            std::process::id()
        ));
        // Refuse an existing path: cleanup belongs only to this synthetic fixture.
        fs::create_dir(&base).unwrap();
        fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
        for directory in ["home", "codex", "tmp"] {
            fs::create_dir(base.join(directory)).unwrap();
            fs::set_permissions(base.join(directory), fs::Permissions::from_mode(0o700)).unwrap();
        }
        let layout = install(&base.join("installed")).unwrap();
        let fixture = Self { base, layout };
        let mutation = MutationGuard::try_acquire(&fixture.layout.runtime).unwrap();
        StorageBarrier::initialize(&fixture.layout.root, &mutation).unwrap();
        drop(mutation);

        // These first three records contain lifecycle/timing/token metadata only.
        // The input lives outside the accounting root so it cannot become unknown storage.
        let content_free = include_str!("../../adapter-codex/tests/fixtures/codex-handoff.jsonl")
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert!(!content_free.contains("RAW_"));
        let handoff = fixture.base.join("handoff.jsonl");
        fs::write(&handoff, content_free).unwrap();
        fs::set_permissions(&handoff, fs::Permissions::from_mode(0o600)).unwrap();
        let ingest = fixture
            .command()
            .arg("codex-ingest")
            .arg(&fixture.layout.root)
            .arg(&handoff)
            .output()
            .unwrap();
        assert!(success(ingest).contains("observations=3\n"));
        let report = fixture.run("report");
        assert!(success(report).contains("report="));
        assert!(fixture.record_count() > 0);
        fixture
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_agent-observability"));
        command
            .env_clear()
            .env("HOME", self.base.join("home"))
            .env("CODEX_HOME", self.base.join("codex"))
            .env("TMPDIR", self.base.join("tmp"))
            .current_dir(&self.base);
        command
    }

    fn run(&self, command: &str) -> Output {
        self.command()
            .arg(command)
            .arg(&self.layout.root)
            .output()
            .unwrap()
    }

    fn record_count(&self) -> usize {
        let store = LocalStore::open_report_reader(self.layout.state.join("store")).unwrap();
        assert!(!store.report_status().unwrap().pending());
        store.current_records().unwrap().len()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).unwrap();
    }
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn initialized_runtime_check_owns_ingested_store_report_config_and_stable_locks() {
    let fixture = Fixture::populated("populated");
    let config = fs::read(&fixture.layout.config).unwrap();
    load(&fixture.layout.config).unwrap();
    let report_path = fixture.layout.logs.join("agent-observability-report.html");
    let report = fs::read(&report_path).unwrap();
    assert!(!report.is_empty());
    let records = fixture.record_count();
    let lock_paths = ["mutation.lock", "storage-accounting.lock", "runtime.lock"];
    let identities = lock_paths.map(|name| {
        let metadata = fs::metadata(fixture.layout.runtime.join(name)).unwrap();
        (metadata.dev(), metadata.ino())
    });

    let stdout = success(fixture.run("runtime-check"));
    for line in [
        "storage_accounting=observed",
        "accounting_stage=post_store_open",
        "accounting_unknown_bytes=0",
        "accounting_unknown_entries=0",
        "singleton=held",
        "storage_admission=allowed",
        "separated_admission=disabled",
    ] {
        assert!(stdout.lines().any(|actual| actual == line), "{stdout}");
    }
    assert!(!stdout.contains(fixture.layout.root.to_str().unwrap()));
    assert_eq!(fs::read(&fixture.layout.config).unwrap(), config);
    load(&fixture.layout.config).unwrap();
    assert_eq!(fs::read(report_path).unwrap(), report);
    assert_eq!(fixture.record_count(), records);
    for (name, expected) in lock_paths.into_iter().zip(identities) {
        let metadata = fs::metadata(fixture.layout.runtime.join(name)).unwrap();
        assert_eq!((metadata.dev(), metadata.ino()), expected);
        assert_eq!(metadata.len(), 0);
    }
}

#[test]
fn initialized_runtime_check_does_not_recreate_a_missing_mutation_lock() {
    let fixture = Fixture::populated("missing-mutation");
    let config = fs::read(&fixture.layout.config).unwrap();
    let records = fixture.record_count();
    let mutation = fixture.layout.runtime.join("mutation.lock");
    fs::remove_file(&mutation).unwrap();

    let output = fixture.run("runtime-check");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    assert!(!mutation.exists());
    assert_eq!(fs::read(&fixture.layout.config).unwrap(), config);
    load(&fixture.layout.config).unwrap();
    assert_eq!(fixture.record_count(), records);
}
