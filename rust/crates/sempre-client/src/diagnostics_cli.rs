use std::{
    fs, io,
    io::{Read as _, Seek as _},
    path::Path,
    time::Duration,
};

use sempre_control::PublicEndpoint;
use sempre_manager::Manager;
use sempre_state::{Layout, Mode, Store};
use serde_json::json;
use tokio::process::Command;

use crate::ClientError;

const INITIAL_LOG_BYTES: u64 = 64 << 10;

pub(crate) async fn status(mode: Mode, json_output: bool) -> Result<(), ClientError> {
    let layout = Layout::for_mode(mode)?;
    let manager = Manager::new(Store::new(layout.clone()))?;
    let state = manager.state()?;
    let runtime = manager.runtime_status()?;
    let service = sempre_service::status().await.map_or_else(
        |error| format!("unavailable: {error}"),
        |value| value.to_string(),
    );
    let catalog = manager.subscriptions().read()?;
    let profile = state
        .active_profile_id
        .as_deref()
        .and_then(|id| catalog.profiles.iter().find(|profile| profile.id == id));
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "mode": mode_label(mode),
                "selected": state.selected,
                "active": state.active,
                "pending": state.pending,
                "last_error": state.last_error,
                "service": service,
                "runtime": runtime,
                "subscription": {
                    "profile": profile,
                    "schedule": state.subscription,
                    "auto_restart": state.subscription_auto_restart,
                },
                "data": layout.home,
                "logs": [layout.manager_log, layout.core_stdout_log, layout.core_stderr_log],
            }))
            .map_err(ClientError::Json)?
        );
        return Ok(());
    }
    println!("Mode: {}", mode_label(mode));
    println!(
        "Selected: {}",
        state.selected.as_ref().map_or_else(
            || "none".into(),
            |value| reference(&value.core, value.repository.as_deref(), &value.reference)
        )
    );
    println!(
        "Core: {}",
        state.active.as_ref().map_or_else(
            || "not selected".into(),
            |value| format!(
                "{} ({})",
                reference(&value.core, value.repository.as_deref(), &value.reference),
                value.version
            )
        )
    );
    println!("Deployment pending: {}", state.pending);
    if let Some(error) = state.last_error {
        println!("Last deployment error: {error}");
    }
    println!("System service: {service}");
    println!(
        "Desired core state: {}",
        enum_label(&runtime.desired_state)?
    );
    println!(
        "Supervisor: {}, PID {}, restarts {}",
        enum_label(&runtime.runtime_state)?,
        runtime.pid,
        runtime.restart_count
    );
    if let Some(profile) = profile {
        println!(
            "Subscription: {} ({} sources), every {}",
            profile.name,
            profile.sources.len(),
            state.subscription.interval
        );
    } else {
        println!("Subscription: not configured");
    }
    println!("Data: {}", layout.home.display());
    println!("Stdout log: {}", layout.core_stdout_log.display());
    println!("Stderr log: {}", layout.core_stderr_log.display());
    Ok(())
}

pub(crate) async fn logs(mode: Mode, follow: bool) -> Result<(), ClientError> {
    let layout = Layout::for_mode(mode)?;
    let paths = [
        layout.manager_log,
        layout.core_stdout_log,
        layout.core_stderr_log,
    ];
    let mut cursors = vec![LogCursor::default(); paths.len()];
    loop {
        for (index, path) in paths.iter().enumerate() {
            print_delta(path, &mut cursors[index], follow)?;
        }
        if !follow {
            return Ok(());
        }
        tokio::select! {
            _ = tokio::signal::ctrl_c() => return Ok(()),
            () = tokio::time::sleep(Duration::from_millis(250)) => {}
        }
    }
}

pub(crate) fn open(mode: Mode) -> Result<(), ClientError> {
    let layout = Layout::for_mode(mode)?;
    let endpoint = PublicEndpoint::read(&layout.endpoint)?;
    let address = endpoint.local_url;
    if !browser_environment_available() {
        println!("Open this URL in a browser: {address}");
        return Ok(());
    }
    let mut command = browser_command(&address);
    match command.spawn() {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            println!("Open this URL in a browser: {address}");
            Ok(())
        }
        Err(source) => Err(ClientError::Io {
            operation: "open control UI",
            path: address.into(),
            source,
        }),
    }
}

