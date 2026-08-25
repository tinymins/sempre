use std::sync::Arc;

use axum::{Json, Router, extract::State, routing::get};
use serde_json::json;

use super::*;
use crate::new_profile;

#[derive(Clone)]
struct TestState {
    base: String,
    content: String,
}

#[tokio::test]
async fn verifies_same_origin_remote_artifact_and_updates_metadata() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let content = "{\"outbounds\":[]}".to_owned();
    let state = Arc::new(TestState {
        base: format!("http://{address}"),
        content: content.clone(),
    });
    let app = Router::new()
        .route(
            "/manifest",
            get(|State(state): State<Arc<TestState>>| async move {
                Json(json!({
                    "schema": 1,
                    "service": "sempre",
                    "profile": {
                        "name": "Server profile",
                        "revision": 4,
                        "updated_at": Utc::now()
                    },
                    "target": Target::parse("sing-box-v13").expect("target"),
                    "artifact": {
                        "url": format!("{}/artifact", state.base),
                        "sha256": format!("{:x}", Sha256::digest(state.content.as_bytes())),
                        "node_count": 3,
                        "created_at": Utc::now()
                    },
                    "edit_url": format!("{}/edit", state.base),
                    "read_only": true
                }))
            }),
        )
        .route(
            "/artifact",
            get(|State(state): State<Arc<TestState>>| async move { state.content.clone() }),
        )
        .with_state(state);
    let server = tokio::spawn(async move { axum::serve(listener, app).await });

    let mut profile = new_profile("Remote");
    profile.extra.insert("mode".into(), json!("remote"));
    profile.extra.insert(
        "remote".into(),
        json!({ "manifest_url": format!("http://{address}/manifest") }),
    );
    let target = Target::parse("sing-box-v13").expect("target");
    let result = RemoteClient::new()
        .expect("client")
        .render(&profile, &target)
        .await
        .expect("remote render");
    assert_eq!(result.content, content);
    assert_eq!(result.node_count, 3);
    assert_eq!(result.artifact_hash.len(), 64);
    assert_eq!(result.profile.extra["remote"]["server_revision"], 4);
    server.abort();
}

#[tokio::test]
async fn rejects_manifest_hash_mismatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let app = Router::new()
        .route(
            "/manifest",
            get(move || async move {
                Json(json!({
                    "schema": 1, "service": "sempre",
                    "profile": { "name": "Remote", "revision": 1, "updated_at": Utc::now() },
                    "target": Target::parse("sing-box-v13").expect("target"),
                    "artifact": { "url": format!("http://{address}/artifact"), "sha256": "a".repeat(64), "node_count": 1, "created_at": Utc::now() },
                    "edit_url": "", "read_only": true
                }))
            }),
        )
        .route("/artifact", get(|| async { "different" }));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let mut profile = new_profile("Remote");
    profile.extra.insert("mode".into(), json!("remote"));
    profile.extra.insert(
        "remote".into(),
        json!({ "manifest_url": format!("http://{address}/manifest") }),
    );
    let error = RemoteClient::new()
        .expect("client")
        .render(&profile, &Target::parse("sing-box-v13").expect("target"))
        .await
        .expect_err("hash mismatch");
    assert!(error.to_string().contains("SHA-256"));
    server.abort();
}
