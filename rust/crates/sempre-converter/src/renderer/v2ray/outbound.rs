use serde_json::{Map, Value, json};

use crate::{FieldDiff, Profile, Proxy};

pub(super) fn convert(proxy: &Proxy, modern: bool) -> (Option<Value>, FieldDiff) {
    let mut diff = FieldDiff {
        node: proxy.name.clone(),
        represented: true,
        consumed: proxy.extra.keys().cloned().collect(),
        ignored: Vec::new(),
        dropped: Vec::new(),
        warnings: Vec::new(),
        outbound: None,
    };
    diff.consumed.sort();
    if !modern && object(proxy, "reality-opts").is_some() {
        diff.represented = false;
        diff.warnings.push(format!(
            "{}: Reality is not supported by V2Ray-core",
            proxy.name
        ));
        return (None, diff);
    }
    let Some((protocol, settings)) = settings(proxy, modern) else {
        diff.represented = false;
        diff.warnings.push(format!(
            "{}: unsupported proxy type {}",
            proxy.name, proxy.proxy_type
        ));
        return (None, diff);
    };
    let outbound = json!({
        "tag": proxy.name,
        "protocol": protocol,
        "settings": settings,
        "streamSettings": stream(proxy, modern)
    });
    diff.outbound = Some(outbound.clone());
    (Some(outbound), diff)
}

fn settings(proxy: &Proxy, modern: bool) -> Option<(&'static str, Value)> {
    if modern {
        return modern_settings(proxy);
    }
    legacy_settings(proxy)
}

fn modern_settings(proxy: &Proxy) -> Option<(&'static str, Value)> {
    let common = json!({ "address": proxy.server, "port": proxy.port });
    let (protocol, mut settings) = match proxy.proxy_type.as_str() {
        "vmess" => ("vmess", common),
        "vless" => ("vless", common),
        "trojan" => ("trojan", common),
        "ss" => ("shadowsocks", common),
        "socks5" => ("socks", common),
        "http" => ("http", common),
        _ => return None,
    };
    match proxy.proxy_type.as_str() {
        "vmess" => {
            settings["id"] = field(proxy, "uuid").clone();
            settings["security"] = json!(string_or(proxy, "cipher", "auto"));
        }
        "vless" => {
            settings["id"] = field(proxy, "uuid").clone();
            settings["encryption"] = json!("none");
            copy_nonempty(&mut settings, proxy, "flow");
        }
        "trojan" => settings["password"] = field(proxy, "password").clone(),
        "ss" => {
            settings["method"] = json!(string_or(proxy, "cipher", "aes-256-gcm"));
            settings["password"] = field(proxy, "password").clone();
        }
        "socks5" | "http" => {
            settings["user"] = field(proxy, "username").clone();
            settings["pass"] = field(proxy, "password").clone();
        }
        _ => unreachable!(),
    }
    Some((protocol, settings))
}

fn legacy_settings(proxy: &Proxy) -> Option<(&'static str, Value)> {
    let mut server = json!({ "address": proxy.server, "port": proxy.port });
    let (protocol, settings) = match proxy.proxy_type.as_str() {
        "vmess" => {
            server["users"] = json!([{
                "id": field(proxy, "uuid"), "alterId": integer(proxy, "alterId"),
                "security": string_or(proxy, "cipher", "auto")
            }]);
            ("vmess", json!({ "vnext": [server] }))
        }
        "vless" => {
            let mut user = json!({ "id": field(proxy, "uuid"), "encryption": "none" });
            copy_nonempty(&mut user, proxy, "flow");
            server["users"] = json!([user]);
            ("vless", json!({ "vnext": [server] }))
        }
        "trojan" => {
            server["password"] = field(proxy, "password").clone();
            ("trojan", json!({ "servers": [server] }))
        }
        "ss" => {
            server["method"] = json!(string_or(proxy, "cipher", "aes-256-gcm"));
            server["password"] = field(proxy, "password").clone();
            ("shadowsocks", json!({ "servers": [server] }))
        }
        "socks5" | "http" => {
            if !string(proxy, "username").is_empty() {
                server["users"] = json!([{
                    "user": field(proxy, "username"), "pass": field(proxy, "password")
                }]);
            }
            let protocol = if proxy.proxy_type == "socks5" {
                "socks"
            } else {
                "http"
            };
            (protocol, json!({ "servers": [server] }))
        }
        _ => return None,
    };
    Some((protocol, settings))
}

