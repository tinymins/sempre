use std::collections::HashMap;

use serde_json::{Value, json};

use crate::{Profile, SourceSnapshot, Target, rule_provider_snapshot_id};

use super::private_access::Resolved;

pub(super) fn inbounds(profile: &Profile, target: &Target, private: &Resolved) -> Vec<Value> {
    let local = if target.standalone {
        Vec::new()
    } else {
        local_inbounds(profile)
    };
    local
        .into_iter()
        .chain(super::super::dns::sing_box_system_inbounds(profile, target))
        .chain(super::super::transparent::sing_box_inbounds(
            profile,
            target,
            &private.capture_cidrs,
        ))
        .collect()
}

fn local_inbounds(profile: &Profile) -> Vec<Value> {
    let users = vec![
        json!({ "username": profile.local_proxy.username, "password": profile.local_proxy.password }),
    ];
    vec![
        json!({ "type": "socks", "tag": "sempre-socks-in", "listen": "127.0.0.1", "listen_port": profile.local_proxy.socks_port, "users": users }),
        json!({ "type": "http", "tag": "sempre-http-in", "listen": "127.0.0.1", "listen_port": profile.local_proxy.http_port, "users": users }),
    ]
}

pub(super) fn route(
    profile: &Profile,
    target: &Target,
    snapshots: &[SourceSnapshot],
    private: &Resolved,
    warnings: &mut Vec<String>,
) -> Value {
    let final_outbound = final_outbound(profile);
    let snapshots = snapshots
        .iter()
        .map(|snapshot| (snapshot.source_id.as_str(), snapshot.content.as_str()))
        .collect::<HashMap<_, _>>();
    let mut rule_sets = Vec::new();
    let mut rules = super::super::dns::sing_box_system_route_rules(profile, target);
    rules.extend(if target.version == "11" {
        vec![json!({ "protocol": "dns", "outbound": "dns-out" })]
    } else {
        vec![
            json!({ "action": "sniff" }),
            json!({ "protocol": "dns", "action": "hijack-dns" }),
        ]
    });
    if !private.direct_domains.is_empty() {
        rules.push(json!({
            "domain": private.direct_domains, "action": "route", "outbound": "direct"
        }));
    }
    rules.extend(private.route_rules.iter().cloned());
    let direct_modes = profile
        .network_policy
        .get("directNetworkIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(crate::network_mode)
        .collect::<Vec<_>>();
    if !direct_modes.is_empty() {
        rules.push(json!({
            "clash_mode": direct_modes, "action": "route", "outbound": "direct"
        }));
    }
    rules.push(json!({ "ip_is_private": true, "outbound": "direct" }));
    append_rule_providers(
        profile
            .rule_providers
            .iter()
            .filter(|provider| provider.priority),
        &snapshots,
        final_outbound,
        &mut rule_sets,
        &mut rules,
    );
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
    let (dns_rule_sets, dns_routes) = super::super::dns::sing_box_route_policy(profile, target);
    rule_sets.extend(dns_rule_sets);
    rules.extend(dns_routes);
    append_rule_providers(
        profile
            .rule_providers
            .iter()
            .filter(|provider| !provider.priority),
        &snapshots,
        final_outbound,
        &mut rule_sets,
        &mut rules,
    );
    let mut route = json!({ "rules": rules, "rule_set": rule_sets, "final": final_outbound });
    if target.version != "11" {
        route["default_domain_resolver"] =
            json!({ "server": "bootstrap", "strategy": "ipv4_only" });
    }
    if target.platform != "default" || profile.transparent_proxy.mode == "tun-router" {
        route["auto_detect_interface"] = json!(true);
    }
    route
}

fn append_rule_providers<'a>(
    providers: impl Iterator<Item = &'a crate::RuleProvider>,
    snapshots: &HashMap<&str, &str>,
    final_outbound: &str,
    rule_sets: &mut Vec<Value>,
    rules: &mut Vec<Value>,
) {
    for provider in providers {
        let snapshot_id = rule_provider_snapshot_id(&provider.tag);
        if let Some(content) = snapshots.get(snapshot_id.as_str())
            && let Some(inline) = crate::rule_set::inline_rules(content)
        {
            rule_sets.push(json!({ "type": "inline", "tag": provider.tag, "rules": inline }));
        } else {
            let format = if provider.format.is_empty() {
                "source"
            } else {
                &provider.format
            };
            rule_sets.push(json!({ "type": "remote", "tag": provider.tag, "format": format, "url": provider.url, "download_detour": "direct" }));
        }
        rules.push(json!({ "rule_set": [provider.tag], "outbound": if provider.outbound.is_empty() { final_outbound } else { normalize_outbound(&provider.outbound) } }));
    }
}

fn final_outbound(profile: &Profile) -> &str {
    profile
        .groups
        .iter()
        .find(|group| group.name == "⚓️ 其他流量")
        .or_else(|| profile.groups.first())
        .map_or("proxy", |group| group.name.as_str())
}

fn normalize_outbound(value: &str) -> &str {
    match value {
        "DIRECT" | "🚀 直接连接" => "direct",
        "REJECT" => "reject",
        _ => value,
    }
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
    Some(json!({ (key): parts[1], "outbound": normalize_outbound(parts[2]) }))
}

pub(super) fn experimental(profile: &Profile, target: &Target, store_fakeip: bool) -> Value {
    let desktop = target.standalone && target.platform != "default";
    let external_controller = if desktop {
        loopback_controller(&profile.management_api.external_controller)
    } else {
        profile.management_api.external_controller.clone()
    };
    let external_ui = if desktop {
        "./ui".into()
    } else {
        profile.management_api.external_ui.clone()
    };
    json!({
        "cache_file": {
            "enabled": true, "path": "cache.db",
            "store_fakeip": store_fakeip, "store_rdrc": false
        },
        "clash_api": {
            "external_controller": external_controller,
            "external_ui": external_ui,
            "secret": profile.management_api.secret,
            "default_mode": "rule"
        }
    })
}

fn loopback_controller(value: &str) -> String {
    let port = value
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .unwrap_or(9999);
    format!("127.0.0.1:{port}")
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
