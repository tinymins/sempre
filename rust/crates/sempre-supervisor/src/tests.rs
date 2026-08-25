use std::{fs, time::Duration};

use sempre_core::CommandSpec;
use tokio::io::AsyncWriteExt as _;

use super::*;

#[tokio::test]
async fn rolling_output_keeps_bounded_backups() {
    let root = tempfile::tempdir().expect("temporary directory");
    let path = root.path().join("core.log");
    let (mut input, output) = tokio::io::duplex(64);
    let task = tokio::spawn(log::copy_rolling(output, path.clone(), 8, 2));
    input.write_all(b"12345678").await.expect("first write");
    input.write_all(b"abcdefgh").await.expect("second write");
    input.shutdown().await.expect("shutdown");
    task.await.expect("task").expect("copy");
    assert_eq!(fs::read(&path).expect("current"), b"abcdefgh");
    assert_eq!(
        fs::read(path.with_file_name("core.log.1")).expect("backup"),
        b"12345678"
    );
}

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
