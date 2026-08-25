use std::{
    future::Future,
    path::Path,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
};

use chrono::Utc;
use sempre_core::Adapter;
use sempre_state::{Installation, Layout, Selection, Store};

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
        Box::pin(async { Ok("1.2.3".into()) })
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

fn fixture() -> (tempfile::TempDir, Manager<FakeRunner>) {
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
        .expect("seed state");
    (root, manager)
}

#[tokio::test]
async fn validates_candidate_against_the_selected_installed_core() {
    let (_root, manager) = fixture();
    manager
        .validate_config_content(br#"{"log":{"level":"info"}}"#)
        .await
        .expect("validate");
    assert_eq!(manager.runner.validations.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn rejects_oversized_candidate_before_running_a_core() {
    let (_root, manager) = fixture();
    let content = vec![b' '; MAX_CONFIG_SIZE + 1];
    assert!(matches!(
        manager.validate_config_content(&content).await,
        Err(ManagerError::ConfigurationTooLarge { .. })
    ));
    assert_eq!(manager.runner.validations.load(Ordering::Relaxed), 0);
}

#[test]
fn reads_only_the_selected_cores_content_addressed_configuration() {
    let (_root, manager) = fixture();
    let hash = "b".repeat(64);
    manager
        .store
        .update(|document| {
            document.configs.insert("sing-box".into(), hash.clone());
            Ok(())
        })
        .expect("config state");
    sempre_state::write_atomic(
        &manager.store.layout().config("sing-box", &hash),
        b"generated",
        0o600,
    )
    .expect("config file");
    assert_eq!(
        manager.current_config().expect("current config"),
        CurrentConfig {
            hash,
            content: "generated".into(),
        }
    );
}
