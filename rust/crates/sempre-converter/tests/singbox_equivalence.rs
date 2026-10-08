use sempre_converter::{
    CompileRequest, DIRECT_OUTBOUND_NAME, Profile, SourceSnapshot, Target, compile,
    rule_provider_snapshot_id,
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
    assert_eq!(document["route"]["find_process"], true);
    assert_eq!(document["inbounds"][2]["sniff"], true);
    assert_eq!(document["inbounds"][2]["sniff_override_destination"], true);
    assert_eq!(document["outbounds"][0]["tag"], DIRECT_OUTBOUND_NAME);
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
                "homeNetwork": {
                    "enabled": true,
                    "networkIds": ["d286d2f8-33c5-4f1e-b871-d22a9ba91143"]
                },
                "routes": { "ipCidrs": ["10.8.28.0/24"] },
                "dns": [{
                    "tag": "private-dns", "server": "10.8.28.1",
                    "domainSuffixes": ["internal.example"]
                }]
            }]
        },
        "network_policy": {
            "enabled": true,
            "directNetworkIds": [
                "d286d2f8-33c5-4f1e-b871-d22a9ba91143",
                "450c5c7f-6ac8-4433-92a2-a4991dd06cc4"
            ]
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
        assert_home_auto_rules(&document);
    }
}

fn assert_home_auto_rules(document: &Value) {
    let servers = document["dns"]["servers"].as_array().expect("DNS servers");
    let direct_server = servers
        .iter()
        .find(|server| server["tag"] == "private-dns-home-direct")
        .expect("direct private DNS server");
    assert!(direct_server.get("detour").is_none());
    let dns_rules = document["dns"]["rules"].as_array().expect("DNS rules");
    let dns_index = |tag| {
        dns_rules
            .iter()
            .position(|rule| rule["server"] == tag)
            .expect("DNS rule")
    };
    let direct_dns = dns_index("private-dns-home-direct");
    assert!(
        direct_dns < dns_index("private-dns") && dns_index("private-dns") < dns_index("fakeip")
    );
    assert_eq!(
        dns_rules[direct_dns]["clash_mode"],
        json!("Sempre Network d286d2f8-33c5-4f1e-b871-d22a9ba91143")
    );
    assert_eq!(
        dns_rules
            .iter()
            .filter_map(|rule| rule.get("clash_mode")?.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Sempre Network d286d2f8-33c5-4f1e-b871-d22a9ba91143",
            "Sempre Network d286d2f8-33c5-4f1e-b871-d22a9ba91143",
            "Sempre Network 450c5c7f-6ac8-4433-92a2-a4991dd06cc4",
        ]
    );

    let route_rules = document["route"]["rules"].as_array().expect("route rules");
    let route_index = |outbound| {
        route_rules
            .iter()
            .position(|rule| {
                rule["ip_cidr"] == json!(["10.8.28.0/24"]) && rule["outbound"] == outbound
            })
            .expect("private route rule")
    };
    let direct = route_index(DIRECT_OUTBOUND_NAME);
    assert!(direct < route_index("private-wg"));
    assert_eq!(
        route_rules[direct]["clash_mode"],
        json!("Sempre Network d286d2f8-33c5-4f1e-b871-d22a9ba91143")
    );
    let public_direct = route_rules
        .iter()
        .position(|rule| {
            rule["outbound"] == DIRECT_OUTBOUND_NAME
                && rule["clash_mode"] == "Sempre Network d286d2f8-33c5-4f1e-b871-d22a9ba91143"
                && rule.get("ip_cidr").is_none()
        })
        .expect("public direct rule");
    assert!(route_index("private-wg") < public_direct);
    assert_eq!(
        route_rules
            .iter()
            .filter_map(|rule| rule.get("clash_mode")?.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Sempre Network d286d2f8-33c5-4f1e-b871-d22a9ba91143",
            "Sempre Network d286d2f8-33c5-4f1e-b871-d22a9ba91143",
            "Sempre Network 450c5c7f-6ac8-4433-92a2-a4991dd06cc4",
        ]
    );
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
