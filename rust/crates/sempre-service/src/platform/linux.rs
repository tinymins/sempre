use std::{fs, path::Path};

use crate::{NAME, ServiceError, State, checked, command, render, require_administrator};

const UNIT_PATH: &str = "/etc/systemd/system/sempre.service";
const UNIT: &str = "sempre.service";

pub async fn status() -> Result<State, ServiceError> {
    let load = command("systemctl", &["show", "-p", "LoadState", "--value", UNIT]).await?;
    if !load.success || matches!(load.text.as_str(), "" | "not-found") {
        return Ok(State::NotInstalled);
    }
    let active = command("systemctl", &["is-active", UNIT]).await?;
    Ok(match active.text.as_str() {
        "active" => State::Running,
        "activating" => State::StartPending,
        "deactivating" => State::StopPending,
        "inactive" | "failed" => State::Stopped,
        _ => State::Unknown,
    })
}

pub async fn install(executable: &Path, working_directory: &Path) -> Result<(), ServiceError> {
    require_administrator()?;
    let unit = render::systemd(executable, working_directory)?;
    let previous = fs::read(UNIT_PATH).ok();
    sempre_state::write_atomic(Path::new(UNIT_PATH), unit.as_bytes(), 0o644).map_err(|source| {
        ServiceError::Io {
            operation: "write systemd registration",
            path: UNIT_PATH.into(),
            source,
        }
    })?;
    if let Err(error) = reload_and_enable().await {
        restore_registration(previous.as_deref())?;
        let _ = checked("systemctl", &["daemon-reload"]).await;
        return Err(error);
    }
    Ok(())
}

pub async fn uninstall() -> Result<(), ServiceError> {
    require_administrator()?;
    let _ = stop().await;
    let _ = checked("systemctl", &["disable", UNIT]).await;
    match fs::remove_file(UNIT_PATH) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(ServiceError::Io {
                operation: "remove systemd registration",
                path: UNIT_PATH.into(),
                source,
            });
        }
    }
    checked("systemctl", &["daemon-reload"]).await
}

pub async fn start() -> Result<(), ServiceError> {
    require_administrator()?;
    checked("systemctl", &["start", UNIT]).await
}

pub async fn stop() -> Result<(), ServiceError> {
    require_administrator()?;
    if matches!(status().await?, State::NotInstalled | State::Stopped) {
        Ok(())
    } else {
        checked("systemctl", &["stop", UNIT]).await
    }
}

pub async fn restart() -> Result<(), ServiceError> {
    require_administrator()?;
    checked("systemctl", &["restart", UNIT]).await
}

async fn reload_and_enable() -> Result<(), ServiceError> {
    checked("systemctl", &["daemon-reload"]).await?;
    checked("systemctl", &["enable", &format!("{NAME}.service")]).await
}

fn restore_registration(previous: Option<&[u8]>) -> Result<(), ServiceError> {
    match previous {
        Some(data) => {
            sempre_state::write_atomic(Path::new(UNIT_PATH), data, 0o644).map_err(|source| {
                ServiceError::Io {
                    operation: "restore systemd registration",
                    path: UNIT_PATH.into(),
                    source,
                }
            })
        }
        None => match fs::remove_file(UNIT_PATH) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(ServiceError::Io {
                operation: "remove failed systemd registration",
                path: UNIT_PATH.into(),
                source,
            }),
        },
    }
}
