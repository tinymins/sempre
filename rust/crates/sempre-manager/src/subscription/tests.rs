use std::{
    future::Future,
    path::Path,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
};

use chrono::Utc;
use sempre_converter::Source;
use sempre_core::Adapter;
use sempre_state::{Installation, Layout, Selection, Store};
use serde_json::{Map, Value, json};

use super::*;

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
        Box::pin(async { Ok("1.13.2".into()) })
    }
}

impl ValidationRunner for FakeRunner {
    fn validate<'a>(
        &'a self,
        _: &'a dyn Adapter,
        _: &'a Path,
        config: &'a Path,
        _: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), ManagerError>> + Send + 'a>> {
        Box::pin(async move {
            assert!(config.is_file());
            self.validations.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
    }
}

fn fixture() -> (tempfile::TempDir, Manager<FakeRunner>, String) {
    let root = tempfile::tempdir().expect("temporary directory");
    let manager = Manager::with_runner(Store::new(Layout::at(root.path())), FakeRunner::default())
        .expect("manager");
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
                    digest: "a".repeat(64),
                    source: "https://example.invalid/sing-box.zip".into(),
                    installed_at: Utc::now(),
                },
            );
            source.channels.insert("stable".into(), "1.13.2".into());
            Ok(())
        })
        .expect("seed core");
    let mut profile_id = String::new();
    manager
        .subscriptions
        .update(|catalog| {
            let profile = &mut catalog.profiles[0];
            profile_id.clone_from(&profile.id);
            profile.sources.push(raw_source());
            Ok(())
        })
        .expect("seed subscription");
    (root, manager, profile_id)
}

fn raw_source() -> Source {
    Source {
        id: "source-1".into(),
        kind: "raw".into(),
        enabled: true,
        url: String::new(),
        remark: "fixture".into(),
        prefix: String::new(),
        content: "trojan://secret@example.com:443#edge".into(),
        user_agent: String::new(),
        extra: Map::new(),
    }
}

#[tokio::test]
async fn refresh_validates_inactive_profile_without_deploying_it() {
    let (_root, manager, profile_id) = fixture();
    let (change, render) = manager
        .refresh_subscription_profile(&profile_id)
        .await
        .expect("refresh");
    assert!(change.changed && !change.needs_restart);
    assert!(render.runtime_validated);
    assert!(render.format.starts_with("sing-box-v13"));
    assert_eq!(manager.runner.validations.load(Ordering::Relaxed), 1);
    let document = manager.state().expect("state");
    assert!(document.active.is_none());
    assert!(document.active_profile_id.is_none());
    let profile = &manager.subscriptions.read().expect("catalog").profiles[0];
    assert_eq!(profile.extra["last_runtime_validated"], json!(true));
    assert_eq!(
        profile.sources[0].extra["last_status"],
        Value::String("raw content".into())
    );
}

#[tokio::test]
async fn activation_switches_profile_and_stages_validated_configuration() {
    let (_root, manager, profile_id) = fixture();
    let (change, render) = manager
        .activate_subscription_profile(&profile_id)
        .await
        .expect("activate");
    assert!(change.changed && change.needs_restart);
    let document = manager.state().expect("state");
    assert_eq!(
        document.active_profile_id.as_deref(),
        Some(profile_id.as_str())
    );
    assert_eq!(document.config_builds["sing-box"].profile_id, profile_id);
    assert_eq!(
        document.active.expect("deployment").config_hash,
        render.artifact_hash
    );
    assert_eq!(
        document.subscription.last_result.as_deref(),
        Some("configuration updated")
    );
}

#[tokio::test]
async fn switching_to_equivalent_profile_is_a_change_without_restart() {
    let (_root, manager, first_id) = fixture();
    let second_id = sempre_subscription::new_profile("second").id;
    manager
        .subscriptions
        .update(|catalog| {
            let mut second = catalog.profiles[0].clone();
            second.id.clone_from(&second_id);
            second.name = "second".into();
            catalog.profiles.push(second);
            Ok(())
        })
        .expect("add equivalent profile");
    manager
        .activate_subscription_profile(&first_id)
        .await
        .expect("activate first");
    let (change, _) = manager
        .activate_subscription_profile(&second_id)
        .await
        .expect("activate second");
    assert!(change.changed);
    assert!(!change.needs_restart);
    assert_eq!(
        manager.state().expect("state").active_profile_id.as_deref(),
        Some(second_id.as_str())
    );
}

#[tokio::test]
async fn runtime_validation_is_deferred_until_refresh() {
    let (_root, manager, profile_id) = fixture();
    manager
        .subscriptions
        .update(|catalog| {
            catalog.profiles[0]
                .transparent_proxy
                .tun
                .as_object_mut()
                .expect("tun object")
                .insert("interface_name".into(), json!(" "));
            Ok(())
        })
        .expect("save invalid runtime input");
    let error = manager
        .refresh_subscription_profile(&profile_id)
        .await
        .expect_err("refresh rejects invalid runtime");
    assert!(error.to_string().contains("TUN interface name"));
    assert_eq!(manager.runner.validations.load(Ordering::Relaxed), 0);
}
