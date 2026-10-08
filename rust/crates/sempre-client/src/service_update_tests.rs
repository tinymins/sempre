use super::*;

fn manifest(version: &str) -> Manifest {
    Manifest {
        schema: 1,
        version: version.into(),
        published_at: "2026-09-07T09:15:18Z".into(),
        notes: "Fixed update handling.".into(),
        repository: "https://example.com/sempre".into(),
        releases: Vec::new(),
        assets: Vec::new(),
    }
}

fn asset(sha256: &str) -> ManifestAsset {
    ManifestAsset {
        target: "linux-amd64".into(),
        name: "sempre-bundle-linux-amd64.zip".into(),
        url: "https://example.com/sempre.zip".into(),
        sha256: sha256.into(),
        size: 1,
    }
}

#[test]
fn manifest_rejects_untrusted_repository_and_asset_urls() {
    let mut value = manifest("2.0.8");
    value.repository = "http://example.com/sempre".into();
    assert!(validate_manifest(&value, false).is_err());
    let mut value = asset(&"a".repeat(64));
    value.url = "http://example.com/sempre.zip".into();
    assert!(release_artifact(&value, "linux-amd64").is_err());
}

#[test]
fn uploaded_updates_require_state_and_the_platform_installer() {
    let root = tempfile::tempdir().unwrap();
    let error = validate_uploaded_entrypoints(root.path()).unwrap_err();
    assert!(error.contains(".sempre"));
    assert!(error.contains(installer_name()));

    std::fs::create_dir(root.path().join(".sempre")).unwrap();
    assert!(validate_uploaded_entrypoints(root.path()).is_err());
    std::fs::write(root.path().join(installer_name()), b"installer").unwrap();
    validate_uploaded_entrypoints(root.path()).unwrap();

    assert_eq!(installer_name_for_os("windows"), "install.cmd");
    assert_eq!(installer_name_for_os("macos"), "install.command");
    assert_eq!(installer_name_for_os("linux"), "install.sh");
}

#[test]
fn manifest_rejects_prerelease_versions() {
    let value = manifest("2.0.10-beta.1");
    assert!(validate_manifest(&value, false).is_err());
    assert!(validate_manifest(&value, true).is_ok());
}
