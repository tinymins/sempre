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

use crate::{ClientError, VERSION, api, listener};

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
    let (rebind, rebind_requests) = listener::channel();
    let mut app_state = api::AppState::new(
        Arc::clone(&manager),
        web.clone(),
        daemon_endpoint.token.clone(),
        bind.clone(),
        local_url.clone(),
    );
    app_state.attach_rebind(rebind);
    let endpoint_state = app_state.endpoint.clone();
    let state = Arc::new(app_state);
    let app = api::router(state);
    info!(%bind, %local_url, mode = ?mode, "Sempre Rust client daemon listening");
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let signal_sender = shutdown_sender.clone();
    let signal = tokio::spawn(async move {
        shutdown_signal().await;
        let _ = signal_sender.send(true);
    });
    let server_shutdown = shutdown_sender.subscribe();
    let server_layout = layout.clone();
    let server = tokio::spawn(listener::run(
        listener,
        app,
        endpoint_state,
        web,
        daemon_endpoint,
        server_layout,
        rebind_requests,
        server_shutdown,
    ));
    let supervisor_manager = Arc::clone(&manager);
    let scheduler_manager = Arc::clone(&manager);
    let tunnel_manager = Arc::clone(&manager);
    let scheduler_shutdown = shutdown_receiver.clone();
    let supervisor = tokio::spawn(async move {
        supervisor_manager.run_supervisor(shutdown_receiver).await?;
        Ok(())
    });
    let scheduler = tokio::spawn(async move {
        scheduler_manager
            .run_subscription_scheduler(scheduler_shutdown)
            .await?;
        Ok(())
    });
    let tunnel_shutdown = shutdown_sender.subscribe();
    let tunnels = tokio::spawn(async move {
        tunnel_manager.run_tunnels(tunnel_shutdown).await?;
        Ok(())
    });

    let result = coordinate_tasks(shutdown_sender, server, supervisor, scheduler, tunnels).await;
    signal.abort();
    result
}

async fn coordinate_tasks(
    shutdown_sender: watch::Sender<bool>,
    mut server: JoinHandle<Result<(), ClientError>>,
    mut supervisor: JoinHandle<Result<(), ClientError>>,
    mut scheduler: JoinHandle<Result<(), ClientError>>,
    mut tunnels: JoinHandle<Result<(), ClientError>>,
) -> Result<(), ClientError> {
    tokio::select! {
        result = &mut server => {
            let _ = shutdown_sender.send(true);
            settle_tasks(result, "API server", [
                (supervisor, "core supervisor"),
                (scheduler, "subscription scheduler"),
                (tunnels, "tunnel supervisor"),
            ]).await
        },
        result = &mut supervisor => {
            let _ = shutdown_sender.send(true);
            settle_tasks(result, "core supervisor", [
                (server, "API server"),
                (scheduler, "subscription scheduler"),
                (tunnels, "tunnel supervisor"),
            ]).await
        },
        result = &mut scheduler => {
            let _ = shutdown_sender.send(true);
            settle_tasks(result, "subscription scheduler", [
                (server, "API server"),
                (supervisor, "core supervisor"),
                (tunnels, "tunnel supervisor"),
            ]).await
        },
        result = &mut tunnels => {
            let _ = shutdown_sender.send(true);
            settle_tasks(result, "tunnel supervisor", [
                (server, "API server"),
                (supervisor, "core supervisor"),
                (scheduler, "subscription scheduler"),
            ]).await
        },
    }
}

async fn settle_tasks<const N: usize>(
    first: Result<Result<(), ClientError>, JoinError>,
    first_name: &'static str,
    remaining: [(JoinHandle<Result<(), ClientError>>, &'static str); N],
) -> Result<(), ClientError> {
    let mut result = task_result(first, first_name);
    for (task, name) in remaining {
        result = result.and(task_result(task.await, name));
    }
    result
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
