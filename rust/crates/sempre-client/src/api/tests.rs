use axum::{
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::{HeaderValue, Request, StatusCode, header},
};
use sempre_control::{DaemonEndpoint, WebConfigStore};
use sempre_manager::Manager;
use sempre_state::{Layout, Store};
use tower::ServiceExt;

use super::*;

pub(super) fn test_state(root: &tempfile::TempDir) -> (Arc<AppState>, String) {
    let layout = Layout::at(root.path());
    let manager = Arc::new(Manager::new(Store::new(layout.clone())).expect("manager"));
    let web = WebConfigStore::new(layout.web_config.clone());
    web.initialize().expect("web config");
    let endpoint = DaemonEndpoint::new("http://127.0.0.1:33211").expect("endpoint");
    let token = endpoint.token.clone();
    (
        Arc::new(
            AppState::new(
                manager,
                web,
                endpoint.token,
                "127.0.0.1:33211".into(),
                "http://127.0.0.1:33211".into(),
            )
            .expect("app state"),
        ),
        token,
    )
}

pub(super) fn request(method: &str, path: &str, body: Body, remote: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .extension(ConnectInfo(
            remote.parse::<SocketAddr>().expect("remote address"),
        ))
        .body(body)
        .expect("request")
}

#[tokio::test]
async fn health_is_public_and_inventory_requires_authentication() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    let app = router(state);
    let health = app
        .clone()
        .oneshot(request(
            "GET",
            "/api/v1/health",
            Body::empty(),
            "127.0.0.1:1",
        ))
        .await
        .expect("health");
    assert_eq!(health.status(), StatusCode::OK);
    let unauthorized = app
        .clone()
        .oneshot(request(
            "GET",
            "/api/v1/cores",
            Body::empty(),
            "127.0.0.1:1",
        ))
        .await
        .expect("inventory");
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let mut authenticated = request("GET", "/api/v1/cores", Body::empty(), "127.0.0.1:1");
    authenticated.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.oneshot(authenticated).await.expect("inventory");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn bundle_export_streams_an_authenticated_archive_and_cleans_it_on_drop() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    let runtime = state.manager.store().layout().runtime.clone();
    let app = router(state);
    let mut export = request("GET", "/api/v1/bundle/export", Body::empty(), "127.0.0.1:1");
    export.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.oneshot(export).await.expect("bundle export");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&HeaderValue::from_static("application/zip"))
    );
    assert!(
        response
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("sempre-bundle-"))
    );
    assert_eq!(bundle_archives(&runtime), 1);
    drop(response);
    assert_eq!(bundle_archives(&runtime), 0);
}

fn bundle_archives(runtime: &std::path::Path) -> usize {
    std::fs::read_dir(runtime)
        .expect("runtime directory")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("sempre-bundle-")
        })
        .count()
}

