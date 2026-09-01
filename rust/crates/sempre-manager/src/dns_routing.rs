use std::collections::HashSet;

use sempre_converter::{
    CompileOverlay, ProxyGroup, RuleProvider, SourceSnapshot, rule_provider_snapshot_id,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{DnsSettings, ManagerError};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DnsRoutingRuleSet {
    pub id: String,
    pub name: String,
    pub mode: String,
    #[serde(default)]
    pub domains: Vec<DnsRoutingDomain>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DnsRoutingDomain {
    pub id: String,
    pub domain: String,
    #[serde(default)]
    pub include_subdomains: bool,
}

impl DnsSettings {
    pub(crate) fn routing_overlay(&self, snapshots: &mut Vec<SourceSnapshot>) -> CompileOverlay {
        let mut groups = Vec::new();
        let mut providers = Vec::new();
        for rule_set in self
            .rule_sets
            .iter()
            .filter(|rule_set| !rule_set.domains.is_empty())
        {
            let provider_tag = format!("sempre-dns-rule-set:{}", rule_set.id);
            let outbound = if rule_set.mode == "direct" {
                "DIRECT".into()
            } else {
                let group_name = proxy_group_name(rule_set);
                groups.push(ProxyGroup {
                    name: group_name.clone(),
                    include_all: true,
                    ..ProxyGroup::default()
                });
                group_name
            };
            snapshots.push(SourceSnapshot {
                source_id: rule_provider_snapshot_id(&provider_tag),
                content: compile_domains(rule_set, true),
                content_hash: String::new(),
            });
            providers.push(RuleProvider {
                tag: provider_tag,
                outbound,
                priority: true,
                ..RuleProvider::default()
            });
        }
        CompileOverlay {
            groups,
            rule_providers: providers,
        }
    }

    pub(crate) fn frontend_rule_sets(&self) -> Vec<sempre_gateway::DnsRuleSet> {
        self.rule_sets
            .iter()
            .filter(|rule_set| !rule_set.domains.is_empty())
            .map(|rule_set| sempre_gateway::DnsRuleSet {
                id: rule_set.id.clone(),
                name: rule_set.name.clone(),
                enabled: true,
                kind: "inline".into(),
                url: String::new(),
                rules: compile_domains(rule_set, false)
                    .lines()
                    .map(str::to_owned)
                    .collect(),
                upstream: if rule_set.mode == "direct" {
                    "local"
                } else {
                    "remote"
                }
                .into(),
            })
            .collect()
    }

    pub(crate) fn apply_compiled_sing_box_overlay(
        &self,
        content: &str,
    ) -> Result<String, ManagerError> {
        let active = self
            .rule_sets
            .iter()
            .filter(|rule_set| !rule_set.domains.is_empty())
            .collect::<Vec<_>>();
        if active.is_empty() {
            return Ok(content.into());
        }
        let mut document = serde_json::from_str::<Value>(content).map_err(|error| {
            ManagerError::InvalidOperation(format!(
                "decode remote sing-box configuration for DNS routing rules: {error}"
            ))
        })?;
        let proxy_names = compiled_proxy_names(&document)?;
        let outbounds = document["outbounds"]
            .as_array_mut()
            .expect("validated outbounds");
        let mut route_rules = Vec::new();
        let mut route_rule_sets = Vec::new();
        for rule_set in active {
            let provider_tag = format!("sempre-dns-rule-set:{}", rule_set.id);
            let outbound = if rule_set.mode == "direct" {
                "direct".into()
            } else {
                if proxy_names.is_empty() {
                    return Err(ManagerError::InvalidOperation(format!(
                        "DNS routing rule set {:?} has no proxy nodes to select",
                        rule_set.name
                    )));
                }
                let group_name = proxy_group_name(rule_set);
                outbounds.push(json!({
                    "type": "selector",
                    "tag": group_name,
                    "outbounds": proxy_names,
                    "default": proxy_names[0],
                    "interrupt_exist_connections": true,
                }));
                group_name
            };
            route_rule_sets.push(json!({
                "type": "inline",
                "tag": provider_tag,
                "rules": [inline_rule(rule_set)],
            }));
            route_rules.push(json!({ "rule_set": [provider_tag], "outbound": outbound }));
        }
        let route = document
            .get_mut("route")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| {
                ManagerError::InvalidOperation("remote sing-box configuration has no route".into())
            })?;
        let rule_sets = route
            .entry("rule_set")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                ManagerError::InvalidOperation(
                    "remote sing-box route.rule_set must be an array".into(),
                )
            })?;
        rule_sets.extend(route_rule_sets);
        let rules = route
            .entry("rules")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                ManagerError::InvalidOperation(
                    "remote sing-box route.rules must be an array".into(),
                )
            })?;
        let insert_at = rules
            .iter()
            .enumerate()
            .filter(|(_, rule)| {
                matches!(
                    rule.get("action").and_then(Value::as_str),
                    Some("sniff" | "hijack-dns")
                ) || rule.get("protocol").and_then(Value::as_str) == Some("dns")
            })
            .map(|(index, _)| index + 1)
            .max()
            .unwrap_or(0);
        rules.splice(insert_at..insert_at, route_rules);
        let mut content = serde_json::to_string_pretty(&document).map_err(|error| {
            ManagerError::InvalidOperation(format!(
                "encode remote sing-box configuration with DNS routing rules: {error}"
            ))
        })?;
        content.push('\n');
        Ok(content)
    }
}

