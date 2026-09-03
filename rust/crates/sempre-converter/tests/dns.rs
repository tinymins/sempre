use sempre_converter::{CompileRequest, Target, compile};
use serde_json::{Value, json};

fn request(format: &str) -> CompileRequest {
    CompileRequest {
        protocol: 1,
        profile: serde_json::from_value(json!({
            "name": "DNS",
            "groups": [{ "name": "foreign", "type": "select" }],
            "manual_servers": [{
                "name": "edge", "type": "socks5",
                "server": "edge.example.com", "port": 1080
            }],
            "dns": { "shared": {
                "localDnsTransport": "tls", "localDns": "223.5.5.5",
                "localDnsPort": 853, "localServerName": "dns.alidns.com",
                "remoteDns": "9.9.9.9", "remoteDnsPort": 853,
                "remoteServerName": "dns.quad9.net", "fakeipTtl": 180,
                "cnDomainRuleSetUrl": "https://rules.example/geosite-cn.srs",
                "cnIpRuleSetUrl": "https://rules.example/geoip-cn.srs",
                "hkIpRuleSetUrl": "https://rules.example/geoip-hk.srs",
                "hkIpRuleSetDetour": "foreign"
            }}
        }))
        .expect("profile"),
        snapshots: vec![],
        custom_nodes: vec![],
        target: Target {
            core: String::new(),
            format: format.into(),
            version: String::new(),
            platform: String::new(),
            standalone: false,
        },
    }
}

#[test]
fn sing_box_compiles_shared_dns_for_modern_and_legacy_schemas() {
    let modern = compile(&request("sing-box-v13")).expect("modern DNS");
    let modern: Value = serde_json::from_str(&modern.content).expect("modern JSON");
    assert_eq!(modern["dns"]["servers"][0]["type"], "tls");
    assert_eq!(modern["dns"]["servers"][0]["server_port"], 853);
    assert_eq!(
        modern["dns"]["servers"][0]["tls"]["server_name"],
        "dns.alidns.com"
    );
    assert_eq!(
        modern["dns"]["rules"][0]["domain"],
        json!(["edge.example.com"])
    );
    assert_eq!(modern["dns"]["rules"][0]["server"], "bootstrap");
    assert_eq!(
        modern["route"]["default_domain_resolver"]["server"],
        "bootstrap"
    );
    assert!(
        modern["route"]["rule_set"]
            .as_array()
            .is_some_and(|values| {
                values.iter().any(|value| {
                    value["tag"] == "geoip-hk"
                        && value["download_detour"] == "foreign"
                        && value["format"] == "binary"
                })
            })
    );

    let legacy = compile(&request("sing-box")).expect("legacy DNS");
    let legacy: Value = serde_json::from_str(&legacy.content).expect("legacy JSON");
    assert_eq!(
        legacy["dns"]["servers"][0]["address"],
        "tls://223.5.5.5:853"
    );
    assert_eq!(legacy["dns"]["fakeip"]["enabled"], true);
    assert!(legacy["outbounds"].as_array().is_some_and(|values| {
        values
            .iter()
            .any(|value| value["tag"] == "dns-out" && value["type"] == "dns")
    }));
}

#[test]
fn sing_box_v14_uses_explicit_dns_response_matching() {
    let output = compile(&request("sing-box-v14-windows")).expect("sing-box v14 DNS");
    let output: Value = serde_json::from_str(&output.content).expect("sing-box JSON");
    let rules = output["dns"]["rules"].as_array().expect("DNS rules");
    let evaluate = rules
        .iter()
        .position(|rule| rule["action"] == "evaluate")
        .expect("response evaluation");
    assert_eq!(rules[evaluate]["server"], "remote");
    assert_eq!(rules[evaluate + 1]["match_response"], true);
    assert_eq!(rules[evaluate + 1]["action"], "route");
    assert_eq!(rules[evaluate + 1]["server"], "local");
    let geoip = rules
        .iter()
        .find(|rule| rule["type"] == "logical")
        .expect("GeoIP response rule");
    assert!(geoip["rules"].as_array().is_some_and(|sub_rules| {
        sub_rules
            .iter()
            .all(|sub_rule| sub_rule["match_response"] == true)
    }));
}

