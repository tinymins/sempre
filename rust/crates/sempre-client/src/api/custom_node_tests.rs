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
            r#"{"name":"edge","proxy":{"name":"ignored","type":"socks5","server":"edge.example.com","port":1080},"subscription_ids":[]}"#,
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

#[tokio::test]
async fn custom_node_api_saves_batch_links_in_one_request_without_new_storage_fields() {
    let (root, app, token) = fixture();
    let manager = Manager::new(Store::new(Layout::at(root.path()))).unwrap();
    let first = manager.subscriptions().read().unwrap().profiles[0]
        .id
        .clone();
    let second = sempre_subscription::new_profile("second");
    let second_id = second.id.clone();
    manager
        .subscriptions()
        .update(|catalog| {
            catalog.profiles.push(second);
            Ok(())
        })
        .unwrap();
    let mut input = serde_json::json!({
        "name": "shared", "proxy": {"name": "shared", "type": "socks5", "server": "edge.example.com", "port": 1080},
        "subscription_ids": [first, second_id]
    });
    let response = call(
        app.clone(),
        &token,
        "POST",
        "/api/v1/custom-nodes",
        Body::from(input.to_string()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let node: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    let id = node["id"].as_str().unwrap();
    assert!(node.get("subscription_ids").is_none());
    let catalog = manager.subscriptions().read().unwrap();
    assert!(
        catalog
            .profiles
            .iter()
            .all(|profile| profile.custom_node_ids == [id])
    );
    let path = format!("/api/v1/custom-nodes/{id}");

    input["subscription_ids"] = serde_json::json!(["missing"]);
    input["name"] = serde_json::json!("must not be saved");
    let before = std::fs::read(&manager.store().layout().subscription_catalog).unwrap();
    let response = call(
        app.clone(),
        &token,
        "PUT",
        &path,
        Body::from(input.to_string()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        std::fs::read(&manager.store().layout().subscription_catalog).unwrap(),
        before
    );

    input["subscription_ids"] = serde_json::json!([]);
    input["name"] = serde_json::json!("shared");
    let response = call(app, &token, "PUT", &path, Body::from(input.to_string())).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        manager
            .subscriptions()
            .read()
            .unwrap()
            .profiles
            .iter()
            .all(|profile| profile.custom_node_ids.is_empty())
    );
}

#[tokio::test]
async fn single_user_creation_defaults_link_both_directions_and_skip_remote_profiles() {
    let (root, app, token) = fixture();
    for input in [
        serde_json::json!({ "name": "local" }),
        serde_json::json!({ "name": "remote", "mode": "remote", "manifest_url": "https://example.com/manifest" }),
    ] {
        let response = call(
            app.clone(),
            &token,
            "POST",
            "/api/v1/subscriptions",
            Body::from(input.to_string()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    let response = call(
        app.clone(), &token, "POST", "/api/v1/custom-nodes",
        Body::from(r#"{"name":"edge","proxy":{"name":"edge","type":"socks5","server":"edge.example.com","port":1080}}"#),
    ).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let node: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    let id = node["id"].as_str().unwrap();
    let manager = Manager::new(Store::new(Layout::at(root.path()))).unwrap();
    let catalog = manager.subscriptions().read().unwrap();
    for profile in &catalog.profiles {
        assert_eq!(
            profile.custom_node_ids.contains(&id.to_owned()),
            profile.name != "remote"
        );
    }
    for input in [
        serde_json::json!({ "name": "after-node" }),
        serde_json::json!({ "name": "remote-after-node", "mode": "remote", "manifest_url": "https://example.com/manifest" }),
    ] {
        let response = call(
            app.clone(),
            &token,
            "POST",
            "/api/v1/subscriptions",
            Body::from(input.to_string()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let profile: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
        let expected = if input["mode"] == "remote" {
            serde_json::json!([])
        } else {
            serde_json::json!([id])
        };
        assert_eq!(profile["custom_node_ids"], expected);
        let stored = manager
            .subscriptions()
            .read()
            .unwrap()
            .profiles
            .into_iter()
            .find(|item| item.id == profile["id"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            serde_json::to_value(stored.custom_node_ids).unwrap(),
            expected
        );
    }
}
