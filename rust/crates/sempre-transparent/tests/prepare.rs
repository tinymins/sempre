use std::fs;

use sempre_converter::Profile;
use sempre_network::{Interface, Inventory};
use sempre_transparent::{BYPASS_MARK, Mode, prepare_with_inventory};
use serde_json::{Value, json};

fn profile(mode: &str) -> Profile {
    serde_json::from_value(json!({
        "name": "transparent",
        "transparent_proxy": {
            "mode": mode,
            "capture_host": true,
            "lan_interfaces": [],
            "route_exclusions": ["10.10.0.0/16", "198.18.0.0/15"],
            "interface_mode": "include",
            "interfaces": ["eth0"],
            "auto_exclude_local_routes": true,
            "auto_exclude_vpn_routes": true,
            "tun": { "interface_name": "sempre-tun" },
            "tproxy": { "listen_port": 7893, "dns_listen_port": 1053 },
            "ebpf": { "wan_interface": "auto", "auto_config_kernel_parameter": false }
        }
    }))
    .expect("profile")
}

fn inventory() -> Inventory {
    Inventory {
        supported: true,
        interfaces: vec![Interface {
            name: "eth0".into(),
            index: 2,
            kind: "physical".into(),
            up: true,
            default_route: false,
            addresses: vec!["192.168.1.1/24".into()],
        }],
        recommended_lan_interfaces: vec!["eth0".into()],
        local_prefixes: vec!["192.168.1.0/24".into()],
        vpn_prefixes: vec!["10.64.0.0/10".into()],
        occupied_prefixes: vec!["172.19.0.0/30".into(), "192.168.1.0/24".into()],
        ..Inventory::default()
    }
}

#[test]
fn sing_box_tun_resolves_address_and_dynamic_exclusions() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("config.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "inbounds": [{ "type": "tun", "tag": "tun-in" }],
            "route": {},
            "dns": { "servers": [{
                "type": "fakeip", "inet4_range": "198.18.0.0/15",
                "inet6_range": "fc00::/18"
            }] }
        }))
        .expect("JSON"),
    )
    .expect("write config");
    let plan = prepare_with_inventory("sing-box", &profile("tun-router"), &path, &inventory())
        .expect("prepare TUN");
    assert_eq!(plan.mode, Mode::Tun);
    assert_eq!(plan.tun_address, "172.19.0.5/30");
    assert_eq!(
        plan.route_exclusions,
        ["10.10.0.0/16", "10.64.0.0/10", "192.168.1.0/24"]
    );
    let config: Value =
        serde_json::from_slice(&fs::read(path).expect("read config")).expect("updated JSON");
    assert_eq!(config["inbounds"][0]["address"], json!(["172.19.0.5/30"]));
    assert_eq!(config["inbounds"][0]["include_interface"], json!(["eth0"]));
    assert_eq!(
        config["inbounds"][0]["route_exclude_address"],
        json!(["10.10.0.0/16", "10.64.0.0/10", "192.168.1.0/24"])
    );
}

#[test]
fn sing_box_tproxy_marks_bypass_and_resolves_sources() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("config.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "inbounds": [
                { "type": "tproxy", "tag": "tproxy-in" },
                { "type": "direct", "tag": "dns-in" }
            ],
            "route": {},
            "outbounds": [{ "type": "socks", "server": "203.0.113.7" }]
        }))
        .expect("JSON"),
    )
    .expect("write config");
    let plan = prepare_with_inventory("sing-box", &profile("tproxy"), &path, &inventory())
        .expect("prepare TProxy");
    assert_eq!(plan.mode, Mode::TProxy);
    assert_eq!(plan.lan_interfaces, ["eth0"]);
    assert!(plan.excluded_prefixes.contains(&"203.0.113.7/32".into()));
    assert!(plan.excluded_prefixes.contains(&"192.168.0.0/16".into()));
    let config: Value =
        serde_json::from_slice(&fs::read(path).expect("read config")).expect("updated JSON");
    assert_eq!(config["route"]["default_mark"], BYPASS_MARK);
    assert_eq!(config["route"]["auto_detect_interface"], true);
}

#[test]
fn mihomo_and_xray_tun_runtime_fields_follow_core_contracts() {
    let directory = tempfile::tempdir().expect("directory");
    let mihomo = directory.path().join("mihomo.yaml");
    fs::write(
        &mihomo,
        "tun: {}\ndns:\n  enhanced-mode: fake-ip\n  fake-ip-range: 198.18.0.0/15\n",
    )
    .expect("write Mihomo");
    let plan = prepare_with_inventory("mihomo", &profile("tun-router"), &mihomo, &inventory())
        .expect("prepare Mihomo");
    assert!(plan.tun_address.is_empty());
    let config: Value =
        serde_yaml::from_slice(&fs::read(mihomo).expect("read Mihomo")).expect("Mihomo YAML");
    assert_eq!(config["tun"]["device"], "sempre-tun");
    assert_eq!(config["tun"]["include-interface"], json!(["eth0"]));

    let xray = directory.path().join("xray.json");
    fs::write(
        &xray,
        serde_json::to_vec(&json!({
            "inbounds": [{ "tag": "tun-in", "protocol": "tun", "settings": {} }],
            "routing": { "rules": [] }
        }))
        .expect("JSON"),
    )
    .expect("write Xray");
    let plan = prepare_with_inventory("xray", &profile("tun-router"), &xray, &inventory())
        .expect("prepare Xray");
    assert_eq!(plan.tun_address, "172.19.0.5/30");
    let config: Value =
        serde_json::from_slice(&fs::read(xray).expect("read Xray")).expect("Xray JSON");
    assert_eq!(
        config["inbounds"][0]["settings"]["gateway"],
        json!(["172.19.0.5/30"])
    );
    assert_eq!(config["routing"]["rules"][0]["outboundTag"], "direct");
}

#[test]
fn v2ray_tproxy_marks_every_outbound_socket() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("v2ray.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "inbounds": [
                { "tag": "tproxy-in", "protocol": "dokodemo-door" },
                { "tag": "dns-in", "protocol": "dokodemo-door" }
            ],
            "outbounds": [
                { "protocol": "freedom" },
                { "protocol": "vmess", "streamSettings": { "sockopt": {} } }
            ]
        }))
        .expect("JSON"),
    )
    .expect("write V2Ray");
    prepare_with_inventory("v2ray", &profile("tproxy"), &path, &inventory())
        .expect("prepare V2Ray");
    let config: Value =
        serde_json::from_slice(&fs::read(path).expect("read V2Ray")).expect("V2Ray JSON");
    assert!(config["outbounds"].as_array().is_some_and(|values| {
        values
            .iter()
            .all(|value| value["streamSettings"]["sockopt"]["mark"] == BYPASS_MARK)
    }));
}
