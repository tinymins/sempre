use std::{future::Future, path::Path, pin::Pin, time::Duration};

use sempre_core::Adapter;
use tokio::{process::Command, time::timeout};

use crate::ManagerError;

pub trait VersionRunner: Send + Sync {
    fn version<'a>(
        &'a self,
        adapter: &'a dyn Adapter,
        binary: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<String, ManagerError>> + Send + 'a>>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessRunner;

impl VersionRunner for ProcessRunner {
    fn version<'a>(
        &'a self,
        adapter: &'a dyn Adapter,
        binary: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<String, ManagerError>> + Send + 'a>> {
        Box::pin(async move {
            let binary = binary
                .to_str()
                .ok_or_else(|| ManagerError::NonUnicodePath(binary.to_path_buf()))?;
            let spec = adapter.version_command(binary);
            let mut command = Command::new(&spec.program);
            command
                .args(&spec.arguments)
                .envs(&spec.environment)
                .kill_on_drop(true);
            if let Some(directory) = &spec.working_directory {
                command.current_dir(directory);
            }
            let output = timeout(Duration::from_secs(30), command.output())
                .await
                .map_err(|_| ManagerError::VersionTimeout(adapter.id().into()))?
                .map_err(|error| {
                    ManagerError::io(format!("run {} version", adapter.id()), error)
                })?;
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if !output.status.success() {
                return Err(ManagerError::VersionCommand {
                    core: adapter.id().into(),
                    status: output
                        .status
                        .code()
                        .map_or_else(|| "signal".into(), |code| code.to_string()),
                    output: combined.trim().into(),
                });
            }
            Ok(adapter.parse_version(&combined)?)
        })
    }
}
