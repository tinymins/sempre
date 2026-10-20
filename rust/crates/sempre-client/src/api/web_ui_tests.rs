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
