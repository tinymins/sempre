use std::sync::{Arc, RwLock};

use axum::Router;
use chrono::Utc;
use sempre_control::{DaemonEndpoint, PublicEndpoint, WebConfigStore, local_url, validate_listen};
use sempre_state::Layout;
use tokio::{
    net::TcpListener,
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};

use crate::{ClientError, VERSION};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Endpoint {
    pub(crate) bind: String,
    pub(crate) local_url: String,
}

#[derive(Clone, Debug)]
pub(crate) struct EndpointStore(Arc<RwLock<Endpoint>>);

impl EndpointStore {
    pub(crate) fn new(bind: String, local_url: String) -> Self {
        Self(Arc::new(RwLock::new(Endpoint { bind, local_url })))
    }

    pub(crate) fn get(&self) -> Endpoint {
        self.0.read().expect("endpoint state lock").clone()
    }

    fn set(&self, endpoint: Endpoint) {
        *self.0.write().expect("endpoint state lock") = endpoint;
    }
}

pub(crate) struct RebindRequest {
    listen: String,
    response: oneshot::Sender<Result<Endpoint, String>>,
}

#[derive(Clone, Debug)]
pub(crate) struct RebindHandle(mpsc::Sender<RebindRequest>);

impl RebindHandle {
    pub(crate) async fn request(&self, listen: &str) -> Result<Endpoint, String> {
        let (response, result) = oneshot::channel();
        self.0
            .send(RebindRequest {
                listen: listen.into(),
                response,
            })
            .await
            .map_err(|_| "web listener manager is unavailable".to_owned())?;
        result
            .await
            .map_err(|_| "web listener manager stopped before applying the change".to_owned())?
    }
}

pub(crate) fn channel() -> (RebindHandle, mpsc::Receiver<RebindRequest>) {
    let (sender, receiver) = mpsc::channel(1);
    (RebindHandle(sender), receiver)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run(
    listener: TcpListener,
    app: Router,
    endpoint: EndpointStore,
    web: WebConfigStore,
    daemon_endpoint: DaemonEndpoint,
    layout: Layout,
    mut requests: mpsc::Receiver<RebindRequest>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), ClientError> {
    let (mut stop, stop_request) = oneshot::channel();
    let mut server = serve(listener, app.clone(), stop_request);
    loop {
        tokio::select! {
            result = &mut server => return server_result(result),
            request = requests.recv() => {
                let Some(request) = request else {
                    continue;
                };
                match prepare_rebind(
                    &request.listen,
                    &endpoint,
                    &web,
                    &daemon_endpoint,
                    &layout,
                ).await {
                    Ok(prepared) => {
                        let (next_stop, stop_request) = oneshot::channel();
                        let next = serve(prepared.listener, app.clone(), stop_request);
                        let previous = std::mem::replace(&mut server, next);
                        let previous_stop = std::mem::replace(&mut stop, next_stop);
                        let _ = previous_stop.send(());
                        tokio::spawn(async move { let _ = previous.await; });
                        let _ = request.response.send(Ok(prepared.endpoint));
                    }
                    Err(error) => {
                        let _ = request.response.send(Err(error.to_string()));
                    }
                }
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    let _ = stop.send(());
                    return server_result(server.await);
                }
            }
        }
    }
}

struct Prepared {
    listener: TcpListener,
    endpoint: Endpoint,
}

async fn prepare_rebind(
    listen: &str,
    endpoint_store: &EndpointStore,
    web: &WebConfigStore,
    daemon_template: &DaemonEndpoint,
    layout: &Layout,
) -> Result<Prepared, ClientError> {
    validate_listen(listen)?;
    let listener = TcpListener::bind(listen)
        .await
        .map_err(|source| ClientError::Bind {
            address: listen.into(),
            source,
        })?;
    let next = Endpoint {
        bind: listen.into(),
        local_url: local_url(listen)?,
    };
    let previous_endpoint = endpoint_store.get();
    let previous_config = web.read()?;
    web.set_listen(listen)?;
    if let Err(error) = write_endpoints(&next, daemon_template, layout) {
        let _ = web.set_listen(&previous_config.listen);
        let _ = write_endpoints(&previous_endpoint, daemon_template, layout);
        return Err(error);
    }
    endpoint_store.set(next.clone());
    Ok(Prepared {
        listener,
        endpoint: next,
    })
}

