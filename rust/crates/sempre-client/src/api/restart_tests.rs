use axum::{
    body::Body,
    http::{HeaderValue, StatusCode},
};
use tower::ServiceExt;

use super::{
    DAEMON_TOKEN_HEADER, router,
    tests::{request, test_state},
};

#[tokio::test]
async fn restart_task_and_configuration_require_authentication() {
    let root = tempfile::tempdir().unwrap();
    let (state, token) = test_state(&root);
    let app = router(state);
    for path in [
        "/api/v1/runtime/restart",
        "/api/v1/runtime/restart/config?id=missing",
    ] {
        let response = app
            .clone()
            .oneshot(request("GET", path, Body::empty(), "127.0.0.1:1"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let mut authenticated = request("GET", path, Body::empty(), "127.0.0.1:1");
        authenticated
            .headers_mut()
            .insert(DAEMON_TOKEN_HEADER, HeaderValue::from_str(&token).unwrap());
        let response = app.clone().oneshot(authenticated).await.unwrap();
        assert_eq!(
            response.status(),
            if path.ends_with("restart") {
                StatusCode::OK
            } else {
                StatusCode::NOT_FOUND
            }
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn restart_http_response_does_not_wait_for_validation_or_allow_a_second_restart() {
    use axum::body::to_bytes;
    use chrono::Utc;
    use sempre_state::{Installation, Selection};
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

    let root = tempfile::tempdir().unwrap();
    let (state, token) = test_state(&root);
    let layout = state.manager.store().layout();
    let binary = layout.core_binary("sing-box", None, "1.14.0-beta.13");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, "#!/bin/sh\nsleep 1\necho validation-output\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    state
        .manager
        .store()
        .update(|document| {
            document.selected = Some(Selection {
                core: "sing-box".into(),
                repository: None,
                reference: "stable".into(),
            });
            let core = &mut document.core_mut("sing-box").default;
            core.channels
                .insert("stable".into(), "1.14.0-beta.13".into());
            core.installed.insert(
                "1.14.0-beta.13".into(),
                Installation {
                    explicit: false,
                    digest: "a".repeat(64),
                    source: "fixture".into(),
                    installed_at: Utc::now(),
                },
            );
            Ok(())
        })
        .unwrap();
    let app = router(state.clone());
    let authenticated = |method, path| {
        let mut request = request(method, path, Body::empty(), "127.0.0.1:1");
        request
            .headers_mut()
            .insert(DAEMON_TOKEN_HEADER, HeaderValue::from_str(&token).unwrap());
        request
    };
    let response = tokio::time::timeout(
        Duration::from_millis(500),
        app.clone()
            .oneshot(authenticated("POST", "/api/v1/runtime/restart")),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["task"]["state"], "running");
    assert_eq!(body["task"]["config_available"], false);
    for path in [
        "/api/v1/runtime/restart",
        "/api/v1/runtime/start",
        "/api/v1/runtime/stop",
        "/api/v1/runtime/reload",
    ] {
        let response = app
            .clone()
            .oneshot(authenticated("POST", path))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT, "{path}");
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        while !state.manager.restart_task().unwrap().config_available {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let task = state.manager.restart_task().unwrap();
    assert!(
        task.logs
            .iter()
            .any(|entry| entry.message.contains("validation-output"))
    );
    assert!(state.manager.restart_task_config(&task.id).is_some());
}
