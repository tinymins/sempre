use std::collections::HashMap;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::{Map, Value, json};
use url::Url;

use crate::Proxy;

pub(super) fn parse(value: &str) -> Result<Proxy, String> {
    if value.starts_with("vmess://") {
        return parse_vmess(value);
    }
    if value.starts_with("ss://") {
        return parse_shadowsocks(value);
    }
    let parsed = Url::parse(value).map_err(|error| error.to_string())?;
    match parsed.scheme() {
        "vless" => parse_vless(&parsed),
        "trojan" => parse_trojan(&parsed),
        "hysteria2" | "hy2" => parse_hysteria2(&parsed),
        "anytls" => parse_anytls(&parsed),
        scheme => Err(format!("unsupported scheme {scheme}")),
    }
}

fn endpoint(url: &Url) -> Result<(String, u16, String), String> {
    let server = url.host_str().ok_or("missing server")?.into();
    let port = url.port().ok_or("missing port")?;
    let name = url
        .fragment()
        .map(decode)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("{server}:{port}"));
    Ok((server, port, name))
}

fn query(url: &Url) -> HashMap<String, String> {
    url.query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

fn parse_vless(url: &Url) -> Result<Proxy, String> {
    let (server, port, name) = endpoint(url)?;
    let values = query(url);
    let mut extra = Map::new();
    extra.insert("uuid".into(), json!(decode(url.username())));
    extra.insert("udp".into(), json!(true));
    add_transport(&mut extra, &values);
    add_tls(&mut extra, &values);
    copy_query(&mut extra, &values, &["flow"]);
    Ok(Proxy {
        name,
        proxy_type: "vless".into(),
        server,
        port,
        extra,
    })
}

fn parse_trojan(url: &Url) -> Result<Proxy, String> {
    let (server, port, name) = endpoint(url)?;
    let values = query(url);
    let mut extra = Map::new();
    extra.insert("password".into(), json!(decode(url.username())));
    extra.insert("udp".into(), json!(true));
    add_transport(&mut extra, &values);
    add_tls(&mut extra, &values);
    Ok(Proxy {
        name,
        proxy_type: "trojan".into(),
        server,
        port,
        extra,
    })
}

fn parse_hysteria2(url: &Url) -> Result<Proxy, String> {
    let (server, port, name) = endpoint(url)?;
    let values = query(url);
    let mut extra = Map::new();
    extra.insert("password".into(), json!(decode(url.username())));
    copy_query(
        &mut extra,
        &values,
        &[
            "sni",
            "alpn",
            "obfs",
            "obfs-password",
            "ports",
            "up",
            "down",
        ],
    );
    if matches!(
        values.get("insecure").map(String::as_str),
        Some("1" | "true")
    ) {
        extra.insert("skip-cert-verify".into(), json!(true));
    }
    Ok(Proxy {
        name,
        proxy_type: "hysteria2".into(),
        server,
        port,
        extra,
    })
}

fn parse_anytls(url: &Url) -> Result<Proxy, String> {
    let (server, port, name) = endpoint(url)?;
    let values = query(url);
    let mut extra = Map::new();
    extra.insert("password".into(), json!(decode(url.username())));
    extra.insert("udp".into(), json!(true));
    add_tls(&mut extra, &values);
    Ok(Proxy {
        name,
        proxy_type: "anytls".into(),
        server,
        port,
        extra,
    })
}

fn parse_vmess(value: &str) -> Result<Proxy, String> {
    let encoded = value
        .trim_start_matches("vmess://")
        .split('#')
        .next()
        .unwrap_or_default();
    let decoded = decode_base64(encoded).ok_or("invalid vmess payload")?;
    let object: Value = serde_json::from_str(&decoded).map_err(|error| error.to_string())?;
    let server = string(&object, "add");
    let port = number(&object, "port").ok_or("invalid vmess port")?;
    let name = [string(&object, "ps"), string(&object, "remarks")]
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| format!("{server}:{port}"));
    if server.is_empty() {
        return Err("missing vmess server".into());
    }
    let mut extra = Map::new();
    extra.insert("uuid".into(), json!(string(&object, "id")));
    extra.insert("alterId".into(), json!(number(&object, "aid").unwrap_or(0)));
    let cipher = string(&object, "scy");
    extra.insert(
        "cipher".into(),
        json!(if cipher.is_empty() { "auto" } else { &cipher }),
    );
    extra.insert("udp".into(), json!(true));
    let network = string(&object, "net");
    let mut values = HashMap::new();
    values.insert(
        "type".into(),
        if network.is_empty() {
            "tcp".into()
        } else {
            network
        },
    );
    values.insert("path".into(), string(&object, "path"));
    values.insert("host".into(), string(&object, "host"));
    add_transport(&mut extra, &values);
    if string(&object, "tls") == "tls" {
        extra.insert("tls".into(), json!(true));
        for (source, target) in [("sni", "servername"), ("fp", "client-fingerprint")] {
            let value = string(&object, source);
            if !value.is_empty() {
                extra.insert(target.into(), json!(value));
            }
        }
    }
    Ok(Proxy {
        name,
        proxy_type: "vmess".into(),
        server,
        port,
        extra,
    })
}

