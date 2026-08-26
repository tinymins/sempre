use sempre_converter::{
    CompileRequest, Profile, SourceSnapshot, Target, compile, rule_provider_snapshot_id,
};
use serde_json::{Value, json};

fn compile_document(format: &str) -> Value {
    let profile: Profile = serde_json::from_value(json!({
        "name": "equivalence",
        "manual_servers": [{
            "name": "edge", "type": "socks5", "server": "edge.example.com", "port": 1080
        }],
        "groups": [
            { "name": "🔰 国外流量", "type": "select", "proxies": ["DIRECT"] },
            { "name": "🚀 直接连接", "type": "select", "proxies": ["DIRECT"], "readonly": true },
            { "name": "⚓️ 其他流量", "type": "select", "proxies": ["🔰 国外流量", "DIRECT"], "readonly": true }
        ],
        "rule_providers": [{
            "tag": "sites", "url": "https://example.test/sites.yaml", "outbound": "🔰 国外流量"
        }],
        "private_access": {
            "enabled": true,
            "connectors": [{
                "type": "wireguard", "tag": "private-wg",
                "endpoint": {
                    "privateKey": "private", "address": ["192.0.2.2/32"],
                    "peers": [{
                        "address": "vpn.example.com", "port": 51820,
                        "publicKey": "public", "allowedIps": ["0.0.0.0/0"],
                        "persistentKeepaliveInterval": 25
                    }]
                },
                "routes": { "ipCidrs": ["198.51.100.0/24"] },
                "dns": [{
                    "tag": "private-dns", "server": "192.0.2.53",
                    "domainSuffixes": ["corp.example.com"]
                }]
            }]
        },
        "management_api": {
            "external_controller": "127.0.0.1:9090", "secret": "secret", "external_ui": "./ui"
        }
    }))
    .expect("profile");
    let result = compile(&CompileRequest {
        protocol: 1,
        profile,
        snapshots: vec![SourceSnapshot {
            source_id: rule_provider_snapshot_id("sites"),
            content: "payload:\n  - DOMAIN-SUFFIX,example.com\n  - IP-CIDR,192.0.2.0/24".into(),
            content_hash: String::new(),
        }],
        custom_nodes: vec![],
        target: Target::parse(format).expect("target"),
    })
    .expect("compile");
    serde_json::from_str(&result.content).expect("JSON")
}

#[test]
fn modern_sing_box_preserves_v1_runtime_and_private_access_semantics() {
    let document = compile_document("sing-box-v12-macos");
    assert_eq!(document["route"]["final"], "⚓️ 其他流量");
    assert_eq!(document["inbounds"][2]["sniff"], true);
    assert_eq!(document["inbounds"][2]["sniff_override_destination"], true);
    assert_eq!(document["outbounds"][0]["tag"], "direct");
    assert_eq!(document["outbounds"][1]["tag"], "reject");
    let edge = document["outbounds"]
        .as_array()
        .expect("outbounds")
        .iter()
        .find(|outbound| outbound["tag"] == "edge")
        .expect("edge");
    assert_eq!(edge["domain_resolver"]["server"], "bootstrap");
    assert_eq!(document["endpoints"][0]["type"], "wireguard");
    assert_eq!(
        document["endpoints"][0]["peers"][0]["persistent_keepalive_interval"],
        25
    );
    assert_eq!(document["dns"]["servers"].as_array().map(Vec::len), Some(5));
    assert!(
        document["route"]["rules"]
            .as_array()
            .expect("rules")
            .iter()
            .any(|rule| rule["outbound"] == "private-wg")
    );
    let provider = document["route"]["rule_set"]
        .as_array()
        .expect("rule sets")
        .iter()
        .find(|rule_set| rule_set["tag"] == "sites")
        .expect("provider");
    assert_eq!(provider["type"], "inline");
    assert_eq!(provider["rules"][0]["domain_suffix"][0], "example.com");
    assert_eq!(document["experimental"]["cache_file"]["enabled"], true);
    assert_eq!(
        document["experimental"]["clash_api"]["default_mode"],
        "rule"
    );
}

#[test]
fn custom_nodes_follow_profile_reference_order() {
    let mut request = CompileRequest {
        protocol: 1,
        profile: serde_json::from_value(json!({
            "custom_node_ids": ["second", "first"]
        }))
        .expect("profile"),
        snapshots: vec![],
        custom_nodes: vec![
            serde_json::from_value(json!({
                "id": "first", "name": "first",
                "proxy": { "name": "first", "type": "socks5", "server": "first.test", "port": 1080 }
            }))
            .expect("first"),
            serde_json::from_value(json!({
                "id": "second", "name": "second",
                "proxy": { "name": "second", "type": "socks5", "server": "second.test", "port": 1080 }
            }))
            .expect("second"),
        ],
        target: Target::parse("sing-box-v12").expect("target"),
    };
    request.profile.groups.clear();
    let result = compile(&request).expect("compile");
    let document: Value = serde_json::from_str(&result.content).expect("JSON");
    assert_eq!(document["outbounds"][2]["tag"], "second");
    assert_eq!(document["outbounds"][3]["tag"], "first");
}

#[test]
fn legacy_sing_box_ignores_private_access() {
    let document = compile_document("sing-box");
    assert!(document.get("endpoints").is_none());
    assert!(
        document["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .all(|outbound| outbound["tag"] != "private-wg")
    );
}
