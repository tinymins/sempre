mod log;
mod platform;

use std::{
    io,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::Duration,
};

use sempre_core::CommandSpec;
use thiserror::Error;
use tokio::{
    process::{Child, Command},
    task::JoinHandle,
    time::timeout,
};

const LOG_LIMIT: u64 = 10 << 20;
const LOG_BACKUPS: usize = 3;

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
    #[error("managed core output task failed: {0}")]
    OutputTask(#[source] tokio::task::JoinError),
    #[error("write managed core output: {0}")]
    Output(#[source] io::Error),
}

pub struct ManagedProcess {
    child: Child,
    pid: u32,
    output: Vec<JoinHandle<io::Result<()>>>,
}

impl ManagedProcess {
    pub fn spawn(
        spec: &CommandSpec,
        stdout_path: impl Into<PathBuf>,
        stderr_path: impl Into<PathBuf>,
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
            output.push(tokio::spawn(log::copy_rolling(
                stdout,
                stdout_path.into(),
                LOG_LIMIT,
                LOG_BACKUPS,
            )));
        }
        if let Some(stderr) = child.stderr.take() {
            output.push(tokio::spawn(log::copy_rolling(
                stderr,
                stderr_path.into(),
                LOG_LIMIT,
                LOG_BACKUPS,
            )));
        }
        Ok(Self { child, pid, output })
    }

    pub const fn pid(&self) -> u32 {
        self.pid
    }

    pub async fn wait(&mut self) -> Result<ExitStatus, SupervisorError> {
        let status = self.child.wait().await.map_err(SupervisorError::Wait)?;
        self.finish_output().await?;
        Ok(status)
    }

    pub async fn terminate(&mut self, grace: Duration) -> Result<ExitStatus, SupervisorError> {
        if self
            .child
            .try_wait()
            .map_err(SupervisorError::Wait)?
            .is_none()
            && let Err(error) = platform::terminate_tree(self.pid, false).await
            && self
                .child
                .try_wait()
                .map_err(SupervisorError::Wait)?
                .is_none()
        {
            return Err(SupervisorError::Signal(error));
        }
        let status = if let Ok(result) = timeout(grace, self.child.wait()).await {
            result.map_err(SupervisorError::Wait)?
        } else {
            if platform::terminate_tree(self.pid, true).await.is_err()
                && self
                    .child
                    .try_wait()
                    .map_err(SupervisorError::Wait)?
                    .is_none()
            {
                self.child.start_kill().map_err(SupervisorError::Signal)?;
            }
            self.child.wait().await.map_err(SupervisorError::Wait)?
        };
        self.finish_output().await?;
        Ok(status)
    }

    async fn finish_output(&mut self) -> Result<(), SupervisorError> {
        for task in self.output.drain(..) {
            task.await
                .map_err(SupervisorError::OutputTask)?
                .map_err(SupervisorError::Output)?;
        }
        Ok(())
    }
}

pub fn append_log(path: &Path, message: &str) -> Result<(), SupervisorError> {
    log::append_rolling(path, message.as_bytes(), LOG_LIMIT, LOG_BACKUPS)
        .map_err(SupervisorError::Output)
}

#[cfg(test)]
mod tests;
