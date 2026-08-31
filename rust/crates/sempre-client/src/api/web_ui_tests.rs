use std::{fs, io::Write as _, net::SocketAddr, sync::Arc};

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

fn request(method: &str, path: &str, body: Body, token: Option<&str>) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .extension(ConnectInfo(
            "127.0.0.1:1".parse::<SocketAddr>().expect("remote address"),
        ))
        .body(body)
        .expect("request");
    if let Some(token) = token {
        request.headers_mut().insert(
            DAEMON_TOKEN_HEADER,
            HeaderValue::from_str(token).expect("token"),
        );
    }
    request
}

#[tokio::test]
async fn web_password_update_invalidates_sessions_and_rebind_is_explicitly_rejected() {
    let (_root, app, _token) = fixture();
    let mut login = request(
        "POST",
        "/api/v1/auth/login",
        Body::from(r#"{"password":""}"#),
        None,
    );
    login.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let login = app.clone().oneshot(login).await.expect("login");
    let body = to_bytes(login.into_body(), 64 * 1024)
        .await
        .expect("login body");
    let session: serde_json::Value = serde_json::from_slice(&body).expect("login JSON");
    let session = session["token"].as_str().expect("session token");
    let mut password = request(
        "PATCH",
        "/api/v1/web",
        Body::from(r#"{"password":"administrator"}"#),
        None,
    );
    password.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    password.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {session}")).expect("authorization"),
    );
    assert_eq!(
        app.clone()
            .oneshot(password)
            .await
            .expect("password")
            .status(),
        StatusCode::OK
    );
    let mut expired = request("GET", "/api/v1/web", Body::empty(), None);
    expired.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {session}")).expect("authorization"),
    );
    assert_eq!(
        app.clone()
            .oneshot(expired)
            .await
            .expect("invalidated session")
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let (_root, app, token) = fixture();
    let mut rebind = request(
        "PATCH",
        "/api/v1/web",
        Body::from(r#"{"listen":"0.0.0.0:33211"}"#),
        Some(&token),
    );
    rebind.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    assert_eq!(
        app.oneshot(rebind).await.expect("rebind").status(),
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn static_ui_serves_spa_without_exposing_metadata_or_unknown_api_routes() {
    let (root, app, _token) = fixture();
    let current = root.path().join(".sempre/ui/current");
    fs::create_dir_all(current.join("assets")).expect("UI directories");
    fs::write(current.join("index.html"), "<main>Sempre UI</main>").expect("index");
    fs::write(current.join("assets/app.js"), "console.log('Sempre')").expect("asset");
    fs::write(
        current.join(sempre_ui::METADATA_NAME),
        r#"{"manifest":{"schema":1,"name":"Sempre UI","version":"1","entry":"index.html","api":{"major":1}},"source_type":"local","source":"test.zip","sha256":"abc","installed_at":"2026-01-01T00:00:00Z"}"#,
    )
    .expect("metadata");

    let response = app
        .clone()
        .oneshot(request("GET", "/subscriptions", Body::empty(), None))
        .await
        .expect("SPA");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert!(!response.headers().contains_key(header::LAST_MODIFIED));
    let body = to_bytes(response.into_body(), 4096)
        .await
        .expect("SPA body");
    assert_eq!(&body[..], b"<main>Sempre UI</main>");

    let mut conditional = request("GET", "/", Body::empty(), None);
    conditional.headers_mut().insert(
        header::IF_MODIFIED_SINCE,
        HeaderValue::from_static("Wed, 21 Oct 2099 07:28:00 GMT"),
    );
    let conditional = app
        .clone()
        .oneshot(conditional)
        .await
        .expect("conditional index");
    assert_eq!(conditional.status(), StatusCode::OK);
    let body = to_bytes(conditional.into_body(), 4096)
        .await
        .expect("conditional index body");
    assert_eq!(&body[..], b"<main>Sempre UI</main>");

    let asset = app
        .clone()
        .oneshot(request("GET", "/assets/app.js", Body::empty(), None))
        .await
        .expect("asset");
    assert_eq!(
        asset.headers()[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
    assert_eq!(
        app.clone()
            .oneshot(request("GET", "/assets/removed.js", Body::empty(), None))
            .await
            .expect("missing asset")
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.clone()
            .oneshot(request("GET", "/.sempre-source.json", Body::empty(), None))
            .await
            .expect("metadata")
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.oneshot(request("GET", "/api/v1/missing", Body::empty(), None))
            .await
            .expect("API")
            .status(),
        StatusCode::NOT_FOUND
    );
}

fn ui_archive() -> Vec<u8> {
    let mut data = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut data);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file(sempre_ui::MANIFEST_NAME, options)
            .expect("manifest");
        zip.write_all(br#"{"schema":1,"name":"Uploaded UI","version":"1","entry":"index.html","api":{"major":1}}"#)
            .expect("manifest data");
        zip.start_file("index.html", options).expect("entry");
        zip.write_all(b"<main>Uploaded UI</main>")
            .expect("entry data");
        zip.finish().expect("finish archive");
    }
    data.into_inner()
}

#[tokio::test]
async fn ui_upload_is_immediately_served_and_can_be_removed() {
    let (_root, app, token) = fixture();
    let mut upload = request(
        "POST",
        "/api/v1/ui/upload",
        Body::from(ui_archive()),
        Some(&token),
    );
    upload.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    assert_eq!(
        app.clone().oneshot(upload).await.expect("upload").status(),
        StatusCode::OK
    );
    let page = app
        .clone()
        .oneshot(request("GET", "/", Body::empty(), None))
        .await
        .expect("page");
    let body = to_bytes(page.into_body(), 4096).await.expect("page body");
    assert_eq!(&body[..], b"<main>Uploaded UI</main>");
    assert_eq!(
        app.clone()
            .oneshot(request("DELETE", "/api/v1/ui", Body::empty(), Some(&token)))
            .await
            .expect("remove")
            .status(),
        StatusCode::NO_CONTENT
    );
    let unavailable = app
        .oneshot(request("GET", "/", Body::empty(), None))
        .await
        .expect("unavailable");
    assert_eq!(unavailable.status(), StatusCode::SERVICE_UNAVAILABLE);
}
