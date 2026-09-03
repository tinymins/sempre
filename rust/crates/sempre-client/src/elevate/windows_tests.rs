use std::{fs, path::PathBuf, process::Stdio, thread, time::Duration};

use super::*;

const FIXTURE: &str = "elevate::windows_tests::elevation_process_fixture";

#[test]
fn elevation_process_fixture() {
    let Some(root) = std::env::var_os("SEMPRE_ELEVATION_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    if std::env::var("SEMPRE_ELEVATION_TEST_CHILD").is_err() {
        let _child = Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", FIXTURE])
            .env("SEMPRE_ELEVATION_TEST_CHILD", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("cleanup descendant");
        std::process::exit(7);
    }
    for _ in 0..200 {
        if root.join("released").exists() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    fs::write(root.join("completed"), b"done").expect("completion marker");
}

#[test]
fn elevation_waits_only_for_command_and_preserves_its_exit_code() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let executable = std::env::current_exe().expect("test executable");
    let script = windows_elevation_script(
        executable.to_str().expect("executable path"),
        &format!("--exact {FIXTURE}"),
        temporary.path().to_str().expect("working directory"),
    )
    // Exercise the same waiting logic without opening an interactive UAC prompt.
    .replace(" -Verb RunAs", " -NoNewWindow");
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .env("SEMPRE_ELEVATION_TEST_ROOT", temporary.path())
        .stdout(Stdio::null())
        .status()
        .expect("elevation wrapper");
    let descendant_pending = !temporary.path().join("completed").exists();
    fs::write(temporary.path().join("released"), b"release").expect("release descendant");
    for _ in 0..100 {
        if temporary.path().join("completed").exists() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(temporary.path().join("completed").exists());
    assert_eq!(status.code(), Some(7));
    assert!(descendant_pending, "wrapper waited for cleanup descendant");
}

#[test]
fn elevated_failure_fixture() {
    if std::env::var_os("SEMPRE_ELEVATION_FAILURE_FIXTURE").is_some() {
        report_error(&"elevated failure fixture", true);
        std::process::exit(7);
    }
}

#[test]
fn elevated_failure_without_a_console_reports_the_error_and_exits() {
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "elevate::windows_tests::elevated_failure_fixture",
            "--nocapture",
        ])
        .env("SEMPRE_ELEVATION_FAILURE_FIXTURE", "1")
        .stdin(Stdio::null())
        .output()
        .expect("failure fixture");
    assert_eq!(output.status.code(), Some(7));
    let error = String::from_utf8(output.stderr).expect("error output");
    assert!(error.contains("ERROR: elevated failure fixture"));
    assert!(!error.contains("Press Enter"));
}
