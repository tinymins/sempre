mod api;
mod args;
mod custom_node_api;
mod daemon;
mod runtime_api;
mod subscription_api;
mod system_api;
mod web_ui_api;

use std::io;

use args::{Arguments, Command, CoreCommand};
use clap::Parser;
use sempre_control::ControlError;
use sempre_manager::{Manager, ManagerError};
use sempre_state::{Layout, LayoutError, Mode, StateError, Store};
use sempre_subscription::SubscriptionError;
use thiserror::Error;
use tracing_subscriber::EnvFilter;

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Error)]
pub(crate) enum ClientError {
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    Manager(#[from] ManagerError),
    #[error(transparent)]
    Control(#[from] ControlError),
    #[error(transparent)]
    Subscription(#[from] SubscriptionError),
    #[error("bind local API at {address}: {source}")]
    Bind { address: String, source: io::Error },
    #[error("read local API address: {0}")]
    LocalAddress(#[source] io::Error),
    #[error("serve local API: {0}")]
    Serve(#[source] io::Error),
    #[error("{component} task failed: {source}")]
    Task {
        component: &'static str,
        #[source]
        source: tokio::task::JoinError,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("sempre=info")),
        )
        .init();
    if let Err(error) = run(Arguments::parse()).await {
        eprintln!("ERROR: {error}");
        std::process::exit(1);
    }
}

async fn run(arguments: Arguments) -> Result<(), ClientError> {
    let mode = if arguments.portable {
        Mode::Portable
    } else {
        Mode::System
    };
    match arguments.command {
        Command::Version => {
            println!("Sempre {VERSION}");
            Ok(())
        }
        Command::Daemon { listen } => daemon::run(mode, listen.as_deref()).await,
        Command::Core { command } => run_core(mode, command).await,
    }
}

async fn run_core(mode: Mode, command: CoreCommand) -> Result<(), ClientError> {
    let manager = Manager::new(Store::new(Layout::for_mode(mode)?))?;
    match command {
        CoreCommand::Install { reference } => {
            let result = manager.install_core(&reference).await?;
            let action = if result.changed {
                "installed"
            } else {
                "is already installed"
            };
            println!("{}@{} {action}.", result.core, result.version);
        }
        CoreCommand::List => {
            let inventory = manager.core_inventory()?;
            if inventory.installed.is_empty() {
                println!("No proxy cores are installed.");
            } else {
                for item in inventory.installed {
                    let channels = if item.channels.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", item.channels.join(", "))
                    };
                    println!("{}{}", item.reference, channels);
                }
            }
        }
        CoreCommand::Use { reference } => {
            let change = manager.select_core(&reference).await?;
            println!("{}", change.message);
            if !change.current_detail.is_empty() {
                println!("{}", change.current_detail);
            }
        }
        CoreCommand::Remove { reference } => {
            let change = manager.remove_core(&reference)?;
            println!("{}", change.message);
        }
    }
    Ok(())
}
