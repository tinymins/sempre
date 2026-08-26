use std::net::IpAddr;

use serde_json::{Map, Value, json};

#[derive(Default)]
pub(super) struct Resolved {
    pub endpoints: Vec<Value>,
    pub outbounds: Vec<Value>,
    pub direct_domains: Vec<String>,
    pub route_rules: Vec<Value>,
    pub dns_servers: Vec<Value>,
    pub dns_rules: Vec<Value>,
}

pub(super) fn resolve(config: &Value, modern: bool, desktop: bool) -> Resolved {
    let mut resolved = Resolved::default();
    if !modern || config.get("enabled").and_then(Value::as_bool) != Some(true) {
        return resolved;
    }
    let Some(connectors) = config.get("connectors").and_then(Value::as_array) else {
        return resolved;
    };
    for (index, value) in connectors.iter().enumerate() {
        let Some(connector) = value.as_object() else {
            continue;
        };
        if connector.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let tag = string(connector, "tag")
            .map_or_else(|| format!("private-access-{}", index + 1), str::to_owned);
        let kind = string(connector, "type").unwrap_or("outbound");
        let represented = match kind {
            "wireguard" | "tailscale" => endpoint(connector, kind, &tag, desktop, &mut resolved),
            kind if supported_outbound(kind) => {
                outbound(connector, kind, &tag, desktop, &mut resolved)
            }
            _ => false,
        };
        if !represented {
            continue;
        }
        if let Some(routes) = connector.get("routes").and_then(Value::as_object) {
            let mut rule = json!({ "action": "route", "outbound": tag });
            add_matchers(&mut rule, routes);
            if rule.as_object().is_some_and(|rule| rule.len() > 2) {
                resolved.route_rules.push(rule);
            }
        }
        if let Some(items) = connector.get("dns").and_then(Value::as_array) {
            for (dns_index, value) in items.iter().enumerate() {
                let Some(dns) = value.as_object() else {
                    continue;
                };
                let Some(server) = string(dns, "server") else {
                    continue;
                };
                let dns_tag = string(dns, "tag")
                    .map_or_else(|| format!("{tag}-dns-{}", dns_index + 1), str::to_owned);
                resolved.dns_servers.push(json!({
                    "type": "udp", "tag": dns_tag, "server": server,
                    "server_port": integer(dns.get("serverPort"), 53), "detour": tag
                }));
                let mut rule = json!({ "action": "route", "server": dns_tag });
                add_matchers(&mut rule, dns);
                resolved.dns_rules.push(rule);
            }
        }
    }
    resolved
}

fn endpoint(
    connector: &Map<String, Value>,
    kind: &str,
    tag: &str,
    desktop: bool,
    resolved: &mut Resolved,
) -> bool {
    let Some(mut value) = connector.get("endpoint").cloned() else {
        return false;
    };
    normalize_keys(&mut value);
    value["type"] = json!(kind);
    value["tag"] = json!(tag);
    if let Some(domain) = first_endpoint_domain(&value) {
        push_unique(&mut resolved.direct_domains, domain);
        if value.get("domain_resolver").is_none() {
            value["domain_resolver"] = resolver(if desktop { "bootstrap" } else { "local" });
        }
    }
    resolved.endpoints.push(value);
    true
}

fn outbound(
    connector: &Map<String, Value>,
    kind: &str,
    tag: &str,
    desktop: bool,
    resolved: &mut Resolved,
) -> bool {
    let Some(mut value) = connector.get("outbound").cloned() else {
        return false;
    };
    normalize_keys(&mut value);
    if !matches!(kind, "outbound" | "v2ray" | "xray") {
        value["type"] = json!(if kind == "socks5" { "socks" } else { kind });
    }
    value["tag"] = json!(tag);
    if let Some(domain) = value
        .get("server")
        .and_then(Value::as_str)
        .and_then(domain_name)
    {
        push_unique(&mut resolved.direct_domains, domain);
        if desktop && value.get("domain_resolver").is_none() {
            value["domain_resolver"] = resolver("bootstrap");
        }
    }
    resolved.outbounds.push(value);
    true
}

fn add_matchers(target: &mut Value, source: &Map<String, Value>) {
    for (from, to) in [
        ("ipCidrs", "ip_cidr"),
        ("domains", "domain"),
        ("domainSuffixes", "domain_suffix"),
        ("domainKeywords", "domain_keyword"),
        ("domainRegexes", "domain_regex"),
    ] {
        let values = clean_strings(source.get(from));
        if !values.is_empty() {
            target[to] = json!(values);
        }
    }
}

fn clean_strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn normalize_keys(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (from, to) in [
                ("privateKey", "private_key"),
                ("publicKey", "public_key"),
                ("preSharedKey", "pre_shared_key"),
                ("allowedIps", "allowed_ips"),
                (
                    "persistentKeepaliveInterval",
                    "persistent_keepalive_interval",
                ),
                ("domainResolver", "domain_resolver"),
                ("serverPort", "server_port"),
                ("listenPort", "listen_port"),
                ("alterId", "alter_id"),
            ] {
                if let Some(item) = object.remove(from) {
                    object.insert(to.into(), item);
                }
            }
            for item in object.values_mut() {
                normalize_keys(item);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalize_keys),
        _ => {}
    }
}

fn first_endpoint_domain(endpoint: &Value) -> Option<String> {
    endpoint
        .get("peers")?
        .as_array()?
        .first()?
        .get("address")?
        .as_str()
        .and_then(domain_name)
}

fn domain_name(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.parse::<IpAddr>().is_err()).then(|| value.to_owned())
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn string<'a>(value: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn integer(value: Option<&Value>, fallback: u64) -> u64 {
    value.and_then(Value::as_u64).unwrap_or(fallback)
}

fn resolver(server: &str) -> Value {
    json!({ "server": server, "strategy": "ipv4_only" })
}

fn supported_outbound(value: &str) -> bool {
    matches!(
        value,
        "outbound"
            | "v2ray"
            | "xray"
            | "vmess"
            | "vless"
            | "trojan"
            | "socks"
            | "socks5"
            | "http"
            | "ssh"
            | "hysteria2"
            | "tuic"
            | "anytls"
    )
}
