#[cfg(unix)]
use std::fs;

#[cfg(unix)]
use sempre_core::CommandSpec;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
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
    // The shell installs its TERM trap before printing; wait for it under slow emulation.
    for _ in 0..100 {
        if fs::read_to_string(&stdout).is_ok_and(|output| output.contains("started")) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let status = process
        .terminate(Duration::from_secs(2))
        .await
        .expect("terminate");
    assert!(status.success());
    assert!(String::from_utf8_lossy(&fs::read(stdout).expect("stdout")).contains("started"));
}
