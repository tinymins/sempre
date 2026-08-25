mod api;
mod args;
mod bundle_api;
mod core_management_api;
mod custom_node_api;
mod daemon;
mod elevate;
mod gateway_api;
mod listener;
mod runtime_api;
mod runtime_control_api;
mod runtime_events_api;
mod subscription_api;
mod subscription_debug_api;
mod subscription_profile_debug_api;
mod subscription_tools_api;
mod system_api;
mod tunnel_api;
mod web_ui_api;
#[cfg(windows)]
mod windows_service_host;

use std::{fs, io, path::PathBuf};

use args::{Arguments, BundleCommand, Command, CoreCommand};
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
    #[error(transparent)]
    Bundle(#[from] sempre_bundle::BundleError),
    #[error("deployment cancelled")]
    Cancelled,
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
    #[error("{operation} {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("sempre=info")),
        )
        .init();
    let raw_arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let arguments = Arguments::parse();
    #[cfg(windows)]
    if matches!(arguments.command, Command::ServiceHost) {
        if let Err(error) = windows_service_host::dispatch() {
            eprintln!("ERROR: {error}");
            std::process::exit(1);
        }
        return;
    }
    match elevate::ensure(&arguments, &raw_arguments) {
        Ok(elevate::Outcome::Continue) => {}
        Ok(elevate::Outcome::Exit(code)) => std::process::exit(code),
        Err(error) => {
            eprintln!("ERROR: {error}");
            std::process::exit(1);
        }
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build Sempre runtime");
    if let Err(error) = runtime.block_on(run(arguments)) {
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
        Command::Install { yes } => run_install(yes).await,
        Command::Version => {
            println!("Sempre {VERSION}");
            Ok(())
        }
        #[cfg(windows)]
        Command::ServiceHost => unreachable!("service host is dispatched before the async runtime"),
        Command::Daemon {
            listen,
            development_root,
        } => match development_root {
            Some(root) => daemon::run_development(&root, listen.as_deref()).await,
            None => daemon::run(mode, listen.as_deref()).await,
        },
        Command::Core { command } => run_core(mode, command).await,
        Command::Bundle { command } => run_bundle(mode, command).await,
    }
}

async fn run_install(yes: bool) -> Result<(), ClientError> {
    let executable = current_executable("locate release executable")?;
    let source = Layout::portable_at(&executable);
    sempre_bundle::validate_release(&source.root)?;
    let manager = Manager::new(Store::new(source))?;
    let target = Layout::for_mode(Mode::System)?;
    deploy_bundle(&manager, &target, sempre_bundle::BundleKind::Release, yes).await?;
    println!("Sempre installed, enabled, and started.");
    Ok(())
}

async fn run_bundle(mode: Mode, command: BundleCommand) -> Result<(), ClientError> {
    match command {
        BundleCommand::Export { directory } => {
            let manager = Manager::new(Store::new(Layout::for_mode(mode)?))?;
            let result = manager.export_bundle()?;
            fs::create_dir_all(&directory).map_err(|source| ClientError::Io {
                operation: "create bundle output directory",
                path: directory.clone(),
                source,
            })?;
            let destination = directory.join(&result.download_name);
            if let Err(source) = fs::copy(&result.archive, &destination) {
                let _ = fs::remove_file(&result.archive);
                return Err(ClientError::Io {
                    operation: "copy bundle archive",
                    path: destination,
                    source,
                });
            }
            let _ = fs::remove_file(&result.archive);
            println!("Bundle archive: {}", destination.display());
            Ok(())
        }
        BundleCommand::Restore { yes } => {
            let executable = current_executable("locate snapshot executable")?;
            let source = Layout::portable_at(&executable);
            sempre_bundle::validate_snapshot(&source.root)?;
            let manager = Manager::new(Store::new(source))?;
            let target = Layout::for_mode(Mode::System)?;
            deploy_bundle(&manager, &target, sempre_bundle::BundleKind::Snapshot, yes).await?;
            println!("Sempre snapshot restored, enabled, and started.");
            Ok(())
        }
    }
}

async fn deploy_bundle(
    manager: &Manager,
    target: &Layout,
    kind: sempre_bundle::BundleKind,
    allow_replace: bool,
) -> Result<(), ClientError> {
    let result = match kind {
        sempre_bundle::BundleKind::Release => manager.install_release(target, allow_replace).await,
        sempre_bundle::BundleKind::Snapshot => manager.restore_bundle(target, allow_replace).await,
    };
    let Err(ManagerError::ConfirmationRequired(message)) = result else {
        return result.map_err(ClientError::from);
    };
    if allow_replace || !confirm_replacement(&message)? {
        return Err(ClientError::Cancelled);
    }
    match kind {
        sempre_bundle::BundleKind::Release => manager.install_release(target, true).await?,
        sempre_bundle::BundleKind::Snapshot => manager.restore_bundle(target, true).await?,
    }
    Ok(())
}

fn confirm_replacement(message: &str) -> Result<bool, ClientError> {
    eprint!("{message}. Replace it? [y/N]: ");
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|source| ClientError::Io {
            operation: "read deployment confirmation",
            path: PathBuf::from("stdin"),
            source,
        })?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn current_executable(operation: &'static str) -> Result<PathBuf, ClientError> {
    std::env::current_exe().map_err(|source| ClientError::Io {
        operation,
        path: PathBuf::from("sempre"),
        source,
    })
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
