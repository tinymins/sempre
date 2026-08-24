use std::collections::HashMap;
use std::fmt::Write as _;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use url::Url;

use crate::{CompileError, FieldDiff, Profile, Proxy};

pub(super) fn render(
    profile: &Profile,
    proxies: &[Proxy],
) -> Result<(String, Vec<FieldDiff>, Vec<String>), CompileError> {
    let mut links = Vec::new();
    let mut diffs = Vec::new();
    let mut warnings = Vec::new();
    for proxy in proxies {
        let Some(link) = node_uri(proxy) else {
            let warning = format!(
                "{}: unsupported proxy type {}",
                proxy.name, proxy.proxy_type
            );
            diffs.push(FieldDiff {
                node: proxy.name.clone(),
                represented: false,
                consumed: vec![],
                ignored: vec![],
                dropped: proxy.extra.keys().cloned().collect(),
                warnings: vec![warning.clone()],
                outbound: None,
            });
            warnings.push(warning);
            continue;
        };
        diffs.push(FieldDiff {
            node: proxy.name.clone(),
            represented: true,
            consumed: proxy.extra.keys().cloned().collect(),
            ignored: vec![],
            dropped: vec![],
            warnings: vec![],
            outbound: Some(json!({ "type": "dae-node", "tag": proxy.name, "uri": link })),
        });
        links.push((proxy.name.clone(), link));
    }
    if links.is_empty() {
        return Err(CompileError::EmptyProfile);
    }
    let represented: Vec<String> = links.iter().map(|(name, _)| name.clone()).collect();
    let (group_blocks, group_tags, final_group) = groups(profile, &represented)?;
    let mut routing = vec![
        "  dip(geoip:private) -> direct".into(),
        "  dip(geoip:cn) -> direct".into(),
        "  domain(geosite:cn) -> direct".into(),
    ];
    for rule in &profile.rules {
        let Some(rule) = rule.as_str() else {
            warnings.push("native sing-box custom rule is not representable by dae".into());
            continue;
        };
        if let Some(converted) = rule_line(rule, &group_tags) {
            routing.push(format!("  {converted}"));
        } else {
            warnings.push(format!("unsupported rule: {rule}"));
        }
    }
    for provider in &profile.rule_providers {
        warnings.push(format!(
            "rule provider {} is not representable by dae",
            provider.tag
        ));
    }
    let level = if profile.log_level == "off" {
        "error"
    } else {
        &profile.log_level
    };
    let nodes = links
        .iter()
        .map(|(_, link)| format!("  {}", quote(link)))
        .collect::<Vec<_>>()
        .join("\n");
    let mut content = format!(
        "global {{\n  tproxy_port: 12345\n  tproxy_port_protect: true\n  log_level: {level}\n  auto_config_kernel_parameter: false\n  bootstrap_resolver: {}\n}}\nnode {{\n{nodes}\n}}\ndns {{\n  ipversion_prefer: 0\n  upstream {{\n    local: {}\n    remote: {}\n  }}\n  routing {{\n    request {{\n      qname(geosite:cn) -> local\n      fallback: remote\n    }}\n  }}\n}}\ngroup {{\n{group_blocks}\n}}\nrouting {{\n{}\n  fallback: {final_group}\n}}\n",
        quote("223.5.5.5:53"),
        quote("udp://223.5.5.5:53"),
        quote("tls://1.1.1.1:853"),
        routing.join("\n")
    );
    if let Some(append) = profile
        .core_overrides
        .get("dae")
        .and_then(|value| value["append"].as_str())
        .filter(|value| !value.is_empty())
    {
        writeln!(content, "\n{append}").map_err(|error| CompileError::Render(error.to_string()))?;
    }
    Ok((content, diffs, warnings))
}

fn groups(
    profile: &Profile,
    represented: &[String],
) -> Result<(String, HashMap<String, String>, String), CompileError> {
    let mut tags = HashMap::new();
    let mut blocks = Vec::new();
    let groups = if profile.groups.is_empty() {
        vec![crate::ProxyGroup {
            name: "proxy".into(),
            proxies: represented.to_vec(),
            ..crate::ProxyGroup::default()
        }]
    } else {
        profile.groups.clone()
    };
    for (index, group) in groups.iter().enumerate() {
        let tag = format!("sempre_group_{}", index + 1);
        tags.insert(group.name.clone(), tag.clone());
        let mut members = group.proxies.clone();
        if !group.readonly {
            for name in represented {
                if !members.contains(name) {
                    members.push(name.clone());
                }
            }
        }
        members.retain(|name| represented.contains(name));
        if members.is_empty() {
            continue;
        }
        let default_index = members
            .iter()
            .position(|name| name == &group.default)
            .unwrap_or(0);
        let policy = if group.group_type == "url-test" {
            "min_moving_avg".into()
        } else {
            format!("fixed({default_index})")
        };
        let names = members
            .iter()
            .map(|name| quote(name))
            .collect::<Vec<_>>()
            .join(", ");
        blocks.push(format!(
            "  {tag} {{\n    filter: name({names})\n    policy: {policy}\n  }}"
        ));
    }
    let final_name = groups.first().map_or("proxy", |group| group.name.as_str());
    let final_group = tags.get(final_name).cloned().ok_or_else(|| {
        CompileError::Render(format!(
            "dae final proxy group {final_name:?} has no represented members"
        ))
    })?;
    Ok((blocks.join("\n"), tags, final_group))
}