#[tokio::test]
async fn core_auto_diagnosis_and_update_routes_use_the_rust_manager() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    let app = router(state);
    let mut diagnose = request(
        "POST",
        "/api/v1/cores/auto/diagnose",
        Body::empty(),
        "127.0.0.1:1",
    );
    diagnose.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.clone().oneshot(diagnose).await.expect("diagnose");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let report: serde_json::Value = serde_json::from_slice(&body).expect("report JSON");
    assert!(
        !report["candidates"]
            .as_array()
            .expect("candidates")
            .is_empty()
    );

    let mut update = request(
        "POST",
        "/api/v1/cores/update",
        Body::from(r#"{"reference":"sing-box@1.12.20"}"#),
        "127.0.0.1:1",
    );
    update.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    update.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    assert_eq!(
        app.oneshot(update).await.expect("update").status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn tunnel_routes_read_and_persist_validated_rust_configuration() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    let app = router(state);
    let mut status = request("GET", "/api/v1/tunnels", Body::empty(), "127.0.0.1:1");
    status.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.clone().oneshot(status).await.expect("status");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("status JSON");
    assert_eq!(value["config"]["schema"], 1);
    assert_eq!(value["binary"]["version"], "10.5.5");

    let mut update = request(
        "PUT",
        "/api/v1/tunnels",
        Body::from(r#"{"schema":1,"instances":[]}"#),
        "127.0.0.1:1",
    );
    update.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    update.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.oneshot(update).await.expect("update");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("update JSON");
    assert_eq!(
        value["status"]["config"]["instances"],
        serde_json::json!([])
    );
    assert_eq!(value["core_restart_requested"], false);
}

#[tokio::test]
async fn daemon_token_is_rejected_off_loopback_and_same_origin_login_works() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    let app = router(state);
    let mut remote = request("GET", "/api/v1/cores", Body::empty(), "192.0.2.1:1");
    remote.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    assert_eq!(
        app.clone().oneshot(remote).await.expect("remote").status(),
        StatusCode::UNAUTHORIZED
    );

    let mut login = request(
        "POST",
        "/api/v1/auth/login",
        Body::from(r#"{"password":""}"#),
        "127.0.0.1:1",
    );
    login.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    login
        .headers_mut()
        .insert(header::HOST, HeaderValue::from_static("127.0.0.1:33211"));
    login.headers_mut().insert(
        header::ORIGIN,
        HeaderValue::from_static("http://127.0.0.1:33211"),
    );
    let response = app.clone().oneshot(login).await.expect("login");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("login JSON");
    let session = value["token"].as_str().expect("session token");
    assert_eq!(session.len(), 64);

    let mut inventory = request("GET", "/api/v1/cores", Body::empty(), "127.0.0.1:1");
    inventory.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {session}")).expect("authorization"),
    );
    assert_eq!(
        app.oneshot(inventory).await.expect("inventory").status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn cross_origin_login_requires_an_administrator_password() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, _) = test_state(&root);
    let app = router(state);
    let mut login = request(
        "POST",
        "/api/v1/auth/login",
        Body::from(r#"{"password":""}"#),
        "127.0.0.1:1",
    );
    login.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    login
        .headers_mut()
        .insert(header::HOST, HeaderValue::from_static("127.0.0.1:33211"));
    login.headers_mut().insert(
        header::ORIGIN,
        HeaderValue::from_static("https://console.invalid"),
    );
    assert_eq!(
        app.oneshot(login).await.expect("login").status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn authenticated_core_selection_uses_the_manager_transaction() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    state
        .manager
        .store()
        .update(|document| {
            let source = &mut document.core_mut("sing-box").default;
            source.installed.insert(
                "1.2.3".into(),
                sempre_state::Installation {
                    explicit: false,
                    digest: "a".repeat(64),
                    source: "https://example.invalid/sing-box.zip".into(),
                    installed_at: chrono::Utc::now(),
                },
            );
            source.channels.insert("stable".into(), "1.2.3".into());
            Ok(())
        })
        .expect("seed installation");
    let app = router(state);
    let mut select = request(
        "POST",
        "/api/v1/cores/use",
        Body::from(r#"{"reference":"sing-box@stable"}"#),
        "127.0.0.1:1",
    );
    select.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    select.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.clone().oneshot(select).await.expect("select core");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("change JSON");
    assert_eq!(value["Changed"], true);
    assert_eq!(value["NeedsRestart"], false);

    let mut remove = request(
        "POST",
        "/api/v1/cores/remove",
        Body::from(r#"{"reference":"sing-box@1.2.3"}"#),
        "127.0.0.1:1",
    );
    remove.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    remove.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    assert_eq!(
        app.oneshot(remove).await.expect("remove core").status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn runtime_routes_report_status_and_stable_readiness_errors() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    let app = router(state);
    let mut status = request(
        "GET",
        "/api/v1/runtime/status",
        Body::empty(),
        "127.0.0.1:1",
    );
    status.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.clone().oneshot(status).await.expect("runtime status");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("status JSON");
    assert_eq!(value["runtime_state"], "idle");
    assert_eq!(value["actions"]["start"]["allowed"], false);

    let mut start = request(
        "POST",
        "/api/v1/runtime/start",
        Body::empty(),
        "127.0.0.1:1",
    );
    start.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.oneshot(start).await.expect("runtime start");
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("error JSON");
    assert_eq!(value["error"]["code"], "RUNTIME_NOT_READY");
    assert_eq!(value["error"]["details"]["status"]["runtime_state"], "idle");
}

#[tokio::test]
async fn current_configuration_is_read_only_and_requires_a_selected_config() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    let app = router(state);
    let mut get_config = request(
        "GET",
        "/api/v1/configs/current",
        Body::empty(),
        "127.0.0.1:1",
    );
    get_config.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    assert_eq!(
        app.clone()
            .oneshot(get_config)
            .await
            .expect("get config")
            .status(),
        StatusCode::BAD_REQUEST
    );

    let mut write_config = request(
        "PUT",
        "/api/v1/configs/current",
        Body::empty(),
        "127.0.0.1:1",
    );
    write_config.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.oneshot(write_config).await.expect("write config");
    assert_eq!(response.status(), StatusCode::GONE);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("error JSON");
    assert_eq!(value["error"]["code"], "DIRECT_CONFIG_REMOVED");
}
