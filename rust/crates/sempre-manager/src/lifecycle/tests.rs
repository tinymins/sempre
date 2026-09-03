use std::{
    future::Future,
    path::Path,
    pin::Pin,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use chrono::Utc;
use sempre_converter::Source;
use sempre_core::Adapter;
use sempre_state::{Deployment, Installation, Layout, Selection, Store};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::*;

#[derive(Default)]
struct FakeRunner {
    validation_calls: AtomicUsize,
    reject: AtomicBool,
    validations: Mutex<Vec<(String, String)>>,
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
        binary: &'a Path,
        config: &'a Path,
        _: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), ManagerError>> + Send + 'a>> {
        Box::pin(async move {
            self.validation_calls.fetch_add(1, Ordering::Relaxed);
            self.validations.lock().expect("validations").push((
                binary.display().to_string(),
                fs::read_to_string(config).expect("candidate config"),
            ));
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
    crate::rule_provider::write_bundled_rule_fixture(manager.store.layout());
    for version in ["1.12.20", "1.14.0-beta.13"] {
        let directory = manager
            .store
            .layout()
            .core_version_dir("sing-box", None, version);
        fs::create_dir_all(&directory).expect("core directory");
        fs::write(directory.join("sing-box"), b"binary").expect("core binary");
    }
    manager
        .store
        .update(|document| {
            let source = &mut document.core_mut("sing-box").default;
            for version in ["1.12.20", "1.14.0-beta.13"] {
                source.installed.insert(
                    version.into(),
                    Installation {
                        explicit: false,
                        digest: "a".repeat(64),
                        source: "https://example.invalid/sing-box.zip".into(),
                        installed_at: Utc::now(),
                    },
                );
            }
            source.channels.insert("stable".into(), "1.12.20".into());
            Ok(())
        })
        .expect("seed installation");
    (root, manager)
}

fn seed_v12_configuration(manager: &Manager<FakeRunner>) -> String {
    let source_content = "trojan://secret@example.com:443#edge";
    let snapshot_hash = manager
        .subscriptions
        .save_blob(source_content.as_bytes())
        .expect("source snapshot");
    manager
        .subscriptions
        .update(|catalog| {
            let profile = &mut catalog.profiles[0];
            let mut extra = Map::new();
            extra.insert("snapshot_hash".into(), Value::String(snapshot_hash));
            profile.sources.push(Source {
                id: "cached-source".into(),
                kind: "url".into(),
                enabled: true,
                url: "https://offline.invalid/subscription".into(),
                remark: String::new(),
                prefix: String::new(),
                content: String::new(),
                user_agent: String::new(),
                extra,
            });
            Ok(())
        })
        .expect("seed cached source");
    let before = manager.state().expect("state before config");
    let reference = CoreRef::parse("sing-box@stable").expect("v12 reference");
    let build = manager
        .active_subscription_build_for(&before, &reference, "1.12.20")
        .expect("v12 build")
        .expect("active profile build");
    let content =
        br#"{"dns":{"rules":[{"ip_cidr":["10.0.0.0/8"],"action":"route","server":"local"}]}}"#;
    let hash = format!("{:x}", Sha256::digest(content));
    sempre_state::write_atomic(
        &manager.store.layout().config("sing-box", &hash),
        content,
        0o600,
    )
    .expect("v12 config");
    manager
        .store
        .update(|document| {
            document.selected = Some(Selection {
                core: "sing-box".into(),
                repository: None,
                reference: "stable".into(),
            });
            document.configs.insert("sing-box".into(), hash.clone());
            document.config_builds.insert("sing-box".into(), build);
            document.active = Some(Deployment {
                core: "sing-box".into(),
                repository: None,
                reference: "stable".into(),
                version: "1.12.20".into(),
                config_hash: hash.clone(),
            });
            document.pending = false;
            Ok(())
        })
        .expect("seed v12 deployment");
    hash
}

#[tokio::test]
async fn version_selection_compiles_cached_subscription_for_candidate_target() {
    let (_root, manager) = fixture();
    let old_hash = seed_v12_configuration(&manager);
    let change = manager
        .select_core("sing-box@1.14.0-beta.13")
        .await
        .expect("select core");
    assert!(change.changed && change.needs_restart);
    assert_eq!(manager.runner.validation_calls.load(Ordering::Relaxed), 1);
    let validations = manager.runner.validations.lock().expect("validations");
    assert!(validations[0].0.contains("1.14.0-beta.13"));
    let candidate: Value = serde_json::from_str(&validations[0].1).expect("candidate JSON");
    assert!(
        candidate["dns"]["rules"]
            .as_array()
            .is_some_and(|rules| { rules.iter().any(|rule| rule["match_response"] == true) })
    );
    drop(validations);
    let document = manager.state().expect("state");
    assert!(document.cores["sing-box"].default.installed["1.14.0-beta.13"].explicit);
    assert_eq!(
        document.selected.expect("selection").reference,
        "1.14.0-beta.13"
    );
    assert_eq!(
        document.active.expect("deployment").version,
        "1.14.0-beta.13"
    );
    assert_ne!(document.configs["sing-box"], old_hash);
    assert!(
        document.config_builds["sing-box"]
            .target_key
            .contains("v14")
    );
    assert_eq!(
        document
            .previous
            .as_ref()
            .expect("rollback deployment")
            .config_hash,
        old_hash
    );
    assert!(
        document
            .previous_config_build
            .as_ref()
            .expect("rollback build")
            .target_key
            .contains("v12")
    );
    assert!(document.pending);
    let catalog = manager.subscriptions.read().expect("catalog");
    assert_eq!(
        catalog.profiles[0].sources[0].extra["last_status"],
        Value::String("local snapshot".into())
    );
}

#[tokio::test]
async fn rejected_candidate_preserves_the_complete_v12_state() {
    let (_root, manager) = fixture();
    seed_v12_configuration(&manager);
    let before = manager.state().expect("state before");
    manager.runner.reject.store(true, Ordering::Relaxed);
    assert!(
        manager
            .select_core("sing-box@1.14.0-beta.13")
            .await
            .is_err()
    );
    assert_eq!(manager.state().expect("state after"), before);
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
        .core_version_dir("sing-box", None, "1.14.0-beta.13");
    let change = manager
        .remove_core("sing-box@1.14.0-beta.13")
        .expect("remove core");
    assert!(change.changed);
    assert!(!directory.exists());
    assert!(
        !manager.state().expect("state").cores["sing-box"]
            .default
            .installed
            .contains_key("1.14.0-beta.13")
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
                    reference: "1.14.0-beta.13".into(),
                    version: "1.14.0-beta.13".into(),
                    config_hash: "b".repeat(64),
                };
                match usage {
                    "selected" => {
                        document.selected = Some(sempre_state::Selection {
                            core: "sing-box".into(),
                            repository: None,
                            reference: "1.14.0-beta.13".into(),
                        });
                    }
                    "active" => document.active = Some(deployment),
                    _ => document.previous = Some(deployment),
                }
                Ok(())
            })
            .expect("referenced state");
        assert!(matches!(
            manager.remove_core("sing-box@1.14.0-beta.13"),
            Err(ManagerError::CoreInUse { .. })
        ));
    }
}
