use std::{future::Future, path::Path, pin::Pin, time::Duration};

use sempre_core::{Adapter, CommandSpec};
use tokio::{process::Command, time::timeout};

use crate::ManagerError;

pub trait VersionRunner: Send + Sync {
    fn version<'a>(
        &'a self,
        adapter: &'a dyn Adapter,
        binary: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<String, ManagerError>> + Send + 'a>>;
}

pub trait ValidationRunner: Send + Sync {
    fn validate<'a>(
        &'a self,
        adapter: &'a dyn Adapter,
        binary: &'a Path,
        config: &'a Path,
        data_directory: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), ManagerError>> + Send + 'a>>;

    fn validate_output<'a>(
        &'a self,
        adapter: &'a dyn Adapter,
        binary: &'a Path,
        config: &'a Path,
        data_directory: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<String, ManagerError>> + Send + 'a>> {
        Box::pin(async move {
            self.validate(adapter, binary, config, data_directory)
                .await?;
            Ok(String::new())
        })
    }
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

impl ValidationRunner for ProcessRunner {
    fn validate<'a>(
        &'a self,
        adapter: &'a dyn Adapter,
        binary: &'a Path,
        config: &'a Path,
        data_directory: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), ManagerError>> + Send + 'a>> {
        Box::pin(async move {
            self.validate_output(adapter, binary, config, data_directory)
                .await
                .map(|_| ())
        })
    }

    fn validate_output<'a>(
        &'a self,
        adapter: &'a dyn Adapter,
        binary: &'a Path,
        config: &'a Path,
        data_directory: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<String, ManagerError>> + Send + 'a>> {
        Box::pin(async move {
            let binary = path_text(binary)?;
            let config = path_text(config)?;
            let data_directory = path_text(data_directory)?;
            let spec = adapter.validation_command(binary, config, data_directory);
            let output = run(&spec, adapter.id(), "configuration validation").await?;
            if output.status.success() {
                Ok(combined_output(&output))
            } else {
                Err(ManagerError::ValidationCommand {
                    core: adapter.id().into(),
                    status: output
                        .status
                        .code()
                        .map_or_else(|| "signal".into(), |code| code.to_string()),
                    output: combined_output(&output),
                })
            }
        })
    }
}

async fn run(
    spec: &CommandSpec,
    core: &str,
    operation: &'static str,
) -> Result<std::process::Output, ManagerError> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.arguments)
        .envs(&spec.environment)
        .kill_on_drop(true);
    if let Some(directory) = &spec.working_directory {
        command.current_dir(directory);
    }
    timeout(Duration::from_secs(30), command.output())
        .await
        .map_err(|_| ManagerError::CommandTimeout {
            core: core.into(),
            operation,
        })?
        .map_err(|error| ManagerError::io(format!("run {core} {operation}"), error))
}

fn combined_output(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .trim()
    .into()
}

fn path_text(path: &Path) -> Result<&str, ManagerError> {
    path.to_str()
        .ok_or_else(|| ManagerError::NonUnicodePath(path.to_path_buf()))
}
