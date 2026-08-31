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

#[test]
fn sing_box_system_dns_takeover_supports_linux_and_managed_macos_frontend() {
    let mut input = request("sing-box-v13");
    input.profile.dns["shared"]["systemDnsTakeoverEnabled"] = json!(true);
    input.profile.dns["shared"]["systemDnsListenHosts"] = json!(["127.0.0.1"]);
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

    input.target.format = "sing-box-v13-macos".into();
    let output = compile(&input).expect("managed macOS frontend");
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
    assert!(
        output["dns"]["servers"]
            .as_array()
            .is_some_and(|servers| { servers.iter().any(|server| server["type"] == "fakeip") })
    );
    assert!(output["dns"]["rules"].as_array().is_some_and(|rules| {
        rules.iter().any(|rule| {
            rule["inbound"] == json!(["sempre-dns-core-in"]) && rule["server"] == "fakeip"
        })
    }));

    input.profile.dns["shared"]["fakeipEnabled"] = json!(false);
    let output = compile(&input).expect("managed macOS real-IP frontend");
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

    input.target.format = "sing-box-macos".into();
    assert!(
        compile(&input)
            .expect_err("legacy macOS frontend must fail")
            .to_string()
            .contains("1.12 or newer")
    );

    input.target.format = "sing-box-v13".into();
    input.profile.dns["shared"]["fakeipEnabled"] = json!(true);
    input.profile.dns["shared"]["localDns"] = json!("local");
    input.profile.dns["shared"]["localDnsTransport"] = json!("system");
    assert!(
        compile(&input)
            .expect_err("recursive local resolver must fail")
            .to_string()
            .contains("explicit local DNS")
    );
}
