use std::{io::Write as _, net::SocketAddr, sync::Arc, time::Duration};

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

fn system_fixture() -> (tempfile::TempDir, Router, String) {
    let root = tempfile::tempdir().expect("temporary directory");
    let layout = Layout::system_at(root.path());
    fixture_with_layout(root, layout)
}

fn fixture_with_layout(
    root: tempfile::TempDir,
    layout: Layout,
) -> (tempfile::TempDir, Router, String) {
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

async fn authenticated_json(
    app: Router,
    token: &str,
    method: &str,
    path: &str,
    body: &'static str,
) -> axum::response::Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .extension(ConnectInfo(
            "127.0.0.1:1".parse::<SocketAddr>().expect("remote address"),
        ))
        .body(Body::from(body))
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
async fn system_and_network_inventory_match_the_control_ui_contract() {
    let (_root, app, token) = fixture();
    let response = authenticated_get(app.clone(), &token, "/api/v1/system").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("system body");
    let system: serde_json::Value = serde_json::from_slice(&body).expect("system JSON");
    assert_eq!(system["mode"], "portable");
    assert!(
        system["service_memory"]
            .as_u64()
            .is_some_and(|value| value > 0)
    );
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
async fn service_update_task_starts_empty() {
    let (_root, app, token) = fixture();
    let response = authenticated_get(app, &token, "/api/v1/service/update/task").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 16 * 1024)
        .await
        .expect("task body");
    let task: serde_json::Value = serde_json::from_slice(&body).expect("task JSON");
    assert!(task["task"].is_null());
}

#[tokio::test]
async fn uploaded_update_reports_missing_release_entrypoints_in_the_update_task() {
    let (_root, app, token) = system_fixture();
    let archive = invalid_update_archive();
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/v1/service/update/upload?name=sempre-update.zip")
        .extension(ConnectInfo(
            "127.0.0.1:1".parse::<SocketAddr>().expect("remote address"),
        ))
        .body(Body::from(archive))
        .expect("request");
    request.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.clone().oneshot(request).await.expect("upload response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let task = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let response =
                authenticated_get(app.clone(), &token, "/api/v1/service/update/task").await;
            let body = to_bytes(response.into_body(), 16 * 1024)
                .await
                .expect("task body");
            let value: serde_json::Value = serde_json::from_slice(&body).expect("task JSON");
            if value["task"]["state"] == "failed" {
                break value["task"].clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("failed update task");
    assert_eq!(task["stage"], "validating");
    let error = task["error"].as_str().expect("task error");
    assert!(error.contains(".sempre"));
    assert!(error.contains("install"));
}

fn invalid_update_archive() -> Vec<u8> {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        value => value,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        value => value,
    };
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    archive
        .start_file(
            format!("sempre-{os}-{arch}/README.txt"),
            zip::write::SimpleFileOptions::default(),
        )
        .expect("archive entry");
    archive
        .write_all(b"not a release bundle")
        .expect("archive data");
    archive.finish().expect("finish archive").into_inner()
}

#[tokio::test]
async fn service_update_settings_default_to_stable_and_persist_prerelease_consent() {
    let (root, app, token) = fixture();
    let response = authenticated_get(app.clone(), &token, "/api/v1/service/update/settings").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 16 * 1024)
        .await
        .expect("settings body");
    let settings: serde_json::Value = serde_json::from_slice(&body).expect("settings JSON");
    assert_eq!(settings["settings"]["allow_prerelease"], false);

    let response = authenticated_json(
        app.clone(),
        &token,
        "PUT",
        "/api/v1/service/update/settings",
        r#"{"allow_prerelease":true}"#,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let saved = std::fs::read_to_string(root.path().join(".sempre/service-update.json"))
        .expect("saved settings");
    assert!(saved.contains(r#""allow_prerelease": true"#));

    let response = authenticated_get(app, &token, "/api/v1/service/update/settings").await;
    let body = to_bytes(response.into_body(), 16 * 1024)
        .await
        .expect("settings body");
    let settings: serde_json::Value = serde_json::from_slice(&body).expect("settings JSON");
    assert_eq!(settings["settings"]["allow_prerelease"], true);
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
    let response = app.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("service body");
    let error: serde_json::Value = serde_json::from_slice(&body).expect("service JSON");
    assert_eq!(error["error"]["code"], "SERVICE_UNAVAILABLE");

    let mut request = Request::builder()
        .method("POST")
        .uri("/api/v1/service/update")
        .extension(ConnectInfo(
            "127.0.0.1:1".parse::<SocketAddr>().expect("remote address"),
        ))
        .body(Body::empty())
        .expect("request");
    request.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::CONFLICT);
}
