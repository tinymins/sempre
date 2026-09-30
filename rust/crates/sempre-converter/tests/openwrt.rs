use sempre_converter::{CompileRequest, DIRECT_OUTBOUND_NAME, Profile, Target, compile};
use serde_json::{Value, json};

fn render(version: &str) -> Value {
    render_with_gfw(version, true)
}

fn render_with_gfw(version: &str, gfw_black: bool) -> Value {
    let mut profile: Profile = serde_json::from_value(json!({
        "manual_servers": [{
            "name": "example-node", "type": "socks5", "server": "edge.example.test", "port": 1080
        }],
        "groups": [{
            "name": "🔰 国外流量", "type": "select",
            "proxies": ["DIRECT", "example-node"]
        }],
        "rules": [
            { "domain": "direct", "outbound": "direct" },
            { "type": "logical", "mode": "or", "rules": [
                { "domain_suffix": "example.test", "outbound": "direct" }
            ] }
        ],
        "dns": { "shared": {
            "localDns": "192.0.2.53", "localDnsPort": 53,
            "gfwBlackRuleSetUrl": "https://rules.example.test/gfwblack.json"
        }}
    }))
    .expect("profile");
    if !gfw_black {
        profile.dns["shared"]
            .as_object_mut()
            .expect("DNS settings")
            .remove("gfwBlackRuleSetUrl");
    }
    profile.transparent_proxy.mode = "tproxy".into();
    profile.transparent_proxy.tproxy.dns_listen_port = 1053;
    profile.transparent_proxy.tproxy.listen_port = 7893;
    profile.management_api.external_controller = "0.0.0.0:9999".into();
    profile.management_api.external_ui = "/etc/sb/ui".into();
    let target = Target::parse(&format!("{version}-openwrt")).expect("OpenWrt target");
    let result = compile(&CompileRequest {
        protocol: 1,
        profile,
        snapshots: vec![],
        custom_nodes: vec![],
        target,
    })
    .expect("compile OpenWrt configuration");
    serde_json::from_str(&result.content).expect("JSON")
}

fn assert_direct_outbound_name(config: &Value, version: &str) {
    let outbounds = config["outbounds"].as_array().expect("outbounds");
    assert!(
        outbounds.iter().any(|outbound| {
            outbound["type"] == "direct" && outbound["tag"] == DIRECT_OUTBOUND_NAME
        }),
        "{version}"
    );
    for name in ["🔰 国外流量", "内网地址", "中国地址"] {
        let selector = outbounds
            .iter()
            .find(|outbound| outbound["tag"] == name)
            .expect("selector");
        assert_eq!(
            selector["default"], DIRECT_OUTBOUND_NAME,
            "{version}: {name}"
        );
        assert_eq!(
            selector["outbounds"][0], DIRECT_OUTBOUND_NAME,
            "{version}: {name}"
        );
    }
    let route_rules = config["route"]["rules"].as_array().expect("route rules");
    assert!(
        route_rules
            .iter()
            .any(|rule| { rule["domain"] == "direct" && rule["outbound"] == DIRECT_OUTBOUND_NAME }),
        "{version}"
    );
    assert!(
        route_rules.iter().any(|rule| {
            rule["type"] == "logical" && rule["rules"][0]["outbound"] == DIRECT_OUTBOUND_NAME
        }),
        "{version}"
    );
    let rule_sets = config["route"]["rule_set"].as_array().expect("rule sets");
    assert!(
        rule_sets.iter().all(|set| {
            set.get("download_detour").is_none() || set["download_detour"] == DIRECT_OUTBOUND_NAME
        }),
        "{version}"
    );
    if version == "sing-box" {
        let dns_servers = config["dns"]["servers"].as_array().expect("DNS servers");
        assert!(
            dns_servers.iter().any(|server| {
                server["tag"] == "local" && server["detour"] == DIRECT_OUTBOUND_NAME
            }),
            "{version}"
        );
    }
}

