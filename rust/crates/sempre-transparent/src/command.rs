use std::{future::Future, pin::Pin, process::Stdio, time::Duration};

use tokio::{io::AsyncWriteExt as _, process::Command, time::timeout};

use crate::TransparentError;

pub(crate) struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub(crate) trait Runner: Send + Sync {
    fn run<'a>(
        &'a self,
        program: &'a str,
        arguments: &'a [&'a str],
        input: Option<&'a [u8]>,
    ) -> Pin<Box<dyn Future<Output = Result<Output, TransparentError>> + Send + 'a>>;
}

pub(crate) struct SystemRunner;

impl Runner for SystemRunner {
    fn run<'a>(
        &'a self,
        program: &'a str,
        arguments: &'a [&'a str],
        input: Option<&'a [u8]>,
    ) -> Pin<Box<dyn Future<Output = Result<Output, TransparentError>> + Send + 'a>> {
        Box::pin(async move {
            let mut command = Command::new(program);
            command
                .args(arguments)
                .kill_on_drop(true)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            if input.is_some() {
                command.stdin(Stdio::piped());
            }
            let mut child = command
                .spawn()
                .map_err(|source| TransparentError::CommandStart {
                    program: program.into(),
                    source,
                })?;
            if let Some(input) = input {
                child
                    .stdin
                    .take()
                    .expect("command stdin is piped")
                    .write_all(input)
                    .await
                    .map_err(|source| TransparentError::Io {
                        context: format!("write {program} input"),
                        source,
                    })?;
            }
            let output = timeout(Duration::from_secs(30), child.wait_with_output())
                .await
                .map_err(|_| TransparentError::CommandTimeout {
                    program: program.into(),
                })?
                .map_err(|source| TransparentError::Io {
                    context: format!("wait for {program}"),
                    source,
                })?;
            Ok(Output {
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        })
    }
}

pub(crate) fn require_success(program: &str, output: Output) -> Result<Output, TransparentError> {
    if output.success {
        Ok(output)
    } else {
        let detail = format!("{}{}", output.stdout, output.stderr)
            .trim()
            .to_owned();
        Err(TransparentError::CommandFailed {
            program: program.into(),
            detail,
        })
    }
}
