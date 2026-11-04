mod default_interface;
mod interface_addresses;
mod inventory;
mod route_table;
mod routes;

pub use default_interface::{DefaultInterface, default_interface, normalize_mac};
pub use interface_addresses::{InterfaceAddress, interface_addresses};
pub use inventory::{Interface, Inventory, inventory};
pub use route_table::{SystemRoute, route_table};
pub use routes::{route_prefixes, route_prefixes_excluding};

use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("inspect network state: {0}")]
    Io(#[from] io::Error),
}

#[cfg(target_os = "windows")]
pub mod windows;
