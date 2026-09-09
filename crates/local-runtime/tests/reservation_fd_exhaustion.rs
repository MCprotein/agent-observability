#![cfg(unix)]

use agent_observability_local_runtime::{
    MutationGuard, ReservationError, SingletonError, install,
    reservation::ReportReservationEvidence,
};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

const CHILD: &str = "AGENTOBS_RESERVATION_FD_EXHAUSTION_CHILD";
const FIXTURE: &str = "AGENTOBS_RESERVATION_FD_EXHAUSTION_FIXTURE";
const MAX_FILLER_ATTEMPTS: usize = 257;

struct ExactFixtureCleanup(Option<PathBuf>);

impl ExactFixtureCleanup {
    fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }

    fn remove(&mut self) {
        let Some(path) = self.0.take() else {
            return;
        };
        match fs::remove_dir_all(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => panic!("exact fixture cleanup failed: {error:?}"),
        }
    }
}

impl Drop for ExactFixtureCleanup {
    fn drop(&mut self) {
        self.remove();
    }
}

#[test]
fn reservation_matcher_reports_descriptor_exhaustion_and_recovers() {
    if std::env::var_os(CHILD).is_some() {
        run_child_probe();
        return;
    }

    let executable = std::env::current_exe().unwrap();
    let fixture = std::env::temp_dir().join(format!(
        "local-runtime-reservation-fd-exhaustion-parent-{}",
        std::process::id()
    ));
    assert!(!fixture.exists());
    fs::create_dir(&fixture).unwrap();
    fs::set_permissions(&fixture, fs::Permissions::from_mode(0o700)).unwrap();
    let mut cleanup = ExactFixtureCleanup::new(fixture.clone());
    let mut child = Command::new("/bin/sh")
        .args([
            "-c",
            "ulimit -n 256 || exit 125; exec \"$1\" --exact reservation_matcher_reports_descriptor_exhaustion_and_recovers --test-threads=1",
            "reservation-fd-exhaustion",
        ])
        .arg(executable)
        .env(CHILD, "1")
        // Routing only: the child validates the exact parent-created private directory.
        .env(FIXTURE, &fixture)
        .spawn()
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            cleanup.remove();
            assert!(status.success(), "descriptor-exhaustion child failed");
            return;
        }
        if Instant::now() >= deadline {
            kill_and_reap(&mut child);
            cleanup.remove();
            panic!("descriptor-exhaustion child exceeded its bounded timeout");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn kill_and_reap(child: &mut Child) {
    child.kill().unwrap();
    child.wait().unwrap();
}

fn run_child_probe() {
    let fixture = PathBuf::from(std::env::var_os(FIXTURE).unwrap());
    assert!(fixture.is_absolute());
    let metadata = fs::symlink_metadata(&fixture).unwrap();
    assert!(metadata.is_dir());
    assert!(!metadata.file_type().is_symlink());
    assert_eq!(metadata.permissions().mode() & 0o7777, 0o700);
    assert!(fs::read_dir(&fixture).unwrap().next().is_none());
    let layout = install(&fixture.join("runtime-root")).unwrap();
    let guard = MutationGuard::try_acquire(&layout.runtime).unwrap();
    let evidence = ReportReservationEvidence::capture(&layout.root, &guard).unwrap();
    assert_eq!(evidence.captured_reserved_bytes(), 0);

    let candidate_path = layout.logs.join("synthetic-candidate");
    let candidate = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&candidate_path)
        .unwrap();

    let mut fillers = Vec::with_capacity(256);
    let mut exhausted = false;
    for _ in 0..MAX_FILLER_ATTEMPTS {
        match File::open("/dev/null") {
            Ok(file) => fillers.push(file),
            Err(error) if error.raw_os_error() == Some(24) => {
                exhausted = true;
                break;
            }
            Err(error) => panic!("unexpected descriptor filler failure: {error:?}"),
        }
    }
    assert!(
        exhausted,
        "descriptor limit was not reached within the bound"
    );

    assert!(matches!(
        evidence.matches_staging(&candidate_path, &candidate),
        Err(ReservationError::Lock(SingletonError::Io(error)))
            if error.raw_os_error() == Some(24)
    ));

    drop(fillers);
    assert!(
        !evidence
            .matches_staging(&candidate_path, &candidate)
            .unwrap()
    );

    drop(candidate);
    drop(evidence);
    drop(guard);
}
