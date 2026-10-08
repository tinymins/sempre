use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use serde_json::Value;
use tokio::sync::{oneshot, watch};

use super::*;

#[tokio::test]
async fn migration_failure_never_reports_service_running() {
    let root = tempfile::tempdir().unwrap();
    let layout = Layout::at(root.path());
    layout.ensure().unwrap();
    let document = crate::traffic_history_migrations::tests::beta2_document("unknown-checksum");
    fs::write(
        &layout.traffic_history,
        serde_json::to_vec(&document).unwrap(),
    )
    .unwrap();
    let reported = Arc::new(AtomicBool::new(false));
    let ready_reported = Arc::clone(&reported);
    let result = run_with_layout(
        layout.clone(),
        Some("127.0.0.1:33211"),
        None,
        Some(Box::new(move || {
            ready_reported.store(true, Ordering::SeqCst);
            Ok(())
        })),
    )
    .await;
    assert!(matches!(
        result,
        Err(ClientError::TrafficHistory(
            crate::traffic_history::TrafficError::Migration(
                sempre_state::MigrationError::ChecksumDrift { .. }
            )
        ))
    ));
    assert!(!reported.load(Ordering::SeqCst));
    assert!(!layout.endpoint.exists());
}

#[tokio::test]
async fn beta2_history_reaches_web_health_before_clean_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let layout = Layout::at(root.path());
    layout.ensure().unwrap();
    let document = crate::traffic_history_migrations::tests::beta2_document(
        "08dc71a5c51739768636910627db9229078a5cb173bdbfdad027dbd0539a4ef3",
    );
    fs::write(
        &layout.traffic_history,
        serde_json::to_vec(&document).unwrap(),
    )
    .unwrap();
    let (stop, receiver) = watch::channel(false);
    let (ready_sender, ready_receiver) = oneshot::channel();
    let ready_layout = layout.clone();
    let reservation = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen = reservation.local_addr().unwrap().to_string();
    drop(reservation);
    let task_layout = layout.clone();
    let task = tokio::spawn(async move {
        run_with_layout(
            task_layout,
            Some(&listen),
            Some(receiver),
            Some(Box::new(move || {
                let endpoint = PublicEndpoint::read(&ready_layout.endpoint).unwrap();
                ready_sender.send(endpoint).unwrap();
                Ok(())
            })),
        )
        .await
    });
    let endpoint = tokio::time::timeout(Duration::from_secs(5), ready_receiver)
        .await
        .expect("daemon readiness timeout");
    let Ok(endpoint) = endpoint else {
        panic!("daemon exited before readiness: {:?}", task.await);
    };
    let health: Value = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap()
        .get(format!("{}/api/v1/health", endpoint.local_url))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    stop.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(health["status"], "ok");
    assert_eq!(health["version"], VERSION);
    assert!(!layout.endpoint.exists());
}
