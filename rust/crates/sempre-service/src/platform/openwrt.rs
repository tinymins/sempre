//! procd registration used on `OpenWrt`, where `systemctl` does not exist.
use std::{fs, path::Path};

use crate::{ServiceError, State, checked, command, render, require_administrator};

const INIT_SCRIPT: &str = "/etc/init.d/sempre";

pub async fn status() -> Result<State, ServiceError> {
    if !Path::new(INIT_SCRIPT).exists() {
        return Ok(State::NotInstalled);
    }
    let status = command(INIT_SCRIPT, &["status"]).await?;
    Ok(match status.text.as_str() {
        "running" => State::Running,
        "inactive" | "stopped" | "not found" => State::Stopped,
        _ => State::Unknown,
    })
}

pub async fn install(executable: &Path, working_directory: &Path) -> Result<(), ServiceError> {
    require_administrator()?;
    let script = render::procd(executable, working_directory)?;
    sempre_state::write_atomic(Path::new(INIT_SCRIPT), script.as_bytes(), 0o755).map_err(
        |source| ServiceError::Io {
            operation: "write procd registration",
            path: INIT_SCRIPT.into(),
            source,
        },
    )?;
    checked(INIT_SCRIPT, &["enable"]).await
}

pub async fn uninstall() -> Result<(), ServiceError> {
    require_administrator()?;
    if !Path::new(INIT_SCRIPT).exists() {
        return Ok(());
    }
    let _ = checked(INIT_SCRIPT, &["stop"]).await;
    let _ = checked(INIT_SCRIPT, &["disable"]).await;
    fs::remove_file(INIT_SCRIPT).map_err(|source| ServiceError::Io {
        operation: "remove procd registration",
        path: INIT_SCRIPT.into(),
        source,
    })
}

pub async fn start() -> Result<(), ServiceError> {
    require_administrator()?;
    checked(INIT_SCRIPT, &["start"]).await
}

pub async fn stop() -> Result<(), ServiceError> {
    require_administrator()?;
    if matches!(status().await?, State::NotInstalled | State::Stopped) {
        Ok(())
    } else {
        checked(INIT_SCRIPT, &["stop"]).await
    }
}

pub async fn restart() -> Result<(), ServiceError> {
    require_administrator()?;
    checked(INIT_SCRIPT, &["restart"]).await
}
