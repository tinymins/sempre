mod sockets;

use std::{
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use axum::{
    Router, body::Body, extract::Request, middleware, response::Response, serve::ListenerExt,
};
use chrono::Utc;
use futures_util::StreamExt;
use sempre_control::{DaemonEndpoint, PublicEndpoint, WebConfigStore, local_url, primary_listen};
use sempre_state::Layout;
#[cfg(test)]
use tokio::net::TcpListener;
use tokio::{
    net::TcpStream,
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};

use crate::{ClientError, VERSION};
use sockets::Sockets;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Endpoint {
    pub(crate) bind: String,
    pub(crate) bind_addresses: Vec<String>,
    pub(crate) local_url: String,
}

impl Endpoint {
    pub(crate) fn from_addresses(addresses: Vec<String>) -> Result<Self, ClientError> {
        let bind = primary_listen(&addresses)?.to_owned();
        Ok(Self {
            local_url: local_url(&bind)?,
            bind,
            bind_addresses: addresses,
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct EndpointStore(Arc<RwLock<Endpoint>>);

impl EndpointStore {
    pub(crate) fn new(bind: String, local_url: String) -> Self {
        Self(Arc::new(RwLock::new(Endpoint {
            bind_addresses: vec![bind.clone()],
            bind,
            local_url,
        })))
    }
    pub(crate) fn get(&self) -> Endpoint {
        self.0.read().expect("endpoint state lock").clone()
    }
    pub(crate) fn set(&self, endpoint: Endpoint) {
        *self.0.write().expect("endpoint state lock") = endpoint;
    }
}

pub(crate) struct RebindRequest {
    listen: Vec<String>,
    response: oneshot::Sender<Result<Endpoint, String>>,
}

#[derive(Clone, Debug)]
pub(crate) struct RebindHandle(mpsc::Sender<RebindRequest>);

impl RebindHandle {
    pub(crate) async fn request(&self, listen: &[String]) -> Result<Endpoint, String> {
        let (response, result) = oneshot::channel();
        self.0
            .send(RebindRequest {
                listen: listen.to_vec(),
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

pub(crate) struct ManagedListener {
    sockets: Sockets,
    endpoint: EndpointStore,
    web: WebConfigStore,
    daemon_endpoint: DaemonEndpoint,
    layout: Layout,
    requests: mpsc::Receiver<RebindRequest>,
    fatal: Option<oneshot::Sender<String>>,
}

pub(crate) async fn bind(values: &[String]) -> Result<Sockets, ClientError> {
    Sockets::bind(values).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run(
    sockets: Sockets,
    app: Router,
    endpoint: EndpointStore,
    web: WebConfigStore,
    daemon_endpoint: DaemonEndpoint,
    layout: Layout,
    requests: mpsc::Receiver<RebindRequest>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), ClientError> {
    let (failed, mut failure) = oneshot::channel();
    let listener = ManagedListener {
        sockets,
        endpoint,
        web,
        daemon_endpoint,
        layout,
        requests,
        fatal: Some(failed),
    };
    let (stop, stop_request) = oneshot::channel();
    let mut server = serve(listener, app, stop_request);
    tokio::select! {
        result = &mut server => server_result(result),
        error = &mut failure => {
            let _ = stop.send(());
            let _ = server.await;
            Err(ClientError::Runtime(error.unwrap_or_else(|_| "web listener stopped".into())))
        }
        () = async { while !*shutdown.borrow() && shutdown.changed().await.is_ok() {} } => {
            let _ = stop.send(());
            server_result(server.await)
        }
    }
}

impl axum::serve::Listener for ManagedListener {
    type Io = TcpStream;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (TcpStream, SocketAddr) {
        loop {
            tokio::select! {
                accepted = self.sockets.accept() => match accepted {
                    Ok(connection) => return connection,
                    Err(error) => {
                        tracing::warn!(%error, "accept web connection failed");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                },
                Some(request) = self.requests.recv() => {
                    let previous = self.endpoint.get();
                    let result = self.sockets.change(&request.listen, |addresses| {
                        let next = Endpoint::from_addresses(addresses.to_vec())?;
                        let saved = write_endpoints(&next, &self.daemon_endpoint, &self.layout)
                            .and_then(|()| self.web.set_listens(&request.listen).map(|_| ()).map_err(ClientError::from));
                        if let Err(error) = saved {
                            if let Err(restore) = write_endpoints(&previous, &self.daemon_endpoint, &self.layout) {
                                return Err(ClientError::Runtime(format!("{error}; restoring web discovery files failed: {restore}")));
                            }
                            return Err(error);
                        }
                        self.endpoint.set(next.clone());
                        Ok(next)
                    }).await;
                    match result {
                        Ok(endpoint) => { let _ = request.response.send(Ok(endpoint)); }
                        Err(error) => {
                            let _ = request.response.send(Err(error.message.clone()));
                            if error.fatal {
                                if let Some(failed) = self.fatal.take() { let _ = failed.send(error.message); }
                                return std::future::pending().await;
                            }
                        }
                    }
                }
            }
        }
    }
    fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.endpoint
            .get()
            .bind
            .parse()
            .map_err(std::io::Error::other)
    }
}

pub(crate) fn write_endpoints(
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

fn serve<L>(
    listener: L,
    app: Router,
    stop: oneshot::Receiver<()>,
) -> JoinHandle<Result<(), ClientError>>
where
    L: axum::serve::Listener<Addr = std::net::SocketAddr>,
{
    tokio::spawn(async move {
        let (closing, closed) = watch::channel(false);
        let app = app.layer(middleware::from_fn(
            move |request: Request, next: middleware::Next| {
                let mut closed = closed.clone();
                async move {
                    let response = next.run(request).await;
                    // Event streams must end before graceful shutdown can finish draining connections.
                    if !response
                        .headers()
                        .get(axum::http::header::CONTENT_TYPE)
                        .is_some_and(|value| value.as_bytes().starts_with(b"text/event-stream"))
                    {
                        return response;
                    }
                    let (parts, body) = response.into_parts();
                    let stream = body.into_data_stream().take_until(async move {
                        let _ = closed.wait_for(|value| *value).await;
                    });
                    Response::from_parts(parts, Body::from_stream(stream))
                }
            },
        ));
        axum::serve(
            listener.tap_io(|_| {}),
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            let _ = stop.await;
            let _ = closing.send(true);
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
    async fn shutdown_closes_an_open_event_stream_before_waiting_for_connections() {
        use axum::response::{Sse, sse::Event};
        use std::{convert::Infallible, time::Duration};
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/events",
            axum::routing::get(|| async {
                Sse::new(
                    futures_util::stream::once(async {
                        Ok::<_, Infallible>(Event::default().data("connected"))
                    })
                    .chain(futures_util::stream::pending()),
                )
            }),
        );
        let (stop, stopped) = oneshot::channel();
        let server = serve(listener, app, stopped);
        let mut response = reqwest::get(format!("http://{address}/events"))
            .await
            .unwrap();
        assert!(response.chunk().await.unwrap().is_some());
        stop.send(()).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), response.chunk())
                .await
                .expect("stream must end on shutdown")
                .unwrap()
                .is_none()
        );
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .expect("listener must finish with an open browser")
            .unwrap()
            .unwrap();
    }
}