fn proxy_group_name(rule_set: &DnsRoutingRuleSet) -> String {
    format!("DNS · {}", rule_set.name)
}

fn compiled_proxy_names(document: &Value) -> Result<Vec<String>, ManagerError> {
    let outbounds = document
        .get("outbounds")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ManagerError::InvalidOperation("remote sing-box configuration has no outbounds".into())
        })?;
    Ok(outbounds
        .iter()
        .filter_map(|outbound| {
            let kind = outbound.get("type")?.as_str()?;
            let tag = outbound.get("tag")?.as_str()?;
            (!matches!(kind, "direct" | "block" | "dns" | "selector" | "urltest"))
                .then(|| tag.to_owned())
        })
        .collect())
}

pub(crate) fn validate(settings: &DnsSettings) -> Result<(), ManagerError> {
    for upstream in &settings.direct_upstreams {
        if upstream.parse::<std::net::SocketAddr>().is_err() {
            return Err(ManagerError::InvalidOperation(format!(
                "direct DNS upstream {upstream:?} must be an IP address with a port"
            )));
        }
    }
    let mut rule_set_ids = HashSet::new();
    let mut rule_set_names = HashSet::new();
    for rule_set in &settings.rule_sets {
        if !valid_id(&rule_set.id) || rule_set.name.trim().is_empty() {
            return Err(ManagerError::InvalidOperation(
                "DNS routing rule set id and name are required".into(),
            ));
        }
        if !matches!(rule_set.mode.as_str(), "direct" | "proxy") {
            return Err(ManagerError::InvalidOperation(format!(
                "DNS routing rule set {:?} has an invalid mode",
                rule_set.name
            )));
        }
        if !rule_set_ids.insert(rule_set.id.clone())
            || !rule_set_names.insert(rule_set.name.trim().to_ascii_lowercase())
        {
            return Err(ManagerError::InvalidOperation(
                "DNS routing rule set ids and names must be unique".into(),
            ));
        }
        validate_domains(rule_set)?;
    }
    Ok(())
}

pub(crate) fn normalize(settings: &mut DnsSettings) {
    let mut upstreams = HashSet::new();
    settings.direct_upstreams = settings
        .direct_upstreams
        .drain(..)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty() && upstreams.insert(value.clone()))
        .collect();
    for rule_set in &mut settings.rule_sets {
        rule_set.name = rule_set.name.trim().to_owned();
        for entry in &mut rule_set.domains {
            entry.domain = entry
                .domain
                .trim()
                .trim_start_matches("*.")
                .trim_start_matches('.')
                .trim_end_matches('.')
                .to_ascii_lowercase();
        }
    }
}

fn validate_domains(rule_set: &DnsRoutingRuleSet) -> Result<(), ManagerError> {
    let mut domain_ids = HashSet::new();
    let mut domains = HashSet::new();
    for entry in &rule_set.domains {
        let domain = entry
            .domain
            .trim()
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if !valid_id(&entry.id) || !valid_domain(&domain) {
            return Err(ManagerError::InvalidOperation(format!(
                "DNS routing rule set {:?} contains an invalid domain",
                rule_set.name
            )));
        }
        if !domain_ids.insert(entry.id.clone())
            || !domains.insert((domain, entry.include_subdomains))
        {
            return Err(ManagerError::InvalidOperation(format!(
                "DNS routing rule set {:?} contains duplicate domains",
                rule_set.name
            )));
        }
    }
    Ok(())
}