#[test]
fn openwrt_uses_tproxy_local_dns_and_first_selector_member_across_versions() {
    for version in ["sing-box", "sing-box-v12", "sing-box-v13", "sing-box-v14"] {
        let config = render(version);
        let inbounds = config["inbounds"].as_array().expect("inbounds");
        assert_eq!(inbounds.len(), 2, "{version}");
        assert_eq!(inbounds[0]["type"], "direct", "{version}");
        assert_eq!(inbounds[0]["listen_port"], 1053, "{version}");
        assert_eq!(inbounds[1]["type"], "tproxy", "{version}");
        assert_eq!(inbounds[1]["listen_port"], 7893, "{version}");
        assert_direct_outbound_name(&config, version);
        assert_eq!(config["route"]["find_process"], Value::Null, "{version}");
        assert_eq!(
            config["route"]["auto_detect_interface"],
            Value::Null,
            "{version}"
        );
        assert_eq!(
            config["experimental"]["clash_api"]["external_controller"],
            "0.0.0.0:9999"
        );
        assert_eq!(
            config["experimental"]["clash_api"]["external_ui"],
            "/etc/sb/ui"
        );
        assert!(
            config["experimental"]["clash_api"]["external_ui_download_url"]
                .as_str()
                .is_some_and(|url| url.contains("metacubexd"))
        );
        assert_eq!(
            config["experimental"]["cache_file"]["store_fakeip"], true,
            "{version}"
        );

        let route_rules = config["route"]["rules"].as_array().expect("route rules");
        if version == "sing-box" {
            assert_eq!(
                route_rules[0],
                json!({
                    "inbound": ["dns-in"], "protocol": "dns", "outbound": "dns-out"
                })
            );
        } else {
            assert_eq!(
                route_rules[0],
                json!({ "inbound": "dns-in", "action": "hijack-dns" })
            );
            assert_eq!(route_rules[1], json!({ "action": "sniff" }));
        }
        assert!(
            !route_rules
                .iter()
                .any(|rule| { rule["protocol"] == "dns" && rule["action"] == "hijack-dns" })
        );

        let dns_servers = config["dns"]["servers"].as_array().expect("DNS servers");
        assert!(dns_servers.iter().any(|server| server["tag"] == "local"));
        assert!(dns_servers.iter().any(|server| server["tag"] == "fakeip"));
        assert!(
            !dns_servers
                .iter()
                .any(|server| server["tag"] == "bootstrap" || server["tag"] == "remote")
        );
        if version != "sing-box" {
            assert_eq!(
                config["route"]["default_domain_resolver"]["server"], "local",
                "{version}"
            );
            let node = config["outbounds"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["tag"] == "example-node")
                .expect("node");
            assert_eq!(node["domain_resolver"]["server"], "local", "{version}");
        }
        let rule_sets = config["route"]["rule_set"].as_array().expect("rule sets");
        assert!(rule_sets.iter().any(|set| set["tag"] == "geoip-gfwblack"));
        let dns_rules = config["dns"]["rules"].as_array().expect("DNS rules");
        assert!(
            dns_rules
                .iter()
                .any(|rule| rule.to_string().contains("geoip-gfwblack"))
        );
    }
}

#[test]
fn openwrt_v14_uses_response_matching_for_address_filters() {
    let config = render("sing-box-v14");
    let rules = config["dns"]["rules"].as_array().expect("DNS rules");
    let evaluate = rules
        .iter()
        .position(|rule| rule["action"] == "evaluate")
        .expect("evaluate before response checks");
    let private = rules
        .iter()
        .position(|rule| rule.get("ip_cidr").is_some())
        .expect("private address rule");
    assert!(evaluate < private);
    assert_eq!(rules[private]["match_response"], true);
}

#[test]
fn openwrt_omits_gfw_exclusion_when_rule_set_url_is_unavailable() {
    let config = render_with_gfw("sing-box-v13", false);
    assert!(
        !config["route"]["rule_set"]
            .as_array()
            .expect("rule sets")
            .iter()
            .any(|rule_set| rule_set["tag"] == "geoip-gfwblack")
    );
    assert!(
        !config["dns"]["rules"]
            .as_array()
            .expect("DNS rules")
            .iter()
            .any(|rule| rule.to_string().contains("geoip-gfwblack"))
    );
}

#[test]
fn openwrt_keeps_address_group_routes_before_final_proxy() {
    let config = render("sing-box-v13");
    let rules = config["route"]["rules"].as_array().expect("route rules");
    let private = rules
        .iter()
        .position(|rule| rule["ip_is_private"] == true && rule["outbound"] == "内网地址")
        .expect("private address route");
    let cn_domain = rules
        .iter()
        .position(|rule| {
            rule["rule_set"] == json!(["geosite-cn"]) && rule["outbound"] == "中国地址"
        })
        .expect("CN domain address route");
    let cn_ip = rules
        .iter()
        .position(|rule| rule["rule_set"] == json!(["geoip-cn"]) && rule["outbound"] == "中国地址")
        .expect("CN IP address route");
    assert!(private < cn_domain && cn_domain < cn_ip);
    assert_eq!(config["route"]["final"], "🔰 国外流量");
}
