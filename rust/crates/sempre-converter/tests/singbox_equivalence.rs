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
fn managed_desktop_private_access_routes_dns_and_traffic_through_core() {
    let profile: Profile = serde_json::from_value(json!({
        "dns": { "shared": {
            "systemDnsTakeoverEnabled": true,
            "systemDnsListenHosts": ["127.0.0.1"],
            "systemDnsListenPort": 53,
            "fakeipEnabled": true
        }},
        "private_access": {
            "enabled": true,
            "connectors": [{
                "type": "wireguard", "tag": "private-wg",
                "endpoint": {
                    "privateKey": "private", "address": ["192.0.2.2/32"],
                    "peers": [{
                        "address": "vpn.example.com", "port": 51820,
                        "publicKey": "public", "allowedIps": ["0.0.0.0/0"]
                    }]
                },
                "routes": { "ipCidrs": ["10.8.28.0/24"] },
                "dns": [{
                    "tag": "private-dns", "server": "10.8.28.1",
                    "domainSuffixes": ["internal.example"]
                }]
            }]
        }
    }))
    .expect("profile");
    for format in ["sing-box-v13-macos", "sing-box-v14-windows"] {
        let result = compile(&CompileRequest {
            protocol: 1,
            profile: profile.clone(),
            snapshots: vec![],
            custom_nodes: vec![],
            target: Target::parse(format).expect("target"),
        })
        .expect("compile");
        let document: Value = serde_json::from_str(&result.content).expect("JSON");
        assert_eq!(
            document["experimental"]["cache_file"],
            json!({
                "enabled": true, "path": "cache.db",
                "store_fakeip": true, "store_rdrc": false
            })
        );
        let tun = document["inbounds"]
            .as_array()
            .expect("inbounds")
            .iter()
            .find(|inbound| inbound["tag"] == "tun-in")
            .expect("TUN inbound");

        assert_eq!(
            tun["route_address"],
            json!(["198.18.0.0/15", "fc00::/18", "10.8.28.0/24"])
        );
        let private_dns = document["dns"]["servers"]
            .as_array()
            .expect("DNS servers")
            .iter()
            .find(|server| server["tag"] == "private-dns")
            .expect("private DNS server");
        assert_eq!(private_dns["detour"], "private-wg");
        let dns_rules = document["dns"]["rules"].as_array().expect("DNS rules");
        let private_dns_index = dns_rules
            .iter()
            .position(|rule| rule["server"] == "private-dns")
            .expect("private DNS rule");
        let fakeip_index = dns_rules
            .iter()
            .position(|rule| rule["server"] == "fakeip")
            .expect("FakeIP rule");
        assert!(private_dns_index < fakeip_index);
        assert!(
            document["route"]["rules"]
                .as_array()
                .expect("route rules")
                .iter()
                .any(|rule| {
                    rule["ip_cidr"] == json!(["10.8.28.0/24"]) && rule["outbound"] == "private-wg"
                })
        );
    }
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

#[test]
fn multiplex_numeric_strings_are_normalized_for_sing_box() {
    let profile: Profile = serde_json::from_value(json!({
        "manual_servers": [{
            "name": "edge", "type": "vless", "server": "edge.example.com", "port": 443,
            "uuid": "id", "flow": "", "smux": {
                "enabled": "true", "max-connections": "4", "min-streams": "12", "padding": "false"
            }
        }]
    }))
    .expect("profile");
    let result = compile(&CompileRequest {
        protocol: 1,
        profile,
        snapshots: vec![],
        custom_nodes: vec![],
        target: Target::parse("sing-box-v13").expect("target"),
    })
    .expect("compile");
    let document: Value = serde_json::from_str(&result.content).expect("JSON");
    let edge = &document["outbounds"][2];
    assert_eq!(edge["multiplex"]["max_connections"], 4);
    assert_eq!(edge["multiplex"]["min_streams"], 12);
    assert_eq!(edge["multiplex"]["padding"], false);
    assert!(edge.get("flow").is_none());
}

#[test]
fn anytls_is_omitted_from_sing_box_v11_only() {
    let profile: Profile = serde_json::from_value(json!({
        "manual_servers": [{
            "name": "AnyTLS edge",
            "type": "anytls",
            "server": "edge.example.com",
            "port": 443,
            "password": "secret",
            "sni": "edge.example.com"
        }, {
            "name": "SOCKS fallback",
            "type": "socks5",
            "server": "1.1.1.1",
            "port": 1080
        }]
    }))
    .expect("profile");

    let legacy = compile(&CompileRequest {
        protocol: 1,
        profile: profile.clone(),
        snapshots: vec![],
        custom_nodes: vec![],
        target: Target::parse("sing-box").expect("target"),
    })
    .expect("legacy compile");
    let legacy_document: Value = serde_json::from_str(&legacy.content).expect("legacy config");
    assert!(
        !legacy_document["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .any(|outbound| outbound["type"] == "anytls")
    );
    assert!(legacy.field_diffs.iter().any(|diff| {
        diff.node == "AnyTLS edge"
            && !diff.represented
            && diff
                .warnings
                .iter()
                .any(|warning| warning.contains("1.12 or newer"))
    }));

    let modern = compile(&CompileRequest {
        protocol: 1,
        profile,
        snapshots: vec![],
        custom_nodes: vec![],
        target: Target::parse("sing-box-v12").expect("target"),
    })
    .expect("modern compile");
    let modern_document: Value = serde_json::from_str(&modern.content).expect("modern config");
    assert!(
        modern_document["outbounds"]
            .as_array()
            .expect("outbounds")
            .iter()
            .any(|outbound| outbound["type"] == "anytls")
    );
}
