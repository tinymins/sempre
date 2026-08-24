use serde_json::{Value, json};

use crate::{CompileError, FieldDiff, Profile, Proxy, ProxyGroup, Target};

mod fields;
use fields::{consumed_keys, deep_merge};

pub(super) fn render(
    profile: &Profile,
    proxies: &[Proxy],
    target: &Target,
) -> Result<(String, Vec<FieldDiff>, Vec<String>), CompileError> {
    let mut outbounds = Vec::new();
    let mut diffs = Vec::new();
    let mut warnings = Vec::new();
    for proxy in proxies {
        let (outbound, diff) = convert_proxy(proxy);
        if let Some(outbound) = outbound {
            outbounds.push(outbound);
        }
        warnings.extend(diff.warnings.clone());
        diffs.push(diff);
    }
    if outbounds.is_empty() {
        return Err(CompileError::EmptyProfile);
    }
    let names: Vec<String> = outbounds
        .iter()
        .filter_map(|value| value["tag"].as_str().map(str::to_owned))
        .collect();
    outbounds.splice(0..0, selector_outbounds(&profile.groups, &names));
    outbounds.extend([
        json!({ "type": "direct", "tag": "direct" }),
        json!({ "type": "block", "tag": "block" }),
    ]);
    let mut config = json!({
        "log": log_config(&profile.log_level),
        "inbounds": local_inbounds(profile),
        "outbounds": outbounds,
        "route": route_config(profile),
        "experimental": management_api(profile)
    });
    if profile.dns.is_object() {
        config["dns"] = profile.dns.clone();
    }
    if let Some(override_value) = profile.core_overrides.get("sing-box") {
        deep_merge(&mut config, override_value);
    }
    normalize_for_version(&mut config, target);
    let mut content = serde_json::to_string_pretty(&config)
        .map_err(|error| CompileError::Render(error.to_string()))?;
    content.push('\n');
    Ok((content, diffs, warnings))
}

fn convert_proxy(proxy: &Proxy) -> (Option<Value>, FieldDiff) {
    let consumed = consumed_keys(&proxy.proxy_type);
    let mut diff = FieldDiff {
        node: proxy.name.clone(),
        represented: true,
        consumed: vec![],
        ignored: vec![],
        dropped: vec![],
        warnings: vec![],
        outbound: None,
    };
    for key in proxy.extra.keys() {
        if consumed.contains(key.as_str()) {
            diff.consumed.push(key.clone());
        } else {
            diff.dropped.push(key.clone());
        }
    }
    let Some(mut outbound) = protocol_outbound(proxy) else {
        diff.represented = false;
        diff.warnings.push(format!(
            "{}: unsupported proxy type {}",
            proxy.name, proxy.proxy_type
        ));
        return (None, diff);
    };
    if boolean(field(proxy, "tfo")) {
        outbound["tcp_fast_open"] = json!(true);
    }
    if boolean(field(proxy, "mptcp")) {
        outbound["tcp_multi_path"] = json!(true);
    }
    if !diff.dropped.is_empty() {
        diff.warnings.push(format!(
            "{}: fields not representable in sing-box: {}",
            proxy.name,
            diff.dropped.join(", ")
        ));
    }
    diff.consumed.sort();
    diff.dropped.sort();
    diff.outbound = Some(outbound.clone());
    (Some(outbound), diff)
}

