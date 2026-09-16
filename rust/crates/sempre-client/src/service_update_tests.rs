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

fn tar_gz_asset(sha256: &str) -> ManifestAsset {
    ManifestAsset {
        target: "linux-amd64".into(),
        name: "sempre-bundle-linux-amd64.tar.gz".into(),
        url: "https://example.com/sempre.tar.gz".into(),
        sha256: sha256.into(),
        size: 1,
    }
}

#[test]
fn update_status_uses_semantic_version_ordering() {
    let newer = status(&manifest("999.0.0"), false).expect("newer status");
    assert!(newer.update_available);
    let older = status(&manifest("0.1.0"), false).expect("older status");
    assert!(!older.update_available);
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
fn release_manifest_digest_is_normalized_for_the_verified_downloader() {
    let digest = "Aa".repeat(32);
    let artifact = release_artifact(&asset(&digest), "linux-amd64").expect("artifact");
    assert_eq!(artifact.digest, format!("sha256:{}", digest.to_lowercase()));

    for invalid in ["", "00", &format!("{}g", "0".repeat(63))] {
        assert!(release_artifact(&asset(invalid), "linux-amd64").is_err());
    }
}

#[test]
fn unix_updates_prefer_tar_gz_and_fall_back_to_zip() {
    let digest = "a".repeat(64);
    let zip = asset(&digest);
    let tar_gz = tar_gz_asset(&digest);
    let assets = [zip.clone(), tar_gz.clone()];
    let (selected, format) = release_asset(&assets, "linux-amd64").expect("Linux release asset");
    assert_eq!(selected.name, tar_gz.name);
    assert_eq!(format, ArchiveFormat::TarGz);

    let assets = [zip];
    let (selected, format) = release_asset(&assets, "linux-amd64").expect("legacy Linux ZIP");
    assert_eq!(selected.name, "sempre-bundle-linux-amd64.zip");
    assert_eq!(format, ArchiveFormat::Zip);
    assert!(release_asset(&[tar_gz], "windows-amd64").is_none());
}

#[test]
fn uploaded_updates_accept_only_release_archive_formats() {
    assert_eq!(
        uploaded_archive_format("sempre-bundle-linux-amd64.zip").unwrap(),
        ArchiveFormat::Zip
    );
    assert_eq!(
        uploaded_archive_format("sempre-bundle-linux-amd64.tar.gz").unwrap(),
        ArchiveFormat::TarGz
    );
    assert!(uploaded_archive_format("sempre.exe").is_err());
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
fn nested_update_errors_include_the_root_cause() {
    #[derive(Debug, thiserror::Error)]
    #[error("request failed")]
    struct RequestError(#[source] std::io::Error);

    let error = RequestError(std::io::Error::new(
        std::io::ErrorKind::ConnectionRefused,
        "connection refused",
    ));
    assert_eq!(describe_error(&error), "request failed: connection refused");
}

#[test]
fn manifest_rejects_prerelease_versions() {
    let value = manifest("2.0.10-beta.1");
    assert!(validate_manifest(&value, false).is_err());
    assert!(validate_manifest(&value, true).is_ok());
}

#[test]
fn update_status_joins_stable_release_history_in_descending_semver_order() {
    let current = parse_version(VERSION).expect("current version");
    let middle = format!("{}.0.0", current.major + 1);
    let latest = format!("{}.0.0", current.major + 2);
    let mut value = manifest(&latest);
    value.releases = vec![
        ManifestRelease {
            version: latest.clone(),
            published_at: "2026-09-08T00:00:00Z".into(),
            notes: "Latest notes.".into(),
        },
        ManifestRelease {
            version: format!("{middle}-beta.1"),
            published_at: "2026-09-07T12:00:00Z".into(),
            notes: "Beta notes.".into(),
        },
        ManifestRelease {
            version: middle.clone(),
            published_at: "2026-09-07T00:00:00Z".into(),
            notes: "Middle notes.".into(),
        },
    ];

    let result = status(&value, false).expect("update status");
    assert_eq!(
        result
            .release_history
            .iter()
            .map(|release| release.version.as_str())
            .collect::<Vec<_>>(),
        vec![latest.as_str(), middle.as_str()]
    );
    assert_eq!(
        result.release_notes,
        format!("## v{latest}\n\nLatest notes.\n\n## v{middle}\n\nMiddle notes.")
    );
}

#[test]
fn prerelease_channel_includes_all_semver_prerelease_labels() {
    let current = parse_version(VERSION).expect("current version");
    let base = format!("{}.0.0", current.major + 1);
    let latest = format!("{base}-test.2");
    let mut value = manifest(&latest);
    value.releases = vec![
        ManifestRelease {
            version: latest.clone(),
            published_at: "2026-09-09T00:00:00Z".into(),
            notes: "Test notes.".into(),
        },
        ManifestRelease {
            version: format!("{base}-dev.1"),
            published_at: "2026-09-08T00:00:00Z".into(),
            notes: "Dev notes.".into(),
        },
        ManifestRelease {
            version: format!("{base}-beta.1"),
            published_at: "2026-09-07T00:00:00Z".into(),
            notes: "Beta notes.".into(),
        },
    ];

    validate_manifest(&value, true).expect("preview manifest");
    let result = status(&value, true).expect("preview status");
    assert!(result.update_available);
    assert_eq!(result.latest_version, latest);
    assert_eq!(result.release_history.len(), 3);
}
