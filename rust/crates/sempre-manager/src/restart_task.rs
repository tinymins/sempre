use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::{
    CurrentConfig, Manager, ManagerError, RuntimePendingChange, ValidationRunner, VersionRunner,
};

const MAX_LOG_ENTRIES: usize = 2000;

#[derive(Clone, Debug, Serialize)]
pub struct RestartLogEntry {
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub stage: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<RuntimePendingChange>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RestartTask {
    pub id: String,
    pub state: &'static str,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub logs: Vec<RestartLogEntry>,
    pub omitted_logs: u64,
    pub config_available: bool,
    #[serde(skip)]
    config: Option<CurrentConfig>,
    #[serde(skip)]
    armed: bool,
    #[serde(skip)]
    rolled_back: bool,
}

#[derive(Default)]
pub(crate) struct RestartTasks(Mutex<Option<RestartTask>>);

impl RestartTasks {
    pub fn snapshot(&self) -> Option<RestartTask> {
        self.0.lock().unwrap().as_ref().map(|task| RestartTask {
            config: None,
            logs: task.logs.clone(),
            id: task.id.clone(),
            ..*task
        })
    }

    pub fn running(&self) -> bool {
        self.0
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|task| task.state == "running")
    }

    fn begin(&self, changes: Vec<RuntimePendingChange>) -> Result<RestartTask, ManagerError> {
        let mut current = self.0.lock().unwrap();
        if current.as_ref().is_some_and(|task| task.state == "running") {
            return Err(busy());
        }
        let now = Utc::now();
        let mut task = RestartTask {
            id: now.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            state: "running",
            started_at: now,
            finished_at: None,
            logs: Vec::new(),
            omitted_logs: 0,
            config_available: false,
            config: None,
            armed: false,
            rolled_back: false,
        };
        task.push("begin", "");
        for change in changes {
            task.push("change", "");
            task.logs.last_mut().unwrap().change = Some(change);
        }
        *current = Some(task.clone());
        Ok(task)
    }

    pub fn log(&self, stage: &str, message: &str) {
        self.update(false, |task| task.push(stage, message));
    }

    pub fn runtime_log(&self, stage: &str, message: &str) {
        self.update(true, |task| task.push(stage, message));
    }

    pub fn prepared(&self, config: CurrentConfig) {
        self.update(false, |task| {
            task.config_available = true;
            task.config = Some(config);
            task.push("compiled", "");
            task.armed = true;
        });
    }

    pub fn failure(&self, stage: &str, error: &str, restored: Option<&str>) {
        self.update(true, |task| {
            task.push("error", &format!("{stage}: {error}"));
            if let Some(restored) = restored {
                task.rolled_back = true;
                task.push("rollback", restored);
            } else {
                task.finish("failed", "");
            }
        });
    }

    pub fn healthy(&self) {
        self.update(true, |task| {
            task.push("healthy", "");
            task.finish(
                if task.rolled_back {
                    "rolled_back"
                } else {
                    "succeeded"
                },
                "",
            );
        });
    }

    pub fn fail(&self, message: &str) {
        self.update(false, |task| task.finish("failed", message));
    }

    pub fn supervisor_exited(&self, error: Option<&ManagerError>) {
        self.fail(&error.map_or_else(|| "Sempre service stopped".into(), ToString::to_string));
    }

    pub fn stopped(
        &self,
        result: &Result<std::process::ExitStatus, sempre_supervisor::SupervisorError>,
    ) {
        if let Ok(status) = result {
            self.runtime_log("stopped", &status.to_string());
        }
    }

    fn update(&self, armed: bool, update: impl FnOnce(&mut RestartTask)) {
        if let Some(task) = self.0.lock().unwrap().as_mut()
            && task.state == "running"
            && (!armed || task.armed)
        {
            update(task);
        }
    }
}

impl RestartTask {
    fn push(&mut self, stage: &str, message: &str) {
        // Bound both entry count and individual raw output chunks.
        if self.logs.len() == MAX_LOG_ENTRIES {
            self.logs.remove(0);
            self.omitted_logs += 1;
        }
        self.logs.push(RestartLogEntry {
            sequence: self.omitted_logs + self.logs.len() as u64,
            timestamp: Utc::now(),
            stage: stage.into(),
            message: message.chars().take(16 * 1024).collect(),
            change: None,
        });
    }

    fn finish(&mut self, state: &'static str, message: &str) {
        self.push(state, message);
        self.state = state;
        self.finished_at = Some(Utc::now());
    }
}

impl<R: VersionRunner> Manager<R> {
    pub(crate) fn log_restart_stopping(&self, pid: u32) {
        let label = self.store.read().ok().map(|document| {
            format!(
                "{}@{} · PID {pid}",
                document.runtime.core.as_deref().unwrap_or("core"),
                document.runtime.version.as_deref().unwrap_or("unknown")
            )
        });
        self.restart_tasks
            .runtime_log("stopping", &label.unwrap_or_else(|| format!("PID {pid}")));
    }

    pub fn restart_task(&self) -> Option<RestartTask> {
        self.restart_tasks.snapshot()
    }

    pub fn restart_task_config(&self, id: &str) -> Option<CurrentConfig> {
        self.restart_tasks
            .0
            .lock()
            .unwrap()
            .as_ref()
            .filter(|task| task.id == id)
            .and_then(|task| task.config.clone())
    }
}

impl<R: VersionRunner + ValidationRunner + 'static> Manager<R> {
    pub fn start_restart_task(self: &Arc<Self>) -> Result<RestartTask, ManagerError> {
        let status = self.runtime_status()?;
        if !status.actions.restart.allowed {
            return Err(ManagerError::RuntimeAction {
                code: "RUNTIME_ACTION_UNAVAILABLE",
                message: status.actions.restart.reason,
            });
        }
        let task = self.restart_tasks.begin(status.pending_changes)?;
        let manager = self.clone();
        tokio::spawn(async move {
            if let Err(error) = manager.runtime_action_inner("restart", true).await {
                manager.restart_tasks.fail(&error.to_string());
            }
        });
        Ok(task)
    }
}

pub(crate) fn busy() -> ManagerError {
    ManagerError::RuntimeAction {
        code: "RUNTIME_RESTART_IN_PROGRESS",
        message: "a core restart is already in progress".into(),
    }
}

#[cfg(test)]
mod tests;
