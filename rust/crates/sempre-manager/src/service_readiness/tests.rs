use serde_json::json;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpListener,
};

use super::*;

const VERSION: &str = "2.0.20-beta.6";

#[tokio::test]
async fn health_gate_requires_matching_version_success_and_api() {
    for (status, reported, api, ready) in [
        (200, VERSION, API_MAJOR, true),
        (200, "2.0.20-beta.2", API_MAJOR, false),
        (503, VERSION, API_MAJOR, false),
        (200, VERSION, API_MAJOR + 1, false),
    ] {
        let root = tempfile::tempdir().unwrap();
        let layout = Layout::at(root.path());
        layout.ensure().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bind = listener.local_addr().unwrap().to_string();
        PublicEndpoint::new(VERSION, &bind, &local_url(&bind).unwrap())
            .unwrap()
            .write(&layout.endpoint)
            .unwrap();
        let body = json!({"status": "ok", "version": reported, "api_major": api}).to_string();
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 1024];
                let length = socket.read(&mut request).await.unwrap();
                assert!(
                    String::from_utf8_lossy(&request[..length]).starts_with("GET /api/v1/health ")
                );
                let response = format!(
                    "HTTP/1.1 {status} Result\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let result = wait_for_health(&layout, VERSION, Duration::from_millis(50)).await;
        server.abort();
        assert_eq!(
            result.is_ok(),
            ready,
            "HTTP {status}, {reported}, API {api}"
        );
    }
}

#[tokio::test]
async fn missing_stale_and_dead_endpoints_cannot_commit_an_install() {
    let root = tempfile::tempdir().unwrap();
    let layout = Layout::at(root.path());
    layout.ensure().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bind = listener.local_addr().unwrap().to_string();
    drop(listener);
    for version in [None, Some("2.0.20-beta.2"), Some(VERSION)] {
        if let Some(version) = version {
            PublicEndpoint::new(version, &bind, &local_url(&bind).unwrap())
                .unwrap()
                .write(&layout.endpoint)
                .unwrap();
        }
        assert!(matches!(
            wait_for_health(&layout, VERSION, Duration::from_millis(30)).await,
            Err(ManagerError::RuntimeNotReady(_))
        ));
    }
}

#[tokio::test]
async fn failed_health_retains_the_old_executable_and_ui_for_rollback() {
    let root = tempfile::tempdir().unwrap();
    let source = Layout::at(&root.path().join("source"));
    let target = Layout::system_at(&root.path().join("target"));
    for layout in [&source, &target] {
        sempre_state::Store::new(layout.clone())
            .initialize()
            .unwrap();
        sempre_control::WebConfigStore::new(&layout.web_config)
            .initialize()
            .unwrap();
        std::fs::create_dir_all(layout.service_executable.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&layout.ui).unwrap();
    }
    std::fs::write(&source.service_executable, b"new executable").unwrap();
    std::fs::write(source.ui.join("index.html"), b"new UI").unwrap();
    std::fs::write(&target.service_executable, b"old executable").unwrap();
    std::fs::write(target.ui.join("index.html"), b"old UI").unwrap();
    sempre_bundle::mark_release_directory(&source.root).unwrap();
    let mut transaction = sempre_bundle::stage_install(&source, &target).unwrap();
    transaction.activate().unwrap();
    assert_eq!(
        std::fs::read(&target.service_executable).unwrap(),
        b"new executable"
    );
    assert!(
        wait_for_health(&target, VERSION, Duration::from_millis(30))
            .await
            .is_err()
    );
    transaction.rollback();
    assert_eq!(
        std::fs::read(&target.service_executable).unwrap(),
        b"old executable"
    );
    assert_eq!(
        std::fs::read(target.ui.join("index.html")).unwrap(),
        b"old UI"
    );
}
