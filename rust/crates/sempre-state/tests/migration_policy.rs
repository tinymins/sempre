use std::{collections::BTreeMap, fs, path::Path};

use sha2::{Digest as _, Sha256};

const FROZEN: &[(&str, &str)] = &[
    (
        "sempre-state/src/migrations/v0002_pending_change_contract.rs",
        "b18ed94bd5dc948a621d31b0736390a321a69d43cf3b34aa63bd0a9d4daed956",
    ),
    (
        "sempre-state/src/migrations/v0003_remove_runtime_rollback.rs",
        "b6c200860bed6363d20d83bd48ae5e5e2c597dc4ea751a18eda63e9239787546",
    ),
    (
        "sempre-client/src/traffic_history_migrations/v0002_monthly_retention.rs",
        "28663676c1dc892c91ea74498944c4b0b01712920989c28e612c48d0d03bba28",
    ),
    (
        "sempre-client/src/traffic_history_migrations/v0003_rolling_window.rs",
        "41ac28fc6cff9d9d294da8192fbae8375d9630815ac4e3e1ea7846fd498d37c4",
    ),
    (
        "sempre-server/migrations/0001_initial.sql",
        "cdc8c975d090edf0fc71bee3c962b6ef646396e68f6714dd72a1694753581fc4",
    ),
    (
        "sempre-server/migrations/0002_toolbox_capabilities.sql",
        "f972478574c147644d9ca8b077fac3496a07e6ec250742b5ef12f6ce058394fb",
    ),
    (
        "sempre-server/migrations/0003_profile_revisions.sql",
        "f295330e9d6d1c70146c48214d159cdc808f26ce07332d1c89bd59e4c4f65631",
    ),
    (
        "sempre-server/migrations/0004_profile_refresh.sql",
        "7d01d3f44662efab6e821525396329c1b9d39d6fdc9aa9161816e2bf8c1c2ad5",
    ),
    (
        "sempre-server/migrations/0005_auth_limits.sql",
        "a9085c04d3e51e4fd800205a33b0601a3393b1d1288059102d6af5085400bbb6",
    ),
    (
        "sempre-server/migrations/0006_refresh_outcomes.sql",
        "3ac9bdfe4a2efb6698e2a039d9209d6d32623941c1914eebe1a6c024a4494c07",
    ),
    (
        "sempre-server/toolbox-migrations/0001_toolbox.sql",
        "1833114ab137c0d2ff2f20b90bf36a006350bc85e1c7c54bc12c95599688e07d",
    ),
    (
        "sempre-server/toolbox-migrations/0002_subscription_source_snapshots.sql",
        "eae3e6b3a5b311ed74423b7f3a1a6cf3ac21b851253a85ad0091a2d89e446c74",
    ),
    (
        "sempre-server/toolbox-migrations/0003_debug_source_cache.sql",
        "ca8cf6a0c6e1da9a36bdd7ad531e215e2d14209db6f7650e86e2bc20e4f99273",
    ),
    (
        "sempre-server/toolbox-migrations/0004_subscription_artifacts.sql",
        "87e1bc2ca5aee826bce41f351fe51af91332967dd519f75faf97b92554ebfbde",
    ),
    (
        "sempre-server/toolbox-migrations/0005_toolbox_management.sql",
        "b81784d361a95c5686ba8d0a07427e8e165b8870263b1835720c53a10b66baad",
    ),
    (
        "sempre-server/toolbox-migrations/0006_artifact_last_success.sql",
        "e852f06ccb27d39b9ebcbb4e4b72bbe01961c480e3ad84c35a542d351041c166",
    ),
    (
        "sempre-server/toolbox-migrations/0007_remove_workspaces.sql",
        "4719f8aeb55b1e6c2a4c12fb9e6659d995f3330db3603bfdf75e6225446482aa",
    ),
    (
        "sempre-server/toolbox-migrations/0008_access_totals.sql",
        "c5f5875ded2d6eb001a873f8844cb2a1ede62b8afa508c42e4ea98a8c2d95323",
    ),
    (
        "sempre-server/toolbox-migrations/0009_artifact_revision.sql",
        "7fc1ab95d91afaf887e83560038c89710310d7c1919d8a96bdc5584517344b84",
    ),
    (
        "sempre-server/toolbox-migrations/0010_shared_editor.sql",
        "d4045755b417a02011bcbb51c094004895c6a61bac559503e492e029158d783e",
    ),
];

#[test]
fn migration_sources_are_frozen_and_contain_no_inline_tests() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let frozen: BTreeMap<_, _> = FROZEN.iter().copied().collect();
    for folder in [
        "sempre-state/src/migrations",
        "sempre-client/src/traffic_history_migrations",
        "sempre-server/migrations",
        "sempre-server/toolbox-migrations",
    ] {
        for entry in fs::read_dir(crates.join(folder)).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap();
            let rust_migration =
                name.starts_with('v') && path.extension().is_some_and(|ext| ext == "rs");
            let sql_migration = path.extension().is_some_and(|ext| ext == "sql");
            if !rust_migration && !sql_migration {
                continue;
            }
            let key = format!("{folder}/{name}");
            let source = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
            assert!(
                !contains_inline_tests(&source),
                "{key}: migration tests must live in a separate file"
            );
            let expected = frozen.get(key.as_str()).unwrap_or_else(|| {
                panic!("{key}: register the new migration's canonical checksum in FROZEN")
            });
            assert_eq!(
                format!("{:x}", Sha256::digest(source.as_bytes())),
                *expected,
                "{key}: published migration sources are immutable; add a new migration"
            );
        }
    }
    for &(path, _) in FROZEN {
        assert!(
            crates.join(path).is_file(),
            "published migration {path} is missing"
        );
    }
}

fn contains_inline_tests(source: &str) -> bool {
    let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("modtests{")
        || compact.split("#[").skip(1).any(|attribute| {
            attribute
                .split(']')
                .next()
                .unwrap()
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|word| word == "test")
        })
}

#[test]
fn inline_test_detection_includes_conditional_and_async_tests() {
    for source in [
        "#[test] fn checks() {}",
        "#[cfg(any(test, windows))] mod checks {}",
        "#[tokio::test] async fn checks() {}",
        "mod tests {}",
    ] {
        assert!(contains_inline_tests(source), "{source}");
    }
    assert!(!contains_inline_tests("#[derive(Debug)] struct State;"));
}
