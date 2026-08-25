use std::{path::Path, time::Duration};

use tokio::time::{Instant, sleep};

use crate::{DESCRIPTION, DISPLAY_NAME, NAME, ServiceError, State, checked, command};

pub async fn status() -> Result<State, ServiceError> {
    let output = command("sc.exe", &["query", NAME]).await?;
    if !output.success && output.text.contains("1060") {
        return Ok(State::NotInstalled);
    }
    Ok(parse_status(&output.text))
}

pub async fn install(executable: &Path, _: &Path) -> Result<(), ServiceError> {
    let executable = executable
        .to_str()
        .filter(|_| executable.is_absolute())
        .ok_or_else(|| ServiceError::InvalidPath(executable.display().to_string()))?;
    let command_line = format!("\"{executable}\" service-host");
    if status().await? == State::NotInstalled {
        checked(
            "sc.exe",
            &[
                "create",
                NAME,
                "binPath=",
                &command_line,
                "start=",
                "delayed-auto",
                "DisplayName=",
                DISPLAY_NAME,
            ],
        )
        .await?;
    } else {
        checked(
            "sc.exe",
            &[
                "config",
                NAME,
                "binPath=",
                &command_line,
                "start=",
                "delayed-auto",
            ],
        )
        .await?;
    }
    checked("sc.exe", &["description", NAME, DESCRIPTION]).await?;
    checked(
        "sc.exe",
        &[
            "failure",
            NAME,
            "reset=",
            "300",
            "actions=",
            "restart/5000/restart/15000/restart/60000",
        ],
    )
    .await
}

pub async fn uninstall() -> Result<(), ServiceError> {
    if status().await? == State::NotInstalled {
        return Ok(());
    }
    stop().await?;
    checked("sc.exe", &["delete", NAME]).await
}

pub async fn start() -> Result<(), ServiceError> {
    if status().await? == State::Running {
        return Ok(());
    }
    checked("sc.exe", &["start", NAME]).await?;
    wait_for(State::Running).await
}

pub async fn stop() -> Result<(), ServiceError> {
    if matches!(status().await?, State::NotInstalled | State::Stopped) {
        return Ok(());
    }
    checked("sc.exe", &["stop", NAME]).await?;
    wait_for(State::Stopped).await
}

async fn wait_for(expected: State) -> Result<(), ServiceError> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if status().await? == expected {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(ServiceError::Timeout {
                program: "Windows service transition".into(),
            });
        }
        sleep(Duration::from_millis(250)).await;
    }
}

pub async fn restart() -> Result<(), ServiceError> {
    stop().await?;
    start().await
}

fn parse_status(value: &str) -> State {
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
