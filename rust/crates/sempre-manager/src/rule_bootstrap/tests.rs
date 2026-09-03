use sempre_subscription::SubscriptionStore;

use super::*;

fn fixture() -> (tempfile::TempDir, Fetcher) {
    let root = tempfile::tempdir().unwrap();
    let store = SubscriptionStore::new(sempre_state::Layout::at(root.path()));
    store.initialize().unwrap();
    (root, Fetcher::new(store).unwrap())
}

#[test]
fn missing_rule_sets_do_not_block_startup_or_broaden_logical_conditions() {
    let (_root, fetcher) = fixture();
    let config = json!({
        "route": {
            "rule_set": [
                {"type":"inline", "tag":"known", "rules":[{"domain":["local.example"]}]},
                {"type":"remote", "tag":"user-rules", "format":"source", "url":"https://offline.invalid/custom.yaml"},
                {"type":"remote", "tag":"user-binary", "format":"binary", "url":"https://offline.invalid/custom.srs"}
            ],
            "rules": [
                {"ip_is_private":true, "outbound":"direct"},
                {"type":"logical", "mode":"and", "rules":[{"rule_set":["user-rules"]},{"domain":["special.example"]}], "outbound":"proxy"},
                {"rule_set":["known"], "outbound":"direct"}
            ],
            "final":"proxy"
        },
        "dns": {"rules":[{"rule_set":["user-binary"],"server":"local"},{"query_type":["HTTPS"],"action":"reject"}], "final":"remote"}
    });
    let (base, missing) = materialize(&fetcher, &config, &HashMap::new()).unwrap();
    assert_eq!(missing.len(), 2);
    assert_eq!(base["route"]["rule_set"].as_array().unwrap().len(), 1);
    assert_eq!(
        base["route"]["rules"],
        json!([
            {"ip_is_private":true,"outbound":"direct"}, {"rule_set":["known"],"outbound":"direct"}
        ])
    );
    assert_eq!(base["route"]["final"], "proxy");
    assert_eq!(base["dns"]["rules"].as_array().unwrap().len(), 1);
    assert_eq!(config["route"]["rule_set"].as_array().unwrap().len(), 3);
    assert_eq!(
        materialize(&fetcher, &json!({}), &HashMap::new())
            .unwrap()
            .0,
        json!({})
    );
}

#[test]
fn only_validated_user_rules_complete_the_original_configuration() {
    let (_root, fetcher) = fixture();
    let config = json!({"route": {
        "rule_set":[
            {"tag":"user-one","type":"remote","format":"source","url":"https://arbitrary.invalid/user.yaml"},
            {"tag":"user-two","type":"remote","format":"source","url":"https://another.invalid/native.json"}
        ],
        "rules":[{"rule_set":["user-one","user-two"],"outbound":"proxy"}]
    }});
    let contents = [
        "payload:\n  - DOMAIN-SUFFIX,user.example\n",
        r#"{"version":3,"rules":[{"ip_cidr":["203.0.113.0/24"]}]}"#,
    ];
    let (_, missing) = materialize(&fetcher, &config, &HashMap::new()).unwrap();
    let mut candidates = HashMap::new();
    for (resource, content) in missing.iter().zip(contents) {
        let snapshot = fetcher
            .rule_set_candidate(content.as_bytes().to_vec())
            .unwrap();
        let normalized = normalize_rule(&fetcher, resource, snapshot).unwrap();
        candidates.insert(resource.tag.clone(), normalized);
    }
    assert_eq!(
        materialize(&fetcher, &config, &HashMap::new())
            .unwrap()
            .1
            .len(),
        2
    );
    let (candidate, missing_candidate) = materialize(&fetcher, &config, &candidates).unwrap();
    assert!(missing_candidate.is_empty());
    let source = |tag: &str| serde_json::from_slice::<Value>(&candidates[tag].content).unwrap();
    assert_eq!(
        source("user-one")["rules"][0]["domain_suffix"][0],
        "user.example"
    );
    assert_eq!(
        source("user-two")["rules"][0]["ip_cidr"][0],
        "203.0.113.0/24"
    );
    assert_eq!(candidate["route"]["rules"], config["route"]["rules"]);
    for resource in missing {
        fetcher
            .accept_rule_set(&resource.url, &resource.format, &candidates[&resource.tag])
            .unwrap();
    }
    let (complete, missing) = materialize(&fetcher, &config, &HashMap::new()).unwrap();
    assert!(missing.is_empty());
    assert!(
        complete["route"]["rule_set"]
            .as_array()
            .unwrap()
            .iter()
            .all(|rule| rule["type"] == "local" && rule.get("url").is_none())
    );
    assert_eq!(complete["route"]["rules"], config["route"]["rules"]);
}

#[test]
fn online_rules_keep_their_refresh_interval_and_inferred_binary_format() {
    let (_root, fetcher) = fixture();
    let remote = RemoteRule::parse(
        &json!({"tag":"user","url":"https://arbitrary.invalid/a.srs?v=2","update_interval":"2h"}),
    )
    .unwrap();
    assert_eq!(remote.format, "binary");
    assert_eq!(remote.interval, Duration::from_hours(2));
    assert!(download::refresh_due(&fetcher, &remote));
    let snapshot = fetcher.rule_set_candidate(b"candidate".to_vec()).unwrap();
    fetcher
        .accept_rule_set(&remote.url, &remote.format, &snapshot)
        .unwrap();
    assert!(!download::refresh_due(&fetcher, &remote));
}

#[test]
fn core_without_a_local_proxy_cannot_silently_download_rules_directly() {
    let (_root, fetcher) = fixture();
    assert!(proxy_fetcher(&fetcher, &json!({})).is_err());
}
