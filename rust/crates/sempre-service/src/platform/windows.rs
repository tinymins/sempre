use std::{path::Path, time::Duration};

use tokio::time::{Instant, sleep, timeout_at};

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
    let deadline = Instant::now() + Duration::from_secs(10);
    timeout_at(deadline, stop_before(deadline))
        .await
        .map_err(|_| ServiceError::Timeout {
            program: "Windows service stop".into(),
        })?
}

async fn stop_before(deadline: Instant) -> Result<(), ServiceError> {
    let current = status().await?;
    if matches!(current, State::NotInstalled | State::Stopped) {
        return Ok(());
    }
    let before = command("sc.exe", &["queryex", NAME]).await?;
    if current != State::StopPending {
        checked("sc.exe", &["stop", NAME]).await?;
    }
    // Keep forced termination and its confirmation inside the same ten-second budget.
    let result = wait_until(State::Stopped, deadline - Duration::from_secs(1)).await;
    if matches!(result, Err(ServiceError::Timeout { .. })) {
        let after = command("sc.exe", &["queryex", NAME]).await?;
        if before.success
            && after.success
            && let Some(pid) = stalled_service_pid(&before.text, &after.text)
        {
            // Legacy daemons can wait forever for SSE clients. Do not kill the process
            // tree: an upgrade installer may itself be a descendant of the old daemon.
            checked("taskkill.exe", &["/F", "/PID", &pid.to_string()]).await?;
            return wait_until(State::Stopped, deadline).await;
        }
    }
    result
}

fn stalled_service_pid(before: &str, after: &str) -> Option<u32> {
    fn pid(output: &str) -> Option<u32> {
        output
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                (key.trim() == "PID")
                    .then(|| value.trim().parse::<u32>().ok())
                    .flatten()
            })
            .filter(|pid| *pid != 0)
    }
    let original = pid(before)?;
    (parse_status(after) == State::StopPending && pid(after) == Some(original)).then_some(original)
}

async fn wait_for(expected: State) -> Result<(), ServiceError> {
    wait_until(expected, Instant::now() + Duration::from_secs(20)).await
}

async fn wait_until(expected: State, deadline: Instant) -> Result<(), ServiceError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_stop_only_targets_the_same_stalled_service_process() {
        let before = "STATE : 4 RUNNING\nPID : 1234";
        assert_eq!(
            stalled_service_pid(before, "STATE : 3 STOP_PENDING\nPID : 1234"),
            Some(1234)
        );
        for after in [
            "STATE : 4 RUNNING\nPID : 1234",
            "STATE : 3 STOP_PENDING\nPID : 5678",
            "STATE : 1 STOPPED\nPID : 0",
            "STATE : 3 STOP_PENDING",
        ] {
            assert_eq!(stalled_service_pid(before, after), None);
        }
    }
}
