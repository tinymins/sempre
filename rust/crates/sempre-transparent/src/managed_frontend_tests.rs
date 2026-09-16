use std::fs;

use sempre_network::{Interface, Inventory};
use serde_json::json;

use super::*;

#[test]
fn tproxy_does_not_require_a_duplicate_dns_inbound() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("config.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "inbounds": [
                { "type": "tproxy", "tag": "tproxy-in" },
                {
                    "type": "direct", "tag": "sempre-dns-core-in",
                    "listen": "127.0.0.1", "listen_port": 20553,
                    "override_address": "1.1.1.1", "override_port": 53
                }
            ],
            "route": { "rules": [
                { "inbound": "sempre-dns-core-in", "action": "sniff" },
                { "inbound": "sempre-dns-core-in", "protocol": "dns", "action": "hijack-dns" }
            ] }
        }))
        .expect("JSON"),
    )
    .expect("write config");
    let profile: Profile = serde_json::from_value(json!({
        "transparent_proxy": {
            "mode": "tproxy", "capture_host": false, "lan_interfaces": ["eth0"],
            "interface_mode": "all",
            "tun": { "interface_name": "sempre-tun" },
            "tproxy": { "listen_port": 20582, "dns_listen_port": 20553 }
        },
        "dns": { "shared": {
            "systemDnsTakeoverEnabled": true,
            "managedDnsFrontend": true,
            "systemDnsListenPort": 20554,
            "systemDnsListenHosts": ["0.0.0.0"]
        }}
    }))
    .expect("profile");
    let inventory = Inventory {
        interfaces: vec![Interface {
            name: "eth0".into(),
            up: true,
            ..Interface::default()
        }],
        ..Inventory::default()
    };

    let plan = prepare_with_inventory_authorized("sing-box", &profile, &path, &inventory, true)
        .expect("prepare managed frontend TProxy");

    assert_eq!(plan.dns_port, 20_554);
    assert_eq!(
        plan.system_dns.expect("system DNS").core_listen_port,
        sempre_converter::DEFAULT_CORE_DNS_PORT
    );
}
