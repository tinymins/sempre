use serde_json::json;

use super::*;

#[test]
fn writes_original_dns_bypass_before_global_hijack() {
    let root = tempfile::tempdir().expect("temporary directory");
    let config = root.path().join("config.json");
    fs::write(
        &config,
        serde_json::to_vec(&json!({
            "inbounds": [{
                "type": "direct", "tag": "sempre-dns-core-in", "listen": "127.0.0.1",
                "listen_port": sempre_converter::DEFAULT_CORE_DNS_PORT, "override_address": "1.1.1.1", "override_port": 53
            }],
            "route": { "rules": [
                { "inbound": "sempre-dns-core-in", "action": "sniff" },
                { "inbound": "sempre-dns-core-in", "protocol": "dns", "action": "hijack-dns" },
                { "protocol": "dns", "action": "hijack-dns" }
            ] }
        }))
        .expect("encode config"),
    )
    .expect("write config");
    let profile: Profile = serde_json::from_value(json!({
        "dns": { "shared": { "systemDnsTakeoverEnabled": true } }
    }))
    .expect("profile");
    let plan = prepare(
        Platform::Macos,
        "sing-box",
        "1.13.18",
        &profile,
        &config,
        vec!["223.6.6.6".into(), "2400:3200::1".into()],
    )
    .expect("plan");
    assert_eq!(
        plan.system_dns.expect("system DNS").core_listen_port,
        sempre_converter::DEFAULT_CORE_DNS_PORT
    );
    let output: Value =
        serde_json::from_slice(&fs::read(config).expect("read config")).expect("decode config");
    assert_eq!(
        output["route"]["rules"][2],
        json!({
            "ip_cidr": ["223.6.6.6/32", "2400:3200::1/128"],
            "port": [53], "action": "route", "outbound": "direct"
        })
    );
    assert_eq!(output["route"]["rules"][3]["protocol"], json!("dns"));
}

fn compiled_macos_config() -> Value {
    json!({
        "inbounds": [{
            "type": "direct", "tag": "sempre-dns-core-in", "listen": "127.0.0.1",
            "listen_port": sempre_converter::DEFAULT_CORE_DNS_PORT,
            "override_address": "1.1.1.1", "override_port": 53
        }],
        "outbounds": [{ "type": "direct", "tag": "🚀 直接连接" }],
        "route": { "rules": [
            { "inbound": "sempre-dns-core-in", "action": "sniff" },
            { "inbound": "sempre-dns-core-in", "protocol": "dns", "action": "hijack-dns" },
            { "protocol": "dns", "action": "hijack-dns" }
        ] }
    })
}

fn prepare_macos(config: &Value, upstreams: Vec<String>) -> Value {
    let root = tempfile::tempdir().expect("temporary directory");
    let path = root.path().join("config.json");
    fs::write(&path, serde_json::to_vec(config).expect("encode config")).expect("write config");
    let profile: Profile = serde_json::from_value(json!({
        "dns": { "shared": { "systemDnsTakeoverEnabled": true } }
    }))
    .expect("profile");
    prepare(
        Platform::Macos,
        "sing-box",
        "1.14.3",
        &profile,
        &path,
        upstreams,
    )
    .expect("plan");
    serde_json::from_slice(&fs::read(path).expect("read config")).expect("decode config")
}

#[test]
fn empty_original_dns_inserts_no_bypass() {
    let config = compiled_macos_config();
    let output = prepare_macos(&config, Vec::new());
    assert_eq!(output["route"]["rules"], config["route"]["rules"]);
}

#[test]
fn bypass_uses_compiled_direct_outbound_after_core_dns_hijack() {
    let output = prepare_macos(&compiled_macos_config(), vec!["1.1.1.1".into()]);
    let rules = output["route"]["rules"].as_array().expect("rules");
    assert_eq!(rules[1]["action"], json!("hijack-dns"));
    assert_eq!(
        rules[2],
        json!({
            "ip_cidr": ["1.1.1.1/32"], "port": [53],
            "action": "route", "outbound": "🚀 直接连接"
        })
    );
    assert_eq!(output["outbounds"].as_array().expect("outbounds").len(), 1);
}

#[test]
fn windows_redirect_uses_compiled_direct_outbound() {
    let mut config = json!({
        "inbounds": [{ "type": "tun", "tag": "tun-in" }],
        "outbounds": [{ "type": "direct", "tag": "🚀 直接连接" }],
        "route": { "rules": [] }
    });
    let plan = Plan {
        core: "sing-box".into(),
        system_dns: Some(crate::SystemDnsPlan {
            listen_port: 20_554,
            listen_hosts: vec!["127.0.0.1".into()],
            core_listen_port: 1053,
            original_upstreams: Vec::new(),
            managed_frontend: true,
            takeover_host: true,
        }),
        ..Plan::default()
    };
    configure_windows_dns_redirect(&mut config, &plan, "1.14.3").expect("redirect");
    assert_eq!(
        config["route"]["rules"][0]["outbound"],
        json!("🚀 直接连接")
    );
}

