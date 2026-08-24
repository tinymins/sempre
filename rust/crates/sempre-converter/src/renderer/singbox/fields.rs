use std::collections::HashSet;

use serde_json::Value;

pub(super) fn consumed_keys(proxy_type: &str) -> HashSet<&'static str> {
    let common = ["udp", "tfo", "mptcp"];
    let transport = [
        "tls",
        "servername",
        "sni",
        "alpn",
        "skip-cert-verify",
        "client-fingerprint",
        "reality-opts",
        "ws-opts",
        "h2-opts",
        "grpc-opts",
        "smux",
        "multiplex",
        "network",
    ];
    let specific: &[&str] = match proxy_type {
        "vmess" => &["uuid", "cipher", "alterId"],
        "vless" => &["uuid", "flow"],
        "trojan" => &["password"],
        "ss" => &["cipher", "password", "plugin", "plugin-opts"],
        "hysteria2" => &[
            "password",
            "sni",
            "alpn",
            "skip-cert-verify",
            "up",
            "down",
            "ports",
            "obfs",
            "obfs-password",
        ],
        "hysteria" => &[
            "up",
            "down",
            "obfs",
            "auth-str",
            "sni",
            "alpn",
            "skip-cert-verify",
        ],
        "tuic" => &[
            "uuid",
            "password",
            "sni",
            "alpn",
            "skip-cert-verify",
            "udp-relay-mode",
            "congestion-controller",
        ],
        "http" | "socks5" => &["username", "password", "tls", "sni", "skip-cert-verify"],
        "anytls" => &[
            "password",
            "sni",
            "alpn",
            "skip-cert-verify",
            "client-fingerprint",
        ],
        _ => &[],
    };
    let shared_transport = matches!(proxy_type, "vmess" | "vless" | "trojan");
    common
        .into_iter()
        .chain(specific.iter().copied())
        .chain(shared_transport.then_some(transport).into_iter().flatten())
        .collect()
}

pub(super) fn deep_merge(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Object(target), Value::Object(source)) => {
            for (key, value) in source {
                deep_merge(target.entry(key).or_insert(Value::Null), value);
            }
        }
        (target, source) => *target = source.clone(),
    }
}