#[test]
fn sing_box_native_override_wins_and_macos_removes_fakeip() {
    let mut input = request("sing-box-v13");
    input.profile.dns = json!({
        "modes": { "sing_box_v12": "native" },
        "overrides": { "sing_box_v12": {
            "servers": [{ "type": "fakeip", "tag": "fakeip" }, { "type": "local", "tag": "local" }],
            "rules": [{ "server": "fakeip" }], "final": "local"
        }}
    });
    let native = compile(&input).expect("native override");
    let native: Value = serde_json::from_str(&native.content).expect("native JSON");
    assert_eq!(native["dns"]["final"], "local");
    assert_eq!(native["dns"]["servers"].as_array().map(Vec::len), Some(2));

    input.target.format = "sing-box-v13-macos".into();
    let macos = compile(&input).expect("macOS override");
    let macos: Value = serde_json::from_str(&macos.content).expect("macOS JSON");
    assert_eq!(macos["dns"]["servers"].as_array().map(Vec::len), Some(1));
    assert_eq!(macos["dns"]["rules"].as_array().map(Vec::len), Some(0));
}

#[test]
fn mihomo_and_clash_rs_compile_managed_dns() {
    let mut mihomo = request("clash-meta");
    mihomo.target.core = "mihomo".into();
    let output = compile(&mihomo).expect("Mihomo DNS");
    let output: Value = serde_yaml::from_str(&output.content).expect("Mihomo YAML");
    assert_eq!(output["dns"]["enhanced-mode"], "fake-ip");
    assert_eq!(
        output["dns"]["nameserver"][0],
        "tls://9.9.9.9:853#foreign&disable-qtype-65=true"
    );
    assert_eq!(output["dns"]["fake-ip-ttl"], 180);
    assert_eq!(output["sniffer"]["enable"], true);

    let clash_rs = compile(&request("clash-rs")).expect("clash-rs DNS");
    let clash_rs: Value = serde_yaml::from_str(&clash_rs.content).expect("clash-rs YAML");
    assert_eq!(
        clash_rs["dns"]["nameserver-policy"]["geosite:cn"],
        "udp://223.5.5.5:853"
    );
    assert_eq!(clash_rs["dns"]["fake-ip-range"], "198.18.0.0/15");
}

#[test]
fn v2ray_family_compiles_split_dns_and_native_override() {
    let output = compile(&request("xray")).expect("Xray DNS");
    let output: Value = serde_json::from_str(&output.content).expect("Xray JSON");
    assert_eq!(output["dns"]["queryStrategy"], "UseIPv4");
    assert_eq!(
        output["dns"]["servers"][0]["domains"],
        json!(["full:edge.example.com"])
    );
    assert_eq!(output["dns"]["hosts"]["dns.quad9.net"], "9.9.9.9");
    assert!(output["routing"]["rules"].as_array().is_some_and(|values| {
        values.iter().any(|value| {
            value["inboundTag"] == json!(["remote-dns"]) && value["balancerTag"] == "foreign"
        })
    }));

    let mut native = request("xray");
    native.profile.dns = json!({
        "modes": { "xray": "native" },
        "overrides": { "xray": { "servers": ["1.1.1.1"], "queryStrategy": "UseIPv6" } }
    });
    let native = compile(&native).expect("native Xray DNS");
    let native: Value = serde_json::from_str(&native.content).expect("native JSON");
    assert_eq!(native["dns"]["queryStrategy"], "UseIPv6");
    assert_eq!(native["dns"]["servers"], json!(["1.1.1.1"]));
}

fn takeover_request(format: &str) -> CompileRequest {
    let mut input = request(format);
    input.profile.dns["shared"]["systemDnsTakeoverEnabled"] = json!(true);
    input.profile.dns["shared"]["systemDnsListenHosts"] = json!(["127.0.0.1"]);
    input
}