fn node_uri(proxy: &Proxy) -> Option<String> {
    let host = if proxy.server.contains(':') {
        format!("[{}]:{}", proxy.server, proxy.port)
    } else {
        format!("{}:{}", proxy.server, proxy.port)
    };
    let name = encode(&proxy.name);
    let mut query = transport_query(proxy);
    let link = match proxy.proxy_type.as_str() {
        "vless" => {
            let security = if field(proxy, "reality-opts").is_object() {
                "reality"
            } else if boolean(proxy, "tls") {
                "tls"
            } else {
                "none"
            };
            query.push(("security".into(), security.into()));
            format!(
                "vless://{}@{host}?{}#{name}",
                encode(string(proxy, "uuid")),
                query_string(&query)
            )
        }
        "trojan" => format!(
            "trojan://{}@{host}?{}#{name}",
            encode(string(proxy, "password")),
            query_string(&query)
        ),
        "vmess" => {
            let value = json!({ "v": "2", "ps": proxy.name, "add": proxy.server, "port": proxy.port.to_string(), "id": string(proxy, "uuid"), "aid": field(proxy, "alterId"), "scy": string_default(proxy, "cipher", "auto"), "net": string_default(proxy, "network", "tcp"), "tls": if boolean(proxy, "tls") { "tls" } else { "" }, "sni": server_name(proxy) });
            format!(
                "vmess://{}",
                STANDARD_NO_PAD.encode(serde_json::to_vec(&value).ok()?)
            )
        }
        "ss" => {
            let credential = URL_SAFE_NO_PAD.encode(format!(
                "{}:{}",
                string_default(proxy, "cipher", "aes-256-gcm"),
                string(proxy, "password")
            ));
            format!("ss://{credential}@{host}#{name}")
        }
        "socks5" | "http" => {
            let scheme = if proxy.proxy_type == "http" && boolean(proxy, "tls") {
                "https"
            } else {
                &proxy.proxy_type
            };
            let auth = if string(proxy, "username").is_empty() {
                String::new()
            } else {
                format!(
                    "{}:{}@",
                    encode(string(proxy, "username")),
                    encode(string(proxy, "password"))
                )
            };
            format!("{scheme}://{auth}{host}/#{name}")
        }
        "hysteria2" | "anytls" => format!(
            "{}://{}@{host}?{}#{name}",
            proxy.proxy_type,
            encode(string(proxy, "password")),
            query_string(&query)
        ),
        "tuic" => format!(
            "tuic://{}:{}@{host}?{}#{name}",
            encode(string(proxy, "uuid")),
            encode(string(proxy, "password")),
            query_string(&query)
        ),
        _ => return None,
    };
    Some(link)
}

fn transport_query(proxy: &Proxy) -> Vec<(String, String)> {
    let mut result = Vec::new();
    if let Some(options) = field(proxy, "ws-opts").as_object() {
        result.push(("type".into(), "ws".into()));
        if let Some(path) = options.get("path").and_then(Value::as_str) {
            result.push(("path".into(), path.into()));
        }
    } else if let Some(options) = field(proxy, "grpc-opts").as_object() {
        result.push(("type".into(), "grpc".into()));
        if let Some(service) = options.get("grpc-service-name").and_then(Value::as_str) {
            result.push(("serviceName".into(), service.into()));
        }
    }
    let server_name = server_name(proxy);
    if !server_name.is_empty() {
        result.push(("sni".into(), server_name.into()));
    }
    if boolean(proxy, "skip-cert-verify") {
        result.push(("insecure".into(), "1".into()));
    }
    result
}

fn rule_line(line: &str, tags: &HashMap<String, String>) -> Option<String> {
    let parts: Vec<&str> = line.split(',').map(str::trim).collect();
    if parts.len() < 3 {
        return None;
    }
    let raw_target = *parts.last()?;
    let target = tags.get(raw_target).map_or(raw_target, String::as_str);
    let matcher = match parts[0].to_uppercase().as_str() {
        "DOMAIN" => format!("domain(full: {})", parts[1]),
        "DOMAIN-SUFFIX" => format!("domain(suffix: {})", parts[1]),
        "DOMAIN-KEYWORD" => format!("domain(keyword: {})", parts[1]),
        "GEOSITE" => format!("domain(geosite:{})", parts[1].to_lowercase()),
        "GEOIP" => format!("dip(geoip:{})", parts[1].to_lowercase()),
        "IP-CIDR" | "IP-CIDR6" => format!("dip({})", parts[1]),
        "SRC-IP-CIDR" => format!("sip({})", parts[1]),
        "DST-PORT" => format!("dport({})", parts[1]),
        "NETWORK" => format!("l4proto({})", parts[1].to_lowercase()),
        _ => return None,
    };
    Some(format!("{matcher} -> {target}"))
}

fn query_string(values: &[(String, String)]) -> String {
    let mut url = Url::parse("https://sempre.invalid/").expect("static URL");
    url.query_pairs_mut().extend_pairs(values);
    url.query().unwrap_or_default().into()
}
fn quote(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization")
}
fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
fn field<'a>(proxy: &'a Proxy, key: &str) -> &'a Value {
    proxy.extra.get(key).unwrap_or(&Value::Null)
}
fn string<'a>(proxy: &'a Proxy, key: &str) -> &'a str {
    field(proxy, key).as_str().unwrap_or_default()
}
fn string_default<'a>(proxy: &'a Proxy, key: &str, default: &'a str) -> &'a str {
    let value = string(proxy, key);
    if value.is_empty() { default } else { value }
}
fn boolean(proxy: &Proxy, key: &str) -> bool {
    field(proxy, key).as_bool().unwrap_or(false)
}
fn server_name(proxy: &Proxy) -> &str {
    let value = string(proxy, "servername");
    if value.is_empty() {
        string(proxy, "sni")
    } else {
        value
    }
}
