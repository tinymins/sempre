mod inventory;
mod probe;

pub use inventory::{Interface, Inventory, inventory};
pub use probe::{IpMetadata, NetworkTestReport, NetworkTestResult, run_network_test};

use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("inspect network state: {0}")]
    Io(#[from] io::Error),
    #[error("build network diagnostic client: {0}")]
    Client(#[from] reqwest::Error),
}
