use serde_json::{Value, json};

use crate::{CompileError, Profile, Target};

const GROUP: &str = "内网地址";

pub(crate) fn outbound(profile: &Profile) -> Option<&'static str> {
    if profile
        .extra
        .get("sempre_intranet_routing_enabled")
        .and_then(Value::as_bool)
        == Some(false)
    {
        None
    } else if profile
        .extra
        .get("sempre_intranet_routing_mode")
        .and_then(Value::as_str)
        == Some("direct")
    {
        Some("DIRECT")
    } else {
        Some(GROUP)
    }
}

pub(crate) fn sing_box_rule(profile: &Profile) -> Option<Value> {
    outbound(profile).map(|outbound| json!({"ip_is_private": true, "outbound": if outbound == "DIRECT" { "direct" } else { outbound }}))
}

/// Routable private ranges. Loopback and link-local destinations stay with the host.
pub const INTRANET_CAPTURE_PREFIXES: [&str; 5] = [
    "10.0.0.0/8",
    "100.64.0.0/10",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "fc00::/7",
];

pub fn intranet_proxy_enabled(profile: &Profile) -> bool {
    profile
        .extra
        .get("sempre_intranet_routing_enabled")
        .and_then(Value::as_bool)
        == Some(true)
        && profile
            .extra
            .get("sempre_intranet_routing_mode")
            .and_then(Value::as_str)
            == Some("proxy")
}

/// Apply the local intranet policy to an already compiled subscription.
pub fn apply_intranet_routing(
    profile: &Profile,
    target: &Target,
    content: &str,
) -> Result<String, CompileError> {
    let error = |error: String| CompileError::Render(error);
    match target.core.as_str() {
        "sing-box" => {
            let mut document: Value =
                serde_json::from_str(content).map_err(|e| error(e.to_string()))?;
            apply_sing_box(profile, &mut document)?;
            serde_json::to_string_pretty(&document).map_err(|e| error(e.to_string()))
        }
        "mihomo" | "clash-rs" | ""
            if matches!(target.format.as_str(), "clash" | "clash-meta" | "clash-rs") =>
        {
            let mut document: Value =
                serde_yaml::from_str(content).map_err(|e| error(e.to_string()))?;
            let rules = document["rules"]
                .as_array_mut()
                .ok_or_else(|| error("intranet routing requires rules".into()))?;
            let mut insert_at = rules
                .iter()
                .position(|rule| rule.as_str().is_some_and(is_clash_private))
                .unwrap_or_else(|| {
                    rules
                        .iter()
                        .position(|rule| {
                            rule.as_str().is_some_and(|rule| {
                                rule.starts_with("RULE-SET,")
                                    || rule.starts_with("GEOIP,CN,")
                                    || rule.starts_with("MATCH,")
                            })
                        })
                        .unwrap_or(rules.len())
                });
            rules.retain(|rule| !rule.as_str().is_some_and(is_clash_private));
            insert_at = insert_at.min(rules.len());
            if let Some(outbound) = outbound(profile) {
                rules.splice(
                    insert_at..insert_at,
                    [
                        json!(format!("DOMAIN-SUFFIX,local,{outbound}")),
                        json!(format!("GEOIP,LAN,{outbound},no-resolve")),
                    ],
                );
                if outbound == GROUP {
                    let names = document["proxies"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|node| node["name"].as_str())
                        .map(str::to_owned)
                        .chain(std::iter::once("DIRECT".into()))
                        .collect::<Vec<_>>();
                    let groups = document["proxy-groups"]
                        .as_array_mut()
                        .ok_or_else(|| error("intranet routing requires proxy groups".into()))?;
                    if !groups.iter().any(|group| group["name"] == GROUP) {
                        groups.push(json!({"name": GROUP, "type": "select", "proxies": names}));
                    }
                }
            }
            serde_yaml::to_string(&document).map_err(|e| error(e.to_string()))
        }
        _ => Ok(content.into()),
    }
}

