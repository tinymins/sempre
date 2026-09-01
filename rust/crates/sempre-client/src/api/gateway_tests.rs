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

fn test_state(root: &tempfile::TempDir) -> (Arc<AppState>, String) {
    let layout = Layout::at(root.path());
    let manager = Arc::new(Manager::new(Store::new(layout.clone())).expect("manager"));
    let web = WebConfigStore::new(layout.web_config.clone());
    web.initialize().expect("web config");
    let traffic = Arc::new(
        crate::traffic_history::TrafficStore::open(layout.traffic_history).expect("traffic store"),
    );
    let endpoint = DaemonEndpoint::new("http://127.0.0.1:33211").expect("endpoint");
    let token = endpoint.token.clone();
    (
        Arc::new(AppState::new(
            manager,
            web,
            traffic,
            endpoint.token,
            "127.0.0.1:33211".into(),
            "http://127.0.0.1:33211".into(),
        )),
        token,
    )
}

fn request(method: &str, path: &str, body: Body, token: &str) -> Request<Body> {
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
    request
}

fn json_request(method: &str, path: &str, body: impl Into<Body>, token: &str) -> Request<Body> {
    let mut request = request(method, path, body.into(), token);
    request.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    request
}

#[tokio::test]
async fn gateway_routes_expose_defaults_validation_and_safe_host_plans() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    let app = router(state);

    let response = app
        .clone()
        .oneshot(request("GET", "/api/v1/gateway", Body::empty(), &token))
        .await
        .expect("status");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("status body");
    let status: serde_json::Value = serde_json::from_slice(&body).expect("status JSON");
    assert_eq!(status["config"]["schema"], 2);
    assert!(status["config"].get("dns").is_none());
    assert_eq!(status["host_plan_available"], true);

    let invalid = json_request(
        "POST",
        "/api/v1/gateway/validate",
        r#"{"lan":{"interface":"eth0; reboot"}}"#,
        &token,
    );
    let response = app.clone().oneshot(invalid).await.expect("validate");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("validation body");
    let validation: serde_json::Value = serde_json::from_slice(&body).expect("validation JSON");
    assert_eq!(validation["valid"], false);

    let plan = json_request(
        "POST",
        "/api/v1/gateway/host-plan",
        r#"{"config":{}}"#,
        &token,
    );
    let response = app.clone().oneshot(plan).await.expect("host plan");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("plan body");
    let plan: serde_json::Value = serde_json::from_slice(&body).expect("plan JSON");
    assert!(
        plan["summary"]
            .as_str()
            .expect("summary")
            .contains("<lan-interface>")
    );

    let apply = json_request(
        "POST",
        "/api/v1/gateway/host-apply",
        r#"{"config":{},"confirm":true}"#,
        &token,
    );
    assert_eq!(
        app.oneshot(apply).await.expect("host apply").status(),
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn gateway_runtime_operations_return_domain_errors_without_services() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    let app = router(state);
    let removed_query = json_request(
        "POST",
        "/api/v1/gateway/dns/query",
        r#"{"name":"example.com","type":"INVALID"}"#,
        &token,
    );
    assert_eq!(
        app.clone()
            .oneshot(removed_query)
            .await
            .expect("query")
            .status(),
        StatusCode::NOT_FOUND
    );
    let revoke = json_request(
        "POST",
        "/api/v1/gateway/dhcp/leases/revoke",
        r#"{"mac":"00:01:02:03:04:05"}"#,
        &token,
    );
    assert_eq!(
        app.oneshot(revoke).await.expect("revoke").status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn gateway_update_persists_normalized_configuration() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    let app = router(state);
    let update = json_request(
        "PUT",
        "/api/v1/gateway",
        r#"{"topology":"local-pve"}"#,
        &token,
    );
    let response = app.clone().oneshot(update).await.expect("update");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("update body");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("update JSON");
    assert_eq!(value["config"]["lan"]["gateway_cidr"], "10.10.10.1/24");
    assert_eq!(value["reload_requested"], true);

    let response = app
        .oneshot(request("GET", "/api/v1/gateway", Body::empty(), &token))
        .await
        .expect("status");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn network_mode_defaults_local_and_persists_supported_changes() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    let app = router(state);
    let response = app
        .clone()
        .oneshot(request(
            "GET",
            "/api/v1/network/settings",
            Body::empty(),
            &token,
        ))
        .await
        .expect("network settings");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("network body");
    let current: serde_json::Value = serde_json::from_slice(&body).expect("network JSON");
    assert_eq!(current["settings"]["mode"], "local");

    let mode = "local";
    let update = json_request(
        "PUT",
        "/api/v1/network/settings",
        format!(r#"{{"schema":1,"revision":1,"mode":"{mode}","gateway_capture_host":true}}"#),
        &token,
    );
    let response = app.oneshot(update).await.expect("update network settings");
    assert_eq!(response.status(), StatusCode::OK);
}
