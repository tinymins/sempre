use sempre_state::{Layout, Mode};
use sempre_transparent::Controller;

use crate::{ClientError, args::CaptureCommand};

pub(crate) async fn run(mode: Mode, command: CaptureCommand) -> Result<(), ClientError> {
    let layout = Layout::for_mode(mode)?;
    let controller = Controller::new(&layout);
    let Some(capture) = controller.capture() else {
        return Err(ClientError::Runtime(
            "gateway capture is only available on OpenWrt".into(),
        ));
    };
    match command {
        CaptureCommand::Status => println!("{:#}", capture.status()),
        CaptureCommand::Reapply => {
            capture
                .reapply()
                .await
                .map_err(|error| ClientError::Runtime(error.to_string()))?;
        }
    }
    Ok(())
}
