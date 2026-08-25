use std::net::IpAddr;

use serde_json::{Value, json};

use crate::{Profile, Proxy, Target};

use super::{SharedDns, native_override};

pub(super) fn render(
    profile: &Profile,
    proxies: &[Proxy],
    target: &Target,
    shared: &SharedDns,
) -> Value {
    let modern = target.version != "11";
    let key = if modern {
        "sing_box_v12"
    } else {
        "sing_box_v11"
    };
    if let Some(value) = native_override(&profile.dns, key) {
        return value;
    }
    let fakeip = shared.fakeip_enabled() && target.platform != "macos";
    let bootstrap_domains = proxies
        .iter()
        .filter(|proxy| proxy.server.parse::<IpAddr>().is_err())
        .map(|proxy| proxy.server.clone())
        .collect::<Vec<_>>();
    let remote_detour = if shared.remote_detour.is_empty() {
        profile
            .groups
            .iter()
            .find(|group| group.name == "🔰 国外流量")
            .or_else(|| profile.groups.first())
            .map_or("proxy", |group| group.name.as_str())
    } else {
        shared.remote_detour.as_str()
    };
    if modern {
        modern_dns(
            shared,
            fakeip,
            remote_detour,
            &bootstrap_domains,
            target.version == "14",
        )
    } else {
        legacy_dns(shared, fakeip, remote_detour, &bootstrap_domains)
    }
}

fn modern_dns(
    shared: &SharedDns,
    fakeip: bool,
    remote_detour: &str,
    bootstrap_domains: &[String],
    response_matching: bool,
) -> Value {
    let (local_server, local_system) = shared.local_server();
    let local = modern_local("local", local_server, shared);
    let mut servers = vec![local];
    if fakeip {
        servers.push(json!({ "type": "fakeip", "tag": "fakeip", "inet4_range": shared.fakeip_ipv4_range, "inet6_range": shared.fakeip_ipv6_range }));
    }
    if !local_system {
        servers.push(modern_local("local_v4", local_server, shared));
    }
    servers.push(json!({ "type": "tls", "tag": "bootstrap", "server": shared.bootstrap_dns, "server_port": shared.bootstrap_port, "tls": { "server_name": shared.bootstrap_server_name } }));
    let mut remote = json!({ "type": "tls", "tag": "remote", "server": shared.remote_dns, "server_port": shared.remote_port, "tls": { "server_name": shared.remote_server_name } });
    if !remote_detour.is_empty() && remote_detour != "direct" {
        remote["detour"] = json!(remote_detour);
    }
    servers.push(remote);
    let rules = rules(shared, true, response_matching, fakeip, bootstrap_domains);
    let mut result = json!({
        "servers": servers, "rules": rules, "independent_cache": false,
        "reverse_mapping": true, "final": "remote"
    });
    if shared.prefer_ipv4() {
        result["strategy"] = json!("prefer_ipv4");
    }
    result
}

fn legacy_dns(
    shared: &SharedDns,
    fakeip: bool,
    remote_detour: &str,
    bootstrap_domains: &[String],
) -> Value {
    let (local_server, local_system) = shared.local_server();
    let local_address = if shared.local_transport == "tls" {
        format!("tls://{local_server}:{}", shared.local_port)
    } else {
        local_server.to_owned()
    };
    let mut servers = vec![json!({ "tag": "local", "address": local_address })];
    if fakeip {
        servers.push(json!({ "tag": "fakeip", "address": "fakeip", "strategy": "ipv4_only" }));
    }
    if !local_system {
        servers
            .push(json!({ "tag": "local_v4", "address": local_address, "strategy": "ipv4_only" }));
    }
    servers.push(json!({ "tag": "bootstrap", "address": format!("tls://{}:{}", shared.bootstrap_dns, shared.bootstrap_port) }));
    let mut remote = json!({ "tag": "remote", "address": format!("tls://{}:{}", shared.remote_dns, shared.remote_port) });
    if !remote_detour.is_empty() && remote_detour != "direct" {
        remote["detour"] = json!(remote_detour);
    }
    servers.push(remote);
    let mut result = json!({
        "disable_cache": false, "servers": servers,
        "rules": rules(shared, false, false, fakeip, bootstrap_domains),
        "disable_expire": false, "independent_cache": false,
        "reverse_mapping": true, "final": "remote"
    });
    if shared.prefer_ipv4() {
        result["strategy"] = json!("prefer_ipv4");
    }
    if fakeip {
        result["fakeip"] = json!({ "enabled": true, "inet4_range": shared.fakeip_ipv4_range, "inet6_range": shared.fakeip_ipv6_range });
    }
    result
}

