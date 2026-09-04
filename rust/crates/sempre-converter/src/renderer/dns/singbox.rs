use std::net::IpAddr;

use serde_json::{Value, json};

use crate::{Profile, Proxy, Target};

use super::{SharedDns, managed_frontend, native_override};

const FRONTEND_DNS_INBOUND: &str = "sempre-dns-core-in";

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
    let frontend = managed_frontend(shared, target);
    let fakeip = shared.fakeip_enabled() && (target.platform != "macos" || frontend);
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
            frontend,
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
    frontend: bool,
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
    let rules = rules(
        shared,
        RuleOptions {
            modern: true,
            response_matching,
            fakeip,
            frontend: match (frontend, fakeip) {
                (true, true) => FrontendMode::FakeIp,
                (true, false) => FrontendMode::RealIp,
                (false, _) => FrontendMode::Disabled,
            },
        },
        bootstrap_domains,
    );
    // Older cores otherwise share local/remote answers even with an explicit resolver.
    let mut result = json!({
        "servers": servers, "rules": rules, "independent_cache": true,
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
        "rules": rules(shared, RuleOptions { fakeip, ..RuleOptions::default() }, bootstrap_domains),
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

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum FrontendMode {
    #[default]
    Disabled,
    RealIp,
    FakeIp,
}

#[derive(Clone, Copy, Default)]
struct RuleOptions {
    modern: bool,
    response_matching: bool,
    fakeip: bool,
    frontend: FrontendMode,
}

fn rules(shared: &SharedDns, options: RuleOptions, bootstrap_domains: &[String]) -> Vec<Value> {
    let mut rules = Vec::new();
    if !bootstrap_domains.is_empty() {
        let mut rule = json!({ "domain": bootstrap_domains, "server": "bootstrap" });
        if options.modern {
            rule["action"] = json!("route");
        }
        rules.push(rule);
    }
    if shared.reject_https() {
        rules.push(json!({ "query_type": ["HTTPS"], "action": "reject" }));
    }
    if options.frontend != FrontendMode::Disabled {
        if options.frontend == FrontendMode::FakeIp {
            rules.push(json!({
                "inbound": [FRONTEND_DNS_INBOUND], "query_type": ["A", "AAAA"],
                "server": "fakeip", "action": "route"
            }));
        }
        rules.push(json!({
            "inbound": [FRONTEND_DNS_INBOUND], "server": "remote", "action": "route"
        }));
    }
    let private = [
        "127.0.0.0/8",
        "10.0.0.0/8",
        "172.16.0.0/12",
        "192.168.0.0/16",
    ];
    if !options.response_matching {
        let mut rule = json!({ "ip_cidr": private, "server": "local" });
        if options.modern {
            rule["action"] = json!("route");
        }
        rules.push(rule);
    }
    if shared.cn_domain_local_dns() && shared.cn_domain_rule_set.enabled {
        let mut rule = json!({ "rule_set": ["geosite-cn"], "server": "local" });
        if options.modern {
            rule["action"] = json!("route");
        }
        rules.push(rule);
    }
    if options.response_matching {
        rules.push(json!({ "action": "evaluate", "server": "remote" }));
        rules.push(json!({
            "match_response": true, "ip_cidr": private,
            "action": "route", "server": "local"
        }));
    }
    if shared.cn_ip_local_dns() && shared.cn_ip_rule_set.enabled {
        let mut rule = if shared.exclude_hk_from_cn_ip() && shared.hk_ip_rule_set.enabled {
            json!({ "type": "logical", "mode": "and", "server": "local", "rules": [
                { "rule_set": ["geoip-cn"] }, { "invert": true, "rule_set": ["geoip-hk"] }
            ] })
        } else {
            json!({ "rule_set": ["geoip-cn"], "server": "local" })
        };
        if options.response_matching {
            if let Some(sub_rules) = rule.get_mut("rules").and_then(Value::as_array_mut) {
                for sub_rule in sub_rules {
                    sub_rule["match_response"] = json!(true);
                }
            } else {
                rule["match_response"] = json!(true);
            }
        }
        if options.modern {
            rule["action"] = json!("route");
        }
        rules.push(rule);
    }
    if options.fakeip {
        let mut rule = json!({ "disable_cache": false, "rewrite_ttl": shared.fakeip_ttl, "query_type": ["A", "AAAA"], "server": "fakeip" });
        if options.modern {
            rule["action"] = json!("route");
        }
        rules.push(rule);
    }
    rules
}

pub(super) fn route_policy(profile: &Profile, target: &Target) -> (Vec<Value>, Vec<Value>) {
    let shared = SharedDns::resolve(&profile.dns);
    let mut rule_sets = Vec::new();
    for (tag, rule_set) in [
        ("geoip-cn", &shared.cn_ip_rule_set),
        ("geoip-hk", &shared.hk_ip_rule_set),
        ("geosite-cn", &shared.cn_domain_rule_set),
    ] {
        if rule_set.enabled {
            rule_sets.push(json!({ "tag": tag, "type": "remote", "format": "binary", "url": rule_set.url, "download_detour": rule_set.detour }));
        }
    }
    let mut routes = Vec::new();
    // Known domestic domains keep local/CDN resolution and need no GeoIP lookup.
    if shared.cn_domain_rule_set.enabled {
        routes.push(json!({ "rule_set": ["geosite-cn"], "outbound": "direct" }));
    }
    if shared.cn_ip_rule_set.enabled {
        if target.version != "11" && native_override(&profile.dns, "sing_box_v12").is_none() {
            // FakeIP restores a domain, not the real addresses required by GeoIP.
            // Unknown domains must not inherit poisoned local DNS answers.
            routes.push(json!({ "action": "resolve", "server": "remote" }));
        }
        routes.push(json!({ "rule_set": ["geoip-cn"], "outbound": "direct" }));
    }
    (rule_sets, routes)
}

pub(super) fn system_inbounds(
    profile: &Profile,
    target: &Target,
    shared: &SharedDns,
) -> Vec<Value> {
    if !shared.system_takeover() {
        return Vec::new();
    }
    if managed_frontend(shared, target) {
        let listen_port = match profile.transparent_proxy.tproxy.dns_listen_port {
            0 => crate::DEFAULT_CORE_DNS_PORT,
            port => port,
        };
        return vec![json!({
            "type": "direct", "tag": FRONTEND_DNS_INBOUND,
            "listen": "127.0.0.1",
            "listen_port": listen_port,
            "override_address": "1.1.1.1", "override_port": 53
        })];
    }
    shared
        .system_dns_listen_hosts
        .iter()
        .enumerate()
        .map(|(index, host)| {
            json!({
                "type": "direct", "tag": system_inbound_tag(host, index),
                "listen": host, "listen_port": shared.system_dns_listen_port,
                "override_address": "1.1.1.1", "override_port": 53
            })
        })
        .collect()
}

pub(super) fn system_route_rules(
    _profile: &Profile,
    target: &Target,
    shared: &SharedDns,
) -> Vec<Value> {
    if !shared.system_takeover() {
        return Vec::new();
    }
    if managed_frontend(shared, target) {
        return vec![
            json!({ "inbound": FRONTEND_DNS_INBOUND, "action": "sniff" }),
            json!({ "inbound": FRONTEND_DNS_INBOUND, "protocol": "dns", "action": "hijack-dns" }),
        ];
    }
    shared
        .system_dns_listen_hosts
        .iter()
        .enumerate()
        .flat_map(|(index, host)| {
            let tag = system_inbound_tag(host, index);
            [
                json!({ "inbound": tag, "action": "sniff" }),
                json!({ "inbound": tag, "protocol": "dns", "action": "hijack-dns" }),
            ]
        })
        .collect()
}

fn system_inbound_tag(host: &str, index: usize) -> String {
    match host {
        "127.0.0.1" => "system-dns-in".into(),
        "0.0.0.0" => "system-dns-in-any".into(),
        _ => format!("system-dns-in-{index}"),
    }
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
