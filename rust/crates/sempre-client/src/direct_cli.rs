use sempre_manager::Manager;
use sempre_state::{Layout, Mode, Store};

use crate::ClientError;

pub async fn run(mode: Mode, reference: Option<&str>) -> Result<(), ClientError> {
    let manager = Manager::new(Store::new(Layout::for_mode(mode)?))?;
    manager
        .run_direct(reference, |label| {
            println!("Starting {label}. Press Ctrl+C to stop.");
        })
        .await?;
    Ok(())
}