fn modern_local(tag: &str, server: &str, shared: &SharedDns) -> Value {
    if shared.local_server().1 {
        return json!({ "type": "local", "tag": tag });
    }
    let mut result = json!({ "type": shared.local_transport, "tag": tag, "server": server, "server_port": shared.local_port });
    if shared.local_transport == "tls" && !shared.local_server_name.is_empty() {
        result["tls"] = json!({ "server_name": shared.local_server_name });
    }
    result
}

fn rules(
    shared: &SharedDns,
    modern: bool,
    response_matching: bool,
    fakeip: bool,
    bootstrap_domains: &[String],
) -> Vec<Value> {
    let mut rules = Vec::new();
    if !bootstrap_domains.is_empty() {
        let mut rule = json!({ "domain": bootstrap_domains, "server": "bootstrap" });
        if modern {
            rule["action"] = json!("route");
        }
        rules.push(rule);
    }
    if shared.reject_https() {
        rules.push(json!({ "query_type": ["HTTPS"], "action": "reject" }));
    }
    let private = [
        "127.0.0.0/8",
        "10.0.0.0/8",
        "172.16.0.0/12",
        "192.168.0.0/16",
    ];
    if response_matching {
        rules.push(json!({ "action": "evaluate", "server": "local" }));
        rules.push(json!({ "match_response": true, "ip_cidr": private, "action": "respond" }));
    } else {
        let mut rule = json!({ "ip_cidr": private, "server": "local" });
        if modern {
            rule["action"] = json!("route");
        }
        rules.push(rule);
    }
    if shared.cn_domain_local_dns() && shared.cn_domain_rule_set.enabled {
        let mut rule = json!({ "rule_set": ["geosite-cn"], "server": "local" });
        if modern {
            rule["action"] = json!("route");
        }
        rules.push(rule);
    }
    if shared.cn_ip_local_dns() && shared.cn_ip_rule_set.enabled {
        let mut rule = if shared.exclude_hk_from_cn_ip() && shared.hk_ip_rule_set.enabled {
            json!({ "type": "logical", "mode": "and", "server": "local", "rules": [
                { "rule_set": ["geoip-cn"] }, { "invert": true, "rule_set": ["geoip-hk"] }
            ] })
        } else {
            json!({ "rule_set": ["geoip-cn"], "server": "local" })
        };
        if modern {
            rule["action"] = json!("route");
        }
        rules.push(rule);
    }
    if fakeip {
        let mut rule = json!({ "disable_cache": false, "rewrite_ttl": shared.fakeip_ttl, "query_type": ["A", "AAAA"], "server": "fakeip" });
        if modern {
            rule["action"] = json!("route");
        }
        rules.push(rule);
    }
    rules
}

pub(super) fn route_policy(profile: &Profile) -> (Vec<Value>, Option<Value>) {
    let shared = SharedDns::resolve(&profile.dns);
    let mut rule_sets = Vec::new();
    let mut direct = Vec::new();
    for (tag, rule_set, route_direct) in [
        ("geoip-cn", &shared.cn_ip_rule_set, true),
        ("geoip-hk", &shared.hk_ip_rule_set, false),
        ("geosite-cn", &shared.cn_domain_rule_set, true),
    ] {
        if rule_set.enabled {
            rule_sets.push(json!({ "tag": tag, "type": "remote", "format": "binary", "url": rule_set.url, "download_detour": rule_set.detour }));
            if route_direct {
                direct.push(tag);
            }
        }
    }
    let route = (!direct.is_empty()).then(|| json!({ "rule_set": direct, "outbound": "direct" }));
    (rule_sets, route)
}

pub(super) fn strip_fakeip(config: &mut Value) {
    config.as_object_mut().map(|value| value.remove("fakeip"));
    for key in ["servers", "rules"] {
        if let Some(values) = config.get_mut(key).and_then(Value::as_array_mut) {
            values.retain(|value| {
                value.get("tag").and_then(Value::as_str) != Some("fakeip")
                    && value.get("type").and_then(Value::as_str) != Some("fakeip")
                    && value.get("address").and_then(Value::as_str) != Some("fakeip")
                    && value.get("server").and_then(Value::as_str) != Some("fakeip")
            });
        }
    }
}
