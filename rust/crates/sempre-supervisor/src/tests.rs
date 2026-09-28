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
        None,
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
async fn output_synchronization_drains_ready_observer_lines() {
    let root = tempfile::tempdir().unwrap();
    let (mut input, output) = tokio::io::duplex(64);
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = lines.clone();
    let (sync_requests, request_receiver) = tokio::sync::mpsc::unbounded_channel();
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
        Some(request_receiver),
    ));
    input.write_all(b"ready\n").await.unwrap();
    let (acknowledge, acknowledged) = tokio::sync::oneshot::channel();
    sync_requests.send(acknowledge).unwrap();
    acknowledged.await.unwrap();
    assert_eq!(
        *lines.lock().unwrap(),
        vec![("stdout".into(), "ready".into())]
    );
    input.shutdown().await.unwrap();
    task.await.unwrap().unwrap();
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
        None,
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

#[tokio::test]
async fn output_logging_recovers_after_storage_becomes_writable() {
    let root = tempfile::tempdir().expect("temporary directory");
    let path = root.path().join("core.log");
    fs::create_dir(&path).expect("blocked log path");
    let (mut input, output) = tokio::io::duplex(64);
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = lines.clone();
    let task = tokio::spawn(log::copy_rolling(
        output,
        path.clone(),
        1024,
        2,
        Some(std::sync::Arc::new(move |_, line| {
            observed.lock().unwrap().push(line.to_owned());
        })),
        "stdout",
        None,
    ));

    input
        .write_all(b"while-full\n")
        .await
        .expect("first output");
    tokio::time::timeout(Duration::from_secs(1), async {
        while lines.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("observer output");
    fs::remove_dir(&path).expect("restore writable path");
    tokio::time::sleep(Duration::from_millis(20)).await;
    input
        .write_all(b"after-recovery\n")
        .await
        .expect("recovered output");
    input.shutdown().await.expect("shutdown");

    task.await.expect("task").expect("copy");
    assert_eq!(*lines.lock().unwrap(), ["while-full", "after-recovery"]);
    assert_eq!(fs::read(path).expect("recovered log"), b"after-recovery\n");
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
