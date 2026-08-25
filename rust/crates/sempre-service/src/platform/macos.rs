use std::{fs, path::Path};

use crate::{ServiceError, State, checked, command, render, require_administrator};

const LABEL: &str = "io.github.tinymins.sempre";
const PLIST: &str = "/Library/LaunchDaemons/io.github.tinymins.sempre.plist";

pub async fn status() -> Result<State, ServiceError> {
    let output = query().await?;
    if output.success {
        return Ok(if output.text.contains("state = running") {
            State::Running
        } else {
            State::Stopped
        });
    }
    Ok(if Path::new(PLIST).is_file() {
        State::Stopped
    } else {
        State::NotInstalled
    })
}

pub async fn install(executable: &Path, working_directory: &Path) -> Result<(), ServiceError> {
    require_administrator()?;
    let plist = render::launchd(executable, working_directory)?;
    let previous = fs::read(PLIST).ok();
    stop().await?;
    sempre_state::write_atomic(Path::new(PLIST), plist.as_bytes(), 0o644).map_err(|source| {
        ServiceError::Io {
            operation: "write launchd registration",
            path: PLIST.into(),
            source,
        }
    })?;
    if let Err(error) = bootstrap().await {
        restore_registration(previous.as_deref())?;
        let _ = bootstrap().await;
        return Err(error);
    }
    checked("launchctl", &["enable", &domain()]).await
}

pub async fn uninstall() -> Result<(), ServiceError> {
    require_administrator()?;
    stop().await?;
    match fs::remove_file(PLIST) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ServiceError::Io {
            operation: "remove launchd registration",
            path: PLIST.into(),
            source,
        }),
    }
}

pub async fn start() -> Result<(), ServiceError> {
    require_administrator()?;
    match status().await? {
        State::NotInstalled => Err(ServiceError::InvalidPath(PLIST.into())),
        State::Running => Ok(()),
        _ => {
            checked("launchctl", &["enable", &domain()]).await?;
            if query().await?.success {
                checked("launchctl", &["kickstart", &domain()]).await
            } else {
                bootstrap().await
            }
        }
    }
}

pub async fn stop() -> Result<(), ServiceError> {
    require_administrator()?;
    if query().await?.success {
        checked("launchctl", &["bootout", &domain()]).await
    } else {
        Ok(())
    }
}

pub async fn restart() -> Result<(), ServiceError> {
    require_administrator()?;
    if query().await?.success {
        checked("launchctl", &["enable", &domain()]).await?;
        checked("launchctl", &["kickstart", "-k", &domain()]).await
    } else {
        start().await
    }
}

async fn query() -> Result<crate::Output, ServiceError> {
    command("launchctl", &["print", &domain()]).await
}

async fn bootstrap() -> Result<(), ServiceError> {
    checked("launchctl", &["bootstrap", "system", PLIST]).await
}

fn domain() -> String {
    format!("system/{LABEL}")
}

fn restore_registration(previous: Option<&[u8]>) -> Result<(), ServiceError> {
    match previous {
        Some(data) => sempre_state::write_atomic(Path::new(PLIST), data, 0o644).map_err(|source| {
            ServiceError::Io {
                operation: "restore launchd registration",
                path: PLIST.into(),
                source,
            }
        }),
        None => match fs::remove_file(PLIST) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(ServiceError::Io {
                operation: "remove failed launchd registration",
                path: PLIST.into(),
                source,
            }),
        },
    }
}
