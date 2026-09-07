use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const PERSIST_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ServiceUpdateTask {
    pub(crate) id: String,
    pub(crate) state: String,
    pub(crate) stage: String,
    pub(crate) current_version: String,
    pub(crate) target_version: String,
    pub(crate) artifact: Option<String>,
    pub(crate) downloaded_bytes: u64,
    pub(crate) total_bytes: u64,
    pub(crate) bytes_per_second: u64,
    pub(crate) eta_seconds: Option<u64>,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) finished_at: Option<DateTime<Utc>>,
    pub(crate) error: Option<String>,
}

struct TaskState {
    task: Option<ServiceUpdateTask>,
    download_started: Option<Instant>,
    last_persisted: Instant,
}

pub(crate) struct ServiceUpdateTasks {
    task_path: PathBuf,
    result_path: PathBuf,
    current_version: String,
    state: Mutex<TaskState>,
}

impl ServiceUpdateTasks {
    pub(crate) fn new(home: &Path, current_version: &str) -> Self {
        let task_path = home.join("service-update-task.json");
        let result_path = home.join("service-update-result");
        let task = read_task(&task_path);
        Self {
            task_path,
            result_path,
            current_version: normalized_version(current_version).into(),
            state: Mutex::new(TaskState {
                task,
                download_started: None,
                last_persisted: Instant::now(),
            }),
        }
    }

    pub(crate) fn begin(&self) -> Result<ServiceUpdateTask, String> {
        self.reconcile_result();
        let mut state = self.state.lock().unwrap();
        if state
            .task
            .as_ref()
            .is_some_and(|task| task.state == "running")
        {
            return Err("a Sempre update is already in progress".into());
        }
        remove_if_exists(&self.result_path)?;
        let now = Utc::now();
        let task = ServiceUpdateTask {
            id: uuid::Uuid::new_v4().to_string(),
            state: "running".into(),
            stage: "checking".into(),
            current_version: self.current_version.clone(),
            target_version: String::new(),
            artifact: None,
            downloaded_bytes: 0,
            total_bytes: 0,
            bytes_per_second: 0,
            eta_seconds: None,
            started_at: now,
            updated_at: now,
            finished_at: None,
            error: None,
        };
        self.persist(&task)?;
        state.task = Some(task.clone());
        state.download_started = None;
        state.last_persisted = Instant::now();
        Ok(task)
    }

    pub(crate) fn snapshot(&self) -> Option<ServiceUpdateTask> {
        self.reconcile_result();
        self.state.lock().unwrap().task.clone()
    }

    pub(crate) fn set_release(
        &self,
        id: &str,
        version: &str,
        artifact: &str,
        total: u64,
    ) -> Result<(), String> {
        self.update(id, true, |state, now| {
            let task = state.task.as_mut().expect("matching task");
            task.target_version = normalized_version(version).into();
            task.artifact = Some(artifact.into());
            task.total_bytes = total;
            task.stage = "resolving".into();
            task.updated_at = now;
        })
    }

    pub(crate) fn set_stage(&self, id: &str, stage: &str) -> Result<(), String> {
        self.update(id, true, |state, now| {
            let task = state.task.as_mut().expect("matching task");
            task.stage = stage.into();
            task.updated_at = now;
        })
    }

    pub(crate) fn download_progress(&self, id: &str, downloaded: u64, total: u64) {
        let (task, persist) = {
            let mut state = self.state.lock().unwrap();
            if state
                .task
                .as_ref()
                .is_none_or(|task| task.id != id || task.state != "running")
            {
                return;
            }
            let first_sample = state.download_started.is_none();
            if first_sample {
                state.download_started = Some(Instant::now());
            }
            let elapsed_millis = state
                .download_started
                .expect("download start")
                .elapsed()
                .as_millis();
            let elapsed_millis = u64::try_from(elapsed_millis).unwrap_or(u64::MAX).max(1);
            let speed = downloaded.saturating_mul(1000) / elapsed_millis;
            let task = state.task.as_mut().expect("matching task");
            task.stage = "downloading".into();
            task.downloaded_bytes = downloaded;
            task.total_bytes = total;
            task.bytes_per_second = speed;
            task.eta_seconds =
                (speed > 0 && total > downloaded).then(|| (total - downloaded).div_ceil(speed));
            task.updated_at = Utc::now();
            let task = task.clone();
            let persist = first_sample
                || state.last_persisted.elapsed() >= PERSIST_INTERVAL
                || downloaded == total;
            if persist {
                state.last_persisted = Instant::now();
            }
            (task, persist)
        };
        if persist {
            let _ = self.persist(&task);
        }
    }

