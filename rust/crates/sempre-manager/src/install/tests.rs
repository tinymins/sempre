use sempre_core::{BuiltInAdapter, BuiltInKind};
use sempre_state::{Layout, Store};

use super::*;

#[derive(Clone)]
struct FixedVersion(&'static str);

impl VersionRunner for FixedVersion {
    fn version<'a>(
        &'a self,
        _: &'a dyn Adapter,
        _: &'a Path,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<String, ManagerError>> + Send + 'a>,
    > {
        Box::pin(async move { Ok(self.0.into()) })
    }
}

fn package(version: &str) -> Package {
    Package {
        version: version.into(),
        name: "core.raw".into(),
        url: "https://example.invalid/core.raw".into(),
        digest: format!("sha256:{}", "a".repeat(64)),
        size: 6,
        format: "raw".into(),
    }
}

#[tokio::test]
async fn activates_version_then_records_channel_idempotently() {
    let root = tempfile::tempdir().expect("temporary directory");
    let store = Store::new(Layout::at(root.path()));
    let manager = Manager::with_runner(store, FixedVersion("1.2.3")).expect("manager");
    let archive = root.path().join("core.raw");
    fs::write(&archive, b"binary").expect("archive");
    let reference = CoreRef::parse("sing-box").expect("reference");
    let adapter = Arc::new(BuiltInAdapter::new(BuiltInKind::SingBox));

    let first = manager
        .install_downloaded(&reference, adapter.clone(), &package("1.2.3"), &archive)
        .await
        .expect("first install");
    assert!(first.installed && first.changed && first.binary.is_file());
    let state = manager.state().expect("state");
    assert_eq!(state.cores["sing-box"].default.channels["stable"], "1.2.3");
    assert!(!state.cores["sing-box"].default.installed["1.2.3"].explicit);

    let second = manager
        .install_downloaded(&reference, adapter, &package("1.2.3"), &archive)
        .await
        .expect("second install");
    assert!(!second.installed && !second.changed);
}

#[tokio::test]
async fn rejects_reported_version_before_activation_or_state_change() {
    let root = tempfile::tempdir().expect("temporary directory");
    let store = Store::new(Layout::at(root.path()));
    let manager = Manager::with_runner(store, FixedVersion("9.9.9")).expect("manager");
    let archive = root.path().join("core.raw");
    fs::write(&archive, b"binary").expect("archive");
    let reference = CoreRef::parse("sing-box@1.2.3").expect("reference");
    let adapter = Arc::new(BuiltInAdapter::new(BuiltInKind::SingBox));

    let result = manager
        .install_downloaded(&reference, adapter, &package("1.2.3"), &archive)
        .await;
    assert!(matches!(result, Err(ManagerError::VersionMismatch { .. })));
    assert!(manager.state().expect("state").cores.is_empty());
    assert!(
        !manager
            .store()
            .layout()
            .core_version_dir("sing-box", None, "1.2.3")
            .exists()
    );
}

#[tokio::test]
async fn stable_update_collects_the_unreferenced_implicit_version() {
    let root = tempfile::tempdir().expect("temporary directory");
    let layout = Layout::at(root.path());
    let archive = root.path().join("core.raw");
    fs::write(&archive, b"binary").expect("archive");
    let reference = CoreRef::parse("sing-box").expect("reference");
    let adapter = Arc::new(BuiltInAdapter::new(BuiltInKind::SingBox));
    let first = Manager::with_runner(Store::new(layout.clone()), FixedVersion("1.2.3"))
        .expect("first manager");
    first
        .install_downloaded(&reference, adapter.clone(), &package("1.2.3"), &archive)
        .await
        .expect("first install");

    let second = Manager::with_runner(Store::new(layout.clone()), FixedVersion("1.2.4"))
        .expect("second manager");
    second
        .install_downloaded(&reference, adapter, &package("1.2.4"), &archive)
        .await
        .expect("stable update");

    let state = second.state().expect("state");
    assert!(
        !state.cores["sing-box"]
            .default
            .installed
            .contains_key("1.2.3")
    );
    assert_eq!(state.cores["sing-box"].default.channels["stable"], "1.2.4");
    assert!(!layout.core_version_dir("sing-box", None, "1.2.3").exists());
    assert!(layout.core_version_dir("sing-box", None, "1.2.4").exists());
}

#[tokio::test]
async fn state_validation_failure_removes_the_activated_directory() {
    let root = tempfile::tempdir().expect("temporary directory");
    let layout = Layout::at(root.path());
    let manager =
        Manager::with_runner(Store::new(layout.clone()), FixedVersion("invalid")).expect("manager");
    let archive = root.path().join("core.raw");
    fs::write(&archive, b"binary").expect("archive");
    let reference = CoreRef::parse("sing-box").expect("reference");
    let adapter = Arc::new(BuiltInAdapter::new(BuiltInKind::SingBox));

    assert!(
        manager
            .install_downloaded(&reference, adapter, &package("invalid"), &archive)
            .await
            .is_err()
    );
    assert!(manager.state().expect("state").cores.is_empty());
    assert!(
        !layout
            .core_version_dir("sing-box", None, "invalid")
            .exists()
    );
}
