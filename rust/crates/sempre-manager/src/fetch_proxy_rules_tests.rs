use sempre_state::{Layout, Store};

use super::*;
use crate::FetchProxyInput;

const NODE: &str = "ss://YWVzLTI1Ni1nY206cGFzcw@192.0.2.1:8388#node";

fn manager(root: &std::path::Path) -> crate::Manager {
    Manager::new(Store::new(Layout::at(root))).expect("manager")
}

fn proxy(manager: &crate::Manager) -> String {
    manager
        .create_fetch_proxy(FetchProxyInput {
            name: "upstream".into(),
            content: NODE.into(),
            custom_node_id: None,
            enabled: true,
        })
        .expect("proxy")
        .id
}

fn direct_rule(host: &str) -> ResourceHostRule {
    ResourceHostRule {
        host: host.into(),
        outbound: ResourceOutbound::Direct,
    }
}

#[test]
fn hosts_are_normalized_and_must_be_exact_and_unique() {
    let root = tempfile::tempdir().expect("root");
    let manager = manager(root.path());
    let saved = manager
        .update_resource_rules(ResourceRules {
            host_rules: vec![direct_rule(" GitHub.COM ")],
            ..ResourceRules::default()
        })
        .expect("save");
    assert_eq!(saved.host_rules[0].host, "github.com");
    assert_eq!(saved.revision, 1);
    for host in ["https://github.com", "github.com:443", "*.github.com", ""] {
        let invalid = ResourceRules {
            revision: saved.revision,
            host_rules: vec![direct_rule(host)],
            ..ResourceRules::default()
        };
        assert!(manager.update_resource_rules(invalid).is_err(), "{host}");
    }
    let repeated = ResourceRules {
        revision: saved.revision,
        host_rules: vec![direct_rule("a.test"), direct_rule("A.test")],
        ..ResourceRules::default()
    };
    assert!(manager.update_resource_rules(repeated).is_err());
}

#[test]
fn a_stale_revision_is_refused() {
    let root = tempfile::tempdir().expect("root");
    let manager = manager(root.path());
    manager
        .update_resource_rules(ResourceRules::default())
        .expect("first");
    assert!(
        manager
            .update_resource_rules(ResourceRules::default())
            .is_err()
    );
}

#[test]
fn a_rule_must_name_an_enabled_proxy_and_protects_it_from_removal() {
    let root = tempfile::tempdir().expect("root");
    let manager = manager(root.path());
    let missing = ResourceRules {
        default_outbound: ResourceOutbound::Proxy {
            proxy_id: "missing".into(),
        },
        ..ResourceRules::default()
    };
    assert!(manager.update_resource_rules(missing).is_err());
    let id = proxy(&manager);
    manager
        .update_resource_rules(ResourceRules {
            default_outbound: ResourceOutbound::Proxy {
                proxy_id: id.clone(),
            },
            ..ResourceRules::default()
        })
        .expect("save");
    assert_eq!(manager.fetch_proxies().expect("list")[0].reference_count, 1);
    assert!(manager.remove_fetch_proxy(&id).is_err());
}

#[test]
fn the_most_specific_host_wins_without_matching_subdomains() {
    let rules = ResourceRules {
        default_outbound: ResourceOutbound::Direct,
        host_rules: vec![ResourceHostRule {
            host: "github.com".into(),
            outbound: ResourceOutbound::System,
        }],
        ..ResourceRules::default()
    };
    assert_eq!(
        rules.outbound_for("GITHUB.com").1,
        &ResourceOutbound::System
    );
    assert_eq!(
        rules.outbound_for("api.github.com").1,
        &ResourceOutbound::Direct
    );
}

#[tokio::test]
async fn without_rules_nothing_runs_and_callers_keep_their_path() {
    let root = tempfile::tempdir().expect("root");
    let manager = manager(root.path());
    assert!(
        manager
            .start_resource_routes()
            .await
            .expect("routes")
            .routes
            .is_none()
    );
    let lease = manager
        .resource_egress("https://github.com/")
        .await
        .expect("lease");
    assert!(lease.is_system());
}

#[tokio::test]
async fn a_proxy_that_cannot_start_never_falls_back() {
    let root = tempfile::tempdir().expect("root");
    let manager = manager(root.path());
    let id = proxy(&manager);
    manager
        .update_resource_rules(ResourceRules {
            default_outbound: ResourceOutbound::Proxy { proxy_id: id },
            ..ResourceRules::default()
        })
        .expect("save");
    // No sing-box is installed here, so the proxy cannot start.
    assert!(
        manager
            .resource_egress("https://github.com/")
            .await
            .is_err()
    );
    let active = manager.start_resource_routes().await.expect("routes");
    let result = active
        .apply(&manager.fetcher)
        .fetch_rule_set("http://127.0.0.1:9/rules")
        .await;
    let error = result.err().expect("unavailable").to_string();
    assert!(error.contains("unavailable"), "{error}");
}
