#![cfg(unix)]

use std::{fs, future::Future, net::TcpListener, path::Path, pin::Pin, sync::Arc, time::Duration};

use chrono::Utc;
use sempre_core::Adapter;
use sempre_state::{ConfigBuild, Deployment, Installation, Layout, Selection, Store};

use super::*;

#[derive(Default)]
struct FakeRunner;

impl VersionRunner for FakeRunner {
    fn version<'a>(
        &'a self,
        _: &'a dyn Adapter,
        _: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<String, ManagerError>> + Send + 'a>> {
        Box::pin(async { Ok("1.13.2".into()) })
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
        Box::pin(async { Ok(()) })
    }
}

fn fixture(script: &str) -> (tempfile::TempDir, Arc<Manager<FakeRunner>>) {
    let root = tempfile::tempdir().expect("temporary directory");
    let manager = Arc::new(
        Manager::with_runner(Store::new(Layout::at(root.path())), FakeRunner).expect("manager"),
    );
    // The shell fixture has no DNS listener or system networking support.
    manager
        .dns_settings
        .replace(crate::DnsSettings {
            enabled: false,
            ..manager.dns_settings.read()
        })
        .expect("disable fixture DNS frontend");
    let socks_listener = TcpListener::bind(("127.0.0.1", 0)).expect("free SOCKS port");
    let http_listener = TcpListener::bind(("127.0.0.1", 0)).expect("free HTTP port");
    let socks_port = socks_listener.local_addr().expect("SOCKS address").port();
    let http_port = http_listener.local_addr().expect("HTTP address").port();
    manager
        .subscriptions
        .update(|catalog| {
            catalog.profiles[0].transparent_proxy.mode = "disabled".into();
            catalog.profiles[0].local_proxy.socks_port = socks_port;
            catalog.profiles[0].local_proxy.http_port = http_port;
            Ok(())
        })
        .expect("disable transparent proxy");
    drop((socks_listener, http_listener));
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
                "1.13.2".into(),
                Installation {
                    explicit: false,
                    digest: "b".repeat(64),
                    source: "https://example.invalid/sing-box.zip".into(),
                    installed_at: Utc::now(),
                },
            );
            source.channels.insert("stable".into(), "1.13.2".into());
            document.configs.insert("sing-box".into(), hash.clone());
            document.active = Some(Deployment {
                core: "sing-box".into(),
                repository: None,
                reference: "stable".into(),
                version: "1.13.2".into(),
                config_hash: hash.clone(),
            });
            document.pending = true;
            document.pending_config_fields = vec![sempre_state::PendingConfigField::Dns];
            Ok(())
        })
        .expect("seed state");
    let config = manager.store.layout().config("sing-box", &hash);
    fs::create_dir_all(config.parent().expect("config parent")).expect("config directory");
    fs::write(config, b"{}").expect("configuration");
    let binary = manager
        .store
        .layout()
        .core_binary("sing-box", None, "1.13.2");
    fs::create_dir_all(binary.parent().expect("binary parent")).expect("binary directory");
    fs::write(&binary, script).expect("script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(binary, fs::Permissions::from_mode(0o700)).expect("executable");
    }
    (root, manager)
}

#[test]
fn transparent_cleanup_requires_owned_runtime_evidence() {
    let mut document = sempre_state::Document::default();
    assert!(!transparent_cleanup_required(&document));

    document.runtime.core = Some("sing-box".into());
    assert!(transparent_cleanup_required(&document));
}

#[cfg(unix)]
#[tokio::test]
async fn supervisor_starts_commits_and_stops_the_real_process() {
    let (_root, manager) =
        fixture("#!/bin/sh\ntrap 'exit 0' TERM\necho core-started\nwhile :; do sleep 1; done\n");
    let (shutdown, receiver) = watch::channel(false);
    let running = Arc::clone(&manager);
    let task = tokio::spawn(async move {
        running
            .run_supervisor_with_grace(receiver, Duration::from_millis(50))
            .await
    });
    wait_for_state(&manager, sempre_state::RuntimeState::Running).await;
    let document = manager.state().expect("state");
    assert!(!document.pending);
    assert!(document.pending_config_fields.is_empty());
    assert!(document.runtime.pid.is_some());
    wait_until(Duration::from_secs(2), || {
        fs::read(&manager.store.layout().core_stdout_log)
            .is_ok_and(|data| String::from_utf8_lossy(&data).contains("core-started"))
    })
    .await;
    manager.runtime_action("stop").await.expect("stop action");
    wait_for_state(&manager, sempre_state::RuntimeState::Stopped).await;
    assert!(
        String::from_utf8_lossy(
            &fs::read(&manager.store.layout().core_stdout_log).expect("stdout")
        )
        .contains("core-started")
    );
    shutdown.send(true).expect("shutdown");
    task.await.expect("task").expect("supervisor");
}

