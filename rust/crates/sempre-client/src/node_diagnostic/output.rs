use serde::Serialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Child,
    sync::mpsc,
    task::JoinHandle,
};

#[derive(Serialize)]
pub(crate) struct DiagnosticLog {
    stream: &'static str,
    line: String,
}

pub(super) struct DiagnosticOutput {
    readers: Vec<JoinHandle<()>>,
}

impl DiagnosticOutput {
    pub(super) fn capture(child: &mut Child, sender: mpsc::Sender<DiagnosticLog>) -> Self {
        let mut readers = Vec::new();
        if let Some(stdout) = child.stdout.take() {
            readers.push(read_output(stdout, "stdout", sender.clone()));
        }
        if let Some(stderr) = child.stderr.take() {
            readers.push(read_output(stderr, "stderr", sender));
        }
        Self { readers }
    }

    pub(super) async fn finish(&mut self) {
        for reader in self.readers.drain(..) {
            let _ = reader.await;
        }
    }
}

impl Drop for DiagnosticOutput {
    fn drop(&mut self) {
        for reader in &self.readers {
            reader.abort();
        }
    }
}

fn read_output(
    output: impl AsyncRead + Unpin + Send + 'static,
    stream: &'static str,
    sender: mpsc::Sender<DiagnosticLog>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut lines = BufReader::new(output).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if sender.send(DiagnosticLog { stream, line }).await.is_err() {
                break;
            }
        }
    })
}
