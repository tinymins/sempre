use std::{net::SocketAddr, sync::Arc};

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use clap::{Parser, Subcommand};
use sempre_state::{Layout, LayoutError, Mode, RuntimeState, StateError, Store};
use serde::Serialize;
use thiserror::Error;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

const DEFAULT_LISTEN: &str = "127.0.0.1:33211";
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
#[command(name = "sempre", version = VERSION, about = "Manage external proxy cores")]
struct Arguments {
    #[arg(long, conflicts_with = "portable", global = true)]
    system: bool,
    #[arg(long, conflicts_with = "system", global = true)]
    portable: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the local authenticated control daemon.
    Daemon {
        #[arg(long, default_value = DEFAULT_LISTEN)]
        listen: SocketAddr,
    },
    /// Print build version information.
    Version,
}

#[derive(Debug, Error)]
enum ClientError {
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error(transparent)]
    State(#[from] StateError),
    #[error("bind local API at {address}: {source}")]
    Bind {
        address: SocketAddr,
        source: std::io::Error,
    },
    #[error("serve local API: {0}")]
    Serve(#[source] std::io::Error),
}

#[derive(Clone)]
struct AppState {
    store: Store,
    listen: SocketAddr,
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
    api_major: u32,
    listen: String,
    local_url: String,
    runtime: RuntimeState,
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
        Command::Daemon { listen } => daemon(mode, listen).await,
    }
}

async fn daemon(mode: Mode, listen: SocketAddr) -> Result<(), ClientError> {
    let store = Store::new(Layout::for_mode(mode)?);
    store.initialize()?;
    let _instance = store.acquire_instance()?;
    let listener = TcpListener::bind(listen)
        .await
        .map_err(|source| ClientError::Bind {
            address: listen,
            source,
        })?;
    let state = Arc::new(AppState { store, listen });
    let app = Router::new()
        .route("/api/v1/health", get(health))
        .with_state(state);
    info!(%listen, mode = ?mode, "Sempre Rust client daemon listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await
        .map_err(ClientError::Serve)
}

async fn health(State(state): State<Arc<AppState>>) -> Result<Json<Health>, (StatusCode, String)> {
    let document = state
        .store
        .read()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(Json(Health {
        status: "ok",
        version: VERSION,
        api_major: 1,
        listen: state.listen.to_string(),
        local_url: format!("http://{}", local_address(state.listen)),
        runtime: document.runtime.state,
    }))
}

fn local_address(address: SocketAddr) -> SocketAddr {
    if address.ip().is_unspecified() {
        SocketAddr::new(
            if address.is_ipv4() {
                "127.0.0.1"
            } else {
                "::1"
            }
            .parse()
            .expect("static IP address"),
            address.port(),
        )
    } else {
        address
    }
}

async fn shutdown() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { () = interrupt => {}, () = terminate => {} }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_listeners_have_loopback_discovery_addresses() {
        let ipv4: SocketAddr = "0.0.0.0:33211".parse().expect("IPv4 address");
        let ipv6: SocketAddr = "[::]:33211".parse().expect("IPv6 address");
        assert_eq!(local_address(ipv4).to_string(), "127.0.0.1:33211");
        assert_eq!(local_address(ipv6).to_string(), "[::1]:33211");
    }

    #[test]
    fn system_and_portable_modes_are_mutually_exclusive() {
        assert!(
            Arguments::try_parse_from(["sempre", "--system", "--portable", "version"]).is_err()
        );
        let portable = Arguments::try_parse_from(["sempre", "--portable", "version"])
            .expect("portable arguments");
        assert!(portable.portable);
        assert!(!portable.system);
    }
}
