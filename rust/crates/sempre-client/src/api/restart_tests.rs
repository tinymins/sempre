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
async fn startup_task_and_configuration_require_authentication() {
    let root = tempfile::tempdir().unwrap();
    let (state, token) = test_state(&root);
    let app = router(state);
    for path in [
        "/api/v1/runtime/startup-task",
        "/api/v1/runtime/startup-task/config?id=missing",
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
            if path.ends_with("startup-task") {
                StatusCode::OK
            } else {
                StatusCode::NOT_FOUND
            }
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn restart_http_response_stages_without_prevalidation_or_allowing_a_second_restart() {
    use axum::body::to_bytes;
    use chrono::Utc;
    use sempre_state::{DesiredState, Installation, Selection};
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

    let root = tempfile::tempdir().unwrap();
    let (state, token) = test_state(&root);
    let layout = state.manager.store().layout();
    let binary = layout.core_binary("sing-box", None, "1.14.0-beta.13");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(
        &binary,
        "#!/bin/sh\ntouch \"$0.validation-called\"\nsleep 1\necho validation-output\n",
    )
    .unwrap();
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
    let preparation = state.manager.store().acquire_operation().unwrap();
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
        "/api/v1/runtime/reload",
    ] {
        let response = app
            .clone()
            .oneshot(authenticated("POST", path))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT, "{path}");
    }
    let response = tokio::time::timeout(
        Duration::from_millis(500),
        app.clone()
            .oneshot(authenticated("POST", "/api/v1/runtime/stop")),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        state.manager.state().unwrap().desired_state,
        DesiredState::Stopped
    );
    drop(preparation);
    // Let the queued preparation resume after Stop has replaced its task.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let task = state.manager.startup_task().unwrap();
    assert_eq!(task.action, "stop");
    assert_eq!(
        state.manager.state().unwrap().desired_state,
        DesiredState::Stopped
    );
    assert!(!binary.with_file_name("sing-box.validation-called").exists());
    assert!(
        task.logs
            .iter()
            .all(|entry| !entry.message.contains("validation-output"))
    );
}

#[tokio::test]
async fn update_task_ids_do_not_authorize_api_requests() {
    let root = tempfile::tempdir().unwrap();
    let (state, _) = test_state(&root);
    let task = state.service_updates.begin().unwrap();
    let app = router(state.clone());
    for (method, path, receipt, expected) in [
        (
            "GET",
            "/api/v1/service/update/task",
            task.id.as_str(),
            StatusCode::UNAUTHORIZED,
        ),
        (
            "GET",
            "/api/v1/service/update/task",
            "wrong-receipt",
            StatusCode::UNAUTHORIZED,
        ),
        (
            "POST",
            "/api/v1/service/update",
            task.id.as_str(),
            StatusCode::UNAUTHORIZED,
        ),
        (
            "GET",
            "/api/v1/system",
            task.id.as_str(),
            StatusCode::UNAUTHORIZED,
        ),
    ] {
        let mut req = request(method, path, Body::empty(), "127.0.0.1:1");
        req.headers_mut().insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {receipt}")).unwrap(),
        );
        assert_eq!(app.clone().oneshot(req).await.unwrap().status(), expected);
    }
    state.service_updates.fail(&task.id, "installer failed");
    state.service_updates.begin().unwrap();
    let mut req = request(
        "GET",
        "/api/v1/service/update/task",
        Body::empty(),
        "127.0.0.1:1",
    );
    req.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", task.id)).unwrap(),
    );
    assert_eq!(
        app.oneshot(req).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
}
