mod controller;
mod error;
mod model;
mod package;
mod store;

#[cfg(test)]
mod controller_tests;

pub use controller::Controller;
pub use error::TunnelError;
pub use model::{BinaryStatus, Config, Forward, ForwardEndpoint, Instance, InstanceStatus, Status};
pub use package::{VERSION, install_for};

pub fn initialize(layout: &sempre_state::Layout) -> Result<Config, TunnelError> {
    store::Store::new(layout.tunnels.clone()).initialize()
}
