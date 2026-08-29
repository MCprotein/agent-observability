use agent_observability_local_runtime::{Singleton, SingletonError};
use std::{
    env, fs,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

#[test]
fn separate_processes_share_exclusive_lock_contract() {
    if env::var_os("LOCAL_RUNTIME_LOCK_CHILD").is_some() {
        let dir = PathBuf::from(env::var_os("LOCAL_RUNTIME_LOCK_DIR").unwrap());
        let _owner = Singleton::acquire(&dir).unwrap();
        fs::write(env::var_os("LOCAL_RUNTIME_LOCK_READY").unwrap(), b"ready").unwrap();
        thread::sleep(Duration::from_millis(250));
        return;
    }
    let dir = env::temp_dir().join(format!("local-runtime-process-{}", std::process::id()));
    let ready = dir.join("ready");
    let child = Command::new(env::current_exe().unwrap())
        .arg("--exact")
        .arg("separate_processes_share_exclusive_lock_contract")
        .env("LOCAL_RUNTIME_LOCK_CHILD", "1")
        .env("LOCAL_RUNTIME_LOCK_DIR", &dir)
        .env("LOCAL_RUNTIME_LOCK_READY", &ready)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !ready.exists() && std::time::Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(ready.exists(), "child did not acquire the runtime lock");
    assert!(matches!(
        Singleton::acquire(&dir),
        Err(SingletonError::AlreadyRunning)
    ));
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let _ = fs::remove_dir_all(dir);
}
