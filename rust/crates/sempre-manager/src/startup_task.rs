use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::{
    CurrentConfig, Manager, ManagerError, RuntimePendingChange, ValidationRunner, VersionRunner,
};

const MAX_LOG_ENTRIES: usize = 2000;

#[derive(Clone, Debug, Serialize)]
pub struct StartupLogEntry {
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub stage: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<RuntimePendingChange>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StartupTask {
    pub id: String,
    pub action: &'static str,
    pub state: &'static str,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub logs: Vec<StartupLogEntry>,
    pub omitted_logs: u64,
    pub config_available: bool,
    #[serde(skip)]
    config: Option<CurrentConfig>,
    #[serde(skip)]
    armed: bool,
    #[serde(skip)]
    ready: bool,
}

pub(crate) struct StartupTasks {
    current: Mutex<Option<StartupTask>>,
    preparation_changed: tokio::sync::Notify,
    cancellation: tokio::sync::watch::Sender<u64>,
}

impl Default for StartupTasks {
    fn default() -> Self {
        Self {
            current: Mutex::default(),
            preparation_changed: tokio::sync::Notify::new(),
            cancellation: tokio::sync::watch::channel(0).0,
        }
    }
}

impl StartupTasks {
    pub fn output_observer(self: &Arc<Self>) -> sempre_supervisor::OutputObserver {
        let tasks = Arc::clone(self);
        let task_id = tasks.snapshot().expect("startup cycle has a task").id;
        Arc::new(move |stream, line| {
            if stream == "supervisor" {
                tasks.log(stream, line);
            } else {
                tasks.runtime_log_for(&task_id, stream, line);
            }
        })
    }

    pub fn snapshot(&self) -> Option<StartupTask> {
        self.current
            .lock()
            .unwrap()
            .as_ref()
            .map(|task| StartupTask {
                config: None,
                logs: task.logs.clone(),
                id: task.id.clone(),
                ..*task
            })
    }

    pub(crate) fn begin(
        &self,
        action: &'static str,
        changes: Vec<RuntimePendingChange>,
    ) -> Result<StartupTask, ManagerError> {
        let mut current = self.current.lock().unwrap();
        if current.as_ref().is_some_and(|task| task.state == "running") {
            return Err(busy());
        }
        let task = StartupTask::new(action, changes);
        *current = Some(task.clone());
        Ok(task)
    }

    pub fn blocks(&self, action: &str) -> bool {
        self.current
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|task| task.state == "running" && action != "stop")
    }

    pub fn cancellation(&self) -> tokio::sync::watch::Receiver<u64> {
        self.cancellation.subscribe()
    }

    pub fn cancel_preparation(&self) {
        self.cancellation.send_modify(|generation| *generation += 1);
        self.preparation_changed.notify_waiters();
    }

    pub fn begin_stop(&self) {
        let mut task = StartupTask::new("stop", Vec::new());
        task.armed = true;
        task.ready = true;
        *self.current.lock().unwrap() = Some(task);
        self.cancel_preparation();
    }

    pub fn complete_stop(&self, error: Option<&str>) {
        self.update(false, |task| {
            if task.action == "stop" {
                task.finish(
                    if error.is_some() {
                        "failed"
                    } else {
                        "succeeded"
                    },
                    error.unwrap_or(""),
                );
            }
        });
    }

