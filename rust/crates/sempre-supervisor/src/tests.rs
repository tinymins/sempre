use std::fs;

#[cfg(unix)]
use sempre_core::CommandSpec;
#[cfg(unix)]
use std::time::Duration;

use super::*;

#[cfg(unix)]
#[tokio::test]
async fn terminates_the_managed_process_group() {
    let root = tempfile::tempdir().expect("temporary directory");
    let stdout = root.path().join("stdout.log");
    let stderr = root.path().join("stderr.log");
    let spec = CommandSpec {
        program: "/bin/sh".into(),
        arguments: vec![
            "-c".into(),
            "trap 'exit 0' TERM; echo started; while :; do sleep 1; done".into(),
        ],
        ..CommandSpec::default()
    };
    let mut process = ManagedProcess::spawn(&spec, &stdout, &stderr).expect("spawn");
    assert!(process.pid() > 0);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let status = process
        .terminate(Duration::from_secs(2))
        .await
        .expect("terminate");
    assert!(status.success());
    assert!(String::from_utf8_lossy(&fs::read(stdout).expect("stdout")).contains("started"));
}

#[cfg(unix)]
#[tokio::test]
async fn output_log_failure_preserves_the_managed_process_exit_status() {
    let root = tempfile::tempdir().expect("temporary directory");
    let stdout = root.path().join("stdout.log");
    let stderr = root.path().join("stderr.log");
    fs::create_dir(&stdout).expect("blocked stdout path");
    fs::create_dir(&stderr).expect("blocked stderr path");
    let spec = CommandSpec {
        program: "/bin/sh".into(),
        arguments: vec!["-c".into(), "echo output; echo error >&2; exit 7".into()],
        ..CommandSpec::default()
    };
    let status = ManagedProcess::spawn(&spec, stdout, stderr)
        .expect("spawn")
        .wait()
        .await
        .expect("wait for process");
    assert_eq!(status.code(), Some(7));
}

#[cfg(unix)]
#[tokio::test]
async fn foreground_process_preserves_exit_status() {
    let spec = CommandSpec {
        program: "/bin/sh".into(),
        arguments: vec!["-c".into(), "exit 7".into()],
        ..CommandSpec::default()
    };
    let status = ManagedProcess::spawn_foreground(&spec)
        .expect("spawn foreground process")
        .wait()
        .await
        .expect("wait for foreground process");
    assert_eq!(status.code(), Some(7));
}
