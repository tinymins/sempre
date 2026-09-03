use std::fs;

#[cfg(unix)]
use sempre_core::CommandSpec;
#[cfg(unix)]
use std::time::Duration;
use tokio::io::AsyncWriteExt as _;

use super::*;

#[tokio::test]
async fn output_observer_preserves_split_utf8_lines_and_final_unterminated_output() {
    let root = tempfile::tempdir().unwrap();
    let (mut input, output) = tokio::io::duplex(64);
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = lines.clone();
    let task = tokio::spawn(log::copy_rolling(
        output,
        root.path().join("stdout"),
        8,
        2,
        Some(std::sync::Arc::new(move |stream, line| {
            observed
                .lock()
                .unwrap()
                .push((stream.to_owned(), line.to_owned()));
        })),
        "stdout",
    ));
    let bytes = "中文\nlast line".as_bytes();
    input.write_all(&bytes[..2]).await.unwrap();
    tokio::task::yield_now().await;
    input.write_all(&bytes[2..]).await.unwrap();
    input.shutdown().await.unwrap();
    task.await.unwrap().unwrap();
    assert_eq!(
        *lines.lock().unwrap(),
        vec![
            ("stdout".into(), "中文".into()),
            ("stdout".into(), "last line".into())
        ]
    );
}

#[tokio::test]
async fn rolling_output_keeps_bounded_backups() {
    let root = tempfile::tempdir().expect("temporary directory");
    let path = root.path().join("core.log");
    let (mut input, output) = tokio::io::duplex(64);
    let task = tokio::spawn(log::copy_rolling(
        output,
        path.clone(),
        8,
        2,
        None,
        "stdout",
    ));
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
