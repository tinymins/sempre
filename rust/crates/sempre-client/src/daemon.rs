use std::{fs, path::PathBuf, sync::Arc};

use sempre_control::{DaemonEndpoint, PublicEndpoint, WebConfigStore, local_url, validate_listen};
use sempre_manager::Manager;
use sempre_state::{Layout, Mode, Store};
use tokio::{
    net::TcpListener,
    sync::watch,
    task::{JoinError, JoinHandle},
};
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
    let state = Arc::new(api::AppState::new(
        Arc::clone(&manager),
        web,
        daemon_endpoint.token,
        bind.clone(),
        local_url.clone(),
    ));
    let app = api::router(state);
    info!(%bind, %local_url, mode = ?mode, "Sempre Rust client daemon listening");
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let signal_sender = shutdown_sender.clone();
    let signal = tokio::spawn(async move {
        shutdown_signal().await;
        let _ = signal_sender.send(true);
    });
    let server_shutdown = shutdown_sender.subscribe();
    let mut server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown_requested(server_shutdown))
        .await
        .map_err(ClientError::Serve)
    });
    let mut supervisor = tokio::spawn(async move {
        manager.run_supervisor(shutdown_receiver).await?;
        Ok(())
    });

    let result = tokio::select! {
        result = &mut server => {
            let _ = shutdown_sender.send(true);
            settle_tasks(result, "API server", supervisor, "core supervisor").await
        },
        result = &mut supervisor => {
            let _ = shutdown_sender.send(true);
            settle_tasks(result, "core supervisor", server, "API server").await
        },
    };
    signal.abort();
    result
}

async fn settle_tasks(
    first: Result<Result<(), ClientError>, JoinError>,
    first_name: &'static str,
    second: JoinHandle<Result<(), ClientError>>,
    second_name: &'static str,
) -> Result<(), ClientError> {
    let first = task_result(first, first_name);
    let second = task_result(second.await, second_name);
    first.and(second)
}

fn task_result(
    result: Result<Result<(), ClientError>, JoinError>,
    component: &'static str,
) -> Result<(), ClientError> {
    result.map_err(|source| ClientError::Task { component, source })?
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

async fn shutdown_requested(mut shutdown: watch::Receiver<bool>) {
    if !*shutdown.borrow() {
        let _ = shutdown.changed().await;
    }
}

async fn shutdown_signal() {
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
