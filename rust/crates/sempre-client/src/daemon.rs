use std::{fs, path::PathBuf, sync::Arc};

use sempre_control::{DaemonEndpoint, PublicEndpoint, WebConfigStore, local_url, validate_listen};
use sempre_manager::Manager;
use sempre_state::{Layout, Mode, Store};
use sempre_subscription::SubscriptionStore;
use tokio::net::TcpListener;
use tracing::info;

use crate::{ClientError, VERSION, api};

pub(crate) async fn run(mode: Mode, listen_override: Option<&str>) -> Result<(), ClientError> {
    let layout = Layout::for_mode(mode)?;
    let store = Store::new(layout.clone());
    store.initialize()?;
    let _instance = store.acquire_instance()?;
    let web = WebConfigStore::new(&layout.web_config);
    let config = web.initialize()?;
    let listen = listen_override.unwrap_or(&config.listen);
    validate_listen(listen)?;
    let listener = TcpListener::bind(listen)
        .await
        .map_err(|source| ClientError::Bind {
            address: listen.into(),
            source,
        })?;
    let address = listener.local_addr().map_err(ClientError::LocalAddress)?;
    let bind = address.to_string();
    let local_url = local_url(&bind)?;
    let daemon_endpoint = DaemonEndpoint::new(&local_url)?;
    let public_endpoint = PublicEndpoint::new(VERSION, &bind, &local_url)?;
    let _discovery = DiscoveryGuard::new(layout.daemon_control.clone(), layout.endpoint.clone());
    daemon_endpoint.write(&layout.daemon_control)?;
    public_endpoint.write(&layout.endpoint)?;

    let manager = Arc::new(Manager::new(store)?);
    let subscriptions = SubscriptionStore::new(layout.clone());
    subscriptions.initialize()?;
    let state = Arc::new(api::AppState::new(
        manager,
        web,
        daemon_endpoint.token,
        bind.clone(),
        local_url.clone(),
        subscriptions,
    ));
    let app = api::router(state);
    info!(%bind, %local_url, mode = ?mode, "Sempre Rust client daemon listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await
    .map_err(ClientError::Serve)
}

struct DiscoveryGuard {
    paths: [PathBuf; 2],
}

impl DiscoveryGuard {
    fn new(daemon: PathBuf, public: PathBuf) -> Self {
        Self {
            paths: [daemon, public],
        }
    }
}

impl Drop for DiscoveryGuard {
    fn drop(&mut self) {
        for path in &self.paths {
            let _ = fs::remove_file(path);
        }
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
