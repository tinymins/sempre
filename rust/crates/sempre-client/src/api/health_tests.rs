use std::fs;

use axum::{
    body::{Body, to_bytes},
    http::StatusCode,
};
use serde_json::json;
use tower::ServiceExt as _;

use super::{
    router,
    tests::{request, test_state},
};

#[tokio::test]
async fn public_health_identifies_the_running_service_and_installed_ui() {
    let root = tempfile::tempdir().unwrap();
    let (state, _) = test_state(&root);
    let current = state.manager.store().layout().ui.join("current");
    fs::create_dir_all(&current).unwrap();
    fs::write(current.join("index.html"), "UI").unwrap();
    fs::write(current.join(sempre_ui::METADATA_NAME), serde_json::to_vec(&json!({
        "manifest": { "schema": 1, "name": "sempre-ui", "version": "2.0.12", "entry": "index.html", "api": { "major": 1 } },
        "source_type": "local", "source": "private-install-path", "sha256": "a".repeat(64), "installed_at": "2026-09-08T00:00:00Z"
    })).unwrap()).unwrap();
    let app = router(state);
    let response = app
        .clone()
        .oneshot(request(
            "GET",
            "/api/v1/health",
            Body::empty(),
            "10.23.0.1:1234",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["version"], crate::VERSION);
    assert_eq!(
        body["ui"],
        json!({ "id": "a".repeat(64), "version": "2.0.12" })
    );
    fs::remove_file(current.join("index.html")).unwrap();
    let response = app
        .oneshot(request(
            "GET",
            "/api/v1/health",
            Body::empty(),
            "10.23.0.1:1234",
        ))
        .await
        .unwrap();
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["ui"], serde_json::Value::Null);
}
