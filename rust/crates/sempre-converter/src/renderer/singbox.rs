use std::fmt::Write as _;

use serde_json::{Map, Value, json};

use crate::{CompileError, FieldDiff, Profile, Proxy, SourceSnapshot, Target};

mod assembly;
mod config;
mod fields;
mod private_access;
use fields::consumed_keys;

pub(super) fn render(
    profile: &Profile,
    proxies: &[Proxy],
    target: &Target,
    snapshots: &[SourceSnapshot],
) -> Result<(String, Vec<FieldDiff>, Vec<String>), CompileError> {
    assembly::render(profile, proxies, target, snapshots)
}

pub(super) fn convert_proxy(proxy: &Proxy) -> (Option<Value>, FieldDiff) {
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
            if !boolean_default(field(proxy, "udp"), true) {
                outbound["network"] = json!("tcp");
            }
            add_shadowsocks_plugin(&mut outbound, proxy);
        }
        "hysteria2" => {
            outbound["type"] = json!("hysteria2");
            outbound["password"] = field(proxy, "password").clone();
            outbound["tls"] = tls(proxy, true);
            if let Some(ports) = string_value(field(proxy, "ports")) {
                outbound["server_ports"] = json!([ports.replace('-', ":")]);
            }
            for (source, target) in [("up", "up_mbps"), ("down", "down_mbps")] {
                if let Some(value) = string_value(field(proxy, source)).and_then(parse_mbps) {
                    outbound[target] = json!(value);
                }
            }
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
            if let Some(milliseconds) = unsigned(field(proxy, "heartbeat-interval")) {
                outbound["heartbeat"] = json!(format!("{}s", milliseconds / 1000));
            }
            if boolean(field(proxy, "reduce-rtt")) {
                outbound["zero_rtt_handshake"] = json!(true);
            }
            if boolean(field(proxy, "udp-over-stream")) {
                outbound["udp_over_stream"] = json!(true);
            }
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
            if !boolean_default(field(proxy, "udp"), true) {
                outbound["network"] = json!("tcp");
            }
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

fn add_shadowsocks_plugin(outbound: &mut Value, proxy: &Proxy) {
    let Some(plugin) = string_value(field(proxy, "plugin")) else {
        return;
    };
    let Some(options) = field(proxy, "plugin-opts").as_object() else {
        outbound["plugin"] = json!(if plugin == "obfs" {
            "obfs-local"
        } else {
            plugin
        });
        return;
    };
    if plugin == "shadow-tls" {
        outbound["type"] = json!("shadowtls");
        outbound
            .as_object_mut()
            .expect("outbound object")
            .remove("method");
        copy_from_object(outbound, options, &["password", "version"]);
        if let Some(host) = options.get("host").and_then(Value::as_str) {
            outbound["tls"] = json!({ "enabled": true, "server_name": host });
        }
        return;
    }
    outbound["plugin"] = json!(if plugin == "obfs" {
        "obfs-local"
    } else {
        plugin
    });
    let mut plugin_options = String::new();
    if let Some(mode) = options.get("mode").and_then(Value::as_str) {
        let _ = write!(plugin_options, "mode={mode}");
    }
    if let Some(host) = options.get("host").and_then(Value::as_str) {
        append_plugin_option(&mut plugin_options, &format!("host={host}"));
    }
    if plugin == "v2ray-plugin" {
        if options.get("tls").and_then(Value::as_bool) == Some(true) {
            append_plugin_option(&mut plugin_options, "tls");
        }
        if let Some(path) = options.get("path").and_then(Value::as_str) {
            append_plugin_option(&mut plugin_options, &format!("path={path}"));
        }
        if let Some(mux) = options.get("mux") {
            append_plugin_option(&mut plugin_options, &format!("mux={mux}"));
        }
    }
    outbound["plugin_opts"] = json!(plugin_options);
}

fn append_plugin_option(options: &mut String, value: &str) {
    if !options.is_empty() {
        options.push(';');
    }
    options.push_str(value);
}

fn copy_from_object(target: &mut Value, source: &Map<String, Value>, keys: &[&str]) {
    for key in keys {
        if let Some(value) = source.get(*key) {
            target[*key] = value.clone();
        }
    }
}

fn parse_mbps(value: &str) -> Option<u64> {
    value
        .trim()
        .trim_end_matches(|character: char| character.is_alphabetic() || character == ' ')
        .trim()
        .parse()
        .ok()
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
    if let Some(options) = field(proxy, "http-opts").as_object() {
        let mut result = json!({ "type": "http" });
        if let Some(path) = options
            .get("path")
            .and_then(Value::as_array)
            .and_then(|paths| paths.first())
            .and_then(Value::as_str)
        {
            result["path"] = json!(path);
        }
        if let Some(method) = options.get("method").and_then(Value::as_str) {
            result["method"] = json!(method);
        }
        if let Some(headers) = options.get("headers").and_then(Value::as_object) {
            let values = headers
                .iter()
                .filter_map(|(key, value)| {
                    value
                        .as_array()?
                        .first()?
                        .as_str()
                        .map(|value| (key.clone(), json!(value)))
                })
                .collect::<Map<_, _>>();
            if !values.is_empty() {
                result["headers"] = Value::Object(values);
            }
        }
        return Some(result);
    }
    if let Some(options) = field(proxy, "ws-opts").as_object() {
        let mut result = json!({ "type": "ws", "path": options.get("path").cloned().unwrap_or(json!("/")), "headers": options.get("headers").cloned().unwrap_or(json!({})) });
        copy_aliases_from_object(
            &mut result,
            options,
            &[
                ("max-early-data", "max_early_data"),
                ("early-data-header-name", "early_data_header_name"),
            ],
        );
        return Some(result);
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

fn copy_aliases_from_object(
    target: &mut Value,
    source: &Map<String, Value>,
    keys: &[(&str, &str)],
) {
    for (source_key, target_key) in keys {
        if let Some(value) = source.get(*source_key) {
            target[*target_key] = value.clone();
        }
    }
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

fn field<'a>(proxy: &'a Proxy, key: &str) -> &'a Value {
    proxy.extra.get(key).unwrap_or(&Value::Null)
}
fn boolean(value: &Value) -> bool {
    value.as_bool().unwrap_or(false)
}
fn boolean_default(value: &Value, default: bool) -> bool {
    value.as_bool().unwrap_or(default)
}
fn string_value(value: &Value) -> Option<&str> {
    value.as_str().filter(|value| !value.is_empty())
}
fn unsigned(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
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