#[test]
fn sing_box_system_dns_takeover_supports_linux() {
    let mut input = takeover_request("sing-box-v13");
    let output = compile(&input).expect("Linux system DNS");
    let output: Value = serde_json::from_str(&output.content).expect("sing-box JSON");
    assert!(output["inbounds"].as_array().is_some_and(|values| {
        values.iter().any(|value| {
            value["tag"] == "system-dns-in"
                && value["listen"] == "127.0.0.1"
                && value["listen_port"] == 53
                && value["override_address"] == "1.1.1.1"
        })
    }));
    assert!(output["route"]["rules"].as_array().is_some_and(|rules| {
        rules.windows(2).any(|rules| {
            rules[0]["inbound"] == "system-dns-in"
                && rules[0]["action"] == "sniff"
                && rules[1]["inbound"] == "system-dns-in"
                && rules[1]["action"] == "hijack-dns"
        })
    }));

    input.profile.dns["shared"]["localDns"] = json!("local");
    input.profile.dns["shared"]["localDnsTransport"] = json!("system");
    assert!(
        compile(&input)
            .expect_err("recursive local resolver must fail")
            .to_string()
            .contains("explicit local DNS")
    );
}

fn assert_managed_desktop_frontend(format: &str) {
    let mut input = takeover_request(format);
    let output = compile(&input).expect("managed desktop frontend");
    let output: Value = serde_json::from_str(&output.content).expect("sing-box JSON");
    let inbounds = output["inbounds"].as_array().expect("inbounds");
    assert!(inbounds.iter().any(|inbound| {
        inbound["tag"] == "sempre-dns-core-in"
            && inbound["listen"] == "127.0.0.1"
            && inbound["listen_port"] == 1053
    }));
    let tun = inbounds
        .iter()
        .find(|inbound| inbound["tag"] == "tun-in")
        .expect("TUN inbound");
    assert_eq!(tun["route_address"], json!(["198.18.0.0/15", "fc00::/18"]));
    assert!(output["dns"]["rules"].as_array().is_some_and(|rules| {
        rules.iter().any(|rule| {
            rule["inbound"] == json!(["sempre-dns-core-in"]) && rule["server"] == "fakeip"
        })
    }));

    input.profile.dns["shared"]["fakeipEnabled"] = json!(false);
    let output = compile(&input).expect("managed desktop real-IP frontend");
    let output: Value = serde_json::from_str(&output.content).expect("sing-box JSON");
    let tun = output["inbounds"]
        .as_array()
        .expect("inbounds")
        .iter()
        .find(|inbound| inbound["tag"] == "tun-in")
        .expect("TUN inbound");
    assert!(tun.get("route_address").is_none());
    assert!(
        output["dns"]["servers"]
            .as_array()
            .is_some_and(|servers| { servers.iter().all(|server| server["type"] != "fakeip") })
    );
}

#[test]
fn managed_desktop_frontend_supports_windows_and_macos_modes() {
    assert_managed_desktop_frontend("sing-box-v14-windows");
    assert_managed_desktop_frontend("sing-box-v13-macos");
}

#[test]
fn managed_desktop_frontend_rejects_legacy_sing_box() {
    for format in ["sing-box-windows", "sing-box-macos"] {
        assert!(
            compile(&takeover_request(format))
                .expect_err("legacy desktop frontend must fail")
                .to_string()
                .contains("1.12 or newer")
        );
    }
}

#[test]
fn managed_windows_frontend_supports_sing_box_v13() {
    assert_managed_desktop_frontend("sing-box-v13-windows");
}

