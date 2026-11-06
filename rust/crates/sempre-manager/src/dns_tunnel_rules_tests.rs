use super::*;

fn tunnels(urls: &[(&str, &str)]) -> sempre_tunnel::Config {
    let instances = urls
        .iter()
        .enumerate()
        .map(|(index, (state, url))| {
            serde_json::json!({
                "id": format!("t{index}"), "name": format!("T{index}"),
                "desired_state": state, "server_url": url
            })
        })
        .collect::<Vec<_>>();
    serde_json::from_value(serde_json::json!({ "schema": 1, "instances": instances }))
        .expect("tunnel config")
}

fn settings() -> DnsSettings {
    serde_json::from_value(serde_json::json!({
        "schema": 3, "revision": 0, "enabled": true,
        "rule_sets": [{ "id": "user", "name": "User", "mode": "proxy",
            "domains": [{ "id": "d", "domain": "example.org", "include_subdomains": true }] }]
    }))
    .expect("DNS settings")
}

#[test]
fn derives_exact_direct_rules_for_saved_tunnel_domains() {
    let config = tunnels(&[
        ("running", "ws://RPHZ.Example.com.:30088"),
        ("stopped", "wss://backup.example.net/path"),
        ("running", "wss://rphz.example.com:443"),
        ("running", "ws://192.0.2.10:30088"),
        ("stopped", "wss://[2001:db8::1]:443"),
    ]);
    let rule_set = tunnel_server_rule_set(&config).expect("tunnel rule set");
    assert_eq!(rule_set.upstream, "local");
    assert!(rule_set.enabled);
    assert_eq!(
        rule_set.rules,
        ["domain,backup.example.net", "domain,rphz.example.com"]
    );
}

#[test]
fn skips_rule_set_without_tunnel_domains() {
    assert_eq!(tunnel_server_rule_set(&tunnels(&[])), None);
    assert_eq!(
        tunnel_server_rule_set(&tunnels(&[("running", "ws://192.0.2.10:1")])),
        None
    );
}

#[test]
fn tunnel_rules_precede_user_rules() {
    let rule_sets = frontend_rule_sets(
        &settings(),
        &tunnels(&[("running", "ws://rphz.example.org:30088")]),
    );
    let ids = rule_sets
        .iter()
        .map(|rule_set| rule_set.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, [RULE_SET_ID, "user"]);
    assert_eq!(frontend_rule_sets(&settings(), &tunnels(&[])).len(), 1);
}