fn compile_domains(rule_set: &DnsRoutingRuleSet, clash: bool) -> String {
    rule_set
        .domains
        .iter()
        .map(|entry| {
            let kind = match (clash, entry.include_subdomains) {
                (true, true) => "DOMAIN-SUFFIX",
                (true, false) => "DOMAIN",
                (false, true) => "domain-suffix",
                (false, false) => "domain",
            };
            format!("{kind},{}", entry.domain)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn inline_rule(rule_set: &DnsRoutingRuleSet) -> Value {
    let exact = rule_set
        .domains
        .iter()
        .filter(|entry| !entry.include_subdomains)
        .map(|entry| entry.domain.clone())
        .collect::<Vec<_>>();
    let suffix = rule_set
        .domains
        .iter()
        .filter(|entry| entry.include_subdomains)
        .map(|entry| entry.domain.clone())
        .collect::<Vec<_>>();
    let mut rule = serde_json::Map::new();
    if !exact.is_empty() {
        rule.insert("domain".into(), json!(exact));
    }
    if !suffix.is_empty() {
        rule.insert("domain_suffix".into(), json!(suffix));
    }
    Value::Object(rule)
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_domain(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value.is_ascii()
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

#[cfg(test)]
mod tests {
    use sempre_converter::{CompileRequest, Profile, Target, compile_with_overlay};
    use serde_json::Value;

    use super::*;

    #[test]
    fn routing_overlay_emits_direct_and_selectable_proxy_rules() {
        let settings = DnsSettings {
            schema: 3,
            revision: 1,
            enabled: true,
            direct_upstreams: vec!["223.5.5.5:53".into()],
            rule_sets: vec![
                rule_set("direct-sites", "Direct sites", "direct", "cn.example"),
                rule_set("proxy-sites", "Proxy sites", "proxy", "proxy.example"),
            ],
            reject_https: true,
            rewrites: Vec::new(),
            query_log_enabled: true,
            query_log_max_entries: 2_000,
        };
        let mut snapshots = Vec::new();
        let overlay = settings.routing_overlay(&mut snapshots);
        let result = compile_with_overlay(
            &CompileRequest {
                protocol: 1,
                profile: Profile::default(),
                snapshots,
                custom_nodes: Vec::new(),
                target: Target::parse("sing-box-v14-windows").expect("target"),
            },
            &overlay,
        )
        .expect("compile overlay");
        let document: Value = serde_json::from_str(&result.content).expect("sing-box JSON");
        let route_rules = document["route"]["rules"].as_array().expect("route rules");
        assert!(route_rules.iter().any(|rule| {
            rule["rule_set"][0] == "sempre-dns-rule-set:direct-sites"
                && rule["outbound"] == "direct"
        }));
        assert!(route_rules.iter().any(|rule| {
            rule["rule_set"][0] == "sempre-dns-rule-set:proxy-sites"
                && rule["outbound"] == "DNS · Proxy sites"
        }));
        assert!(document["outbounds"].as_array().is_some_and(|outbounds| {
            outbounds.iter().any(|outbound| {
                outbound["tag"] == "DNS · Proxy sites" && outbound["type"] == "selector"
            })
        }));
    }

    #[test]
    fn normalization_canonicalizes_domains_and_upstreams() {
        let mut settings = DnsSettings {
            schema: 3,
            revision: 1,
            enabled: true,
            direct_upstreams: vec![" 223.5.5.5:53 ".into(), "223.5.5.5:53".into()],
            rule_sets: vec![rule_set(
                "direct-sites",
                " Direct sites ",
                "direct",
                "*.EXAMPLE.COM.",
            )],
            reject_https: true,
            rewrites: Vec::new(),
            query_log_enabled: true,
            query_log_max_entries: 2_000,
        };
        normalize(&mut settings);
        validate(&settings).expect("normalized settings");
        assert_eq!(settings.direct_upstreams, ["223.5.5.5:53"]);
        assert_eq!(settings.rule_sets[0].name, "Direct sites");
        assert_eq!(settings.rule_sets[0].domains[0].domain, "example.com");
    }

    #[test]
    fn compiled_remote_overlay_preserves_sniffing_and_routes_direct_explicitly() {
        let settings = DnsSettings {
            schema: 3,
            revision: 1,
            enabled: true,
            direct_upstreams: Vec::new(),
            rule_sets: vec![
                rule_set("direct-sites", "Direct sites", "direct", "cn.example"),
                rule_set("proxy-sites", "Proxy sites", "proxy", "proxy.example"),
            ],
            reject_https: true,
            rewrites: Vec::new(),
            query_log_enabled: true,
            query_log_max_entries: 2_000,
        };
        let content = settings
            .apply_compiled_sing_box_overlay(
                r#"{"outbounds":[{"type":"direct","tag":"direct"},{"type":"shadowsocks","tag":"node-a"}],"route":{"rules":[{"action":"sniff"},{"ip_is_private":true,"outbound":"direct"}],"rule_set":[]}}"#,
            )
            .expect("remote overlay");
        let document: Value = serde_json::from_str(&content).expect("sing-box JSON");
        assert_eq!(document["route"]["rules"][0]["action"], "sniff");
        assert_eq!(document["route"]["rules"][1]["outbound"], "direct");
        assert_eq!(
            document["route"]["rules"][2]["outbound"],
            "DNS · Proxy sites"
        );
        assert!(document["outbounds"].as_array().is_some_and(|outbounds| {
            outbounds.iter().any(|outbound| {
                outbound["tag"] == "DNS · Proxy sites" && outbound["outbounds"] == json!(["node-a"])
            })
        }));
    }

    fn rule_set(id: &str, name: &str, mode: &str, domain: &str) -> DnsRoutingRuleSet {
        DnsRoutingRuleSet {
            id: id.into(),
            name: name.into(),
            mode: mode.into(),
            domains: vec![DnsRoutingDomain {
                id: format!("{id}-domain"),
                domain: domain.into(),
                include_subdomains: true,
            }],
        }
    }
}
