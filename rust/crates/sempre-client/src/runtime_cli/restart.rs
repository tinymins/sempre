use std::time::Duration;

use sempre_manager::RuntimeStatus;
use serde::Deserialize;

use crate::{ClientError, local_api::LocalApi};

#[derive(Deserialize)]
pub(super) struct Task {
    id: String,
    state: String,
    logs: Vec<Log>,
}

#[derive(Deserialize)]
struct Log {
    stage: String,
    message: String,
}

#[derive(Deserialize)]
struct Output {
    task: Option<Task>,
}

pub(super) async fn wait(client: &LocalApi, mut task: Task) -> Result<RuntimeStatus, ClientError> {
    let deadline = tokio::time::Instant::now() + Duration::from_mins(1);
    loop {
        if completed(&task)? {
            return Ok(client.get("/api/v1/runtime/status").await?);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(ClientError::Runtime("timed out waiting for core restart; the task continues in the background and can be viewed in the Web console".into()));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        let current: Output = client.get("/api/v1/runtime/startup-task").await?;
        let Some(current) = current.task.filter(|current| current.id == task.id) else {
            return superseded(client).await;
        };
        task = current;
    }
}

/// The supervisor opens a new task when it recovers, as it does after a rollback, so
/// the runtime state decides the outcome of the restart.
async fn superseded(client: &LocalApi) -> Result<RuntimeStatus, ClientError> {
    let status: RuntimeStatus = client.get("/api/v1/runtime/status").await?;
    match status.last_error.as_deref().filter(|_| status.pending) {
        Some(error) => Err(ClientError::Runtime(format!(
            "deployment failed and the previous configuration was restored: {error}"
        ))),
        None => Ok(status),
    }
}

fn completed(task: &Task) -> Result<bool, ClientError> {
    match task.state.as_str() {
        "running" => Ok(false),
        "succeeded" => Ok(true),
        state => {
            let output = task
                .logs
                .iter()
                .filter(|entry| matches!(entry.stage.as_str(), "error" | "failed"))
                .map(|entry| format!("{}: {}", entry.stage, entry.message))
                .collect::<Vec<_>>()
                .join("\n");
            Err(ClientError::Runtime(format!(
                "core restart {state}\n{output}"
            )))
        }
    }
}
