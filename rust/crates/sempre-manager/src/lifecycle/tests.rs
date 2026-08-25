use std::{
    future::Future,
    path::Path,
    pin::Pin,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use chrono::Utc;
use sempre_core::Adapter;
use sempre_state::{Deployment, Installation, Layout, Store};

use super::*;

#[derive(Default)]
struct FakeRunner {
    validation_calls: AtomicUsize,
    reject: AtomicBool,
}

impl VersionRunner for FakeRunner {
    fn version<'a>(
        &'a self,
        _: &'a dyn Adapter,
        _: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<String, ManagerError>> + Send + 'a>> {
        Box::pin(async { Ok("1.2.3".into()) })
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
            self.validation_calls.fetch_add(1, Ordering::Relaxed);
            if self.reject.load(Ordering::Relaxed) {
                Err(ManagerError::ValidationCommand {
                    core: "sing-box".into(),
                    status: "1".into(),
                    output: "invalid".into(),
                })
            } else {
                Ok(())
            }
        })
    }
}

fn fixture() -> (tempfile::TempDir, Manager<FakeRunner>) {
    let root = tempfile::tempdir().expect("temporary directory");
    let store = Store::new(Layout::at(root.path()));
    let manager = Manager::with_runner(store, FakeRunner::default()).expect("manager");
    manager
        .store
        .update(|document| {
            let source = &mut document.core_mut("sing-box").default;
            source.installed.insert(
                "1.2.3".into(),
                Installation {
                    explicit: false,
                    digest: "a".repeat(64),
                    source: "https://example.invalid/sing-box.zip".into(),
                    installed_at: Utc::now(),
                },
            );
            source.channels.insert("stable".into(), "1.2.3".into());
            Ok(())
        })
        .expect("seed installation");
    let directory = manager
        .store
        .layout()
        .core_version_dir("sing-box", None, "1.2.3");
    fs::create_dir_all(&directory).expect("core directory");
    fs::write(directory.join("sing-box"), b"binary").expect("core binary");
    (root, manager)
}

#[tokio::test]
async fn exact_selection_promotes_install_and_stages_existing_configuration() {
    let (_root, manager) = fixture();
    manager
        .store
        .update(|document| {
            document.configs.insert("sing-box".into(), "b".repeat(64));
            Ok(())
        })
        .expect("config state");
    let change = manager
        .select_core("sing-box@1.2.3")
        .await
        .expect("select core");
    assert!(change.changed && change.needs_restart);
    assert_eq!(manager.runner.validation_calls.load(Ordering::Relaxed), 1);
    let document = manager.state().expect("state");
    assert!(document.cores["sing-box"].default.installed["1.2.3"].explicit);
    assert_eq!(document.selected.expect("selection").reference, "1.2.3");
    assert_eq!(
        document.active.expect("deployment").config_hash,
        "b".repeat(64)
    );
    assert!(document.pending);
}

#[tokio::test]
async fn rejected_configuration_preserves_selection_and_deployment() {
    let (_root, manager) = fixture();
    manager.runner.reject.store(true, Ordering::Relaxed);
    manager
        .store
        .update(|document| {
            document.configs.insert("sing-box".into(), "b".repeat(64));
            Ok(())
        })
        .expect("config state");
    assert!(manager.select_core("sing-box@stable").await.is_err());
    let document = manager.state().expect("state");
    assert!(document.selected.is_none());
    assert!(document.active.is_none());
}

#[tokio::test]
async fn selection_without_configuration_waits_without_staging() {
    let (_root, manager) = fixture();
    let change = manager
        .select_core("sing-box@stable")
        .await
        .expect("select core");
    assert!(change.changed && !change.needs_restart);
    assert!(change.current_detail.contains("waiting for configuration"));
    let document = manager.state().expect("state");
    assert_eq!(document.selected.expect("selection").reference, "stable");
    assert!(document.active.is_none());
}

#[test]
fn removal_deletes_version_and_channel_aliases_transactionally() {
    let (_root, manager) = fixture();
    let directory = manager
        .store
        .layout()
        .core_version_dir("sing-box", None, "1.2.3");
    let change = manager.remove_core("sing-box@1.2.3").expect("remove core");
    assert!(change.changed);
    assert!(!directory.exists());
    assert!(
        !manager
            .state()
            .expect("state")
            .cores
            .contains_key("sing-box")
    );
}

#[test]
fn removal_rejects_selected_active_and_rollback_versions() {
    for usage in ["selected", "active", "rollback"] {
        let (_root, manager) = fixture();
        manager
            .store
            .update(|document| {
                let deployment = Deployment {
                    core: "sing-box".into(),
                    repository: None,
                    reference: "stable".into(),
                    version: "1.2.3".into(),
                    config_hash: "b".repeat(64),
                };
                match usage {
                    "selected" => {
                        document.selected = Some(sempre_state::Selection {
                            core: "sing-box".into(),
                            repository: None,
                            reference: "stable".into(),
                        });
                    }
                    "active" => document.active = Some(deployment),
                    _ => document.previous = Some(deployment),
                }
                Ok(())
            })
            .expect("referenced state");
        assert!(matches!(
            manager.remove_core("sing-box@1.2.3"),
            Err(ManagerError::CoreInUse { .. })
        ));
    }
}
