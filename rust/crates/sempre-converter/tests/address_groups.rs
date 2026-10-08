use sempre_converter::{CompileRequest, DIRECT_OUTBOUND_NAME, Profile, Target, compile};
use serde_json::{Value, json};

const PRIVATE: &str = "内网地址";
const CHINA: &str = "中国地址";
const FOREIGN: &str = "🔰 国外流量";
const NODE: &str = "example-node";

fn profile() -> Profile {
    serde_json::from_value(json!({
        "manual_servers": [{
            "name": NODE, "type": "socks5", "server": "192.0.2.10", "port": 1080
        }],
        "groups": [{ "name": FOREIGN, "type": "select", "proxies": ["DIRECT", NODE] }]
    }))
    .expect("profile")
}

fn render(profile: Profile, format: &str) -> Value {
    let result = compile(&CompileRequest {
        protocol: 1,
        profile,
        snapshots: vec![],
        custom_nodes: vec![],
        target: Target::parse(format).expect("target"),
    })
    .expect("compile");
    if format.starts_with("clash") {
        serde_yaml::from_str(&result.content).expect("Clash YAML")
    } else {
        serde_json::from_str(&result.content).expect("sing-box JSON")
    }
}

fn sing_box_group<'a>(config: &'a Value, name: &str) -> &'a Value {
    config["outbounds"]
        .as_array()
        .expect("outbounds")
        .iter()
        .find(|group| group["tag"] == name)
        .expect("address selector")
}

fn clash_group<'a>(config: &'a Value, name: &str) -> &'a Value {
    config["proxy-groups"]
        .as_array()
        .expect("proxy groups")
        .iter()
        .find(|group| group["name"] == name)
        .expect("address selector")
}

#[test]
fn sing_box_versions_route_private_and_cn_traffic_to_independent_selectors() {
    for base in ["sing-box", "sing-box-v12", "sing-box-v13", "sing-box-v14"] {
        for format in [
            base.to_owned(),
            format!("{base}-openwrt"),
            format!("{base}-windows"),
            format!("{base}-macos"),
        ] {
            let config = render(profile(), &format);
            for name in [PRIVATE, CHINA] {
                let group = sing_box_group(&config, name);
                assert_eq!(group["type"], "selector", "{format}: {name}");
                assert_eq!(group["default"], DIRECT_OUTBOUND_NAME, "{format}: {name}");
                let members = group["outbounds"].as_array().expect("selector members");
                for member in [DIRECT_OUTBOUND_NAME, FOREIGN, NODE] {
                    assert!(
                        members.iter().any(|item| item == member),
                        "{format}: {name} missing {member}"
                    );
                }
            }
            let rules = config["route"]["rules"].as_array().expect("route rules");
            assert!(
                rules
                    .iter()
                    .any(|rule| rule["ip_is_private"] == true && rule["outbound"] == PRIVATE),
                "{format}: private route"
            );
            for tag in ["geosite-cn", "geoip-cn"] {
                assert!(
                    rules
                        .iter()
                        .any(|rule| rule["rule_set"] == json!([tag]) && rule["outbound"] == CHINA),
                    "{format}: {tag} route"
                );
            }
        }
    }
}

#[test]
fn clash_targets_route_private_and_both_cn_matches_to_address_groups() {
    for format in ["clash", "clash-meta", "clash-rs"] {
        let config = render(profile(), format);
        for name in [PRIVATE, CHINA] {
            let group = clash_group(&config, name);
            assert_eq!(group["type"], "select", "{format}: {name}");
            let members = group["proxies"].as_array().expect("selector members");
            assert_eq!(
                members.first(),
                Some(&json!("DIRECT")),
                "{format}: {name} default"
            );
            for member in [FOREIGN, NODE] {
                assert!(
                    members.iter().any(|item| item == member),
                    "{format}: {name} missing {member}"
                );
            }
        }
        let rules = config["rules"].as_array().expect("Clash rules");
        let rules = rules.iter().filter_map(Value::as_str).collect::<Vec<_>>();
        assert!(
            rules.contains(&format!("GEOIP,LAN,{PRIVATE},no-resolve").as_str()),
            "{format}: private route"
        );
        assert!(
            rules.contains(&format!("GEOIP,CN,{CHINA},no-resolve").as_str()),
            "{format}: CN IP route"
        );
        let suffix = format!(",{CHINA}");
        let cn_domain = rules
            .iter()
            .find_map(|rule| {
                rule.strip_prefix("RULE-SET,")?
                    .strip_suffix(suffix.as_str())
            })
            .expect("CN domain rule");
        let provider = &config["rule-providers"][cn_domain];
        assert_eq!(
            provider["behavior"], "domain",
            "{format}: CN domain provider"
        );
    }
}

#[test]
fn custom_same_name_groups_keep_their_existing_default_and_members() {
    let mut input = profile();
    input.groups.push(
        serde_json::from_value(json!({
            "name": PRIVATE, "type": "select", "proxies": [FOREIGN, "DIRECT"],
            "default": FOREIGN, "readonly": true
        }))
        .expect("private group"),
    );
    input.groups.push(
        serde_json::from_value(json!({
            "name": CHINA, "type": "select", "proxies": [NODE, "DIRECT"],
            "default": NODE, "readonly": true
        }))
        .expect("China group"),
    );
    for format in ["sing-box-v13", "clash-meta"] {
        let config = render(input.clone(), format);
        if format.starts_with("clash") {
            assert_eq!(
                clash_group(&config, PRIVATE)["proxies"],
                json!([FOREIGN, "DIRECT"])
            );
            assert_eq!(
                clash_group(&config, CHINA)["proxies"],
                json!([NODE, "DIRECT"])
            );
            for name in [PRIVATE, CHINA] {
                assert_eq!(
                    config["proxy-groups"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|group| group["name"] == name)
                        .count(),
                    1
                );
            }
        } else {
            assert_eq!(sing_box_group(&config, PRIVATE)["default"], FOREIGN);
            assert_eq!(sing_box_group(&config, CHINA)["default"], NODE);
            for name in [PRIVATE, CHINA] {
                assert_eq!(
                    config["outbounds"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|group| group["tag"] == name)
                        .count(),
                    1
                );
            }
        }
    }
}
