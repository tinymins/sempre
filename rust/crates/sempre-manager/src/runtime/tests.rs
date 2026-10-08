use std::{
    fs,
    future::Future,
    path::Path,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
};

use chrono::Utc;
use sempre_core::Adapter;
use sempre_state::{Installation, Layout, Selection, Store};

use super::*;

mod restart;

#[derive(Default)]
struct FakeRunner {
    validations: AtomicUsize,
}

impl VersionRunner for FakeRunner {
    fn version<'a>(
        &'a self,
        _: &'a dyn Adapter,
        _: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<String, ManagerError>> + Send + 'a>> {
        Box::pin(async { Ok("1.14.0-beta.13".into()) })
    }
}

impl ValidationRunner for FakeRunner {
    fn validate<'a>(
        &'a self,
        _: &'a dyn Adapter,
        _: &'a Path,
        _: &'a Path,
        _: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), ManagerError>> + Send + 'a>> {
        Box::pin(async move {
            self.validations.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
    }
}

fn fixture() -> (tempfile::TempDir, Manager<FakeRunner>) {
    let root = tempfile::tempdir().expect("temporary directory");
    let manager = Manager::with_runner(Store::new(Layout::at(root.path())), FakeRunner::default())
        .expect("manager");
    let hash = "a".repeat(64);
    manager
        .store
        .update(|document| {
            document.selected = Some(Selection {
                core: "sing-box".into(),
                repository: None,
                reference: "stable".into(),
            });
            let source = &mut document.core_mut("sing-box").default;
            source.installed.insert(
                "1.14.0-beta.13".into(),
                Installation {
                    explicit: false,
                    digest: "b".repeat(64),
                    source: "https://example.invalid/sing-box.zip".into(),
                    installed_at: Utc::now(),
                },
            );
            source
                .channels
                .insert("stable".into(), "1.14.0-beta.13".into());
            document.configs.insert("sing-box".into(), hash.clone());
            Ok(())
        })
        .expect("seed state");
    let binary = manager
        .store
        .layout()
        .core_binary("sing-box", None, "1.14.0-beta.13");
    fs::create_dir_all(binary.parent().expect("binary parent")).expect("core directory");
    fs::write(binary, b"fixture").expect("core binary");
    let config = manager.store.layout().config("sing-box", &hash);
    fs::create_dir_all(config.parent().expect("config parent")).expect("config directory");
    fs::write(config, b"{}").expect("configuration");
    (root, manager)
}

#[tokio::test]
async fn runtime_actions_stage_start_and_serialize_stop_intent() {
    let (_root, manager) = fixture();
    let initial = manager.runtime_status().expect("status");
    assert_eq!(initial.runtime_state, RuntimeState::Idle);
    assert!(initial.active.is_none() && initial.target.is_some());

    let starting = manager.runtime_action(START).await.expect("start");
    assert_eq!(starting.runtime_state, RuntimeState::Starting);
    assert!(starting.active.is_some() && starting.pending);
    let stopping = manager.runtime_action(STOP).await.expect("stop");
    assert_eq!(stopping.desired_state, DesiredState::Stopped);
    assert_eq!(stopping.runtime_state, RuntimeState::Stopping);
    assert_eq!(
        manager
            .runtime_action(STOP)
            .await
            .expect("stop again")
            .runtime_state,
        RuntimeState::Stopping
    );
}

#[tokio::test]
async fn startup_never_fetches_uncached_user_rule_providers_under_the_operation_lock() {
    let (_root, manager) = fixture();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/custom.yaml", listener.local_addr().unwrap());
    manager
        .subscriptions
        .update(|catalog| {
            let profile = &mut catalog.profiles[0];
            profile
                .extra
                .insert("use_system_rules".into(), serde_json::json!(false));
            profile.editor.rule_list = serde_json::json!({
                "proxy": [{
                    "name": "arbitrary-user-rules",
                    "url": url.clone(),
                    "type": "",
                    "format": ""
                }]
            })
            .to_string();
            Ok(())
        })
        .unwrap();
    manager
        .store
        .update(|document| {
            document.configs.clear();
            Ok(())
        })
        .unwrap();
    let started = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        manager.runtime_action(START),
    )
    .await
    .expect("startup cannot wait for online rules")
    .unwrap();
    assert_eq!(started.runtime_state, RuntimeState::Starting);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    let config: serde_json::Value =
        serde_json::from_str(&manager.current_config().unwrap().content).unwrap();
    assert!(
        config["route"]["rule_set"]
            .as_array()
            .unwrap()
            .iter()
            .any(|rule| rule["tag"] == "arbitrary-user-rules" && rule["url"] == url)
    );
    manager.runtime_action(STOP).await.unwrap();
}