#[test]
fn sing_box_resolves_real_addresses_through_remote_dns_before_domestic_ip_routing() {
    for format in [
        "sing-box-v12",
        "sing-box-v13",
        "sing-box-v14",
        "sing-box-v12-macos",
        "sing-box-v13-macos",
        "sing-box-v14-macos",
        "sing-box-v14-windows",
    ] {
        for fakeip in [false, true] {
            let mut input = request(format);
            input.profile.dns["shared"]["fakeipEnabled"] = json!(fakeip);
            if format.ends_with("macos") || format.ends_with("windows") {
                input.profile.dns["shared"]["systemDnsTakeoverEnabled"] = json!(true);
            }
            input.profile.rules = vec![json!("DOMAIN,explicit.example,DIRECT")];
            input.profile.rule_providers = serde_json::from_value(json!([
                { "tag": "priority", "url": "https://rules.example/first.srs", "priority": true },
                { "tag": "ordinary", "url": "https://rules.example/last.srs" }
            ]))
            .expect("providers");
            let output = compile(&input).expect(format);
            let output: Value = serde_json::from_str(&output.content).expect("sing-box JSON");
            let rules = output["route"]["rules"].as_array().expect("route rules");
            let geoip = rules
                .iter()
                .position(|rule| rule["rule_set"] == json!(["geoip-cn"]))
                .expect("domestic IP route");
            assert_eq!(
                rules[geoip - 1],
                json!({ "action": "resolve", "server": "remote" }),
                "{format}, fakeip={fakeip}"
            );
            let explicit = rules
                .iter()
                .position(|rule| rule["domain"] == "explicit.example")
                .expect("explicit domain rule");
            assert!(explicit < geoip - 1, "preserve explicit routing precedence");
            let position = |tag| {
                rules
                    .iter()
                    .position(|rule| rule["rule_set"] == json!([tag]))
                    .expect(tag)
            };
            assert!(position("priority") < explicit);
            assert!(explicit < position("geosite-cn"));
            assert_eq!(position("geosite-cn") + 2, geoip);
            assert!(geoip < position("ordinary"));
            assert_eq!(rules[position("geosite-cn")]["outbound"], "direct");
            assert_eq!(rules[geoip]["outbound"], "direct");
            assert_eq!(output["route"]["final"], "foreign");
            assert_eq!(output["dns"]["independent_cache"], true);
            let remote = output["dns"]["servers"]
                .as_array()
                .expect("servers")
                .iter()
                .find(|server| server["tag"] == "remote")
                .expect("remote resolver");
            assert_eq!(remote["detour"], "foreign");
            assert!(remote["server"] == "9.9.9.9" || remote["address"] == "tls://9.9.9.9:853");
            assert_eq!(
                rules
                    .iter()
                    .filter(|rule| rule["action"] == "resolve")
                    .count(),
                1,
                "never resolve unknown destinations through local DNS"
            );
        }
    }
}

#[test]
fn sing_box_does_not_add_domestic_resolution_without_ip_rules_or_to_legacy() {
    for (format, enabled) in [("sing-box-v14", false), ("sing-box", true)] {
        let mut input = request(format);
        input.profile.dns["shared"]["cnIpRuleSetEnabled"] = json!(enabled);
        let output = compile(&input).expect("sing-box config");
        let output: Value = serde_json::from_str(&output.content).expect("sing-box JSON");
        assert!(
            output["route"]["rules"]
                .as_array()
                .is_some_and(|rules| { rules.iter().all(|rule| rule["action"] != "resolve") })
        );
    }
}

#[test]
fn sing_box_domestic_switches_only_control_their_own_route_stage() {
    for domains in [false, true] {
        for ips in [false, true] {
            let mut input = request("sing-box-v14");
            input.profile.dns["shared"]["cnDomainRuleSetEnabled"] = json!(domains);
            input.profile.dns["shared"]["cnIpRuleSetEnabled"] = json!(ips);
            let output = compile(&input).expect("compile");
            let output: Value = serde_json::from_str(&output.content).expect("JSON");
            let rules = output["route"]["rules"].as_array().expect("rules");
            for (tag, enabled) in [("geosite-cn", domains), ("geoip-cn", ips)] {
                assert_eq!(
                    rules.iter().any(|rule| rule["rule_set"] == json!([tag])),
                    enabled
                );
                assert_eq!(
                    output["route"]["rule_set"]
                        .as_array()
                        .expect("sets")
                        .iter()
                        .any(|set| set["tag"] == tag),
                    enabled
                );
            }
            assert_eq!(rules.iter().any(|rule| rule["action"] == "resolve"), ips);
        }
    }
}

#[test]
fn native_dns_owns_resolution_without_an_injected_remote_server_reference() {
    let mut input = request("sing-box-v14");
    let native = json!({
        "servers": [{ "type": "local", "tag": "bootstrap" }], "final": "bootstrap"
    });
    input.profile.dns["modes"] = json!({ "sing_box_v12": "native" });
    input.profile.dns["overrides"] = json!({ "sing_box_v12": native });
    let output = compile(&input).expect("native DNS");
    let output: Value = serde_json::from_str(&output.content).expect("JSON");
    assert_eq!(output["dns"], native);
    assert!(
        output["route"]["rules"]
            .as_array()
            .expect("rules")
            .iter()
            .all(|rule| rule["action"] != "resolve")
    );
}
