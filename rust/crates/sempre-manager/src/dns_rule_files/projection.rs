use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::{DnsRoutingRuleSet, DnsSettings, ManagerError};

pub(super) fn same_structure(left: &DnsSettings, right: &DnsSettings) -> bool {
    left.enabled == right.enabled
        && left.domestic_domains == right.domestic_domains
        && left.rule_sets.len() == right.rule_sets.len()
        && left
            .rule_sets
            .iter()
            .zip(&right.rule_sets)
            .all(|(a, b)| a.id == b.id && a.name == b.name && a.mode == b.mode)
}

pub(super) fn tag(rule_set: &DnsRoutingRuleSet) -> String {
    format!("sempre-dns-rule-set:{}", rule_set.id)
}

pub(super) fn source(rule_set: &DnsRoutingRuleSet) -> Result<Vec<u8>, ManagerError> {
    let rules = if rule_set.domains.is_empty() {
        Vec::new()
    } else {
        vec![crate::dns_routing::inline_rule(rule_set)]
    };
    super::encode(&json!({ "version": 1, "rules": rules }))
}

pub(super) fn file_path(config: &Path, rule_set: &DnsRoutingRuleSet) -> PathBuf {
    config
        .parent()
        .expect("runtime configuration directory")
        .join("dns-rule-sets")
        .join(format!("{}.json", rule_set.id))
}

pub(super) fn local(rule_set: &DnsRoutingRuleSet, path: &Path) -> Value {
    json!({ "type": "local", "tag": tag(rule_set), "format": "source", "path": path })
}

pub(super) fn matches_inline(document: &Value, settings: &DnsSettings) -> bool {
    let Some(definitions) = document["route"]["rule_set"].as_array() else {
        return settings.rule_sets.is_empty();
    };
    let Some(routes) = document["route"]["rules"].as_array() else {
        return false;
    };
    let mut last_index = None;
    for rule_set in &settings.rule_sets {
        let tag = tag(rule_set);
        let matching = definitions
            .iter()
            .filter(|value| value["tag"] == tag)
            .collect::<Vec<_>>();
        let expected = json!({
            "type": "inline", "tag": tag,
            "rules": [crate::dns_routing::inline_rule(rule_set)],
        });
        if matching.as_slice() != [&expected] {
            return false;
        }
        let matching_routes = routes
            .iter()
            .enumerate()
            .filter(|(_, value)| references(value, &tag))
            .collect::<Vec<_>>();
        let [(index, route)] = matching_routes.as_slice() else {
            return false;
        };
        let Some(outbound) = route["outbound"].as_str() else {
            return false;
        };
        if **route != json!({ "rule_set": [tag], "outbound": outbound })
            || last_index.is_some_and(|previous| previous >= *index)
        {
            return false;
        }
        let expected_outbound = if rule_set.mode == "direct" {
            document["outbounds"]
                .as_array()
                .and_then(|outbounds| {
                    outbounds
                        .iter()
                        .find(|value| value["tag"] == outbound && value["type"] == "direct")
                })
                .is_some()
        } else {
            outbound == format!("DNS · {}", rule_set.name)
                && document["outbounds"].as_array().is_some_and(|outbounds| {
                    outbounds
                        .iter()
                        .any(|value| value["tag"] == outbound && value["type"] == "selector")
                })
        };
        if !expected_outbound {
            return false;
        }
        last_index = Some(*index);
    }
    true
}

fn references(value: &Value, tag: &str) -> bool {
    value.get("rule_set").is_some_and(|value| {
        value.as_str() == Some(tag)
            || value
                .as_array()
                .is_some_and(|values| values.iter().any(|value| value == tag))
    }) || value
        .get("rules")
        .and_then(Value::as_array)
        .is_some_and(|rules| rules.iter().any(|rule| references(rule, tag)))
}
