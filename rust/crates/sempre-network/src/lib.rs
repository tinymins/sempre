mod default_interface;
mod dns_probe;
mod inventory;
mod probe;
mod public_ip;

pub use default_interface::{DefaultInterface, default_interface, normalize_mac};
pub use dns_probe::DnsAnswer;
pub use inventory::{Interface, Inventory, inventory};
pub use probe::{NetworkTestReport, NetworkTestResult, run_network_test};
pub use public_ip::{
    DOMESTIC_IP_PROBE, FOREIGN_IP_PROBE, IpMetadata, PublicIpProbe, lookup_ip_metadata,
};

use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("inspect network state: {0}")]
    Io(#[from] io::Error),
    #[error("build network diagnostic client: {0}")]
    Client(#[from] reqwest::Error),
}
