use std::sync::{Arc, RwLock};

use axum::{Router, body::Body, extract::Request, middleware, response::Response};
use chrono::Utc;
use futures_util::StreamExt;
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
    web.set_listen(listen)?;
    write_endpoints(&next, daemon_template, layout)?;
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
            listener,
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