    pub fn preparing(&self) -> bool {
        self.current
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|task| task.state == "running" && task.action != "stop")
    }

    pub(crate) fn with_preparation<T>(
        &self,
        id: &str,
        operation: impl FnOnce() -> Result<T, ManagerError>,
    ) -> Result<T, ManagerError> {
        let current = self.current.lock().unwrap();
        if !current
            .as_ref()
            .is_some_and(|task| task.id == id && task.state == "running" && task.action != "stop")
        {
            return Err(ManagerError::RuntimeAction {
                code: "RUNTIME_STARTUP_CANCELLED",
                message: "Core startup cancelled by stop request".into(),
            });
        }
        operation()
    }

    pub fn begin_cycle(&self) -> bool {
        let mut current = self.current.lock().unwrap();
        if let Some(task) = current.as_mut().filter(|task| task.state == "running") {
            if !task.ready || task.action == "stop" {
                return false;
            }
            task.armed = true;
            return true;
        }
        let mut task = StartupTask::new("start", Vec::new());
        task.armed = true;
        task.ready = true;
        *current = Some(task);
        true
    }

    pub async fn wait_prepared(&self) {
        loop {
            let notified = self.preparation_changed.notified();
            let preparing = self
                .current
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|task| task.state == "running" && !task.ready);
            if !preparing {
                return;
            }
            notified.await;
        }
    }

    pub fn interrupt_cycle(&self) {
        self.update(true, |task| {
            if task.action != "stop" {
                task.finish("failed", "Core startup interrupted");
            }
        });
    }

    pub fn runtime_log_for(&self, id: &str, stage: &str, message: &str) {
        self.update(true, |task| {
            if task.id == id {
                task.push(stage, message);
            }
        });
    }

    pub fn log(&self, stage: &str, message: &str) {
        self.update(false, |task| task.push(stage, message));
    }

    pub fn runtime_log(&self, stage: &str, message: &str) {
        self.update(true, |task| task.push(stage, message));
    }

    pub fn prepared(&self, config: CurrentConfig) {
        self.update(false, |task| {
            if task.action == "stop" {
                return;
            }
            if !task.config_available {
                task.push("compiled", "");
            }
            task.config_available = true;
            task.config = Some(config);
            task.ready = true;
        });
        self.preparation_changed.notify_one();
    }

    pub fn prepared_for(&self, id: &str, config: CurrentConfig) {
        self.update(false, |task| {
            if task.id == id {
                task.config_available = true;
                task.config = Some(config);
                task.ready = true;
                task.push("compiled", "");
            }
        });
        self.preparation_changed.notify_one();
    }

    pub fn capture_config(&self, hash: &str, path: &std::path::Path) {
        if let Ok(content) = std::fs::read_to_string(path) {
            self.prepared(CurrentConfig {
                hash: hash.into(),
                content,
            });
        }
    }

    pub fn failure(&self, stage: &str, error: &str) {
        self.update(true, |task| {
            if task.action != "stop" {
                task.push("error", &format!("{stage}: {error}"));
                task.finish("failed", "");
            }
        });
    }

    pub fn healthy(&self) {
        self.update(true, |task| {
            if task.action != "stop" {
                task.push("healthy", "");
                task.finish("succeeded", "");
            }
        });
    }

    pub fn fail(&self, message: &str) {
        self.update(false, |task| task.finish("failed", message));
        self.preparation_changed.notify_one();
    }

    pub fn fail_for(&self, id: &str, message: &str) {
        self.update(false, |task| {
            if task.id == id {
                task.finish("failed", message);
            }
        });
        self.preparation_changed.notify_one();
    }

    pub fn supervisor_exited(&self, error: Option<&ManagerError>) {
        self.fail(&error.map_or_else(|| "Sempre service stopped".into(), ToString::to_string));
    }

    pub fn stop_log(&self, stage: &str, message: &str) {
        self.update(false, |task| {
            if task.ready && matches!(task.action, "restart" | "stop") {
                task.push(stage, message);
            }
        });
    }

    pub fn stopped(
        &self,
        result: &Result<std::process::ExitStatus, sempre_supervisor::SupervisorError>,
    ) {
        if let Ok(status) = result {
            self.stop_log("stopped", &status.to_string());
        }
    }

    fn update(&self, armed: bool, update: impl FnOnce(&mut StartupTask)) {
        if let Some(task) = self.current.lock().unwrap().as_mut()
            && task.state == "running"
            && (!armed || task.armed)
        {
            update(task);
        }
    }
}

impl StartupTask {
    fn new(action: &'static str, changes: Vec<RuntimePendingChange>) -> Self {
        let now = Utc::now();
        let mut task = Self {
            id: now.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            action,
            state: "running",
            started_at: now,
            finished_at: None,
            logs: Vec::new(),
            omitted_logs: 0,
            config_available: false,
            config: None,
            armed: false,
            ready: false,
        };
        task.push("begin", "");
        for change in changes {
            task.push("change", "");
            task.logs.last_mut().unwrap().change = Some(change);
        }
        task
    }

    fn push(&mut self, stage: &str, message: &str) {
        // Bound both entry count and individual raw output chunks.
        if self.logs.len() == MAX_LOG_ENTRIES {
            self.logs.remove(0);
            self.omitted_logs += 1;
        }
        self.logs.push(StartupLogEntry {
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
    pub(crate) fn log_startup_stopping(&self, pid: u32) {
        let label = self.store.read().ok().map(|document| {
            format!(
                "{}@{} · PID {pid}",
                document.runtime.core.as_deref().unwrap_or("core"),
                document.runtime.version.as_deref().unwrap_or("unknown")
            )
        });
        self.startup_tasks
            .stop_log("stopping", &label.unwrap_or_else(|| format!("PID {pid}")));
    }

    pub fn startup_task(&self) -> Option<StartupTask> {
        self.startup_tasks.snapshot()
    }

    pub fn startup_task_config(&self, id: &str) -> Option<CurrentConfig> {
        self.startup_tasks
            .current
            .lock()
            .unwrap()
            .as_ref()
            .filter(|task| task.id == id)
            .and_then(|task| task.config.clone())
    }
}

impl<R: VersionRunner + ValidationRunner + 'static> Manager<R> {
    pub fn start_startup_task(
        self: &Arc<Self>,
        action: &'static str,
    ) -> Result<StartupTask, ManagerError> {
        let status = self.runtime_status()?;
        let availability = match action {
            "start" => &status.actions.start,
            "restart" => &status.actions.restart,
            _ => {
                return Err(ManagerError::RuntimeAction {
                    code: "INVALID_RUNTIME_ACTION",
                    message: "startup action must be start or restart".into(),
                });
            }
        };
        if !availability.allowed {
            return Err(ManagerError::RuntimeAction {
                code: "RUNTIME_ACTION_UNAVAILABLE",
                message: availability.reason.clone(),
            });
        }
        let task = self.startup_tasks.begin(action, status.pending_changes)?;
        let manager = self.clone();
        let task_id = task.id.clone();
        tokio::spawn(async move {
            if let Err(error) = manager.runtime_action_inner(action, Some(&task_id)).await {
                manager.startup_tasks.fail_for(&task_id, &error.to_string());
            }
        });
        Ok(task)
    }
}

pub(crate) fn busy() -> ManagerError {
    ManagerError::RuntimeAction {
        code: "RUNTIME_STARTUP_IN_PROGRESS",
        message: "a core startup is already in progress".into(),
    }
}

#[cfg(test)]
mod tests;
