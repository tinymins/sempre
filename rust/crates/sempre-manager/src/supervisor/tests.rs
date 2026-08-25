use std::{fs, future::Future, path::Path, pin::Pin, sync::Arc, time::Duration};

use chrono::Utc;
use sempre_core::Adapter;
use sempre_state::{Deployment, Installation, Layout, Selection, Store};

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
async fn early_exit_rolls_back_a_pending_deployment() {
    let (_root, manager) = fixture("#!/bin/sh\nexit 7\n");
    let old_hash = "c".repeat(64);
    let old_config = manager.store.layout().config("sing-box", &old_hash);
    fs::write(old_config, b"{}").expect("old config");
    manager
        .store
        .update(|document| {
            let mut previous = document.active.clone().expect("active");
            previous.config_hash.clone_from(&old_hash);
            document.previous = Some(previous);
            Ok(())
        })
        .expect("previous deployment");
    let (shutdown, receiver) = watch::channel(false);
    let running = Arc::clone(&manager);
    let task = tokio::spawn(async move {
        running
            .run_supervisor_with_grace(receiver, Duration::from_secs(2))
            .await
    });
    wait_until(Duration::from_secs(2), || {
        manager.state().is_ok_and(|document| {
            document
                .active
                .is_some_and(|item| item.config_hash == old_hash)
        })
    })
    .await;
    let document = manager.state().expect("state");
    assert!(!document.pending && document.previous.is_none());
    let failure = document.runtime.last_failure.expect("failure");
    assert_eq!(failure.failed.expect("failed").config_hash, "a".repeat(64));
    assert_eq!(
        failure.rolled_back_to.expect("rollback").config_hash,
        old_hash
    );
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