fn parse_shadowsocks(value: &str) -> Result<Proxy, String> {
    let raw = value.trim_start_matches("ss://");
    let (without_fragment, fragment) = raw.split_once('#').unwrap_or((raw, ""));
    let name = decode(fragment);
    let (userinfo, endpoint_value) =
        if let Some((userinfo, endpoint)) = without_fragment.rsplit_once('@') {
            (
                decode_base64(userinfo).unwrap_or_else(|| decode(userinfo)),
                endpoint.to_owned(),
            )
        } else {
            let decoded = decode_base64(without_fragment).ok_or("invalid shadowsocks payload")?;
            let (userinfo, endpoint) = decoded
                .rsplit_once('@')
                .ok_or("invalid shadowsocks endpoint")?;
            (userinfo.to_owned(), endpoint.to_owned())
        };
    let (method, password) = userinfo
        .split_once(':')
        .ok_or("invalid shadowsocks credentials")?;
    let endpoint_url =
        Url::parse(&format!("ss://{endpoint_value}")).map_err(|error| error.to_string())?;
    let (server, port, fallback_name) = endpoint(&endpoint_url)?;
    let mut extra = Map::new();
    extra.insert("cipher".into(), json!(method));
    extra.insert("password".into(), json!(password));
    extra.insert("udp".into(), json!(true));
    Ok(Proxy {
        name: if name.is_empty() { fallback_name } else { name },
        proxy_type: "ss".into(),
        server,
        port,
        extra,
    })
}

fn add_transport(extra: &mut Map<String, Value>, values: &HashMap<String, String>) {
    match values.get("type").map_or("tcp", String::as_str) {
        "ws" => {
            let mut options = json!({ "path": values.get("path").filter(|v| !v.is_empty()).map_or("/", String::as_str) });
            if let Some(host) = values.get("host").filter(|value| !value.is_empty()) {
                options["headers"] = json!({ "Host": host });
            }
            extra.insert("network".into(), json!("ws"));
            extra.insert("ws-opts".into(), options);
        }
        "grpc" => {
            extra.insert("network".into(), json!("grpc"));
            extra.insert("grpc-opts".into(), json!({ "grpc-service-name": values.get("serviceName").or_else(|| values.get("path")).cloned().unwrap_or_default() }));
        }
        _ => {}
    }
}

fn add_tls(extra: &mut Map<String, Value>, values: &HashMap<String, String>) {
    let security = values.get("security").map_or("tls", String::as_str);
    if security == "none" {
        return;
    }
    extra.insert("tls".into(), json!(true));
    for (source, target) in [("sni", "servername"), ("fp", "client-fingerprint")] {
        if let Some(value) = values.get(source).filter(|value| !value.is_empty()) {
            extra.insert(target.into(), json!(value));
        }
    }
    if matches!(
        values.get("insecure").map(String::as_str),
        Some("1" | "true")
    ) {
        extra.insert("skip-cert-verify".into(), json!(true));
    }
    if security == "reality" {
        extra.insert("reality-opts".into(), json!({ "public-key": values.get("pbk").cloned().unwrap_or_default(), "short-id": values.get("sid").cloned().unwrap_or_default() }));
    }
}

fn copy_query(extra: &mut Map<String, Value>, values: &HashMap<String, String>, keys: &[&str]) {
    for key in keys {
        if let Some(value) = values.get(*key).filter(|value| !value.is_empty()) {
            if *key == "alpn" {
                extra.insert((*key).into(), json!(value.split(',').collect::<Vec<_>>()));
            } else {
                extra.insert((*key).into(), json!(value));
            }
        }
    }
}

fn decode(value: &str) -> String {
    let encoded = format!("value={value}");
    url::form_urlencoded::parse(encoded.as_bytes())
        .next()
        .map_or_else(String::new, |(_, value)| value.into_owned())
}

fn decode_base64(value: &str) -> Option<String> {
    let mut padded = value.trim().to_string();
    while !padded.len().is_multiple_of(4) {
        padded.push('=');
    }
    [&STANDARD, &URL_SAFE_NO_PAD]
        .into_iter()
        .find_map(|engine| engine.decode(padded.as_bytes()).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

fn string(value: &Value, key: &str) -> String {
    match &value[key] {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    }
}

fn number(value: &Value, key: &str) -> Option<u16> {
    string(value, key).parse().ok().or_else(|| {
        value[key]
            .as_u64()
            .and_then(|value| u16::try_from(value).ok())
    })
}