fn write_endpoints(
    endpoint: &Endpoint,
    daemon_template: &DaemonEndpoint,
    layout: &Layout,
) -> Result<(), ClientError> {
    let daemon = DaemonEndpoint {
        base_url: endpoint.local_url.clone(),
        updated_at: Utc::now(),
        ..daemon_template.clone()
    };
    daemon.write(&layout.daemon_control)?;
    PublicEndpoint::new(VERSION, &endpoint.bind, &endpoint.local_url)?.write(&layout.endpoint)?;
    Ok(())
}

fn serve(
    listener: TcpListener,
    app: Router,
    stop: oneshot::Receiver<()>,
) -> JoinHandle<Result<(), ClientError>> {
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            let _ = stop.await;
        })
        .await
        .map_err(ClientError::Serve)
    })
}

fn server_result(
    result: Result<Result<(), ClientError>, tokio::task::JoinError>,
) -> Result<(), ClientError> {
    result.map_err(|source| ClientError::Task {
        component: "API listener",
        source,
    })?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rebind_prepares_new_listener_and_updates_discovery_transactionally() {
        let root = tempfile::tempdir().expect("temporary directory");
        let layout = Layout::at(root.path());
        sempre_state::Store::new(layout.clone())
            .initialize()
            .expect("layout");
        let web = WebConfigStore::new(&layout.web_config);
        web.initialize().expect("web config");
        let reserve = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve port");
        let listen = reserve.local_addr().expect("reserved address").to_string();
        drop(reserve);
        let current = EndpointStore::new("127.0.0.1:33211".into(), "http://127.0.0.1:33211".into());
        let daemon = DaemonEndpoint::new("http://127.0.0.1:33211").expect("daemon endpoint");
        let prepared = prepare_rebind(&listen, &current, &web, &daemon, &layout)
            .await
            .expect("rebind");
        assert_eq!(prepared.endpoint.bind, listen);
        assert_eq!(current.get(), prepared.endpoint);
        assert_eq!(web.read().expect("web config").listen, listen);
        assert_eq!(
            DaemonEndpoint::read(&layout.daemon_control)
                .expect("daemon discovery")
                .base_url,
            prepared.endpoint.local_url
        );
        assert_eq!(
            PublicEndpoint::read(&layout.endpoint)
                .expect("public discovery")
                .bind,
            listen
        );
    }

    #[tokio::test]
    async fn listener_manager_serves_on_the_rebound_address_without_process_restart() {
        let root = tempfile::tempdir().expect("temporary directory");
        let layout = Layout::at(root.path());
        sempre_state::Store::new(layout.clone())
            .initialize()
            .expect("layout");
        let web = WebConfigStore::new(&layout.web_config);
        web.initialize().expect("web config");
        let initial_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("initial listener");
        let initial = initial_listener
            .local_addr()
            .expect("initial address")
            .to_string();
        let reserve = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve port");
        let next = reserve.local_addr().expect("next address").to_string();
        drop(reserve);
        let endpoint = EndpointStore::new(initial.clone(), format!("http://{initial}"));
        let daemon = DaemonEndpoint::new(&format!("http://{initial}")).expect("daemon endpoint");
        let (handle, requests) = channel();
        let (shutdown, shutdown_request) = watch::channel(false);
        let app = Router::new().route("/", axum::routing::get(|| async { "ok" }));
        let task = tokio::spawn(run(
            initial_listener,
            app,
            endpoint,
            web,
            daemon,
            layout,
            requests,
            shutdown_request,
        ));

        let rebound = handle.request(&next).await.expect("live rebind");
        assert_eq!(rebound.bind, next);
        tokio::net::TcpStream::connect(&next)
            .await
            .expect("rebound listener accepts connections");
        shutdown.send(true).expect("shutdown");
        task.await.expect("listener task").expect("listener result");
    }
}
