use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Instant,
};

use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
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
}

pub(crate) struct ServiceUpdateTasks {
    installer_log_path: PathBuf,
    current_version: String,
    state: Mutex<TaskState>,
}

impl ServiceUpdateTasks {
    pub(crate) fn new(home: &Path, current_version: &str) -> Self {
        let installer_log_path = home.join("service-update");
        Self {
            installer_log_path,
            current_version: normalized_version(current_version).into(),
            state: Mutex::new(TaskState {
                task: None,
                download_started: None,
            }),
        }
    }

    pub(crate) fn begin(&self) -> Result<ServiceUpdateTask, String> {
        let mut state = self.state.lock().unwrap();
        if state
            .task
            .as_ref()
            .is_some_and(|task| task.state == "running")
        {
            return Err("a Sempre update is already in progress".into());
        }
        for extension in ["stdout.log", "stderr.log"] {
            remove_if_exists(&self.installer_log_path.with_extension(extension))?;
        }
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
        state.task = Some(task.clone());
        state.download_started = None;
        Ok(task)
    }

    pub(crate) fn snapshot(&self) -> Option<ServiceUpdateTask> {
        self.state.lock().unwrap().task.clone()
    }

    pub(crate) fn set_release(
        &self,
        id: &str,
        version: &str,
        artifact: &str,
        total: u64,
    ) -> Result<(), String> {
        self.update(id, |state, now| {
            let task = state.task.as_mut().expect("matching task");
            task.target_version = normalized_version(version).into();
            task.artifact = Some(artifact.into());
            task.total_bytes = total;
            task.stage = "resolving".into();
            task.updated_at = now;
        })
    }

    pub(crate) fn set_stage(&self, id: &str, stage: &str) -> Result<(), String> {
        self.update(id, |state, now| {
            let task = state.task.as_mut().expect("matching task");
            task.stage = stage.into();
            task.updated_at = now;
        })
    }

    pub(crate) fn download_progress(&self, id: &str, downloaded: u64, total: u64) {
        let _ = self.update(id, |state, now| {
            let started = state.download_started.get_or_insert_with(Instant::now);
            let millis = u64::try_from(started.elapsed().as_millis())
                .unwrap_or(u64::MAX)
                .max(1);
            let speed = downloaded.saturating_mul(1000) / millis;
            let task = state.task.as_mut().expect("matching task");
            task.stage = "downloading".into();
            task.downloaded_bytes = downloaded;
            task.total_bytes = total;
            task.bytes_per_second = speed;
            task.eta_seconds =
                (speed > 0 && total > downloaded).then(|| (total - downloaded).div_ceil(speed));
            task.updated_at = now;
        });
    }

    pub(crate) fn restart_download(&self, id: &str) -> Result<(), String> {
        self.update(id, |state, now| {
            state.download_started = None;
            let task = state.task.as_mut().expect("matching task");
            task.stage = "downloading".into();
            task.downloaded_bytes = 0;
            task.bytes_per_second = 0;
            task.eta_seconds = None;
            task.updated_at = now;
        })
    }

    pub(crate) fn fail(&self, id: &str, error: &str) {
        let _ = self.update(id, |state, now| {
            let task = state.task.as_mut().expect("matching task");
            task.state = "failed".into();
            task.error = Some(error.into());
            task.updated_at = now;
            task.finished_at = Some(now);
            task.eta_seconds = None;
        });
    }

    pub(crate) fn installer_log_path(&self) -> &Path {
        &self.installer_log_path
    }

    fn update(
        &self,
        id: &str,
        change: impl FnOnce(&mut TaskState, DateTime<Utc>),
    ) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        if state
            .task
            .as_ref()
            .is_none_or(|task| task.id != id || task.state != "running")
        {
            return Err("Sempre update task is no longer available".into());
        }
        change(&mut state, Utc::now());
        Ok(())
    }
}

fn remove_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("clear previous Sempre installer log: {error}")),
    }
}

fn normalized_version(value: &str) -> &str {
    value.strip_prefix('v').unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_lives_only_in_the_current_process() {
        let root = tempfile::tempdir().unwrap();
        let tasks = ServiceUpdateTasks::new(root.path(), "2.0.0");
        let task = tasks.begin().unwrap();
        tasks
            .set_release(&task.id, "2.0.12", "bundle.zip", 100)
            .unwrap();
        tasks.download_progress(&task.id, 50, 100);
        let progress = tasks.snapshot().unwrap();
        assert_eq!(progress.stage, "downloading");
        assert_eq!((progress.downloaded_bytes, progress.total_bytes), (50, 100));
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        assert!(
            ServiceUpdateTasks::new(root.path(), "2.0.12")
                .snapshot()
                .is_none()
        );
        assert!(tasks.begin().is_err());
    }

    #[test]
    fn legacy_task_files_are_never_restored() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("service-update-task.json"),
            r#"{"state":"succeeded"}"#,
        )
        .unwrap();
        let tasks = ServiceUpdateTasks::new(root.path(), "2.0.0");
        assert!(tasks.snapshot().is_none());
        assert!(tasks.begin().is_ok());
    }

    #[test]
    fn failed_tasks_keep_their_stage_and_allow_another_attempt() {
        let root = tempfile::tempdir().unwrap();
        let tasks = ServiceUpdateTasks::new(root.path(), "2.0.0");
        let task = tasks.begin().unwrap();
        tasks.set_stage(&task.id, "downloading").unwrap();
        tasks.fail(&task.id, "connection failed");
        let failed = tasks.snapshot().unwrap();
        assert_eq!(failed.state, "failed");
        assert_eq!(failed.stage, "downloading");
        assert_eq!(failed.error.as_deref(), Some("connection failed"));
        let next = tasks.begin().unwrap();
        tasks.download_progress(&task.id, 50, 100);
        assert_eq!(tasks.snapshot().unwrap().id, next.id);
        assert_eq!(tasks.snapshot().unwrap().stage, "checking");
    }

    #[test]
    fn proxy_retry_resets_visible_download_measurements() {
        let root = tempfile::tempdir().unwrap();
        let tasks = ServiceUpdateTasks::new(root.path(), "2.0.0");
        let task = tasks.begin().unwrap();
        tasks.download_progress(&task.id, 50, 100);
        tasks.restart_download(&task.id).unwrap();
        let retried = tasks.snapshot().unwrap();
        assert_eq!(retried.downloaded_bytes, 0);
        assert_eq!(retried.bytes_per_second, 0);
        assert_eq!(retried.eta_seconds, None);
    }
}
