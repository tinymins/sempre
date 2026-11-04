use axum::{
    Router,
    http::{HeaderMap, StatusCode as AxumStatus, Uri, header::LOCATION},
    response::IntoResponse,
    routing::get,
};

use super::*;

async fn serve(app: Router) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    address
}

fn proxy(address: std::net::SocketAddr) -> ResourceRoute {
    ResourceRoute::Proxy(SourceProxyRoute {
        id: "proxy".into(),
        address,
        username: "sempre".into(),
        password: "secret".into(),
    })
}

fn system() -> Client {
    sempre_http::client_builder()
        .no_proxy()
        .redirect(Policy::none())
        .build()
        .unwrap()
}

#[tokio::test]
async fn host_rules_override_the_default_and_match_exact_hosts() {
    let proxied = serve(Router::new().fallback(get(|uri: Uri| async move {
        format!("proxied {}", uri.host().unwrap_or_default())
    })))
    .await;
    let direct = serve(Router::new().fallback(get(|| async { "direct" }))).await;
    let routes = ResourceRoutes::new(
        proxy(proxied),
        vec![("127.0.0.1".into(), ResourceRoute::Direct)],
    )
    .unwrap();
    let body = routes
        .fetch(&format!("http://{direct}/"), "test", 1024, &system())
        .await
        .unwrap();
    assert_eq!(body, b"direct");
    let body = routes
        .fetch("http://rules.example.test/", "test", 1024, &system())
        .await
        .unwrap();
    assert_eq!(body, b"proxied rules.example.test");
}

#[tokio::test]
async fn each_redirect_selects_the_outbound_again() {
    let proxied = serve(
        Router::new().fallback(get(|uri: Uri, headers: HeaderMap| async move {
            assert_eq!(headers["proxy-authorization"], "Basic c2VtcHJlOnNlY3JldA==");
            format!("via proxy {}", uri.path())
        })),
    )
    .await;
    let origin = serve(Router::new().fallback(get(|| async {
        (
            AxumStatus::FOUND,
            [(LOCATION, "http://cdn.example.test/file")],
        )
            .into_response()
    })))
    .await;
    let routes = ResourceRoutes::new(
        ResourceRoute::Direct,
        vec![("cdn.example.test".into(), proxy(proxied))],
    )
    .unwrap();
    let body = routes
        .fetch(&format!("http://{origin}/start"), "test", 1024, &system())
        .await
        .unwrap();
    assert_eq!(body, b"via proxy /file");
}

#[tokio::test]
async fn an_unavailable_proxy_fails_without_any_request() {
    let requests = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = requests.clone();
    let origin = serve(Router::new().fallback(get(move || async move {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        "direct"
    })))
    .await;
    let routes =
        ResourceRoutes::new(ResourceRoute::Unavailable("stopped".into()), Vec::new()).unwrap();
    let error = routes
        .fetch(&format!("http://{origin}/"), "test", 1024, &system())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("stopped"), "{error}");
    assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_proxy_that_is_not_loopback_is_refused() {
    assert!(ResourceRoutes::new(proxy("192.0.2.1:1".parse().unwrap()), Vec::new()).is_err());
}
