use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::{HeaderValue, Request, StatusCode, header},
};
use sempre_control::{DaemonEndpoint, WebConfigStore};
use sempre_manager::Manager;
use sempre_state::{Layout, Store};
use tower::ServiceExt as _;

use super::*;

fn fixture() -> (tempfile::TempDir, Router, String) {
    let root = tempfile::tempdir().expect("temporary directory");
    let layout = Layout::at(root.path());
    let manager = Arc::new(Manager::new(Store::new(layout.clone())).expect("manager"));
    let web = WebConfigStore::new(layout.web_config);
    web.initialize().expect("web config");
    let traffic = Arc::new(
        crate::traffic_history::TrafficStore::open(layout.traffic_history).expect("traffic store"),
    );
    let endpoint = DaemonEndpoint::new("http://127.0.0.1:33211").expect("endpoint");
    let token = endpoint.token.clone();
    let state = Arc::new(AppState::new(
        manager,
        web,
        traffic,
        endpoint.token,
        "127.0.0.1:33211".into(),
        "http://127.0.0.1:33211".into(),
    ));
    (root, router(state), token)
}

async fn call(app: Router, token: &str, method: &str, path: &str, body: Body) -> Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .extension(ConnectInfo(
            "127.0.0.1:1".parse::<SocketAddr>().expect("remote address"),
        ))
        .body(body)
        .expect("request");
    request.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(token).expect("token"),
    );
    request.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    app.oneshot(request).await.expect("response")
}

#[tokio::test]
async fn custom_node_api_creates_lists_updates_and_removes_nodes() {
    let (_root, app, token) = fixture();
    let response = call(
        app.clone(),
        &token,
        "POST",
        "/api/v1/custom-nodes",
        Body::from(
            r#"{"name":"edge","proxy":{"name":"ignored","type":"socks5","server":"edge.example.com","port":1080}}"#,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("create body");
    let created: serde_json::Value = serde_json::from_slice(&body).expect("create JSON");
    let id = created["id"].as_str().expect("node ID");
    assert_eq!(created["proxy"]["name"], "edge");
    assert!(created["created_at"].is_string());

    let response = call(
        app.clone(),
        &token,
        "GET",
        "/api/v1/custom-nodes",
        Body::empty(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("list body");
    let list: serde_json::Value = serde_json::from_slice(&body).expect("list JSON");
    assert_eq!(list["nodes"].as_array().map(Vec::len), Some(1));

    let response = call(
        app.clone(),
        &token,
        "PUT",
        &format!("/api/v1/custom-nodes/{id}"),
        Body::from(
            r#"{"name":"renamed","proxy":{"name":"edge","type":"socks5","server":"edge.example.com","port":1080}}"#,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = call(
        app,
        &token,
        "DELETE",
        &format!("/api/v1/custom-nodes/{id}"),
        Body::empty(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}
