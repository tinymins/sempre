use chrono::Utc;
use sempre_state::{Installation, Layout, Selection, Store};

use super::*;
use crate::ProcessRunner;

#[test]
fn context_is_common_until_a_core_is_selected() {
    let root = tempfile::tempdir().expect("temporary directory");
    let manager =
        Manager::<ProcessRunner>::new(Store::new(Layout::at(root.path()))).expect("manager");
    assert_eq!(
        manager.configuration_context().expect("context").key,
        "common"
    );
}

#[test]
fn selected_context_key_is_stable_and_target_specific() {
    let root = tempfile::tempdir().expect("temporary directory");
    let manager =
        Manager::<ProcessRunner>::new(Store::new(Layout::at(root.path()))).expect("manager");
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
                "1.13.0".into(),
                Installation {
                    explicit: false,
                    digest: "a".repeat(64),
                    source: "https://example.invalid/core".into(),
                    installed_at: Utc::now(),
                },
            );
            source.channels.insert("stable".into(), "1.13.0".into());
            Ok(())
        })
        .expect("state");
    let first = manager.configuration_context().expect("context");
    let second = manager.configuration_context().expect("context");
    assert_eq!(first.key, second.key);
    assert_eq!(first.key.len(), 64);
    assert_eq!(first.target.expect("target").version, "1.13.0");
}