fn protocol_outbound(proxy: &Proxy) -> Option<Value> {
    let mut outbound =
        json!({ "tag": proxy.name, "server": proxy.server, "server_port": proxy.port });
    match proxy.proxy_type.as_str() {
        "vmess" => {
            outbound["type"] = json!("vmess");
            outbound["uuid"] = field(proxy, "uuid").clone();
            outbound["security"] = default_string(field(proxy, "cipher"), "auto");
            outbound["alter_id"] = json!(number(field(proxy, "alterId")));
            add_transport_tls(&mut outbound, proxy, false);
        }
        "vless" => {
            outbound["type"] = json!("vless");
            outbound["uuid"] = field(proxy, "uuid").clone();
            copy(&mut outbound, proxy, &["flow"]);
            add_transport_tls(&mut outbound, proxy, false);
        }
        "trojan" => {
            outbound["type"] = json!("trojan");
            outbound["password"] = field(proxy, "password").clone();
            add_transport_tls(&mut outbound, proxy, true);
        }
        "ss" => {
            outbound["type"] = json!("shadowsocks");
            outbound["method"] = default_string(field(proxy, "cipher"), "aes-256-gcm");
            outbound["password"] = field(proxy, "password").clone();
        }
        "hysteria2" => {
            outbound["type"] = json!("hysteria2");
            outbound["password"] = field(proxy, "password").clone();
            outbound["tls"] = tls(proxy, true);
            copy_aliases(
                &mut outbound,
                proxy,
                &[("up", "up_mbps"), ("down", "down_mbps")],
            );
        }
        "hysteria" => {
            outbound["type"] = json!("hysteria");
            copy(&mut outbound, proxy, &["up", "down", "obfs"]);
            copy_aliases(&mut outbound, proxy, &[("auth-str", "auth_str")]);
            outbound["tls"] = tls(proxy, true);
        }
        "tuic" => {
            outbound["type"] = json!("tuic");
            copy(&mut outbound, proxy, &["uuid", "password"]);
            outbound["tls"] = tls(proxy, true);
            copy_aliases(
                &mut outbound,
                proxy,
                &[
                    ("udp-relay-mode", "udp_relay_mode"),
                    ("congestion-controller", "congestion_control"),
                ],
            );
        }
        "http" => {
            outbound["type"] = json!("http");
            copy(&mut outbound, proxy, &["username", "password"]);
            if boolean(field(proxy, "tls")) {
                outbound["tls"] = tls(proxy, false);
            }
        }
        "socks5" => {
            outbound["type"] = json!("socks");
            copy(&mut outbound, proxy, &["username", "password"]);
        }
        "anytls" => {
            outbound["type"] = json!("anytls");
            outbound["password"] = field(proxy, "password").clone();
            outbound["tls"] = tls(proxy, true);
        }
        _ => return None,
    }
    Some(outbound)
}

fn add_transport_tls(outbound: &mut Value, proxy: &Proxy, force_tls: bool) {
    if let Some(transport) = transport(proxy) {
        outbound["transport"] = transport;
    }
    if force_tls || boolean(field(proxy, "tls")) {
        outbound["tls"] = tls(proxy, false);
    }
    if let Some(multiplex) = multiplex(proxy) {
        outbound["multiplex"] = multiplex;
    }
}

fn transport(proxy: &Proxy) -> Option<Value> {
    if let Some(options) = field(proxy, "ws-opts").as_object() {
        return Some(
            json!({ "type": "ws", "path": options.get("path").cloned().unwrap_or(json!("/")), "headers": options.get("headers").cloned().unwrap_or(json!({})) }),
        );
    }
    if let Some(options) = field(proxy, "grpc-opts").as_object() {
        return Some(
            json!({ "type": "grpc", "service_name": options.get("grpc-service-name").cloned().unwrap_or(Value::Null) }),
        );
    }
    if let Some(options) = field(proxy, "h2-opts").as_object() {
        return Some(
            json!({ "type": "http", "host": options.get("host").cloned().unwrap_or(Value::Null), "path": options.get("path").cloned().unwrap_or(Value::Null) }),
        );
    }
    None
}

fn tls(proxy: &Proxy, protocol: bool) -> Value {
    let name = ["servername", "sni"]
        .into_iter()
        .map(|key| field(proxy, key))
        .find(|value| value.as_str().is_some_and(|value| !value.is_empty()))
        .cloned()
        .unwrap_or_else(|| {
            if protocol {
                json!(proxy.server)
            } else {
                Value::Null
            }
        });
    let mut result = json!({ "enabled": true });
    if !name.is_null() {
        result["server_name"] = name;
    }
    if !field(proxy, "alpn").is_null() {
        result["alpn"] = field(proxy, "alpn").clone();
    }
    if boolean(field(proxy, "skip-cert-verify")) {
        result["insecure"] = json!(true);
    }
    if let Some(fingerprint) = field(proxy, "client-fingerprint")
        .as_str()
        .filter(|value| !value.is_empty())
    {
        result["utls"] = json!({ "enabled": true, "fingerprint": fingerprint });
    }
    if let Some(reality) = field(proxy, "reality-opts").as_object() {
        result["reality"] = json!({ "enabled": true, "public_key": reality.get("public-key").cloned().unwrap_or(Value::Null), "short_id": reality.get("short-id").cloned().unwrap_or(Value::Null) });
    }
    result
}

fn multiplex(proxy: &Proxy) -> Option<Value> {
    let value = if field(proxy, "smux").is_null() {
        field(proxy, "multiplex")
    } else {
        field(proxy, "smux")
    };
    match value {
        Value::Bool(true) => Some(
            json!({ "enabled": true, "protocol": "h2mux", "max_connections": 8, "min_streams": 16, "padding": true }),
        ),
        Value::Object(options)
            if options.get("enabled").and_then(Value::as_bool) != Some(false) =>
        {
            Some(
                json!({ "enabled": true, "protocol": options.get("protocol").cloned().unwrap_or(json!("h2mux")), "max_connections": options.get("max-connections").cloned().unwrap_or(json!(8)), "min_streams": options.get("min-streams").cloned().unwrap_or(json!(16)), "padding": options.get("padding").cloned().unwrap_or(json!(true)) }),
            )
        }
        _ => None,
    }
}

