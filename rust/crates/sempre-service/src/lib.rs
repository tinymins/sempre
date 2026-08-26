#[cfg(any(target_os = "linux", target_os = "macos", test))]
mod render;

use std::{io, path::Path, time::Duration};

use serde::Serialize;
use thiserror::Error;
use tokio::{process::Command, time::timeout};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) const NAME: &str = "sempre";
#[cfg(target_os = "windows")]
pub(crate) const DISPLAY_NAME: &str = "Sempre";
pub(crate) const DESCRIPTION: &str = "Cross-platform lifecycle manager for proxy cores";

#[cfg(target_os = "linux")]
#[path = "platform/linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "platform/macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "platform/windows.rs"]
mod platform;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
#[path = "platform/unsupported.rs"]
mod platform;
#[cfg(target_os = "windows")]
mod windows_host;

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

impl std::fmt::Display for State {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NotInstalled => "not installed",
            Self::Stopped => "stopped",
            Self::StartPending => "start pending",
            Self::Running => "running",
            Self::StopPending => "stop pending",
            Self::Unknown => "unknown",
        })
    }
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
    #[error("service path must be absolute and valid Unicode: {0}")]
    InvalidPath(String),
    #[error("administrator access is required; rerun this command with elevation")]
    AdministratorRequired,
    #[error("{operation} {path}: {source}")]
    Io {
        operation: &'static str,
        path: String,
        #[source]
        source: io::Error,
    },
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

pub fn require_installation_privileges() -> Result<(), ServiceError> {
    #[cfg(unix)]
    require_administrator()?;
    Ok(())
}

pub async fn install(executable: &Path, working_directory: &Path) -> Result<(), ServiceError> {
    platform::install(executable, working_directory).await
}

pub async fn uninstall() -> Result<(), ServiceError> {
    platform::uninstall().await
}

pub async fn start() -> Result<(), ServiceError> {
    platform::start().await
}

pub async fn stop() -> Result<(), ServiceError> {
    platform::stop().await
}

pub async fn restart() -> Result<(), ServiceError> {
    platform::restart().await
}

pub async fn action(action: Action) -> Result<(), ServiceError> {
    match action {
        Action::Restart => restart().await,
        Action::Stop => stop().await,
    }
}

#[cfg(target_os = "windows")]
pub use windows_host::dispatch as dispatch_windows_service;

pub(crate) struct Output {
    pub success: bool,
    pub status: String,
    pub text: String,
}

pub(crate) async fn command(program: &str, arguments: &[&str]) -> Result<Output, ServiceError> {
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

pub(crate) async fn checked(program: &str, arguments: &[&str]) -> Result<(), ServiceError> {
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

#[cfg(unix)]
pub(crate) fn require_administrator() -> Result<(), ServiceError> {
    if nix::unistd::Uid::effective().is_root() {
        Ok(())
    } else {
        Err(ServiceError::AdministratorRequired)
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, render};

    #[test]
    fn action_parser_rejects_expansive_service_operations() {
        assert_eq!(Action::parse("restart").expect("restart"), Action::Restart);
        assert_eq!(Action::parse("stop").expect("stop"), Action::Stop);
        assert!(Action::parse("uninstall").is_err());
    }

    #[test]
    fn native_registrations_escape_paths_and_use_the_rust_daemon() {
        let root = std::env::current_dir()
            .expect("current directory")
            .join("Sempre & tools");
        let executable = root.join("sempre");
        let working = root.join("data");
        let systemd = render::systemd(&executable, &working).expect("systemd unit");
        let escaped_executable = executable.to_string_lossy().replace('\\', "\\\\");
        assert!(systemd.contains(&format!(
            "ExecStart=\"{escaped_executable}\" --system daemon"
        )));
        let launchd = render::launchd(&executable, &working).expect("launchd plist");
        assert!(launchd.contains("Sempre &amp; tools"));
        assert!(launchd.contains("<string>--system</string><string>daemon</string>"));
    }
}