#[derive(Clone, Default)]
struct LogCursor {
    offset: u64,
    partial: Vec<u8>,
}

fn print_delta(path: &Path, cursor: &mut LogCursor, follow: bool) -> Result<(), ClientError> {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io_error("open log", path, source)),
    };
    let size = file
        .metadata()
        .map_err(|source| io_error("inspect log", path, source))?
        .len();
    if size < cursor.offset {
        *cursor = LogCursor::default();
    }
    let mut trim_initial_line = false;
    if cursor.offset == 0 && size > INITIAL_LOG_BYTES {
        cursor.offset = size - INITIAL_LOG_BYTES;
        trim_initial_line = true;
    }
    file.seek(io::SeekFrom::Start(cursor.offset))
        .map_err(|source| io_error("seek log", path, source))?;
    let mut appended = Vec::new();
    file.read_to_end(&mut appended)
        .map_err(|source| io_error("read log", path, source))?;
    cursor.offset = size;
    let mut data = std::mem::take(&mut cursor.partial);
    data.extend(appended);
    if trim_initial_line {
        if let Some(newline) = data.iter().position(|byte| *byte == b'\n') {
            data.drain(..=newline);
        } else if follow {
            cursor.partial = data;
            return Ok(());
        }
    }
    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("log");
    while let Some(newline) = data.iter().position(|byte| *byte == b'\n') {
        let mut line = data.drain(..=newline).collect::<Vec<_>>();
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        println!("[{label}] {}", String::from_utf8_lossy(&line));
    }
    if follow {
        cursor.partial = data;
    } else if !data.is_empty() {
        if data.last() == Some(&b'\r') {
            data.pop();
        }
        println!("[{label}] {}", String::from_utf8_lossy(&data));
    }
    Ok(())
}

fn browser_command(address: &str) -> Command {
    #[cfg(target_os = "macos")]
    return {
        let mut command = Command::new("open");
        command.arg(address);
        command
    };
    #[cfg(target_os = "windows")]
    return {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", address]);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = Command::new("xdg-open");
        command.arg(address);
        command
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn browser_environment_available() -> bool {
    std::env::var_os("DISPLAY").is_some()
        || std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var_os("BROWSER").is_some()
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
const fn browser_environment_available() -> bool {
    true
}

fn reference(core: &str, repository: Option<&str>, value: &str) -> String {
    repository.map_or_else(
        || format!("{core}@{value}"),
        |repository| format!("{core}:{repository}@{value}"),
    )
}

fn enum_label(value: &impl serde::Serialize) -> Result<String, ClientError> {
    serde_json::to_value(value)
        .map_err(ClientError::Json)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ClientError::Runtime("status value is not a string".into()))
}

const fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::System => "system",
        Mode::Portable => "portable",
        Mode::Development => "development",
    }
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> ClientError {
    ClientError::Io {
        operation,
        path: path.into(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    #[test]
    fn references_preserve_custom_repository_identity() {
        assert_eq!(reference("sing-box", None, "stable"), "sing-box@stable");
        assert_eq!(
            reference("sing-box", Some("tinymins/sing-box"), "1.2.3"),
            "sing-box:tinymins/sing-box@1.2.3"
        );
    }

    #[test]
    fn log_cursor_keeps_partial_lines_and_recovers_from_truncation() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("sempre.log");
        fs::write(&path, b"partial").expect("partial log");
        let mut cursor = LogCursor::default();
        print_delta(&path, &mut cursor, true).expect("read partial log");
        assert_eq!(cursor.partial, b"partial");
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open log")
            .write_all(b" line\ncomplete\n")
            .expect("append log");
        print_delta(&path, &mut cursor, true).expect("read completed log");
        assert!(cursor.partial.is_empty());
        fs::write(&path, b"rotated\n").expect("truncate log");
        print_delta(&path, &mut cursor, true).expect("read rotated log");
        assert!(cursor.partial.is_empty());
        assert_eq!(cursor.offset, 8);
    }
}
