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
        let current: Output = client.get("/api/v1/runtime/restart").await?;
        task = current
            .task
            .filter(|current| current.id == task.id)
            .ok_or_else(|| {
                ClientError::Runtime(
                    "restart task is no longer available; check the runtime status".into(),
                )
            })?;
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
                .filter(|entry| {
                    matches!(
                        entry.stage.as_str(),
                        "error" | "failed" | "rollback" | "rolled_back"
                    )
                })
                .map(|entry| format!("{}: {}", entry.stage, entry.message))
                .collect::<Vec<_>>()
                .join("\n");
            Err(ClientError::Runtime(format!(
                "core restart {state}\n{output}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_uses_the_task_outcome_even_if_the_old_core_is_still_running() {
        let mut task = Task {
            id: "task".into(),
            state: "running".into(),
            logs: Vec::new(),
        };
        assert!(!completed(&task).unwrap());
        task.state = "failed".into();
        task.logs.push(Log {
            stage: "failed".into(),
            message: "configuration validation failed".into(),
        });
        assert!(
            completed(&task)
                .unwrap_err()
                .to_string()
                .contains("configuration validation failed")
        );
        task.state = "rolled_back".into();
        assert!(completed(&task).is_err());
        task.state = "succeeded".into();
        assert!(completed(&task).unwrap());
    }
}
