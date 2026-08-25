use serde_json::{Value, json};

use crate::{Profile, Target, TransparentProxy};

const DISABLED: &str = "disabled";
const TPROXY: &str = "tproxy";
const TUN: &str = "tun-router";

pub(super) fn sing_box_inbounds(profile: &Profile, target: &Target) -> Vec<Value> {
    if target.platform != "default" {
        let mut inbound = json!({
            "type": "tun", "tag": "tun-in", "address": ["172.19.0.1/30"],
            "auto_route": true, "strict_route": true, "stack": "mixed"
        });
        if target.platform == "windows" {
            inbound["interface_name"] = json!("sing-box");
        }
        return vec![inbound];
    }
    let transparent = &profile.transparent_proxy;
    match transparent.mode.as_str() {
        "" | DISABLED => Vec::new(),
        TUN => vec![sing_box_tun(transparent)],
        TPROXY => sing_box_tproxy(transparent, target.version != "11"),
        _ => Vec::new(),
    }
}

fn sing_box_tun(config: &TransparentProxy) -> Value {
    let address = value_or(&config.tun.address, "172.19.0.1/30");
    let mut inbound = json!({
        "type": "tun", "tag": "tun-in", "interface_name": config.tun.interface_name,
        "address": [address], "auto_route": true, "auto_redirect": true,
        "strict_route": true, "stack": "system"
    });
    if !config.route_exclusions.is_empty() {
        inbound["route_exclude_address"] = json!(config.route_exclusions);
    }
    inbound
}

fn sing_box_tproxy(config: &TransparentProxy, modern: bool) -> Vec<Value> {
    let mut dns = json!({
        "type": "direct", "tag": "dns-in", "listen": "::",
        "listen_port": config.tproxy.dns_listen_port
    });
    let mut tproxy = json!({
        "type": "tproxy", "tag": "tproxy-in", "listen": "::",
        "listen_port": config.tproxy.listen_port, "tcp_multi_path": false,
        "tcp_fast_open": true, "udp_fragment": true
    });
    if !modern {
        dns["sniff"] = json!(true);
        tproxy["sniff"] = json!(true);
        tproxy["sniff_override_destination"] = json!(false);
    }
    vec![dns, tproxy]
}

pub(super) fn apply_clash(profile: &Profile, target: &Target, config: &mut Value) {
    let transparent = &profile.transparent_proxy;
    match transparent.mode.as_str() {
        TUN => config["tun"] = clash_tun(transparent, target.core == "clash-rs"),
        TPROXY => {
            config["tproxy-port"] = json!(transparent.tproxy.listen_port);
            let listener = json!({
                "name": "sempre-dns-in", "type": "tproxy", "listen": "0.0.0.0",
                "port": transparent.tproxy.dns_listen_port, "udp": true
            });
            config["listeners"] = json!([listener]);
        }
        _ => {}
    }
}

fn clash_tun(config: &TransparentProxy, clash_rs: bool) -> Value {
    if clash_rs {
        return json!({
            "enable": true, "device": config.tun.interface_name,
            "gateway": value_or(&config.tun.address, "198.18.0.1/30"),
            "route-all": true, "dns-hijack": true
        });
    }
    let mut tun = json!({
        "enable": true, "stack": "system", "device": config.tun.interface_name,
        "auto-route": true, "auto-redirect": true, "strict-route": true,
        "auto-detect-interface": true, "dns-hijack": ["any:53", "tcp://any:53"]
    });
    if !config.route_exclusions.is_empty() {
        tun["route-exclude-address"] = json!(config.route_exclusions);
    }
    match config.interface_mode.as_str() {
        "include" => tun["include-interface"] = json!(config.interfaces),
        "exclude" => tun["exclude-interface"] = json!(config.interfaces),
        _ => {}
    }
    tun
}

pub(super) fn v2ray_inbounds(profile: &Profile, target: &Target) -> Vec<Value> {
    let transparent = &profile.transparent_proxy;
    match transparent.mode.as_str() {
        TUN if target.core == "xray" => vec![json!({
            "tag": "tun-in", "protocol": "tun",
            "settings": {
                "name": transparent.tun.interface_name, "mtu": 9000,
                "gateway": [value_or(&transparent.tun.address, "172.19.0.1/30")],
                "autoSystemRoutingTable": ["0.0.0.0/0", "::/0"],
                "autoOutboundsInterface": "auto"
            },
            "sniffing": { "enabled": true, "destOverride": ["http", "tls", "quic"] }
        })],
        TPROXY => vec![
            json!({
                "tag": "tproxy-in", "listen": "0.0.0.0",
                "port": transparent.tproxy.listen_port, "protocol": "dokodemo-door",
                "settings": { "network": "tcp,udp", "followRedirect": true },
                "streamSettings": { "sockopt": { "tproxy": "tproxy" } },
                "sniffing": { "enabled": true, "destOverride": ["http", "tls", "quic"] }
            }),
            json!({
                "tag": "dns-in", "listen": "0.0.0.0",
                "port": transparent.tproxy.dns_listen_port, "protocol": "dokodemo-door",
                "settings": { "address": remote_dns(profile), "port": 53, "network": "tcp,udp" }
            }),
        ],
        _ => Vec::new(),
    }
}

fn remote_dns(profile: &Profile) -> &str {
    profile
        .dns
        .get("shared")
        .and_then(|value| value.get("remoteDns"))
        .and_then(Value::as_str)
        .unwrap_or("8.8.8.8")
}

fn value_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}
