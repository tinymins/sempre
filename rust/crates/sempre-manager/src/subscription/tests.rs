use std::{
    future::Future,
    path::Path,
    pin::Pin,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use chrono::Utc;
use sempre_converter::Source;
use sempre_core::Adapter;
use sempre_state::{Installation, Layout, Selection, Store};
use serde_json::{Map, json};

use super::*;
use crate::RuntimePendingChange;

#[derive(Default)]
struct FakeRunner {
    reject: AtomicBool,
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

fn fixture() -> (tempfile::TempDir, Manager<FakeRunner>, String) {
    fixture_with_core_version("1.13.2")
}

fn fixture_with_core_version(
    core_version: &str,
) -> (tempfile::TempDir, Manager<FakeRunner>, String) {
    let root = tempfile::tempdir().expect("temporary directory");
    let manager = Manager::with_runner(Store::new(Layout::at(root.path())), FakeRunner::default())
        .expect("manager");
    // These fixtures cover opt-in transitions and older cores without a frontend.
    let mut dns = manager.dns_settings.read();
    dns.enabled = false;
    manager
        .dns_settings
        .replace(dns)
        .expect("legacy DNS fixture");
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
                core_version.into(),
                Installation {
                    explicit: false,
                    digest: "a".repeat(64),
                    source: "https://example.invalid/sing-box.zip".into(),
                    installed_at: Utc::now(),
                },
            );
            source.channels.insert("stable".into(), core_version.into());
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
async fn failed_dns_rebuild_keeps_the_saved_settings() {
    let (_root, manager, profile_id) = fixture();
    manager
        .activate_subscription_profile(&profile_id)
        .await
        .expect("activate profile");
    let before = manager.dns_settings();
    manager.runner.reject.store(true, Ordering::Relaxed);
    let mut candidate = before.clone();
    candidate.enabled = true;

    manager
        .update_dns_settings(candidate)
        .await
        .expect_err("rebuild must fail");

    let saved = manager.dns_settings();
    assert!(saved.enabled);
    assert_eq!(saved.revision, before.revision + 1);
    let status = manager.runtime_status().expect("runtime status");
    assert!(status.pending);
    assert!(status.pending_changes.iter().any(|change| matches!(
        change,
        RuntimePendingChange::Configuration { fields, .. }
            if fields.contains(&PendingConfigField::Dns)
    )));
}

#[tokio::test]
async fn failed_network_rebuild_keeps_the_saved_settings() {
    let (_root, manager, profile_id) = fixture();
    manager
        .activate_subscription_profile(&profile_id)
        .await
        .expect("activate profile");
    let before = manager.network_settings();
    manager.runner.reject.store(true, Ordering::Relaxed);
    let mut candidate = before.clone();
    candidate.automatic_switching = true;

    manager
        .update_network_settings(candidate)
        .await
        .expect_err("rebuild must fail");

    let saved = manager.network_settings();
    assert!(saved.automatic_switching);
    assert_eq!(saved.revision, before.revision + 1);
    let status = manager.runtime_status().expect("runtime status");
    assert!(status.pending);
    assert!(status.pending_changes.iter().any(|change| matches!(
        change,
        RuntimePendingChange::Configuration { fields, .. }
            if [
                PendingConfigField::Dns,
                PendingConfigField::PrivateAccess,
                PendingConfigField::TransparentProxy,
            ].iter().all(|field| fields.contains(field))
    )));
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
