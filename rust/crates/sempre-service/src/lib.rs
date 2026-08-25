use std::{io, time::Duration};

use serde::Serialize;
use thiserror::Error;
use tokio::{process::Command, time::timeout};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum State {
    #[serde(rename = "not installed")]
    NotInstalled,
    #[serde(rename = "stopped")]
    Stopped,
    #[serde(rename = "start pending")]
    StartPending,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "stop pending")]
    StopPending,
    #[serde(rename = "unknown")]
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Restart,
    Stop,
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("service action must be restart or stop")]
    InvalidAction,
    #[error("run {program}: {source}")]
    Start { program: String, source: io::Error },
    #[error("{program} timed out after 30 seconds")]
    Timeout { program: String },
    #[error("{program} failed with {status}: {output}")]
    Command {
        program: String,
        status: String,
        output: String,
    },
}

impl Action {
    pub fn parse(value: &str) -> Result<Self, ServiceError> {
        match value {
            "restart" => Ok(Self::Restart),
            "stop" => Ok(Self::Stop),
            _ => Err(ServiceError::InvalidAction),
        }
    }
}

pub async fn status() -> Result<State, ServiceError> {
    platform::status().await
}

pub async fn action(action: Action) -> Result<(), ServiceError> {
    platform::action(action).await
}

struct Output {
    success: bool,
    status: String,
    text: String,
}

async fn command(program: &str, arguments: &[&str]) -> Result<Output, ServiceError> {
    let result = timeout(
        COMMAND_TIMEOUT,
        Command::new(program).args(arguments).output(),
    )
    .await
    .map_err(|_| ServiceError::Timeout {
        program: program.into(),
    })?
    .map_err(|source| ServiceError::Start {
        program: program.into(),
        source,
    })?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    )
    .trim()
    .to_owned();
    Ok(Output {
        success: result.status.success(),
        status: result.status.to_string(),
        text,
    })
}

async fn checked(program: &str, arguments: &[&str]) -> Result<(), ServiceError> {
    let output = command(program, arguments).await?;
    if output.success {
        Ok(())
    } else {
        Err(ServiceError::Command {
            program: program.into(),
            status: output.status,
            output: output.text,
        })
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{Action, ServiceError, State, checked, command};

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

    pub async fn action(action: Action) -> Result<(), ServiceError> {
        checked(
            "systemctl",
            &[
                match action {
                    Action::Restart => "restart",
                    Action::Stop => "stop",
                },
                UNIT,
            ],
        )
        .await
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::Path;

    use super::{Action, ServiceError, State, checked, command};

    const LABEL: &str = "io.github.tinymins.sempre";
    const PLIST: &str = "/Library/LaunchDaemons/io.github.tinymins.sempre.plist";

    pub async fn status() -> Result<State, ServiceError> {
        let output = command("launchctl", &["print", &format!("system/{LABEL}")]).await?;
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

    pub async fn action(action: Action) -> Result<(), ServiceError> {
        match action {
            Action::Restart => {
                checked("launchctl", &["enable", &format!("system/{LABEL}")]).await?;
                checked(
                    "launchctl",
                    &["kickstart", "-k", &format!("system/{LABEL}")],
                )
                .await
            }
            Action::Stop => checked("launchctl", &["bootout", &format!("system/{LABEL}")]).await,
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{Action, ServiceError, State, checked, command};

    const NAME: &str = "sempre";

    pub async fn status() -> Result<State, ServiceError> {
        let output = command("sc.exe", &["query", NAME]).await?;
        if !output.success && output.text.contains("1060") {
            return Ok(State::NotInstalled);
        }
        Ok(parse_windows_status(&output.text))
    }

    pub async fn action(action: Action) -> Result<(), ServiceError> {
        checked("sc.exe", &["stop", NAME]).await?;
        if action == Action::Restart {
            checked("sc.exe", &["start", NAME]).await?;
        }
        Ok(())
    }

    fn parse_windows_status(value: &str) -> State {
        if value.contains("RUNNING") {
            State::Running
        } else if value.contains("START_PENDING") {
            State::StartPending
        } else if value.contains("STOP_PENDING") {
            State::StopPending
        } else if value.contains("STOPPED") {
            State::Stopped
        } else {
            State::Unknown
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    use super::{Action, ServiceError, State};

    pub async fn status() -> Result<State, ServiceError> {
        Ok(State::Unknown)
    }

    pub async fn action(_action: Action) -> Result<(), ServiceError> {
        Err(ServiceError::InvalidAction)
    }
}

#[cfg(test)]
mod tests {
    use super::Action;

    #[test]
    fn action_parser_rejects_expansive_service_operations() {
        assert_eq!(Action::parse("restart").expect("restart"), Action::Restart);
        assert_eq!(Action::parse("stop").expect("stop"), Action::Stop);
        assert!(Action::parse("uninstall").is_err());
    }
}
