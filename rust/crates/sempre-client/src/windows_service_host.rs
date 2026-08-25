use std::error::Error;

use tokio::sync::watch;

use crate::daemon;

pub(crate) fn dispatch() -> Result<(), Box<dyn Error>> {
    sempre_service::dispatch_windows_service(run_daemon)
}

fn run_daemon(receiver: watch::Receiver<bool>) -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime
        .block_on(daemon::run_with_shutdown(
            sempre_state::Mode::System,
            None,
            Some(receiver),
        ))
        .map_err(Into::into)
}
