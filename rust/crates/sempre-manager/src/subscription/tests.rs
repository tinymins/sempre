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
    let (_root, manager, active_profile_id) = fixture();
    let inactive_profile_id = sempre_subscription::new_profile("inactive").id;
    manager
        .subscriptions
        .update(|catalog| {
            let mut inactive = catalog.profiles[0].clone();
            inactive.id.clone_from(&inactive_profile_id);
            inactive.name = "inactive".into();
            catalog.profiles.push(inactive);
            Ok(())
        })
        .expect("add inactive profile");
    let (change, render) = manager
        .refresh_subscription_profile(&inactive_profile_id)
        .await
        .expect("refresh");
    assert!(change.changed && !change.needs_restart);
    assert!(render.runtime_validated);
    assert!(render.format.starts_with("sing-box-v13"));
    assert_eq!(manager.runner.validations.load(Ordering::Relaxed), 1);
    let document = manager.state().expect("state");
    assert!(document.active.is_none());
    assert_eq!(
        document.active_profile_id.as_deref(),
        Some(active_profile_id.as_str())
    );
    let catalog = manager.subscriptions.read().expect("catalog");
    let profile = catalog
        .profiles
        .iter()
        .find(|profile| profile.id == inactive_profile_id)
        .expect("inactive profile");
    assert_eq!(profile.extra["last_runtime_validated"], json!(true));
    assert_eq!(
        profile.sources[0].extra["last_status"],
        Value::String("raw content".into())
    );
}

