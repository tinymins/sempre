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
    assert_eq!(
        profile.local_proxy.socks_port,
        sempre_converter::DEFAULT_LOCAL_SOCKS_PORT
    );
    assert_eq!(
        profile.local_proxy.http_port,
        sempre_converter::DEFAULT_LOCAL_HTTP_PORT
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

#[test]
fn persisted_catalog_discards_editor_outputs_and_device_overlays() {
    let (_root, store) = store();
    store.initialize().expect("initialize catalog");
    let mut raw: serde_json::Value = serde_json::from_slice(
        &fs::read(&store.layout.subscription_catalog).expect("read catalog file"),
    )
    .expect("catalog JSON");
    let profile = raw["profiles"][0].as_object_mut().expect("profile object");
    profile.insert(
        "manual_servers".into(),
        serde_json::json!([{ "name": "ghost" }]),
    );
    profile.insert("groups".into(), serde_json::json!([{ "name": "ghost" }]));
    profile.insert("rules".into(), serde_json::json!(["MATCH,ghost"]));
    profile.insert("rule_providers".into(), serde_json::json!([]));
    profile.insert("filters".into(), serde_json::json!(["ghost"]));
    profile.insert(
        "dns".into(),
        serde_json::json!({ "shared": { "remoteDns": "ghost" } }),
    );
    profile.insert(
        "private_access".into(),
        serde_json::json!({ "connectors": [{ "tag": "ghost" }] }),
    );
    profile["editor"]["private_access_config"] = serde_json::json!(
        r#"{"enabled":true,"connectors":[{"type":"wireguard","tag":"visible","routes":{"ipCidrs":["10.19.93.0/24"]}}]}"#
    );
    profile.insert(
        "network_policy".into(),
        serde_json::json!({ "enabled": true }),
    );
    profile.insert("unknown_profile_field".into(), serde_json::json!(true));
    profile["transparent_proxy"]["capture_host"] = serde_json::json!(true);
    profile["transparent_proxy"]["lan_interfaces"] = serde_json::json!(["ghost0"]);
    fs::write(
        &store.layout.subscription_catalog,
        serde_json::to_vec_pretty(&raw).expect("encode seeded catalog"),
    )
    .expect("seed legacy catalog");

    let catalog = store.read().expect("read normalized catalog");
    let profile = &catalog.profiles[0];
    assert!(profile.manual_servers.is_empty());
    assert!(profile.groups.is_empty());
    assert!(profile.rules.is_empty());
    assert!(profile.filters.is_empty());
    assert!(profile.dns.is_null());
    assert!(profile.private_access.is_null());
    assert!(profile.network_policy.is_null());
    assert!(!profile.extra.contains_key("unknown_profile_field"));
    assert!(!profile.transparent_proxy.capture_host);
    assert!(profile.transparent_proxy.lan_interfaces.is_empty());
    let effective = sempre_converter::profile_from_editor(profile).expect("effective profile");
    assert_eq!(effective.private_access["connectors"][0]["tag"], "visible");
    assert_eq!(
        effective.private_access["connectors"][0]["routes"]["ipCidrs"],
        serde_json::json!(["10.19.93.0/24"])
    );

    store
        .update(|catalog| {
            catalog.profiles[0].revision += 1;
            Ok(())
        })
        .expect("rewrite normalized catalog");
    let saved: serde_json::Value = serde_json::from_slice(
        &fs::read(&store.layout.subscription_catalog).expect("read rewritten catalog"),
    )
    .expect("rewritten catalog JSON");
    let saved = saved["profiles"][0].as_object().expect("saved profile");
    for key in [
        "manual_servers",
        "groups",
        "rules",
        "rule_providers",
        "filters",
        "dns",
        "private_access",
        "network_policy",
        "unknown_profile_field",
    ] {
        assert!(!saved.contains_key(key), "{key} should not be persisted");
    }
    let transparent = saved["transparent_proxy"]
        .as_object()
        .expect("transparent proxy");
    assert!(!transparent.contains_key("capture_host"));
    assert!(!transparent.contains_key("lan_interfaces"));
}
