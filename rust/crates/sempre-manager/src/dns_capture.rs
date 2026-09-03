use std::{
    path::Path,
    sync::{Arc, RwLock},
};

use crate::ManagerError;

pub(crate) type CaptureError = Arc<RwLock<Option<String>>>;

#[cfg_attr(
    not(all(target_os = "windows", target_arch = "x86_64")),
    allow(clippy::unused_async)
)]
pub(crate) async fn cleanup(resources: &Path) -> Result<(), ManagerError> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        let executable = resources.join("dns-capture/sempre-dns-capture.exe");
        if executable.is_file() {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                tokio::process::Command::new(executable)
                    .arg("--cleanup")
                    .creation_flags(0x0800_0000)
                    .kill_on_drop(true)
                    .output(),
            )
            .await
            .map_err(|_| ManagerError::RuntimeNotReady("DNS capture cleanup timed out".into()))?
            .map_err(|error| ManagerError::io("clean Windows DNS capture driver", error))?;
            if !result.status.success() {
                return Err(ManagerError::RuntimeNotReady(format!(
                    "DNS capture cleanup failed: {}",
                    String::from_utf8_lossy(&result.stderr)
                )));
            }
        }
    }
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    let _ = resources;
    Ok(())
}

pub(crate) struct Capture {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    stop: tokio::sync::oneshot::Sender<()>,
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    task: tokio::task::JoinHandle<()>,
}

impl Capture {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    pub(crate) async fn start(
        resources: Option<&Path>,
        port: u16,
        error: CaptureError,
    ) -> Result<Option<Self>, ManagerError> {
        use std::{process::Stdio, time::Duration};
        use tokio::{
            io::{AsyncBufReadExt as _, BufReader},
            process::Command,
            sync::oneshot,
            time::timeout,
        };

        let Some(resources) = resources else {
            return Ok(None);
        };
        let executable = resources.join("dns-capture/sempre-dns-capture.exe");
        let mut child = Command::new(&executable)
            .args([format!("127.0.0.1:{port}"), std::process::id().to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .creation_flags(0x0800_0000)
            .kill_on_drop(true)
            .spawn()
            .map_err(|source| ManagerError::io("start Windows DNS capture", source))?;
        let input = child.stdin.take().expect("capture stdin");
        let mut output = BufReader::new(child.stdout.take().expect("capture stdout")).lines();
        match timeout(Duration::from_secs(10), output.next_line()).await {
            Ok(Ok(Some(line))) if line == "READY" => {}
            result => {
                return Err(ManagerError::RuntimeNotReady(format!(
                    "Windows DNS capture did not become ready: {result:?}"
                )));
            }
        }
        *error.write().expect("capture status") = None;
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            // Child::wait closes child.stdin; retain the pipe separately until shutdown.
            let _input = input;
            tokio::select! {
                result = child.wait() => {
                    let message = format!("Windows DNS capture stopped unexpectedly: {result:?}");
                    eprintln!("{message}");
                    *error.write().expect("capture status") = Some(message);
                },
                _ = stopped => { let _ = child.kill().await; },
            }
        });
        Ok(Some(Self { stop, task }))
    }

    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    #[allow(clippy::unused_async)]
    pub(crate) async fn start(
        _: Option<&Path>,
        _: u16,
        _: CaptureError,
    ) -> Result<Option<Self>, ManagerError> {
        Ok(None)
    }

    #[cfg_attr(
        not(all(target_os = "windows", target_arch = "x86_64")),
        allow(clippy::unused_async)
    )]
    pub(crate) async fn stop(self) {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            let _ = self.stop.send(());
            let _ = self.task.await;
        }
    }
}