fn stream(proxy: &Proxy, modern: bool) -> Value {
    let ws = object(proxy, "ws-opts");
    let grpc = object(proxy, "grpc-opts");
    let http = object(proxy, "http-opts").or_else(|| object(proxy, "h2-opts"));
    let mut network = string_or(proxy, "network", "tcp").to_owned();
    if ws.is_some() {
        network = "ws".into();
    } else if grpc.is_some() {
        network = "grpc".into();
    } else if http.is_some() {
        network = "http".into();
    }
    let mut result = Map::new();
    let transport = if modern {
        match network.as_str() {
            "tcp" => "raw",
            "ws" => "websocket",
            "http" | "h2" => "xhttp",
            value => value,
        }
    } else if network == "http" {
        "h2"
    } else {
        &network
    };
    result.insert(
        if modern { "method" } else { "network" }.into(),
        json!(transport),
    );
    if let Some(options) = ws {
        let mut settings = json!({ "path": map_string_or(options, "path", "/") });
        if let Some(headers) = options.get("headers").and_then(Value::as_object) {
            settings["headers"] = Value::Object(headers.clone());
        }
        result.insert("wsSettings".into(), settings);
    }
    if let Some(options) = grpc {
        result.insert(
            "grpcSettings".into(),
            json!({ "serviceName": map_string_or(options, "grpc-service-name", "") }),
        );
    }
    if let Some(options) = http {
        let path = first_string(options.get("path")).unwrap_or("/");
        result.insert(
            if modern {
                "xhttpSettings"
            } else {
                "httpSettings"
            }
            .into(),
            json!({ "path": path }),
        );
    }

    let security = if modern && object(proxy, "reality-opts").is_some() {
        "reality"
    } else if boolean(proxy, "tls") || proxy.proxy_type == "trojan" {
        "tls"
    } else {
        "none"
    };
    result.insert("security".into(), json!(security));
    if security == "tls" {
        let mut tls = json!({
            "serverName": server_name(proxy),
            "allowInsecure": boolean(proxy, "skip-cert-verify")
        });
        if let Some(alpn) = proxy.extra.get("alpn") {
            tls["alpn"] = alpn.clone();
        }
        if modern && !string(proxy, "client-fingerprint").is_empty() {
            tls["fingerprint"] = field(proxy, "client-fingerprint").clone();
        }
        result.insert("tlsSettings".into(), tls);
    } else if security == "reality" {
        let reality = object(proxy, "reality-opts").expect("Reality was selected");
        result.insert(
            "realitySettings".into(),
            json!({
                "serverName": server_name(proxy),
                "fingerprint": string_or(proxy, "client-fingerprint", "chrome"),
                "password": map_string_or(reality, "public-key", ""),
                "shortId": map_string_or(reality, "short-id", ""),
                "spiderX": "/"
            }),
        );
    }
    Value::Object(result)
}

pub(super) fn local_inbounds(profile: &Profile, modern: bool) -> Vec<Value> {
    let users_key = if modern { "users" } else { "accounts" };
    let users = json!([{
        "user": profile.local_proxy.username,
        "pass": profile.local_proxy.password
    }]);
    let mut socks_settings = json!({ "auth": "password", "udp": true, "ip": "127.0.0.1" });
    socks_settings[users_key] = users.clone();
    let mut http_settings = json!({});
    http_settings[users_key] = users;
    vec![
        json!({
            "tag": "sempre-socks-in", "listen": "127.0.0.1",
            "port": profile.local_proxy.socks_port, "protocol": "socks",
            "settings": socks_settings
        }),
        json!({
            "tag": "sempre-http-in", "listen": "127.0.0.1",
            "port": profile.local_proxy.http_port, "protocol": "http",
            "settings": http_settings
        }),
    ]
}

fn field<'a>(proxy: &'a Proxy, key: &str) -> &'a Value {
    proxy.extra.get(key).unwrap_or(&Value::Null)
}

fn object<'a>(proxy: &'a Proxy, key: &str) -> Option<&'a Map<String, Value>> {
    field(proxy, key).as_object()
}

fn string<'a>(proxy: &'a Proxy, key: &str) -> &'a str {
    field(proxy, key).as_str().unwrap_or_default()
}

fn string_or<'a>(proxy: &'a Proxy, key: &str, fallback: &'a str) -> &'a str {
    let value = string(proxy, key);
    if value.is_empty() { fallback } else { value }
}

fn integer(proxy: &Proxy, key: &str) -> i64 {
    field(proxy, key).as_i64().unwrap_or_default()
}

fn boolean(proxy: &Proxy, key: &str) -> bool {
    field(proxy, key).as_bool().unwrap_or(false)
}

fn copy_nonempty(target: &mut Value, proxy: &Proxy, key: &str) {
    if !string(proxy, key).is_empty() {
        target[key] = field(proxy, key).clone();
    }
}

fn map_string_or<'a>(map: &'a Map<String, Value>, key: &str, fallback: &'a str) -> &'a str {
    map.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
}

fn first_string(value: Option<&Value>) -> Option<&str> {
    value.and_then(|value| {
        value.as_str().or_else(|| {
            value
                .as_array()
                .and_then(|values| values.first())
                .and_then(Value::as_str)
        })
    })
}

fn server_name(proxy: &Proxy) -> &str {
    let servername = string(proxy, "servername");
    if !servername.is_empty() {
        return servername;
    }
    let sni = string(proxy, "sni");
    if sni.is_empty() { &proxy.server } else { sni }
}
