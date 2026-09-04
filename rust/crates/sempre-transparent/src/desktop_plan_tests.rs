use serde_json::json;

use super::*;

#[test]
fn windivert_frontend_accepts_v13_without_a_tun() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.json");
    let config = json!({
        "inbounds": [{ "type": "direct", "tag": "sempre-dns-core-in", "listen": "127.0.0.1", "listen_port": sempre_converter::DEFAULT_CORE_DNS_PORT, "override_address": "1.1.1.1", "override_port": 53 }],
        "route": { "rules": [{ "inbound": "sempre-dns-core-in", "action": "sniff" }, { "inbound": "sempre-dns-core-in", "protocol": "dns", "action": "hijack-dns" }] }
    });
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let profile = serde_json::from_value(
        json!({ "dns": { "shared": { "systemDnsTakeoverEnabled": true } } }),
    )
    .unwrap();
    let plan = prepare(
        Platform::WindowsDivert,
        "sing-box",
        "1.13.18",
        &profile,
        &path,
        vec!["223.5.5.5".into()],
    )
    .unwrap();
    assert_eq!(plan.system_dns.unwrap().listen_port, 1054);
    let output: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(output["inbounds"], config["inbounds"]);
}

#[test]
fn windivert_keeps_fake_ip_routes_without_dns_capture_routes() {
    let mut config = json!({
        "dns": {"servers": [{"type": "fakeip", "inet4_range": "198.18.0.0/15"}]},
        "inbounds": [{"type": "tun", "route_address": ["10.8.28.0/24"]}],
        "route": {"rules": []}
    });
    let profile =
        serde_json::from_value(json!({"dns": {"shared": {"systemDnsTakeoverEnabled": true}}}))
            .unwrap();
    let plan = Plan {
        core: "sing-box".into(),
        system_dns: managed_frontend_plan(
            Platform::WindowsDivert,
            "sing-box",
            &profile,
            vec!["223.5.5.5".into()],
        ),
        ..Plan::default()
    };
    configure_windows_routes(&mut config, &plan).unwrap();
    assert_eq!(
        config["inbounds"][0]["route_address"],
        json!(["10.8.28.0/24", "198.18.0.0/15"])
    );
    assert!(config["inbounds"][0]["dns_mode"].is_null());
    assert!(config["inbounds"][0]["dns_address"].is_null());
    assert_eq!(
        config["inbounds"][0]["route_exclude_address"],
        json!(["223.5.5.5/32"])
    );
}

#[test]
fn managed_frontend_plan_does_not_depend_on_core_runtime_state() {
    let mut profile = Profile {
        dns: json!({ "shared": {
            "systemDnsTakeoverEnabled": true,
            "managedDnsFrontend": true,
            "systemDnsListenPort": 53,
            "systemDnsListenHosts": ["127.0.0.1"]
        }}),
        ..Profile::default()
    };
    profile.transparent_proxy.tproxy.dns_listen_port = 2053;

    let plan = managed_frontend_plan(
        Platform::Macos,
        "sing-box",
        &profile,
        vec!["223.5.5.5".into()],
    )
    .expect("frontend plan");

    assert_eq!(plan.listen_port, 53);
    assert_eq!(plan.core_listen_port, 2053);
    assert_eq!(plan.original_upstreams, ["223.5.5.5"]);
}

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
        output["route"]["rules"][0],
        json!({
            "ip_cidr": ["223.6.6.6/32", "2400:3200::1/128"],
            "port": [53], "action": "route", "outbound": "direct"
        })
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
    assert_eq!(plan.system_dns.expect("system DNS").listen_port, 1054);
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
            "override_address": "127.0.0.1", "override_port": 1054,
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
            listen_port: 1054,
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

#[test]
fn windows_rejects_core_without_configurable_tun_dns() {
    let mut config = json!({
        "inbounds": [{ "type": "tun", "tag": "tun-in" }],
        "route": { "rules": [] }
    });
    let plan = Plan {
        core: "sing-box".into(),
        system_dns: Some(crate::SystemDnsPlan {
            listen_port: 1054,
            listen_hosts: vec!["127.0.0.1".into()],
            core_listen_port: 1053,
            original_upstreams: vec!["223.6.6.6".into()],
            managed_frontend: true,
            takeover_host: true,
        }),
        ..Plan::default()
    };
    let error = configure_windows_dns_redirect(&mut config, &plan, "1.13.18")
        .expect_err("sing-box 1.13 must be rejected");
    assert!(error.to_string().contains("requires sing-box 1.14"));
}
