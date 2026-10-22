mod log;
mod platform;

pub use platform::terminate_tree;

use std::{
    io,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    sync::Arc,
    time::Duration,
};

use sempre_core::CommandSpec;
use thiserror::Error;
use tokio::{
    process::{Child, Command},
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{Instant, timeout, timeout_at},
};

const LOG_LIMIT: u64 = 10 << 20;
const LOG_BACKUPS: usize = 3;
const FORCE_TIMEOUT: Duration = Duration::from_secs(5);
const OUTPUT_TIMEOUT: Duration = Duration::from_secs(2);

pub type OutputObserver = Arc<dyn Fn(&str, &str) + Send + Sync>;

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("start managed core: {0}")]
    Start(#[source] io::Error),
    #[error("managed core has no process ID")]
    MissingPid,
    #[error("wait for managed core: {0}")]
    Wait(#[source] io::Error),
    #[error("signal managed core: {0}")]
    Signal(#[source] io::Error),
    #[error("write managed core output: {0}")]
    Output(#[source] io::Error),
    #[error("managed core did not exit within {0:?} after force termination")]
    TerminationTimeout(Duration),
}

pub struct ManagedProcess {
    child: Child,
    pid: u32,
    output: Vec<OutputTask>,
    observer: Option<OutputObserver>,
}

struct OutputTask {
    handle: JoinHandle<io::Result<()>>,
    synchronizer: mpsc::UnboundedSender<oneshot::Sender<()>>,
}

impl Drop for OutputTask {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

impl ManagedProcess {
    pub fn spawn(
        spec: &CommandSpec,
        stdout_path: impl Into<PathBuf>,
        stderr_path: impl Into<PathBuf>,
    ) -> Result<Self, SupervisorError> {
        Self::spawn_observed(spec, stdout_path, stderr_path, None)
    }

    pub fn spawn_observed(
        spec: &CommandSpec,
        stdout_path: impl Into<PathBuf>,
        stderr_path: impl Into<PathBuf>,
        observer: Option<OutputObserver>,
    ) -> Result<Self, SupervisorError> {
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.arguments)
            .envs(&spec.environment)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(directory) = &spec.working_directory {
            command.current_dir(directory);
        }
        platform::configure(&mut command);
        let mut child = command.spawn().map_err(SupervisorError::Start)?;
        let pid = child.id().ok_or(SupervisorError::MissingPid)?;
        let mut output = Vec::with_capacity(2);
        if let Some(stdout) = child.stdout.take() {
            output.push(output_task(
                stdout,
                stdout_path.into(),
                observer.clone(),
                "stdout",
            ));
        }
        if let Some(stderr) = child.stderr.take() {
            output.push(output_task(
                stderr,
                stderr_path.into(),
                observer.clone(),
                "stderr",
            ));
        }
        Ok(Self {
            child,
            pid,
            output,
            observer,
        })
    }

    pub fn spawn_foreground(spec: &CommandSpec) -> Result<Self, SupervisorError> {
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.arguments)
            .envs(&spec.environment)
            .kill_on_drop(true)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        if let Some(directory) = &spec.working_directory {
            command.current_dir(directory);
        }
        platform::configure_foreground(&mut command);
        let child = command.spawn().map_err(SupervisorError::Start)?;
        let pid = child.id().ok_or(SupervisorError::MissingPid)?;
        Ok(Self {
            child,
            pid,
            output: Vec::new(),
            observer: None,
        })
    }

    pub const fn pid(&self) -> u32 {
        self.pid
    }

    pub async fn synchronize_output(&self) {
        let deadline = Instant::now() + OUTPUT_TIMEOUT;
        for task in &self.output {
            if task.handle.is_finished() {
                continue;
            }
            let (ready, synchronized) = oneshot::channel();
            if task.synchronizer.send(ready).is_ok()
                && timeout_at(deadline, synchronized).await.is_err()
            {
                self.observe("output synchronization timed out after 2s; continuing startup");
                break;
            }
        }
    }

    pub async fn wait(&mut self) -> Result<ExitStatus, SupervisorError> {
        let status = self.child.wait().await.map_err(SupervisorError::Wait)?;
        self.finish_output().await;
        Ok(status)
    }

    pub async fn terminate(&mut self, grace: Duration) -> Result<ExitStatus, SupervisorError> {
        if let Some(status) = self.child.try_wait().map_err(SupervisorError::Wait)? {
            self.finish_output().await;
            return Ok(status);
        }
        let graceful = timeout(grace, async {
            platform::terminate_tree(self.pid, false)
                .await
                .map_err(SupervisorError::Signal)?;
            self.child.wait().await.map_err(SupervisorError::Wait)
        })
        .await;
        match graceful {
            Ok(Ok(status)) => {
                self.finish_output().await;
                return Ok(status);
            }
            Ok(Err(error)) => self.observe(&format!(
                "graceful termination failed: {error}; forcing termination (5s limit)"
            )),
            Err(_) => self.observe(&format!(
                "graceful termination timed out after {grace:?}; forcing termination (5s limit)"
            )),
        }
        let status = timeout(FORCE_TIMEOUT, async {
            self.force_terminate().await?;
            self.child.wait().await.map_err(SupervisorError::Wait)
        })
        .await
        .map_err(|_| {
            self.observe("force termination timed out after 5s; process exit is unconfirmed");
            SupervisorError::TerminationTimeout(FORCE_TIMEOUT)
        })??;
        self.finish_output().await;
        Ok(status)
    }

    async fn force_terminate(&mut self) -> Result<(), SupervisorError> {
        if self
            .child
            .try_wait()
            .map_err(SupervisorError::Wait)?
            .is_some()
        {
            return Ok(());
        }
        if let Err(error) = platform::terminate_tree(self.pid, true).await
            && self
                .child
                .try_wait()
                .map_err(SupervisorError::Wait)?
                .is_none()
        {
            self.observe(&format!(
                "process tree termination failed: {error}; killing managed process directly"
            ));
            self.child.start_kill().map_err(SupervisorError::Signal)?;
        }
        Ok(())
    }

    async fn finish_output(&mut self) {
        let deadline = Instant::now() + OUTPUT_TIMEOUT;
        let mut truncated = false;
        for mut task in self.output.drain(..) {
            // Diagnostic output is ancillary and must not change the process result.
            truncated |= timeout_at(deadline, &mut task.handle).await.is_err();
        }
        if truncated {
            self.observe("output drain timed out after 2s; remaining core output was truncated");
        }
    }

    fn observe(&self, message: &str) {
        if let Some(observer) = &self.observer {
            observer("supervisor", message);
        }
    }
}

fn output_task(
    reader: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    path: PathBuf,
    observer: Option<OutputObserver>,
    stream: &'static str,
) -> OutputTask {
    let (synchronizer, synchronization) = mpsc::unbounded_channel();
    let handle = tokio::spawn(log::copy_rolling(
        reader,
        path,
        LOG_LIMIT,
        LOG_BACKUPS,
        observer,
        stream,
        Some(synchronization),
    ));
    OutputTask {
        handle,
        synchronizer,
    }
}

pub fn append_log(path: &Path, message: &str) -> Result<(), SupervisorError> {
    log::append_rolling(path, message.as_bytes(), LOG_LIMIT, LOG_BACKUPS)
        .map_err(SupervisorError::Output)
}

#[cfg(test)]
mod tests;