    pub(crate) fn fail(&self, id: &str, error: &str) {
        let _ = self.update(id, true, |state, now| {
            let task = state.task.as_mut().expect("matching task");
            task.state = "failed".into();
            task.stage = "failed".into();
            task.error = Some(error.into());
            task.updated_at = now;
            task.finished_at = Some(now);
            task.eta_seconds = None;
        });
    }

    pub(crate) fn result_path(&self) -> &Path {
        &self.result_path
    }

    fn update(
        &self,
        id: &str,
        persist: bool,
        change: impl FnOnce(&mut TaskState, DateTime<Utc>),
    ) -> Result<(), String> {
        let task = {
            let mut state = self.state.lock().unwrap();
            if state.task.as_ref().is_none_or(|task| task.id != id) {
                return Err("Sempre update task is no longer available".into());
            }
            change(&mut state, Utc::now());
            state.task.as_ref().expect("matching task").clone()
        };
        if persist {
            self.persist(&task)?;
        }
        Ok(())
    }

    fn reconcile_result(&self) {
        let result = fs::read_to_string(&self.result_path).ok();
        let mut state = self.state.lock().unwrap();
        let Some(task) = state.task.as_mut().filter(|task| task.state == "running") else {
            return;
        };
        let target_is_running = !task.target_version.is_empty()
            && normalized_version(&task.target_version) == self.current_version;
        let outcome = result.as_deref().map(str::trim);
        if outcome == Some("succeeded") || (outcome.is_none() && target_is_running) {
            finish(task, "succeeded", "completed", None);
        } else if let Some(code) = outcome.and_then(|value| value.strip_prefix("failed:")) {
            finish(
                task,
                "failed",
                "failed",
                Some(format!("Sempre installer exited with code {code}")),
            );
        } else {
            return;
        }
        let snapshot = task.clone();
        drop(state);
        let _ = self.persist(&snapshot);
        let _ = fs::remove_file(&self.result_path);
    }

    fn persist(&self, task: &ServiceUpdateTask) -> Result<(), String> {
        let parent = self
            .task_path
            .parent()
            .ok_or_else(|| "Sempre update task path has no parent".to_string())?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("create Sempre update state directory: {error}"))?;
        let data = serde_json::to_vec_pretty(task)
            .map_err(|error| format!("encode Sempre update task: {error}"))?;
        sempre_state::write_atomic(&self.task_path, &data, 0o600)
            .map_err(|error| format!("write Sempre update task: {error}"))
    }
}

fn read_task(path: &Path) -> Option<ServiceUpdateTask> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn remove_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("clear previous Sempre update result: {error}")),
    }
}

fn finish(task: &mut ServiceUpdateTask, outcome: &str, phase: &str, error: Option<String>) {
    let now = Utc::now();
    task.state = outcome.into();
    task.stage = phase.into();
    task.error = error;
    task.updated_at = now;
    task.finished_at = Some(now);
    task.eta_seconds = None;
}

fn normalized_version(value: &str) -> &str {
    value.strip_prefix('v').unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_is_persisted_and_restored() {
        let root = tempfile::tempdir().expect("temporary directory");
        let tasks = ServiceUpdateTasks::new(root.path(), "2.0.0");
        let task = tasks.begin().expect("task");
        tasks
            .set_release(&task.id, "2.0.10", "bundle.zip", 100)
            .expect("release");
        tasks.download_progress(&task.id, 50, 100);

        let restored = ServiceUpdateTasks::new(root.path(), "2.0.0")
            .snapshot()
            .expect("restored task");
        assert_eq!(restored.stage, "downloading");
        assert_eq!((restored.downloaded_bytes, restored.total_bytes), (50, 100));
    }

    #[test]
    fn running_target_version_reconciles_as_success() {
        let root = tempfile::tempdir().expect("temporary directory");
        let tasks = ServiceUpdateTasks::new(root.path(), "2.0.0");
        let task = tasks.begin().expect("task");
        tasks
            .set_release(&task.id, "2.0.10", "bundle.zip", 100)
            .expect("release");
        tasks.set_stage(&task.id, "installing").expect("stage");

        let completed = ServiceUpdateTasks::new(root.path(), "2.0.10")
            .snapshot()
            .expect("completed task");
        assert_eq!(completed.state, "succeeded");
        assert_eq!(completed.stage, "completed");
    }

    #[test]
    fn installer_failure_is_reported_after_restart() {
        let root = tempfile::tempdir().expect("temporary directory");
        let tasks = ServiceUpdateTasks::new(root.path(), "2.0.0");
        let task = tasks.begin().expect("task");
        tasks
            .set_release(&task.id, "2.0.10", "bundle.zip", 100)
            .expect("release");
        fs::write(tasks.result_path(), "failed:9\n").expect("result");

        let failed = tasks.snapshot().expect("failed task");
        assert_eq!(failed.state, "failed");
        assert_eq!(
            failed.error.as_deref(),
            Some("Sempre installer exited with code 9")
        );
    }
}
