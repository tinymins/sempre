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
    fixture_with_layout(root, layout)
}

fn development_fixture() -> (tempfile::TempDir, Router, String) {
    let root = tempfile::tempdir().expect("temporary directory");
    let layout = Layout::development_at(root.path());
    fixture_with_layout(root, layout)
}

fn fixture_with_layout(
    root: tempfile::TempDir,
    layout: Layout,
) -> (tempfile::TempDir, Router, String) {
    let manager = Arc::new(Manager::new(Store::new(layout.clone())).expect("manager"));
    let web = WebConfigStore::new(layout.web_config);
    web.initialize().expect("web config");
    let endpoint = DaemonEndpoint::new("http://127.0.0.1:33211").expect("endpoint");
    let token = endpoint.token.clone();
    let state = Arc::new(AppState::new(
        manager,
        web,
        endpoint.token,
        "127.0.0.1:33211".into(),
        "http://127.0.0.1:33211".into(),
    ));
    (root, router(state), token)
}

async fn authenticated_get(app: Router, token: &str, path: &str) -> axum::response::Response {
    let mut request = Request::builder()
        .uri(path)
        .extension(ConnectInfo(
            "127.0.0.1:1".parse::<SocketAddr>().expect("remote address"),
        ))
        .body(Body::empty())
        .expect("request");
    request.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(token).expect("token"),
    );
    app.oneshot(request).await.expect("response")
}

#[tokio::test]
async fn system_and_network_inventory_match_the_control_ui_contract() {
    let (_root, app, token) = fixture();
    let response = authenticated_get(app.clone(), &token, "/api/v1/system").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("system body");
    let system: serde_json::Value = serde_json::from_slice(&body).expect("system JSON");
    assert_eq!(system["mode"], "portable");
    assert!(matches!(
        system["service"].as_str(),
        Some(
            "not installed" | "stopped" | "start pending" | "running" | "stop pending" | "unknown"
        )
    ));
    assert_eq!(system["runtime"]["state"], "idle");
    assert_eq!(system["web"]["local_url"], "http://127.0.0.1:33211");

    let response = authenticated_get(app, &token, "/api/v1/system/network").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("network body");
    let inventory: serde_json::Value = serde_json::from_slice(&body).expect("network JSON");
    assert!(inventory["supported"].is_boolean());
    assert!(inventory["interfaces"].is_array());
    assert!(inventory["occupied_prefixes"].is_array());
}

#[tokio::test]
async fn service_action_rejects_unsupported_operations_without_side_effects() {
    let (_root, app, token) = fixture();
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/v1/service/action")
        .extension(ConnectInfo(
            "127.0.0.1:1".parse::<SocketAddr>().expect("remote address"),
        ))
        .body(Body::from(r#"{"action":"uninstall"}"#))
        .expect("request");
    request.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    request.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    assert_eq!(
        app.oneshot(request).await.expect("response").status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn development_mode_reports_isolation_and_rejects_native_service_actions() {
    let (_root, app, token) = development_fixture();
    let response = authenticated_get(app.clone(), &token, "/api/v1/system").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("system body");
    let system: serde_json::Value = serde_json::from_slice(&body).expect("system JSON");
    assert_eq!(system["mode"], "development");
    assert_eq!(system["service"], "not installed");

    let mut request = Request::builder()
        .method("POST")
        .uri("/api/v1/service/action")
        .extension(ConnectInfo(
            "127.0.0.1:1".parse::<SocketAddr>().expect("remote address"),
        ))
        .body(Body::from(r#"{"action":"restart"}"#))
        .expect("request");
    request.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    request.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("service body");
    let error: serde_json::Value = serde_json::from_slice(&body).expect("service JSON");
    assert_eq!(error["error"]["code"], "SERVICE_UNAVAILABLE");
}
