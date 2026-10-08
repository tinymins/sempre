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