fn selector_outbounds(groups: &[ProxyGroup], names: &[String]) -> Vec<Value> {
    let configured = if groups.is_empty() {
        vec![ProxyGroup {
            name: "proxy".into(),
            group_type: "select".into(),
            ..ProxyGroup::default()
        }]
    } else {
        groups.to_vec()
    };
    configured
        .into_iter()
        .map(|group| {
            let mut members = group.proxies;
            if !group.readonly {
                for name in names {
                    if !members.contains(name) {
                        members.push(name.clone());
                    }
                }
            }
            if members.is_empty() {
                members.clone_from_slice(names);
            }
            let outbound_type = match group.group_type.as_str() {
                "url-test" | "fallback" | "load-balance" => "urltest",
                _ => "selector",
            };
            let mut value =
                json!({ "type": outbound_type, "tag": group.name, "outbounds": members });
            if !group.url.is_empty() {
                value["url"] = json!(group.url);
            }
            if group.interval > 0 {
                value["interval"] = json!(format!("{}s", group.interval));
            }
            if !group.default.is_empty() {
                value["default"] = json!(group.default);
            }
            value
        })
        .collect()
}

fn local_inbounds(profile: &Profile) -> Vec<Value> {
    let users = if profile.local_proxy.username.is_empty() {
        vec![]
    } else {
        vec![
            json!({ "username": profile.local_proxy.username, "password": profile.local_proxy.password }),
        ]
    };
    vec![
        json!({ "type": "socks", "tag": "sempre-socks-in", "listen": "127.0.0.1", "listen_port": profile.local_proxy.socks_port, "users": users }),
        json!({ "type": "http", "tag": "sempre-http-in", "listen": "127.0.0.1", "listen_port": profile.local_proxy.http_port, "users": users }),
    ]
}

fn route_config(profile: &Profile) -> Value {
    let final_outbound = profile
        .groups
        .first()
        .map_or("proxy", |group| group.name.as_str());
    let mut rule_sets = Vec::new();
    let mut rules = Vec::new();
    for provider in &profile.rule_providers {
        let format = if provider.format.is_empty() {
            "source"
        } else {
            &provider.format
        };
        rule_sets.push(json!({ "type": "remote", "tag": provider.tag, "format": format, "url": provider.url, "download_detour": "direct" }));
        rules.push(json!({ "rule_set": [provider.tag], "outbound": if provider.outbound.is_empty() { final_outbound } else { &provider.outbound } }));
    }
    json!({ "rules": rules, "rule_set": rule_sets, "final": final_outbound, "auto_detect_interface": true })
}

fn management_api(profile: &Profile) -> Value {
    if profile.management_api.external_controller.is_empty() {
        return json!({});
    }
    json!({ "clash_api": { "external_controller": profile.management_api.external_controller, "secret": profile.management_api.secret, "external_ui": profile.management_api.external_ui } })
}

fn log_config(level: &str) -> Value {
    let disabled = level == "off";
    let level = if matches!(level, "error" | "warn" | "info" | "debug") {
        level
    } else {
        "info"
    };
    json!({ "disabled": disabled, "level": level, "timestamp": true })
}

fn normalize_for_version(config: &mut Value, target: &Target) {
    if target.version == "11" {
        return;
    }
    if let Some(route) = config.get_mut("route").and_then(Value::as_object_mut) {
        route.remove("geoip");
        route.remove("geosite");
    }
}

fn field<'a>(proxy: &'a Proxy, key: &str) -> &'a Value {
    proxy.extra.get(key).unwrap_or(&Value::Null)
}
fn boolean(value: &Value) -> bool {
    value.as_bool().unwrap_or(false)
}
fn number(value: &Value) -> u64 {
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse().ok())
        .unwrap_or_default()
}
fn default_string(value: &Value, default: &str) -> Value {
    value
        .as_str()
        .filter(|value| !value.is_empty())
        .map_or_else(|| json!(default), |value| json!(value))
}
fn copy(outbound: &mut Value, proxy: &Proxy, keys: &[&str]) {
    for key in keys {
        if !field(proxy, key).is_null() {
            outbound[*key] = field(proxy, key).clone();
        }
    }
}
fn copy_aliases(outbound: &mut Value, proxy: &Proxy, aliases: &[(&str, &str)]) {
    for (source, target) in aliases {
        if !field(proxy, source).is_null() {
            outbound[*target] = field(proxy, source).clone();
        }
    }
}
