use std::{error::Error, fs::OpenOptions, io::Write as _};

use tokio::sync::watch;

use crate::daemon;

pub(crate) fn dispatch() -> Result<(), Box<dyn Error>> {
    sempre_service::dispatch_windows_service(run_daemon)
}

fn run_daemon(receiver: watch::Receiver<bool>) -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let layout = sempre_state::Layout::for_mode(sempre_state::Mode::System)?;
    let result = runtime.block_on(daemon::run_with_layout(
        layout.clone(),
        None,
        Some(receiver),
    ));
    if let Err(error) = &result
        && let Ok(mut log) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&layout.manager_log)
    {
        let _ = writeln!(log, "daemon failed: {error}");
    }
    result.map_err(Into::into)
}
