use serde_json::Value;

use super::super::outbound_names::{DIRECT, restore_field};

pub(super) fn restore_direct_name(config: &mut Value) {
    if let Some(outbounds) = config.get_mut("outbounds").and_then(Value::as_array_mut) {
        for outbound in outbounds {
            if outbound.get("type").and_then(Value::as_str) == Some("direct")
                && outbound.get("tag").and_then(Value::as_str) == Some("direct")
            {
                outbound["tag"] = Value::from(DIRECT);
            }
            if matches!(
                outbound.get("type").and_then(Value::as_str),
                Some("selector" | "urltest")
            ) {
                restore_field(outbound, "outbounds");
                restore_field(outbound, "default");
            }
            restore_field(outbound, "detour");
        }
    }
    if let Some(endpoints) = config.get_mut("endpoints").and_then(Value::as_array_mut) {
        for endpoint in endpoints {
            restore_field(endpoint, "detour");
        }
    }
    if let Some(route) = config.get_mut("route") {
        restore_field(route, "final");
        if let Some(rules) = route.get_mut("rules").and_then(Value::as_array_mut) {
            rename_rules(rules);
        }
        if let Some(rule_sets) = route.get_mut("rule_set").and_then(Value::as_array_mut) {
            for rule_set in rule_sets {
                restore_field(rule_set, "download_detour");
            }
        }
    }
    if let Some(dns) = config.get_mut("dns") {
        if let Some(servers) = dns.get_mut("servers").and_then(Value::as_array_mut) {
            for server in servers {
                restore_field(server, "detour");
            }
        }
        if let Some(rules) = dns.get_mut("rules").and_then(Value::as_array_mut) {
            rename_rules(rules);
        }
    }
}

fn rename_rules(rules: &mut [Value]) {
    for rule in rules {
        restore_field(rule, "outbound");
        if let Some(nested) = rule.get_mut("rules").and_then(Value::as_array_mut) {
            rename_rules(nested);
        }
    }
}
