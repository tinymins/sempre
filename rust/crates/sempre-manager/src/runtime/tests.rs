use std::{
    fs,
    future::Future,
    path::Path,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
};

use chrono::Utc;
use sempre_converter::Source;
use sempre_core::Adapter;
use sempre_state::{ConfigBuild, Deployment, Installation, Layout, Selection, Store};
use serde_json::Map;

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
    crate::rule_provider::write_bundled_rule_fixture(manager.store.layout());
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
async fn restart_compiles_bundled_rules_after_cached_state_is_removed() {
    let (_root, manager) = fixture();
    let layout = manager.store.layout();
    let providers = sempre_converter::system_defaults().rule_providers;
    for _ in 0..2 {
        manager.subscriptions.clear_cache().expect("clear cache");
        fs::remove_dir_all(&layout.subscription_blobs).expect("remove snapshots");
        manager
            .store
            .update(|document| {
                document.configs.clear();
                document.config_builds.clear();
                document.active = None;
                document.runtime = sempre_state::Runtime::default();
                Ok(())
            })
            .expect("clear compiled state");
        let status = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            manager.runtime_action(RESTART),
        )
        .await
        .expect("restart must not wait for network")
        .expect("restart");
        assert_eq!(status.runtime_state, RuntimeState::Restarting);
        let document = manager.state().expect("state");
        let content = fs::read(layout.config("sing-box", &document.configs["sing-box"]))
            .expect("compiled configuration");
        let config: serde_json::Value = serde_json::from_slice(&content).expect("JSON");
        let rules = config["route"]["rule_set"].as_array().expect("rule sets");
        for provider in &providers {
            let rule = rules
                .iter()
                .find(|rule| rule["tag"] == provider.tag)
                .expect("bundled provider compiled");
            assert_eq!(rule["type"], "inline");
            assert!(rule.get("url").is_none());
        }
    }
}