fn is_clash_private(rule: &str) -> bool {
    (rule.starts_with("DOMAIN-SUFFIX,local,") || rule.starts_with("GEOIP,LAN,"))
        && rule
            .split(',')
            .nth(2)
            .is_some_and(|outbound| matches!(outbound, GROUP | "DIRECT"))
}

/// Tags owning explicit tunnel routes in a compiled sing-box document.
pub fn private_route_outbounds(document: &Value) -> Vec<String> {
    document["endpoints"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(
            document["outbounds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|outbound| {
                    matches!(outbound["type"].as_str(), Some("wireguard" | "tailscale"))
                }),
        )
        .filter_map(|endpoint| endpoint["tag"].as_str())
        .map(str::to_owned)
        .collect()
}

/// Explicit private destination rules keep precedence over the general policy.
pub fn is_private_route(rule: &Value, outbounds: &[String]) -> bool {
    if rule["ip_is_private"] == true
        || outbounds
            .iter()
            .any(|outbound| rule["outbound"] == *outbound)
    {
        return true;
    }
    let ranges = INTRANET_CAPTURE_PREFIXES
        .map(|prefix| prefix.parse::<ipnet::IpNet>().expect("private prefix"));
    let private = |value: &Value| {
        value
            .as_str()
            .and_then(|value| value.parse::<ipnet::IpNet>().ok())
            .is_some_and(|network| ranges.iter().any(|range| range.contains(&network)))
    };
    match rule.get("ip_cidr") {
        Some(Value::Array(values)) => !values.is_empty() && values.iter().all(private),
        Some(value) => private(value),
        None => false,
    }
}

fn apply_sing_box(profile: &Profile, document: &mut Value) -> Result<(), CompileError> {
    let error = |message: &str| CompileError::Render(message.into());
    let direct = document["outbounds"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item["type"] == "direct")
        .and_then(|item| item["tag"].as_str())
        .map(str::to_owned);
    let selected = outbound(profile);
    if selected == Some(GROUP)
        && !document["outbounds"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|item| item["tag"] == GROUP)
    {
        let names = crate::dns_outbound_members(document);
        document["outbounds"].as_array_mut().ok_or_else(|| error("intranet routing requires outbounds"))?.push(json!({"type": "selector", "tag": GROUP, "outbounds": names, "default": names.first(), "interrupt_exist_connections": true}));
    }
    if selected == Some(GROUP)
        && let Some(group) = document["outbounds"]
            .as_array_mut()
            .and_then(|items| items.iter_mut().find(|item| item["tag"] == GROUP))
        && let Some(default) = group["outbounds"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|member| member.as_str() != direct.as_deref())
            .cloned()
    {
        group["default"] = default;
    }
    let endpoints = private_route_outbounds(document);
    let rules = document["route"]["rules"]
        .as_array_mut()
        .ok_or_else(|| error("intranet routing requires route rules"))?;
    let generic = |rule: &Value| {
        rule["ip_is_private"] == true
            && (rule["outbound"] == GROUP
                || direct
                    .as_deref()
                    .is_some_and(|direct| rule["outbound"] == direct))
            && rule.as_object().is_some_and(|rule| {
                rule.keys()
                    .all(|key| matches!(key.as_str(), "ip_is_private" | "action" | "outbound"))
            })
    };
    let original = rules.iter().position(generic);
    let insert_at = original.unwrap_or_else(|| {
        rules
            .iter()
            .rposition(|rule| {
                matches!(rule["action"].as_str(), Some("sniff" | "hijack-dns"))
                    || rule["protocol"] == "dns"
                    || is_private_route(rule, &endpoints)
                    || rule.get("clash_mode").is_some()
            })
            .map_or(0, |index| index + 1)
    });
    rules.retain(|rule| !generic(rule));
    if let Some(outbound) = selected {
        let outbound = if outbound == "DIRECT" {
            direct
                .as_deref()
                .ok_or_else(|| error("intranet direct routing requires a direct outbound"))?
        } else {
            outbound
        };
        rules.insert(
            insert_at.min(rules.len()),
            json!({"ip_is_private": true, "outbound": outbound}),
        );
    }
    Ok(())
}