#[test]
fn windows_redirects_dns_to_non_privileged_frontend_and_routes_only_fake_ip() {
    let root = tempfile::tempdir().expect("temporary directory");
    let config = root.path().join("config.json");
    fs::write(
        &config,
        serde_json::to_vec(&json!({
            "dns": { "servers": [{
                "type": "fakeip", "tag": "fakeip",
                "inet4_range": "198.18.0.0/15", "inet6_range": "fc00::/18"
            }] },
            "inbounds": [
                {
                    "type": "tun", "tag": "tun-in", "auto_route": true,
                    "route_address": ["198.18.0.0/15", "10.8.28.0/24"]
                },
                {
                    "type": "direct", "tag": "sempre-dns-core-in", "listen": "127.0.0.1",
                    "listen_port": sempre_converter::DEFAULT_CORE_DNS_PORT, "override_address": "1.1.1.1", "override_port": 53
                }
            ],
            "route": { "rules": [
                { "inbound": "sempre-dns-core-in", "action": "sniff" },
                { "inbound": "sempre-dns-core-in", "protocol": "dns", "action": "hijack-dns" },
                { "protocol": "dns", "action": "hijack-dns" }
            ] }
        }))
        .expect("encode config"),
    )
    .expect("write config");
    let profile: Profile = serde_json::from_value(json!({
        "dns": { "shared": { "systemDnsTakeoverEnabled": true } }
    }))
    .expect("profile");
    let plan = prepare_with_windows_ipv6(
        Platform::Windows,
        "sing-box",
        "1.14.0-beta.13",
        &profile,
        &config,
        vec!["223.6.6.6".into()],
        true,
    )
    .expect("plan");
    assert_eq!(plan.system_dns.expect("system DNS").listen_port, 20_554);
    let output: Value =
        serde_json::from_slice(&fs::read(config).expect("read config")).expect("decode config");
    assert_eq!(
        output["inbounds"][0]["route_exclude_address"],
        json!(["223.6.6.6/32"])
    );
    assert_eq!(output["inbounds"][0]["strict_route"], json!(false));
    assert_eq!(output["inbounds"][0]["dns_mode"], json!("native"));
    assert_eq!(output["inbounds"][0]["dns_address"], json!(["192.0.2.1"]));
    assert_eq!(
        output["inbounds"][0]["route_address"],
        json!(["198.18.0.0/15", "fc00::/18", "10.8.28.0/24", "192.0.2.1/32"])
    );
    assert_eq!(
        output["route"]["rules"][0],
        json!({
            "inbound": "tun-in", "network": ["tcp", "udp"], "port": [53],
            "action": "route", "outbound": "direct",
            "override_address": "127.0.0.1", "override_port": 20_554,
            "udp_connect": true
        })
    );
}

#[test]
fn windows_without_ipv6_routes_only_ipv4_fakeip() {
    let root = tempfile::tempdir().expect("temporary directory");
    let config = root.path().join("config.json");
    fs::write(
        &config,
        serde_json::to_vec(&json!({
            "dns": {
                "servers": [{
                    "type": "fakeip", "tag": "fakeip",
                    "inet4_range": "198.18.0.0/15", "inet6_range": "fc00::/18"
                }],
                "rules": [
                    { "server": "fakeip", "query_type": ["A", "AAAA"], "action": "route" },
                    { "server": "remote", "action": "route" }
                ]
            },
            "inbounds": [
                {
                    "type": "tun", "tag": "tun-in",
                    "route_address": ["198.18.0.0/15", "fc00::/18", "10.8.28.0/24"]
                },
                {
                    "type": "direct", "tag": "sempre-dns-core-in",
                    "listen": "127.0.0.1", "listen_port": sempre_converter::DEFAULT_CORE_DNS_PORT,
                    "override_address": "1.1.1.1", "override_port": 53
                }
            ],
            "route": { "rules": [
                { "inbound": "sempre-dns-core-in", "action": "sniff" },
                { "inbound": "sempre-dns-core-in", "protocol": "dns", "action": "hijack-dns" }
            ] }
        }))
        .expect("encode config"),
    )
    .expect("write config");
    let profile: Profile = serde_json::from_value(json!({
        "dns": { "shared": { "systemDnsTakeoverEnabled": true } }
    }))
    .expect("profile");

    prepare_with_windows_ipv6(
        Platform::WindowsDivert,
        "sing-box",
        "1.13.18",
        &profile,
        &config,
        vec!["223.6.6.6".into()],
        false,
    )
    .expect("plan");

    let output: Value =
        serde_json::from_slice(&fs::read(config).expect("read config")).expect("decode config");
    assert!(output["dns"]["servers"][0]["inet6_range"].is_null());
    assert_eq!(output["dns"]["rules"][0]["query_type"], json!(["A"]));
    assert_eq!(
        output["inbounds"][0]["route_address"],
        json!(["198.18.0.0/15", "10.8.28.0/24"])
    );
}

#[test]
fn windows_real_ip_keeps_full_tun_routing() {
    let mut config = json!({
        "inbounds": [{
            "type": "tun", "tag": "tun-in", "auto_route": true,
            "route_address": ["198.18.0.0/15"]
        }],
        "route": { "rules": [{ "protocol": "dns", "action": "hijack-dns" }] }
    });
    let plan = Plan {
        core: "sing-box".into(),
        system_dns: Some(crate::SystemDnsPlan {
            listen_port: 20_554,
            listen_hosts: vec!["127.0.0.1".into()],
            core_listen_port: 1053,
            original_upstreams: vec!["223.6.6.6".into()],
            managed_frontend: true,
            takeover_host: true,
        }),
        ..Plan::default()
    };
    configure_windows_dns_redirect(&mut config, &plan, "1.14.0-beta.13").expect("redirect");
    assert!(config["inbounds"][0]["route_address"].is_null());
    assert_eq!(config["inbounds"][0]["strict_route"], json!(false));
    assert_eq!(
        config["inbounds"][0]["route_exclude_address"],
        json!(["223.6.6.6/32"])
    );
}
