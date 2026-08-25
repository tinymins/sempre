use axum::{
    body::{Body, to_bytes},
    http::{HeaderValue, StatusCode, header},
};
use tower::ServiceExt;

use super::tests::{request, test_state};
use super::*;

#[tokio::test]
async fn subscription_prepare_routes_use_the_manager_boundary() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    let profile_id = state
        .manager
        .subscriptions()
        .read()
        .expect("catalog")
        .profiles[0]
        .id
        .clone();
    let app = router(state);
    for operation in ["refresh", "activate"] {
        let mut prepare = request(
            "POST",
            &format!("/api/v1/subscriptions/{profile_id}/{operation}"),
            Body::empty(),
            "127.0.0.1:1",
        );
        prepare.headers_mut().insert(
            DAEMON_TOKEN_HEADER,
            HeaderValue::from_str(&token).expect("token"),
        );
        let response = app.clone().oneshot(prepare).await.expect("prepare profile");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body");
        let error: serde_json::Value = serde_json::from_slice(&body).expect("error JSON");
        assert_eq!(error["error"]["code"], "SUBSCRIPTION_OPERATION_FAILED");
        assert!(
            error["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("select"))
        );
    }
}

#[tokio::test]
async fn subscription_catalog_supports_authenticated_local_and_remote_creation() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    let app = router(state);
    let mut list = request("GET", "/api/v1/subscriptions", Body::empty(), "127.0.0.1:1");
    list.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.clone().oneshot(list).await.expect("list profiles");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let catalog: serde_json::Value = serde_json::from_slice(&body).expect("catalog JSON");
    assert_eq!(catalog["profiles"].as_array().map(Vec::len), Some(1));
    assert_eq!(catalog["configuration_context"]["key"], "common");
    assert_eq!(catalog["schedule"]["interval"], "24h");
    assert_eq!(
        catalog["defaults"]["groups"].as_array().map(Vec::len),
        Some(24)
    );
    assert_eq!(
        catalog["editor_defaults"]["by_core"]
            .as_object()
            .map(serde_json::Map::len),
        Some(6)
    );

    let mut candidate = catalog["profiles"][0].clone();
    candidate["sources"] = serde_json::json!([{
        "id": "source-1",
        "type": "url",
        "enabled": true,
        "url": "https://offline.example/subscription"
    }]);
    let profile_id = candidate["id"].as_str().expect("profile ID");
    let mut save = request(
        "PUT",
        &format!("/api/v1/subscriptions/{profile_id}"),
        Body::from(serde_json::to_vec(&candidate).expect("candidate JSON")),
        "127.0.0.1:1",
    );
    save.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    save.headers_mut().insert(
        "x-sempre-configuration-context",
        HeaderValue::from_static("common"),
    );
    save.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.clone().oneshot(save).await.expect("save profile");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let saved: serde_json::Value = serde_json::from_slice(&body).expect("save JSON");
    assert_eq!(saved["change"]["Changed"], true);
    assert_eq!(saved["profile"]["revision"], 2);
    assert_eq!(
        saved["profile"]["last_result"],
        "profile saved; runtime configuration needs regeneration"
    );

    let mut create = request(
        "POST",
        "/api/v1/subscriptions",
        Body::from(
            r#"{"name":"Remote","mode":"remote","manifest_url":"https://server.example/share"}"#,
        ),
        "127.0.0.1:1",
    );
    create.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    create.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.oneshot(create).await.expect("create profile");
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let profile: serde_json::Value = serde_json::from_slice(&body).expect("profile JSON");
    assert_eq!(profile["mode"], "remote");
    assert_eq!(
        profile["remote"]["manifest_url"],
        "https://server.example/share"
    );
}

#[tokio::test]
async fn subscription_schedule_patch_persists_validated_settings() {
    let root = tempfile::tempdir().expect("temporary directory");
    let (state, token) = test_state(&root);
    let app = router(Arc::clone(&state));
    let mut patch = request(
        "PATCH",
        "/api/v1/subscription",
        Body::from(r#"{"interval":"12H","auto_restart":false}"#),
        "127.0.0.1:1",
    );
    patch.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    patch.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.clone().oneshot(patch).await.expect("patch schedule");
    assert_eq!(response.status(), StatusCode::OK);
    let document = state.manager.state().expect("state");
    assert_eq!(document.subscription.interval, "12h");
    assert!(!document.subscription_auto_restart);

    let mut invalid = request(
        "PATCH",
        "/api/v1/subscription",
        Body::from(r#"{"interval":"4m"}"#),
        "127.0.0.1:1",
    );
    invalid.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    invalid.headers_mut().insert(
        DAEMON_TOKEN_HEADER,
        HeaderValue::from_str(&token).expect("token"),
    );
    let response = app.oneshot(invalid).await.expect("reject schedule");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        state
            .manager
            .state()
            .expect("unchanged state")
            .subscription
            .interval,
        "12h"
    );
}
