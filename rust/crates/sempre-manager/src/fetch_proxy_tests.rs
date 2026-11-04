use sempre_state::{Layout, Store};
use serde_json::json;

use super::*;

const NODE: &str = "ss://YWVzLTI1Ni1nY206cGFzcw@192.0.2.1:8388#node";

fn manager(root: &std::path::Path) -> crate::Manager {
    Manager::new(Store::new(Layout::at(root))).expect("manager")
}

fn input(content: &str) -> FetchProxyInput {
    FetchProxyInput {
        name: " Upstream ".into(),
        content: content.into(),
        custom_node_id: None,
        enabled: true,
    }
}

#[test]
fn saves_valid_proxies_and_lists_them_without_credentials() {
    let root = tempfile::tempdir().expect("root");
    let manager = manager(root.path());
    assert!(manager.create_fetch_proxy(input("not a proxy")).is_err());
    let proxy = manager.create_fetch_proxy(input(NODE)).expect("proxy");
    assert_eq!(proxy.name, "Upstream");
    assert_eq!(manager.fetch_proxy(&proxy.id).expect("read").content, NODE);
    let listed = manager.fetch_proxies().expect("list");
    assert_eq!(listed[0].protocol.as_deref(), Some("shadowsocks"));
    assert!(
        !serde_json::to_string(&listed)
            .expect("JSON")
            .contains("8388")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let path = &manager.store.layout().fetch_proxies;
        let mode = fs::metadata(path).expect("file").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn a_custom_node_supplies_the_connection_and_cannot_be_missing() {
    let root = tempfile::tempdir().expect("root");
    let manager = manager(root.path());
    let bound = FetchProxyInput {
        custom_node_id: Some("missing".into()),
        ..input("")
    };
    assert!(manager.create_fetch_proxy(bound.clone()).is_err());
    let node = manager
        .save_custom_node(sempre_converter::CustomNode {
            id: String::new(),
            name: "edge".into(),
            proxy: json!({
                "name": "edge", "type": "socks5", "server": "192.0.2.2", "port": 1080
            }),
            created_at: None,
            updated_at: None,
        })
        .expect("node");
    let proxy = manager
        .create_fetch_proxy(FetchProxyInput {
            custom_node_id: Some(node.id),
            ..bound
        })
        .expect("proxy");
    assert!(proxy.content.is_empty());
}

#[test]
fn a_proxy_in_use_cannot_be_removed() {
    let root = tempfile::tempdir().expect("root");
    let manager = manager(root.path());
    let proxy = manager.create_fetch_proxy(input(NODE)).expect("proxy");
    manager
        .subscriptions
        .update(|catalog| {
            let mut source: Source = serde_json::from_value(json!({
                "id": "source", "url": "http://example.test/", "proxy_id": proxy.id
            }))
            .expect("source");
            source.enabled = true;
            catalog.profiles[0].sources.push(source);
            Ok(())
        })
        .expect("catalog");
    assert_eq!(manager.fetch_proxies().expect("list")[0].reference_count, 1);
    assert!(manager.remove_fetch_proxy(&proxy.id).is_err());
    manager
        .subscriptions
        .update(|catalog| {
            catalog.profiles[0].sources.clear();
            Ok(())
        })
        .expect("catalog");
    manager.remove_fetch_proxy(&proxy.id).expect("remove");
    assert!(manager.fetch_proxies().expect("list").is_empty());
}

#[tokio::test]
async fn a_source_naming_an_unavailable_proxy_fails_instead_of_going_direct() {
    let root = tempfile::tempdir().expect("root");
    let manager = manager(root.path());
    let source: Source = serde_json::from_value(json!({
        "id": "source", "url": "http://127.0.0.1:9/", "proxy_id": "missing"
    }))
    .expect("source");
    let proxies = manager
        .start_source_proxies(std::slice::from_ref(&source))
        .await;
    assert_eq!(proxies.warnings.len(), 1);
    let error = proxies
        .fetcher
        .inspect(source, true, |_| Ok(()))
        .await
        .expect_err("unavailable");
    assert!(error.to_string().contains("is unavailable"), "{error}");
}
