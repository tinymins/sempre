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
fn update_status_uses_semantic_version_ordering() {
    let newer = status(&manifest("999.0.0")).expect("newer status");
    assert!(newer.update_available);
    let older = status(&manifest("0.1.0")).expect("older status");
    assert!(!older.update_available);
}

#[test]
fn manifest_rejects_untrusted_repository_and_asset_urls() {
    let mut value = manifest("2.0.8");
    value.repository = "http://example.com/sempre".into();
    assert!(validate_manifest(&value).is_err());
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
fn manifest_rejects_prerelease_versions() {
    assert!(validate_manifest(&manifest("2.0.10-beta.1")).is_err());
}

#[test]
fn update_status_joins_stable_release_history_in_semver_order() {
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

    let result = status(&value).expect("update status");
    assert_eq!(
        result
            .release_history
            .iter()
            .map(|release| release.version.as_str())
            .collect::<Vec<_>>(),
        vec![middle.as_str(), latest.as_str()]
    );
    assert_eq!(
        result.release_notes,
        format!("## v{middle}\n\nMiddle notes.\n\n## v{latest}\n\nLatest notes.")
    );
}
