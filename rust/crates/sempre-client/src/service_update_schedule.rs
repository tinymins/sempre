use std::path::Path;

use tempfile::TempDir;

pub(crate) fn schedule(temporary: TempDir, executable: &Path, log: &Path) -> Result<(), String> {
    let root = temporary.keep();
    let scheduled = platform_schedule(executable, &root, log);
    if scheduled.is_err() {
        let _ = std::fs::remove_dir_all(&root);
    }
    scheduled
}

#[cfg(target_os = "linux")]
fn platform_schedule(executable: &Path, root: &Path, log: &Path) -> Result<(), String> {
    let unit = format!("sempre-update-{}", uuid::Uuid::new_v4());
    let script = "sleep 1; \"$1\" --portable install --yes >\"$3.stdout.log\" 2>\"$3.stderr.log\"; code=$?; if [ \"$code\" -ne 0 ]; then printf '\\nSempre installer exited with code %s\\n' \"$code\" >>\"$3.stderr.log\"; fi; rm -rf -- \"$2\"; exit $code";
    let status = std::process::Command::new("systemd-run")
        .args(["--quiet", "--collect", "--no-block", "--unit", &unit])
        .arg("/bin/sh")
        .args(["-c", script, "sempre-update"])
        .arg(executable)
        .arg(root)
        .arg(log)
        .status()
        .map_err(|error| format!("schedule systemd update: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("schedule systemd update: {status}"))
    }
}

#[cfg(target_os = "macos")]
fn platform_schedule(executable: &Path, root: &Path, log: &Path) -> Result<(), String> {
    let label = format!("io.sempre.update.{}", uuid::Uuid::new_v4());
    let script = "sleep 1; \"$1\" --portable install --yes >\"$3.stdout.log\" 2>\"$3.stderr.log\"; code=$?; if [ \"$code\" -ne 0 ]; then printf '\\nSempre installer exited with code %s\\n' \"$code\" >>\"$3.stderr.log\"; fi; rm -rf -- \"$2\"; exit $code";
    let status = std::process::Command::new("launchctl")
        .args([
            "submit",
            "-l",
            &label,
            "--",
            "/bin/sh",
            "-c",
            script,
            "sempre-update",
        ])
        .arg(executable)
        .arg(root)
        .arg(log)
        .status()
        .map_err(|error| format!("schedule launchd update: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("schedule launchd update: {status}"))
    }
}

#[cfg(target_os = "windows")]
fn platform_schedule(executable: &Path, root: &Path, log: &Path) -> Result<(), String> {
    let executable = executable
        .to_str()
        .ok_or_else(|| "update executable path is not Unicode".to_string())?;
    let root = root
        .to_str()
        .ok_or_else(|| "update directory path is not Unicode".to_string())?;
    let log = log
        .to_str()
        .ok_or_else(|| "update log path is not Unicode".to_string())?;
    let script = format!(
        "$ErrorActionPreference='Stop'; Start-Sleep -Seconds 1; $log={}; $code=1; try {{ $p=Start-Process -FilePath {} -ArgumentList '--portable install --yes' -PassThru -RedirectStandardOutput ($log+'.stdout.log') -RedirectStandardError ($log+'.stderr.log'); $handle=$p.Handle; $p.WaitForExit(); $p.Refresh(); $code=$p.ExitCode }} catch {{ [IO.File]::WriteAllText(($log+'.stderr.log'), $_.ToString()); $code=1 }}; if ($code -ne 0) {{ [IO.File]::AppendAllText(($log+'.stderr.log'), \"`nSempre installer exited with code $code`n\") }}; Remove-Item -LiteralPath {} -Recurse -Force -ErrorAction SilentlyContinue; exit $code",
        powershell_literal(log),
        powershell_literal(executable),
        powershell_literal(root),
    );
    std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("schedule Windows update: {error}"))
}

#[cfg(target_os = "windows")]
fn powershell_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_schedule(_: &Path, _: &Path, _: &Path) -> Result<(), String> {
    Err("Sempre updates are unavailable on this operating system".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{
        fs, thread,
        time::{Duration, Instant},
    };

    #[test]
    fn installer_failure_retains_stderr_after_the_staging_directory_is_removed() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("staging");
        fs::create_dir(&staging).unwrap();
        let log = root.path().join("log");
        // The Rust test harness rejects installer arguments and exits with an error.
        platform_schedule(&std::env::current_exe().unwrap(), &staging, &log).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while staging.exists() {
            assert!(Instant::now() < deadline, "update wrapper did not finish");
            thread::sleep(Duration::from_millis(50));
        }
        assert!(!log.exists());
        assert!(
            fs::read_to_string(log.with_extension("stderr.log"))
                .unwrap()
                .contains("Sempre installer exited with code")
        );
    }
}
