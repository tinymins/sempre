use std::time::Duration;

use sempre_manager::RuntimeStatus;
use sempre_state::{DesiredState, Layout, Mode, RuntimeState};
use serde::Deserialize;

use crate::{ClientError, args::RuntimeCommand, local_api::LocalApi};

#[derive(Deserialize)]
struct ActionOutput {
    status: RuntimeStatus,
}

#[derive(Deserialize)]
struct ReloadOutput {
    status: RuntimeStatus,
}

pub(crate) async fn run(
    mode: Mode,
    command: RuntimeCommand,
    json: bool,
) -> Result<(), ClientError> {
    let layout = Layout::for_mode(mode)?;
    let client = LocalApi::discover(&layout.daemon_control)?;
    let status = match command {
        RuntimeCommand::Status => client.get("/api/v1/runtime/status").await?,
        RuntimeCommand::Reload => {
            let result: ReloadOutput = client.post("/api/v1/runtime/reload").await?;
            result.status
        }
        RuntimeCommand::Start => action(&client, "start").await?,
        RuntimeCommand::Stop => action(&client, "stop").await?,
        RuntimeCommand::Restart => action(&client, "restart").await?,
    };
    print_status(&status, json)?;
    Ok(())
}

async fn action(client: &LocalApi, action: &str) -> Result<RuntimeStatus, ClientError> {
    let before: RuntimeStatus = client.get("/api/v1/runtime/status").await?;
    let accepted: ActionOutput = client.post(&format!("/api/v1/runtime/{action}")).await?;
    if complete(action, &before, &accepted.status) {
        return Ok(accepted.status);
    }
    let deadline = tokio::time::Instant::now() + Duration::from_mins(1);
    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let current: RuntimeStatus = client.get("/api/v1/runtime/status").await?;
        if complete(action, &before, &current) {
            return Ok(current);
        }
        if current.runtime_state == RuntimeState::Failed {
            return Err(ClientError::Runtime(
                current
                    .last_error
                    .unwrap_or_else(|| "managed core entered failed state".into()),
            ));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(ClientError::Runtime(format!(
                "timed out waiting for managed core to {action}"
            )));
        }
    }
}

fn complete(action: &str, before: &RuntimeStatus, current: &RuntimeStatus) -> bool {
    match action {
        "stop" => {
            current.desired_state == DesiredState::Stopped
                && matches!(
                    current.runtime_state,
                    RuntimeState::Stopped | RuntimeState::Idle
                )
        }
        "restart" => {
            current.runtime_state == RuntimeState::Running
                && (before.pid == 0 || current.pid != before.pid)
        }
        _ => current.runtime_state == RuntimeState::Running,
    }
}

fn print_status(status: &RuntimeStatus, json: bool) -> Result<(), ClientError> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(status).map_err(ClientError::Json)?
        );
        return Ok(());
    }
    let deployment = status.active.as_ref().or(status.target.as_ref());
    println!("Desired: {}", label(&status.desired_state)?);
    println!("State: {}", label(&status.runtime_state)?);
    println!(
        "Core: {}",
        deployment.map_or("none", |value| value.exact_reference.as_str())
    );
    println!(
        "Config: {}",
        deployment.map_or("none", |value| value.config_hash.as_str())
    );
    println!("PID: {}", status.pid);
    println!("Uptime: {}s", status.uptime_seconds);
    println!("Restarts: {}", status.restart_count);
    println!("Pending: {}", status.pending);
    if let Some(error) = &status.last_error {
        println!("Last error: {error}");
    }
    Ok(())
}

fn label(value: &impl serde::Serialize) -> Result<String, ClientError> {
    serde_json::to_value(value)
        .map_err(ClientError::Json)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ClientError::Runtime("runtime state is not a string".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use sempre_manager::{RuntimeActionAvailability, RuntimeActions};

    fn status(desired: DesiredState, runtime: RuntimeState, pid: u32) -> RuntimeStatus {
        let available = || RuntimeActionAvailability {
            allowed: true,
            reason: String::new(),
        };
        RuntimeStatus {
            desired_state: desired,
            runtime_state: runtime,
            active: None,
            target: None,
            pid,
            started_at: Some(Utc::now()),
            uptime_seconds: 1,
            restart_count: 0,
            pending: false,
            last_transition: None,
            last_exit: None,
            last_error: None,
            last_failure: None,
            actions: RuntimeActions {
                start: available(),
                stop: available(),
                restart: available(),
            },
        }
    }

    #[test]
    fn lifecycle_completion_requires_observed_runtime_state() {
        let before = status(DesiredState::Running, RuntimeState::Running, 41);
        let stopping = status(DesiredState::Running, RuntimeState::Stopping, 41);
        let restarted = status(DesiredState::Running, RuntimeState::Running, 42);
        assert!(!complete("restart", &before, &stopping));
        assert!(complete("restart", &before, &restarted));
        let stopped = status(DesiredState::Stopped, RuntimeState::Stopped, 0);
        assert!(complete("stop", &before, &stopped));
    }
}