#[test]
fn initialization_activates_the_default_profile_and_repairs_a_stale_selection() {
    let root = tempfile::tempdir().expect("temporary directory");
    let manager = Manager::with_runner(Store::new(Layout::at(root.path())), FakeRunner::default())
        .expect("manager");
    let profile_id = manager.subscriptions.read().expect("catalog").profiles[0]
        .id
        .clone();
    assert_eq!(
        manager.state().expect("state").active_profile_id.as_deref(),
        Some(profile_id.as_str())
    );

    manager
        .store
        .update(|document| {
            document.active_profile_id = Some("missing-profile".into());
            Ok(())
        })
        .expect("store stale selection");
    let repaired = Manager::with_runner(Store::new(Layout::at(root.path())), FakeRunner::default())
        .expect("reopen manager");
    assert_eq!(
        repaired
            .state()
            .expect("state")
            .active_profile_id
            .as_deref(),
        Some(profile_id.as_str())
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
async fn import_appends_raw_source_and_bootstraps_the_active_profile() {
    let (_root, manager, profile_id) = fixture();
    manager
        .subscriptions
        .update(|catalog| {
            catalog.profiles[0].sources.clear();
            Ok(())
        })
        .expect("clear fixture source");
    let change = manager
        .import_subscription_source(
            "subscription.yaml",
            "trojan://second@example.com:443#second",
        )
        .await
        .expect("import");
    assert!(change.changed && change.needs_restart);
    let document = manager.state().expect("state");
    assert_eq!(
        document.active_profile_id.as_deref(),
        Some(profile_id.as_str())
    );
    let catalog = manager.subscriptions.read().expect("catalog");
    assert_eq!(catalog.profiles[0].sources.len(), 1);
    assert_eq!(catalog.profiles[0].sources[0].remark, "subscription.yaml");
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
            catalog.profiles[0].transparent_proxy.tun.interface_name = " ".into();
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

#[tokio::test]
async fn frontend_dns_survives_switches_without_overriding_profile_dns() {
    let (_root, manager, first_id) = fixture();
    let mut settings = manager.dns_settings();
    settings.reject_https = false;
    settings.rewrites.push(sempre_dns::DnsRewrite {
        id: "device-rewrite".into(),
        domain: "router.test".into(),
        record_type: "A".into(),
        answer: "192.0.2.1".into(),
        ttl: 60,
        enabled: true,
        comment: String::new(),
    });
    let (frontend_change, _) = manager
        .update_dns_settings(settings)
        .await
        .expect("update frontend DNS");
    assert!(!frontend_change.changed);

    let second_id = sempre_subscription::new_profile("second").id;
    manager
        .subscriptions
        .update(|catalog| {
            catalog.profiles[0]
                .extra
                .insert("use_system_dns".into(), json!(false));
            catalog.profiles[0].editor.dns_config = r#"{"shared":{"remoteDns":"1.1.1.1"}}"#.into();
            let mut second = catalog.profiles[0].clone();
            second.id.clone_from(&second_id);
            second.name = "second".into();
            second.extra.insert("use_system_dns".into(), json!(false));
            second.editor.dns_config = r#"{"shared":{"remoteDns":"8.8.8.8"}}"#.into();
            catalog.profiles.push(second);
            Ok(())
        })
        .expect("add profile with different legacy DNS");

    manager
        .activate_subscription_profile(&first_id)
        .await
        .expect("activate first");
    let (_, render) = manager
        .activate_subscription_profile(&second_id)
        .await
        .expect("activate second");
    assert!(render.content.contains("8.8.8.8"));
    assert!(!render.content.contains("1.1.1.1"));
    let frontend = manager.dns_settings();
    assert!(!frontend.reject_https);
    assert_eq!(frontend.rewrites[0].id, "device-rewrite");
    assert!(
        !manager.state().expect("state").config_builds["sing-box"]
            .target_key
            .contains("|dns:")
    );
}

#[tokio::test]
async fn enabling_frontend_rebuilds_only_the_private_core_ingress() {
    let (_root, manager, profile_id) = fixture();
    manager
        .subscriptions
        .update(|catalog| {
            let profile = &mut catalog.profiles[0];
            profile.extra.insert("use_system_dns".into(), json!(false));
            profile.editor.dns_config =
                r#"{"shared":{"remoteDns":"9.9.9.9","fakeipEnabled":false}}"#.into();
            Ok(())
        })
        .expect("set profile DNS");
    let mut settings = manager.dns_settings();
    settings.enabled = true;
    let (change, _) = manager
        .update_dns_settings(settings)
        .await
        .expect("enable frontend");
    assert!(change.changed);
    let document = manager.state().expect("state");
    assert_eq!(
        document.active_profile_id.as_deref(),
        Some(profile_id.as_str())
    );
    let hash = &document.configs["sing-box"];
    let content = std::fs::read_to_string(manager.store.layout().config("sing-box", hash))
        .expect("compiled config");
    assert!(content.contains("9.9.9.9"));
    assert!(content.contains("sempre-dns-core-in"));
}

#[tokio::test]
async fn enabling_frontend_rebuilds_system_dns_profile_with_private_core_ingress() {
    let (_root, manager, profile_id) = fixture();
    assert_eq!(
        manager.subscriptions.read().expect("catalog").profiles[0].extra["use_system_dns"],
        json!(true)
    );
    let mut settings = manager.dns_settings();
    settings.enabled = true;

    let (change, _) = manager
        .update_dns_settings(settings)
        .await
        .expect("enable frontend");

    assert!(change.changed);
    let document = manager.state().expect("state");
    assert_eq!(
        document.active_profile_id.as_deref(),
        Some(profile_id.as_str())
    );
    let hash = &document.configs["sing-box"];
    let content = std::fs::read_to_string(manager.store.layout().config("sing-box", hash))
        .expect("compiled config");
    assert!(content.contains("sempre-dns-core-in"));
}

#[test]
fn config_build_schema_invalidates_legacy_target_key() {
    let (_root, manager, _) = fixture();
    let profile = Profile::default();
    let target = Target::parse("sing-box-v14-macos").expect("target");
    let build = config_build(&profile, &target, &manager.dns_settings()).expect("build");
    let mut legacy = build.clone();
    legacy.target_key = format!(
        "{}|build:1",
        build
            .target_key
            .strip_suffix("|build:2")
            .expect("schema suffix")
    );

    assert_ne!(build, legacy);
}
