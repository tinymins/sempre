use std::{future::Future, path::Path, pin::Pin};

use sempre_core::Adapter;
use sempre_state::{Deployment, Layout, Store};
use tempfile::tempdir;

use super::*;

struct FixedVersion(&'static str);

impl VersionRunner for FixedVersion {
    fn version<'a>(
        &'a self,
        _: &'a dyn Adapter,
        _: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<String, ManagerError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.0.to_owned()) })
    }
}

fn manager(root: &Path, version: &'static str) -> Manager<FixedVersion> {
    let store = Store::new(Layout::openwrt_at(root));
    Manager::with_runner(store, FixedVersion(version)).expect("manager")
}

#[test]
fn a_packaged_core_outranks_the_firmware_binary() {
    let directory = tempdir().expect("directory");
    let layout = Layout::openwrt_at(directory.path());
    let packaged = packaged_core_path(&layout);
    assert_eq!(packaged, directory.path().join("etc/sempre/sing-box"));
    fs::create_dir_all(packaged.parent().expect("parent")).expect("directory");
    fs::write(&packaged, b"binary").expect("binary");
    assert_eq!(system_core_binary(&layout), Some(packaged));
}

#[tokio::test]
async fn adopts_the_firmware_binary_as_the_stable_core() {
    let directory = tempdir().expect("directory");
    let binary = directory.path().join("sing-box");
    fs::write(&binary, b"binary").expect("binary");
    let manager = manager(directory.path(), "1.13.21");
    manager.adopt_core_binary(&binary).await.expect("adopt");
    let document = manager.state().expect("state");
    let source = &document.cores["sing-box"].default;
    assert_eq!(source.channels[STABLE], "1.13.21");
    assert_eq!(source.installed["1.13.21"].source, FIRMWARE_SOURCE);
    assert_eq!(document.selected.expect("selected").reference, STABLE);
    let link = manager
        .store
        .layout()
        .core_binary(FIRMWARE_CORE, None, "1.13.21");
    // Hosts without symbolic links get a copy, which is still the adopted core.
    #[cfg(unix)]
    assert_eq!(fs::read_link(&link).expect("link"), binary);
    assert!(link.is_file());
}

#[tokio::test]
async fn adoption_is_repeatable() {
    let directory = tempdir().expect("directory");
    let binary = directory.path().join("sing-box");
    fs::write(&binary, b"binary").expect("binary");
    let manager = manager(directory.path(), "1.13.21");
    manager.adopt_core_binary(&binary).await.expect("first");
    manager.adopt_core_binary(&binary).await.expect("second");
    assert_eq!(
        manager.state().expect("state").cores["sing-box"]
            .default
            .installed
            .len(),
        1
    );
}

#[test]
fn firmware_upgrade_moves_the_active_deployment_to_the_new_version() {
    let mut document = Document::default();
    record_firmware_core(&mut document, "1.13.21");
    document.active = Some(Deployment {
        core: FIRMWARE_CORE.into(),
        repository: None,
        reference: STABLE.into(),
        version: "1.13.21".into(),
        config_hash: "a".repeat(64),
    });
    let stale = record_firmware_core(&mut document, "1.13.22");
    assert_eq!(stale, ["1.13.21"]);
    assert_eq!(document.active.expect("active").version, "1.13.22");
    let source = &document.cores[FIRMWARE_CORE].default;
    assert!(!source.installed.contains_key("1.13.21"));
    assert_eq!(source.channels[STABLE], "1.13.22");
}

#[tokio::test]
async fn firmware_managed_core_cannot_be_installed_separately() {
    let directory = tempdir().expect("directory");
    let manager = manager(directory.path(), "1.13.21");
    assert!(matches!(
        manager.reject_firmware_managed_core("mihomo"),
        Err(ManagerError::FirmwareManagedCore(_))
    ));
    assert!(manager.reject_firmware_managed_core(FIRMWARE_CORE).is_ok());
}

#[tokio::test]
async fn other_hosts_ignore_adoption() {
    let directory = tempdir().expect("directory");
    let store = Store::new(Layout::at(directory.path()));
    let manager = Manager::with_runner(store, FixedVersion("1.13.21")).expect("manager");
    manager.adopt_firmware_core().await.expect("no-op");
    assert!(manager.state().expect("state").cores.is_empty());
    assert!(manager.reject_firmware_managed_core(FIRMWARE_CORE).is_ok());
}
