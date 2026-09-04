use super::*;

fn store() -> (tempfile::TempDir, SubscriptionStore) {
    let root = tempfile::tempdir().expect("temporary directory");
    let layout = sempre_state::Layout::at(root.path());
    (root, SubscriptionStore::new(layout))
}

#[test]
fn initializes_a_private_catalog_with_runtime_credentials() {
    let (_root, store) = store();
    let catalog = store.initialize().expect("initialize catalog");
    assert_eq!(catalog.schema, crate::CATALOG_SCHEMA);
    assert_eq!(catalog.profiles.len(), 1);
    let profile = &catalog.profiles[0];
    assert_eq!(profile.revision, 1);
    assert_eq!(
        profile.transparent_proxy.tproxy.dns_listen_port,
        sempre_converter::DEFAULT_CORE_DNS_PORT
    );
    assert!(!profile.local_proxy.password.is_empty());
    assert!(!profile.management_api.secret.is_empty());
    assert_eq!(
        store.read().expect("read catalog").profiles[0].id,
        profile.id
    );
}

#[test]
fn failed_update_does_not_replace_the_catalog() {
    let (_root, store) = store();
    let initial = store.initialize().expect("initialize catalog");
    let result = store.update(|catalog| {
        catalog.profiles.push(crate::new_profile(""));
        Ok(())
    });
    assert!(matches!(result, Err(SubscriptionError::Invalid(_))));
    assert_eq!(
        store.read().expect("read catalog").profiles.len(),
        initial.profiles.len()
    );
}

#[test]
fn content_addressed_blobs_verify_integrity() {
    let (_root, store) = store();
    store.initialize().expect("initialize catalog");
    let hash = store.save_blob(b"subscription").expect("save blob");
    assert_eq!(store.read_blob(&hash).expect("read blob"), b"subscription");
    fs::write(store.layout.subscription_blobs.join(&hash), b"tampered").expect("tamper blob");
    assert!(matches!(
        store.read_blob(&hash),
        Err(SubscriptionError::SnapshotIntegrity { .. })
    ));
}