#[cfg(unix)]
#[tokio::test]
async fn early_exit_keeps_the_pending_deployment() {
    let (_root, manager) = fixture("#!/bin/sh\nexit 7\n");
    let profiles = manager
        .subscriptions
        .update(|catalog| {
            catalog.profiles[0].transparent_proxy.mode = "disabled".into();
            let mut candidate = sempre_subscription::new_profile("candidate");
            candidate.transparent_proxy.mode = "disabled".into();
            catalog.profiles.push(candidate);
            Ok(())
        })
        .expect("profiles");
    let candidate_profile_id = profiles.profiles[1].id.clone();
    manager
        .store
        .update(|document| {
            document.config_builds.insert(
                "sing-box".into(),
                ConfigBuild {
                    profile_id: candidate_profile_id.clone(),
                    profile_revision: 2,
                    target_key: "sing-box|13|default".into(),
                    runtime_key: Some("candidate-runtime".into()),
                    private_access_policy: serde_json::json!({ "enabled": false, "connectors": [] }),
                },
            );
            document.active_profile_id = Some(candidate_profile_id.clone());
            Ok(())
        })
        .expect("candidate deployment");
    let (shutdown, receiver) = watch::channel(false);
    let running = Arc::clone(&manager);
    let task = tokio::spawn(async move {
        running
            .run_supervisor_with_grace(receiver, Duration::from_secs(2))
            .await
    });
    wait_until(Duration::from_secs(4), || {
        manager
            .state()
            .is_ok_and(|document| document.runtime.last_failure.is_some())
    })
    .await;
    let document = manager.state().expect("state");
    assert!(document.pending);
    assert_eq!(
        document.pending_config_fields,
        vec![sempre_state::PendingConfigField::Dns]
    );
    assert_eq!(
        document.config_builds["sing-box"].profile_id,
        candidate_profile_id
    );
    assert_eq!(
        document.active_profile_id.as_deref(),
        Some(candidate_profile_id.as_str())
    );
    let failure = document.runtime.last_failure.expect("failure");
    assert_eq!(failure.failed.expect("failed").config_hash, "a".repeat(64));
    shutdown.send(true).expect("shutdown");
    task.await.expect("task").expect("supervisor");
}

async fn wait_for_state(manager: &Manager<FakeRunner>, state: sempre_state::RuntimeState) {
    wait_until(Duration::from_secs(3), || {
        manager
            .state()
            .is_ok_and(|document| document.runtime.state == state)
    })
    .await;
}

async fn wait_until(timeout: Duration, condition: impl Fn() -> bool) {
    let started = std::time::Instant::now();
    while !condition() {
        assert!(started.elapsed() < timeout, "condition timed out");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn stalled_rule_download_keeps_basic_core_running_and_does_not_block_stop() {
    let (_root, manager) = fixture("#!/bin/sh\ntrap 'exit 0' TERM\nwhile :; do sleep 1; done\n");
    let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = serde_json::json!({
        "inbounds":[{"type":"http","listen":"127.0.0.1","listen_port":proxy.local_addr().unwrap().port()}],
        "route":{
            "rule_set":[{"tag":"user","type":"remote","format":"source","url":"http://user.invalid/rules"}],
            "rules":[{"rule_set":["user"],"outbound":"proxy"}],"final":"proxy"
        }
    });
    fs::write(
        manager.store.layout().config("sing-box", &"a".repeat(64)),
        config.to_string(),
    )
    .unwrap();
    let (shutdown, receiver) = watch::channel(false);
    let running = Arc::clone(&manager);
    let task = tokio::spawn(async move {
        running
            .run_supervisor_with_grace(receiver, Duration::from_millis(50))
            .await
    });
    wait_for_state(&manager, sempre_state::RuntimeState::Running).await;
    let (_connection, _) = tokio::time::timeout(Duration::from_secs(2), proxy.accept())
        .await
        .unwrap()
        .unwrap();
    let runtime = manager.state().unwrap().runtime;
    let config: serde_json::Value =
        serde_json::from_slice(&fs::read(runtime.runtime_config.unwrap()).unwrap()).unwrap();
    assert_eq!(config["route"]["rule_set"], serde_json::json!([]));
    assert!(runtime.pid.is_some());
    tokio::time::timeout(Duration::from_secs(1), manager.runtime_action("stop"))
        .await
        .expect("stop cannot wait for rule download")
        .unwrap();
    wait_for_state(&manager, sempre_state::RuntimeState::Stopped).await;
    shutdown.send(true).unwrap();
    task.await.unwrap().unwrap();
    assert!(
        manager
            .fetcher
            .cached_rule_set("http://user.invalid/rules", "source")
            .unwrap()
            .is_none()
    );
}
