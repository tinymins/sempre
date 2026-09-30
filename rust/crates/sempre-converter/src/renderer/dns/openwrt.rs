use serde_json::{Value, json};

use crate::Target;

use super::SharedDns;

const PRIVATE_CIDRS: [&str; 4] = [
    "127.0.0.0/8",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
];

pub(super) fn render(target: &Target, shared: &SharedDns, source: &Value) -> Value {
    let modern = target.version != "11";
    let response_matching = target.version == "14";
    let (local_dns, local_system) = shared.local_server();
    let mut servers = vec![if modern {
        json!({ "type": "local", "tag": "local" })
    } else {
        json!({ "tag": "local", "address": local_dns, "detour": "direct" })
    }];
    if shared.fakeip_enabled() {
        servers.push(if modern {
            json!({ "type": "fakeip", "tag": "fakeip", "inet4_range": shared.fakeip_ipv4_range, "inet6_range": shared.fakeip_ipv6_range })
        } else {
            json!({ "tag": "fakeip", "address": "fakeip", "strategy": "ipv4_only" })
        });
    }
    if !local_system {
        servers.push(if modern {
            json!({ "type": "udp", "tag": "local_v4", "server": local_dns, "server_port": shared.local_port })
        } else {
            json!({ "tag": "local_v4", "address": local_dns, "strategy": "ipv4_only", "detour": "direct" })
        });
    }

    let mut rules = Vec::new();
    if shared.reject_https() {
        rules.push(json!({ "query_type": ["HTTPS"], "action": "reject" }));
    }
    if response_matching {
        rules.push(json!({ "action": "evaluate", "server": "local" }));
    }
    let mut private = json!({ "ip_cidr": PRIVATE_CIDRS, "server": "local" });
    if modern {
        private["action"] = json!("route");
    }
    if response_matching {
        private["match_response"] = json!(true);
    }
    rules.push(private);
    if shared.cn_domain_local_dns() && shared.cn_domain_rule_set.enabled {
        let mut cn = json!({ "rule_set": ["geosite-cn"], "server": "local" });
        if modern {
            cn["action"] = json!("route");
        }
        rules.push(cn);
    }
    if shared.cn_ip_local_dns() && shared.cn_ip_rule_set.enabled {
        let mut conditions = vec![json!({ "rule_set": ["geoip-cn"] })];
        if shared.exclude_hk_from_cn_ip() && shared.hk_ip_rule_set.enabled {
            conditions.push(json!({ "rule_set": ["geoip-hk"], "invert": true }));
        }
        if source
            .pointer("/shared/gfwBlackRuleSetUrl")
            .and_then(Value::as_str)
            .is_some_and(|url| !url.trim().is_empty())
        {
            conditions.push(json!({ "rule_set": ["geoip-gfwblack"], "invert": true }));
        }
        let mut cn =
            json!({ "type": "logical", "mode": "and", "rules": conditions, "server": "local" });
        if modern {
            cn["action"] = json!("route");
        }
        if response_matching {
            for condition in cn["rules"].as_array_mut().expect("logical rules") {
                condition["match_response"] = json!(true);
            }
        }
        rules.push(cn);
    }
    if shared.fakeip_enabled() {
        let mut fakeip = json!({
            "disable_cache": false, "rewrite_ttl": shared.fakeip_ttl,
            "query_type": ["A", "AAAA"], "server": "fakeip"
        });
        if modern {
            fakeip["action"] = json!("route");
        }
        rules.push(fakeip);
    }

    if modern {
        let mut output = json!({ "servers": servers, "rules": rules, "independent_cache": false, "final": "local" });
        if shared.prefer_ipv4() {
            output["strategy"] = json!("prefer_ipv4");
        }
        output
    } else {
        let mut output = json!({
            "disable_cache": false, "servers": servers, "rules": rules,
            "disable_expire": false, "independent_cache": false, "reverse_mapping": false
        });
        if shared.prefer_ipv4() {
            output["strategy"] = json!("prefer_ipv4");
        }
        if shared.fakeip_enabled() {
            output["fakeip"] = json!({ "enabled": true, "inet4_range": shared.fakeip_ipv4_range, "inet6_range": shared.fakeip_ipv6_range });
        }
        output
    }
}