#[tokio::test]
async fn missing_rule_snapshot_fails_before_staging_an_invalid_core_config() {
    let (_root, manager) = fixture();
    fs::remove_file(
        manager
            .store
            .layout()
            .resources
            .join("sempre-system-rules.json"),
    )
    .expect("remove bundled rules");
    let original = manager.state().expect("state").configs;
    let error = manager
        .runtime_action(RESTART)
        .await
        .expect_err("missing rules");
    assert_eq!(
        error.runtime_action_code(),
        Some("RUNTIME_PREPARATION_FAILED")
    );
    assert!(error.to_string().contains("no local snapshot"));
    let document = manager.state().expect("state");
    assert_eq!(document.configs, original);
    assert!(document.active.is_none());
    assert_eq!(manager.runner.validations.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn start_prepares_a_stale_active_profile_from_local_inputs() {
    let (_root, manager) = fixture();
    let mut profile_id = String::new();
    manager
        .subscriptions
        .update(|catalog| {
            let profile = &mut catalog.profiles[0];
            profile_id.clone_from(&profile.id);
            profile.sources.push(Source {
                id: "raw".into(),
                kind: "raw".into(),
                enabled: true,
                url: String::new(),
                remark: String::new(),
                prefix: String::new(),
                content: "trojan://secret@example.com:443#edge".into(),
                user_agent: String::new(),
                extra: Map::new(),
            });
            Ok(())
        })
        .expect("seed profile");
    manager
        .store
        .update(|document| {
            document.active_profile_id = Some(profile_id.clone());
            Ok(())
        })
        .expect("select profile");
    let status = manager.runtime_action(START).await.expect("start");
    assert_eq!(status.runtime_state, RuntimeState::Starting);
    assert_eq!(manager.runner.validations.load(Ordering::Relaxed), 1);
    assert_eq!(
        manager.state().expect("state").config_builds["sing-box"].profile_id,
        profile_id
    );
}

#[tokio::test]
async fn start_prepares_the_default_profile_when_configuration_is_missing() {
    let (_root, manager) = fixture();
    manager
        .store
        .update(|document| {
            document.configs.remove("sing-box");
            Ok(())
        })
        .expect("remove initial configuration");

    let initial = manager.runtime_status().expect("status");
    assert!(initial.pending);
    assert!(initial.actions.start.allowed);
    assert!(initial.actions.start.reason.is_empty());

    let starting = manager.runtime_action(START).await.expect("start");
    assert_eq!(starting.runtime_state, RuntimeState::Starting);
    assert!(starting.active.is_some());
    assert_eq!(manager.runner.validations.load(Ordering::Relaxed), 1);
    let document = manager.state().expect("state");
    let active_profile_id = document.active_profile_id.expect("active profile");
    assert_eq!(
        document.config_builds["sing-box"].profile_id,
        active_profile_id
    );
    assert!(manager.dns_settings().enabled);
    let content = fs::read(
        manager
            .store
            .layout()
            .config("sing-box", &document.configs["sing-box"]),
    )
    .expect("default configuration");
    let config: serde_json::Value = serde_json::from_slice(&content).expect("configuration JSON");
    assert!(
        config["inbounds"]
            .as_array()
            .expect("inbounds")
            .iter()
            .any(|inbound| inbound["tag"] == "sempre-dns-core-in")
    );
    if cfg!(any(target_os = "windows", target_os = "macos")) {
        let tun = config["inbounds"]
            .as_array()
            .expect("inbounds")
            .iter()
            .find(|inbound| inbound["type"] == "tun")
            .expect("TUN");
        assert_eq!(
            tun["route_address"],
            serde_json::json!(["198.18.0.0/15", "fc00::/18"])
        );
    }
}

#[test]
fn status_marks_a_stale_recorded_pid_failed() {
    let (_root, manager) = fixture();
    manager
        .store
        .update(|document| {
            let (reference, version) = configuration_target(document).expect("target");
            document.active = Some(Deployment {
                core: reference.core,
                repository: reference.repository,
                reference: reference.reference,
                version,
                config_hash: document.configs["sing-box"].clone(),
            });
            document.runtime.state = RuntimeState::Running;
            document.runtime.pid = Some(u32::MAX);
            Ok(())
        })
        .expect("stale runtime");
    let status = manager.runtime_status().expect("status");
    assert_eq!(status.runtime_state, RuntimeState::Failed);
    assert!(
        status
            .last_error
            .is_some_and(|error| error.contains("not running"))
    );
}

#[tokio::test]
async fn invalid_action_has_a_stable_error_code() {
    let (_root, manager) = fixture();
    let error = manager
        .runtime_action("invalid")
        .await
        .expect_err("invalid");
    assert_eq!(error.runtime_action_code(), Some("INVALID_RUNTIME_ACTION"));
}

#[test]
fn status_describes_directly_recorded_profile_changes_without_exposing_values() {
    let (_root, manager) = fixture();
    let catalog = manager.subscriptions.read().expect("catalog");
    let mut profile = catalog.profiles[0].clone();
    let profile_id = profile.id.clone();
    drop(catalog);
    manager
        .store
        .update(|document| {
            document.active_profile_id = Some(profile_id.clone());
            document.active = Some(Deployment {
                core: "sing-box".into(),
                repository: None,
                reference: "stable".into(),
                version: "1.14.0-beta.13".into(),
                config_hash: "a".repeat(64),
            });
            document.config_builds.insert(
                "sing-box".into(),
                ConfigBuild {
                    profile_id: profile_id.clone(),
                    profile_revision: profile.revision,
                    target_key: "baseline".into(),
                    runtime_key: None,
                },
            );
            Ok(())
        })
        .expect("baseline deployment");
    profile.editor.dns_config = r#"{"final":"local"}"#.into();
    profile.transparent_proxy.capture_host = !profile.transparent_proxy.capture_host;
    profile.management_api.secret = "must-not-appear-in-runtime-status".into();
    manager
        .save_subscription_profile(&profile_id, profile.clone(), None)
        .expect("save pending profile");

    let status = manager.runtime_status().expect("status");
    assert!(status.pending);
    assert_eq!(status.pending_changes.len(), 1);
    let RuntimePendingChange::Configuration {
        fields,
        previous_revision,
        current_revision,
    } = &status.pending_changes[0]
    else {
        panic!("expected configuration change");
    };
    assert_eq!(previous_revision, &Some(profile.revision));
    assert_eq!(current_revision, &Some(profile.revision + 1));
    assert_eq!(
        fields,
        &[
            sempre_state::PendingConfigField::Dns,
            sempre_state::PendingConfigField::TransparentProxy,
            sempre_state::PendingConfigField::ManagementApi,
        ]
    );
    let encoded = serde_json::to_string(&status).expect("runtime status JSON");
    assert!(!encoded.contains("must-not-appear-in-runtime-status"));
}

#[test]
fn status_describes_a_pending_core_switch_without_calling_it_a_config_edit() {
    let (_root, manager) = fixture();
    manager
        .store
        .update(|document| {
            let installation =
                document.cores["sing-box"].default.installed["1.14.0-beta.13"].clone();
            document
                .core_mut("sing-box")
                .default
                .installed
                .insert("1.12.20".into(), installation);
            let previous = Deployment {
                core: "sing-box".into(),
                repository: None,
                reference: "stable".into(),
                version: "1.12.20".into(),
                config_hash: "a".repeat(64),
            };
            let current = Deployment {
                version: "1.14.0-beta.13".into(),
                ..previous.clone()
            };
            document.previous = Some(previous);
            document.active = Some(current);
            document.active_profile_id = None;
            document.pending = true;
            Ok(())
        })
        .expect("pending core");

    let status = manager.runtime_status().expect("status");
    assert_eq!(
        status.pending_changes,
        vec![RuntimePendingChange::Core {
            previous: Some("sing-box@1.12.20".into()),
            current: "sing-box@1.14.0-beta.13".into(),
        }]
    );
}
