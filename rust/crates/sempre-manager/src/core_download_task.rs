use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::task::AbortHandle;

use crate::{
    Manager, ManagerError, ProcessRunner, ValidationRunner, VersionRunner,
    install::CoreInstallProgress,
};

#[derive(Clone, Debug, Serialize)]
pub struct CoreDownloadTask {
    pub id: String,
    pub operation: String,
    pub reference: String,
    pub state: String,
    pub stage: String,
    pub artifact: Option<String>,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

struct ActiveTask {
    task: CoreDownloadTask,
    abort: Option<AbortHandle>,
}

#[derive(Default)]
pub(crate) struct CoreDownloadTasks(Mutex<Option<ActiveTask>>);

impl CoreDownloadTasks {
    fn begin(&self, operation: &str, reference: &str) -> Result<CoreDownloadTask, ManagerError> {
        let mut current = self.0.lock().unwrap();
        if current
            .as_ref()
            .is_some_and(|active| active.task.state == "running")
        {
            return Err(ManagerError::InvalidOperation(
                "a core download is already in progress".into(),
            ));
        }
        let started_at = Utc::now();
        let task = CoreDownloadTask {
            id: started_at.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            operation: operation.into(),
            reference: reference.into(),
            state: "running".into(),
            stage: "queued".into(),
            artifact: None,
            downloaded_bytes: 0,
            total_bytes: 0,
            started_at,
            finished_at: None,
            error: None,
        };
        *current = Some(ActiveTask {
            task: task.clone(),
            abort: None,
        });
        Ok(task)
    }

    fn attach(&self, id: &str, abort: AbortHandle) {
        if let Some(active) = self.0.lock().unwrap().as_mut()
            && active.task.id == id
            && active.task.state == "running"
        {
            active.abort = Some(abort);
        }
    }

    fn progress(&self, id: &str, progress: CoreInstallProgress) {
        let mut current = self.0.lock().unwrap();
        let Some(active) = current.as_mut().filter(|active| active.task.id == id) else {
            return;
        };
        match progress {
            CoreInstallProgress::Resolving { reference } => {
                active.task.reference = reference;
                active.task.stage = "resolving".into();
            }
            CoreInstallProgress::Downloading {
                artifact,
                downloaded,
                total,
            } => {
                active.task.stage = "downloading".into();
                active.task.artifact = Some(artifact);
                active.task.downloaded_bytes = downloaded;
                active.task.total_bytes = total;
            }
            CoreInstallProgress::Installing => active.task.stage = "installing".into(),
        }
    }

    fn finish(&self, id: &str, result: Result<(), ManagerError>) {
        let mut current = self.0.lock().unwrap();
        let Some(active) = current.as_mut().filter(|active| active.task.id == id) else {
            return;
        };
        active.task.finished_at = Some(Utc::now());
        active.abort = None;
        match result {
            Ok(()) => {
                active.task.state = "succeeded".into();
                active.task.stage = "completed".into();
            }
            Err(error) => {
                active.task.state = "failed".into();
                active.task.stage = "failed".into();
                active.task.error = Some(error.to_string());
            }
        }
    }

    fn snapshot(&self) -> Option<CoreDownloadTask> {
        self.0
            .lock()
            .unwrap()
            .as_ref()
            .map(|active| active.task.clone())
    }

    fn cancel_and_clear(&self, id: &str) -> bool {
        let mut current = self.0.lock().unwrap();
        let Some(active) = current.as_ref().filter(|active| active.task.id == id) else {
            return false;
        };
        if let Some(abort) = &active.abort {
            abort.abort();
        }
        *current = None;
        true
    }
}

impl Manager<ProcessRunner> {
    pub fn start_core_download_task(
        self: &Arc<Self>,
        operation: &str,
        reference: &str,
    ) -> Result<CoreDownloadTask, ManagerError> {
        if !matches!(operation, "install" | "update") {
            return Err(ManagerError::InvalidOperation(format!(
                "unsupported core download operation {operation:?}"
            )));
        }
        let task = self.core_download_tasks.begin(operation, reference)?;
        let id = task.id.clone();
        let manager = self.clone();
        let operation = operation.to_owned();
        let task_reference = reference.to_string();
        let task_id = id.clone();
        let handle = tokio::spawn(async move {
            let progress = |event| manager.core_download_tasks.progress(&task_id, event);
            let result = match operation.as_str() {
                "install" => manager
                    .install_core_observed(&task_reference, &progress)
                    .await
                    .map(|_| ()),
                "update" => manager
                    .update_cores_observed(&task_reference, &progress)
                    .await
                    .map(|_| ()),
                _ => unreachable!(),
            };
            manager.core_download_tasks.finish(&task_id, result);
        });
        self.core_download_tasks.attach(&id, handle.abort_handle());
        Ok(task)
    }
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub fn core_download_task(&self) -> Option<CoreDownloadTask> {
        self.core_download_tasks.snapshot()
    }

    pub fn cancel_core_download_task(&self, id: &str) -> bool {
        self.core_download_tasks.cancel_and_clear(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_aborts_and_clears_the_visible_task() {
        let tasks = CoreDownloadTasks::default();
        let task = tasks.begin("install", "sing-box@stable").expect("task");
        let handle = tokio::spawn(std::future::pending::<()>());
        tasks.attach(&task.id, handle.abort_handle());

        assert!(tasks.cancel_and_clear(&task.id));
        assert!(tasks.snapshot().is_none());
        assert!(handle.await.unwrap_err().is_cancelled());
    }

    #[test]
    fn progress_updates_only_the_matching_task() {
        let tasks = CoreDownloadTasks::default();
        let task = tasks.begin("install", "sing-box@stable").expect("task");
        tasks.progress(
            "other",
            CoreInstallProgress::Downloading {
                artifact: "ignored".into(),
                downloaded: 10,
                total: 20,
            },
        );
        tasks.progress(
            &task.id,
            CoreInstallProgress::Downloading {
                artifact: "core.tar.gz".into(),
                downloaded: 10,
                total: 20,
            },
        );

        let task = tasks.snapshot().expect("snapshot");
        assert_eq!(task.stage, "downloading");
        assert_eq!(task.artifact.as_deref(), Some("core.tar.gz"));
        assert_eq!((task.downloaded_bytes, task.total_bytes), (10, 20));
    }
}
