use serde_json::{Value, json};

use crate::{CompileError, FieldDiff, Profile, Proxy, Target};

pub(super) fn render(
    profile: &Profile,
    proxies: &[Proxy],
    target: &Target,
) -> Result<(String, Vec<FieldDiff>, Vec<String>), CompileError> {
    let mut outbounds = Vec::new();
    let mut diffs = Vec::new();
    for proxy in proxies {
        let Some(protocol) = protocol(&proxy.proxy_type) else {
            diffs.push(FieldDiff {
                node: proxy.name.clone(),
                represented: false,
                consumed: vec![],
                ignored: vec![],
                dropped: proxy.extra.keys().cloned().collect(),
                warnings: vec![format!(
                    "{}: unsupported proxy type {}",
                    proxy.name, proxy.proxy_type
                )],
                outbound: None,
            });
            continue;
        };
        let settings = match protocol {
            "vmess" | "vless" => {
                json!({ "vnext": [{ "address": proxy.server, "port": proxy.port, "users": [{ "id": proxy.extra.get("uuid").cloned().unwrap_or(Value::Null), "encryption": if protocol == "vless" { "none" } else { "auto" } }] }] })
            }
            "trojan" => {
                json!({ "servers": [{ "address": proxy.server, "port": proxy.port, "password": proxy.extra.get("password").cloned().unwrap_or(Value::Null) }] })
            }
            "shadowsocks" => {
                json!({ "servers": [{ "address": proxy.server, "port": proxy.port, "method": proxy.extra.get("cipher").cloned().unwrap_or(json!("aes-256-gcm")), "password": proxy.extra.get("password").cloned().unwrap_or(Value::Null) }] })
            }
            "socks" => json!({ "servers": [{ "address": proxy.server, "port": proxy.port }] }),
            _ => Value::Null,
        };
        let outbound = json!({ "tag": proxy.name, "protocol": protocol, "settings": settings });
        diffs.push(FieldDiff {
            node: proxy.name.clone(),
            represented: true,
            consumed: proxy.extra.keys().cloned().collect(),
            ignored: vec![],
            dropped: vec![],
            warnings: vec![],
            outbound: Some(outbound.clone()),
        });
        outbounds.push(outbound);
    }
    if outbounds.is_empty() {
        return Err(CompileError::EmptyProfile);
    }
    outbounds.push(json!({ "tag": "direct", "protocol": "freedom" }));
    outbounds.push(json!({ "tag": "block", "protocol": "blackhole" }));
    let config = json!({
        "log": { "loglevel": if profile.log_level == "off" { "none" } else { &profile.log_level } },
        "inbounds": [
            { "tag": "sempre-socks-in", "listen": "127.0.0.1", "port": profile.local_proxy.socks_port, "protocol": "socks", "settings": { "udp": true } },
            { "tag": "sempre-http-in", "listen": "127.0.0.1", "port": profile.local_proxy.http_port, "protocol": "http" }
        ],
        "outbounds": outbounds,
        "routing": { "domainStrategy": "IPIfNonMatch", "rules": [] }
    });
    let mut content = serde_json::to_string_pretty(&config)
        .map_err(|error| CompileError::Render(error.to_string()))?;
    content.push('\n');
    let warnings = if target.format == "v2ray" {
        vec!["v2ray output uses the shared Xray-compatible subset".into()]
    } else {
        vec![]
    };
    Ok((content, diffs, warnings))
}

fn protocol(proxy_type: &str) -> Option<&'static str> {
    match proxy_type {
        "vmess" => Some("vmess"),
        "vless" => Some("vless"),
        "trojan" => Some("trojan"),
        "ss" => Some("shadowsocks"),
        "socks5" => Some("socks"),
        _ => None,
    }
}
