use serde_json::{Value, json};

use crate::{Profile, Target};

pub(super) fn local_inbounds(profile: &Profile) -> Vec<Value> {
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

pub(super) fn route(profile: &Profile, warnings: &mut Vec<String>) -> Value {
    let final_outbound = profile
        .groups
        .first()
        .map_or("proxy", |group| group.name.as_str());
    let mut rule_sets = Vec::new();
    let mut rules = Vec::new();
    for value in &profile.rules {
        if value.is_object() {
            rules.push(value.clone());
            continue;
        }
        let Some(line) = value.as_str() else {
            warnings.push("custom rule must be a Clash rule string or sing-box object".into());
            continue;
        };
        match custom_route_rule(line) {
            Some(rule) => rules.push(rule),
            None if line.split(',').next() == Some("MATCH") => {}
            None => warnings.push(format!("unsupported custom rule: {line}")),
        }
    }
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

fn custom_route_rule(line: &str) -> Option<Value> {
    let parts = line.split(',').map(str::trim).collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }
    let key = match parts[0] {
        "DOMAIN" => "domain",
        "DOMAIN-SUFFIX" => "domain_suffix",
        "DOMAIN-KEYWORD" => "domain_keyword",
        "DOMAIN-REGEX" => "domain_regex",
        "IP-CIDR" | "IP-CIDR6" => "ip_cidr",
        "SRC-IP-CIDR" => "source_ip_cidr",
        _ => return None,
    };
    Some(json!({ (key): parts[1], "outbound": parts[2] }))
}

pub(super) fn management_api(profile: &Profile) -> Value {
    if profile.management_api.external_controller.is_empty() {
        return json!({});
    }
    json!({ "clash_api": { "external_controller": profile.management_api.external_controller, "secret": profile.management_api.secret, "external_ui": profile.management_api.external_ui } })
}

pub(super) fn log(level: &str) -> Value {
    let disabled = level == "off";
    let level = if matches!(level, "error" | "warn" | "info" | "debug") {
        level
    } else {
        "info"
    };
    json!({ "disabled": disabled, "level": level, "timestamp": true })
}

pub(super) fn normalize_for_version(config: &mut Value, target: &Target) {
    if target.version == "11" {
        return;
    }
    if let Some(route) = config.get_mut("route").and_then(Value::as_object_mut) {
        route.remove("geoip");
        route.remove("geosite");
    }
}
