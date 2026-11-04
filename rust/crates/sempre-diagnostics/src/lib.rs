//! Active checks that reach out over the network. Passive host inventory and routing
//! live in `sempre-network`, so the gateway path does not depend on an HTTP client.
mod diagnostics;
mod dns_probe;
mod probe;
mod public_ip;
mod route_probe;
mod trace;

use std::io;

use sempre_network::NetworkError;
use thiserror::Error;

pub use diagnostics::{
    DiagnosticFinding, DiagnosticLayer, DiagnosticProgress, DiagnosticStatus,
    NetworkDiagnosticReport, run_network_diagnostics, run_network_diagnostics_with_progress,
};
pub use dns_probe::DnsAnswer;
pub use probe::{NetworkTestReport, NetworkTestResult, run_network_test};
pub use public_ip::{
    DOMESTIC_IP_PROBE, FOREIGN_IP_PROBE, IpMetadata, PublicIpProbe, lookup_ip_metadata,
};
pub use trace::{TraceTransportEvent, parse_trace_target, trace_transport};

#[derive(Debug, Error)]
pub enum DiagnosticError {
    #[error(transparent)]
    Network(#[from] NetworkError),
    #[error("inspect network state: {0}")]
    Io(#[from] io::Error),
    #[error("build network diagnostic client: {0}")]
    Client(#[from] reqwest::Error),
    #[error("invalid diagnostic target: {0}")]
    InvalidTarget(String),
}
