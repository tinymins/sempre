use std::collections::HashSet;

use serde_json::{Map, Value, json};

use super::model::RuntimeModel;

pub(super) fn render(model: &RuntimeModel<'_>) -> (Value, Vec<String>) {
    let group_names = model
        .groups
        .iter()
        .map(|group| group.name.as_str())
        .collect::<HashSet<_>>();
    let balancers = model
        .groups
        .iter()
        .map(|group| {
            let members = if group.group_type == "url-test" {
                group.members.clone()
            } else {
                vec![group.default.clone()]
            };
            json!({
                "tag": group.name, "selector": members,
                "strategy": { "type": if group.group_type == "url-test" { "leastPing" } else { "random" } }
            })
        })
        .collect::<Vec<_>>();
    let mut rules = vec![
        json!({ "type": "field", "inboundTag": ["dns-in"], "outboundTag": "dns-out" }),
        json!({ "type": "field", "inboundTag": ["local-dns", "bootstrap-dns"], "outboundTag": "direct" }),
        route_target(
            json!({ "type": "field", "inboundTag": ["remote-dns"] }),
            &model.final_outbound,
            &group_names,
        ),
        json!({ "type": "field", "ip": ["geoip:private", "geoip:cn"], "outboundTag": "direct" }),
        json!({ "type": "field", "domain": ["geosite:cn"], "outboundTag": "direct" }),
    ];
    let mut warnings = Vec::new();
    for value in &model.profile.rules {
        let Some(line) = value.as_str() else {
            warnings.push(format!("unsupported rule: {value}"));
            continue;
        };
        match parse_rule(line) {
            Some((rule, target)) => rules.push(route_target(rule, &target, &group_names)),
            None => warnings.push(format!("unsupported rule: {line}")),
        }
    }
    for provider in &model.profile.rule_providers {
        warnings.push(format!(
            "rule provider {} is not representable by the v2ray-family renderer",
            provider.tag
        ));
    }
    rules.push(route_target(
        json!({ "type": "field", "network": "tcp,udp" }),
        &model.final_outbound,
        &group_names,
    ));
    (
        json!({
            "domainStrategy": "IPIfNonMatch", "domainMatcher": "hybrid",
            "rules": rules, "balancers": balancers
        }),
        warnings,
    )
}

fn parse_rule(line: &str) -> Option<(Value, String)> {
    let parts = line.split(',').map(str::trim).collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }
    let target = parts.last()?.to_string();
    let mut rule = Map::new();
    rule.insert("type".into(), json!("field"));
    let (key, value) = match parts[0].to_ascii_uppercase().as_str() {
        "DOMAIN" => ("domain", json!([format!("full:{}", parts[1])])),
        "DOMAIN-SUFFIX" => ("domain", json!([format!("domain:{}", parts[1])])),
        "DOMAIN-KEYWORD" => ("domain", json!([format!("keyword:{}", parts[1])])),
        "GEOSITE" => (
            "domain",
            json!([format!("geosite:{}", parts[1].to_ascii_lowercase())]),
        ),
        "GEOIP" => (
            "ip",
            json!([format!("geoip:{}", parts[1].to_ascii_lowercase())]),
        ),
        "IP-CIDR" | "IP-CIDR6" => ("ip", json!([parts[1]])),
        "SRC-IP-CIDR" => ("source", json!([parts[1]])),
        "DST-PORT" => ("port", json!(parts[1])),
        "NETWORK" => ("network", json!(parts[1].to_ascii_lowercase())),
        _ => return None,
    };
    rule.insert(key.into(), value);
    Some((Value::Object(rule), target))
}

fn route_target(mut rule: Value, target: &str, groups: &HashSet<&str>) -> Value {
    let target = normalize_target(target);
    if groups.contains(target) {
        rule["balancerTag"] = json!(target);
    } else {
        rule["outboundTag"] = json!(target);
    }
    rule
}

fn normalize_target(target: &str) -> &str {
    match target {
        "DIRECT" | "🚀 直接连接" => "direct",
        "REJECT" => "reject",
        value => value,
    }
}

pub(super) fn observatory(model: &RuntimeModel<'_>) -> Option<Value> {
    let mut selectors = Vec::new();
    let mut seen = HashSet::new();
    let mut interval = 300;
    for group in &model.groups {
        if group.group_type != "url-test" {
            continue;
        }
        for member in &group.members {
            if seen.insert(member.as_str()) {
                selectors.push(member.clone());
            }
        }
        if group.interval > 0 {
            interval = interval.min(group.interval);
        }
    }
    (!selectors.is_empty()).then(|| {
        json!({
            "subjectSelector": selectors,
            "probeURL": "https://www.gstatic.com/generate_204",
            "probeInterval": format!("{interval}s"),
            "enableConcurrency": true
        })
    })
}
