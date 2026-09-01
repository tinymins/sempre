use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    CompileError, Profile, SourceSnapshot, Target, convert_clash_rule_set, prepare_profile,
    rule_provider_snapshot_id,
};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct DnsFrontendPolicy {
    pub enabled: bool,
    pub fakeip_enabled: bool,
    pub fakeip_ipv4_range: String,
    pub fakeip_ipv6_range: String,
    pub core_listen_port: u16,
    pub reject_https: bool,
    pub proxy_rules: Vec<String>,
    pub direct_rules: Vec<String>,
    pub complete: bool,
    pub warnings: Vec<String>,
}

pub fn dns_frontend_policy(
    profile: &Profile,
    target: &Target,
    snapshots: &[SourceSnapshot],
) -> Result<DnsFrontendPolicy, CompileError> {
    let profile = prepare_profile(profile, target)?;
    let snapshots = snapshots
        .iter()
        .map(|snapshot| (snapshot.source_id.as_str(), snapshot.content.as_str()))
        .collect::<HashMap<_, _>>();
    let shared = profile.dns.get("shared").unwrap_or(&profile.dns);
    let enabled = target.core == "sing-box"
        && matches!(target.platform.as_str(), "macos" | "windows")
        && shared
            .get("systemDnsTakeoverEnabled")
            .and_then(Value::as_bool)
            == Some(true);
    let mut policy = DnsFrontendPolicy {
        enabled,
        fakeip_enabled: boolean(shared, "fakeipEnabled", true),
        fakeip_ipv4_range: string(shared, "fakeipIpv4Range", "198.18.0.0/15"),
        fakeip_ipv6_range: string(shared, "fakeipIpv6Range", "fc00::/18"),
        core_listen_port: match profile.transparent_proxy.tproxy.dns_listen_port {
            0 => 1053,
            port => port,
        },
        reject_https: boolean(shared, "rejectHttps", true),
        complete: true,
        ..DnsFrontendPolicy::default()
    };
    if !enabled {
        return Ok(policy);
    }
    for rule in &profile.rules {
        add_profile_rule(rule, &mut policy);
    }
    for provider in &profile.rule_providers {
        let id = rule_provider_snapshot_id(&provider.tag);
        let Some(content) = snapshots.get(id.as_str()) else {
            policy.complete = false;
            policy.warnings.push(format!(
                "DNS frontend rule provider {:?} is unavailable",
                provider.tag
            ));
            continue;
        };
        let rules = provider_domain_rules(content);
        if direct_outbound(&provider.outbound) {
            policy.direct_rules.extend(rules);
        } else {
            policy.proxy_rules.extend(rules);
        }
    }
    let mut private_rules = Vec::new();
    collect_domain_fields(&profile.private_access, &mut private_rules);
    policy.proxy_rules.extend(private_rules);
    normalize(&mut policy.proxy_rules);
    normalize(&mut policy.direct_rules);
    Ok(policy)
}

fn boolean(value: &Value, key: &str, fallback: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}

fn string(value: &Value, key: &str, fallback: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
        .into()
}

fn add_profile_rule(rule: &Value, policy: &mut DnsFrontendPolicy) {
    let Some(rule) = rule.as_str() else {
        add_native_rule(rule, policy);
        return;
    };
    let fields = rule.split(',').map(str::trim).collect::<Vec<_>>();
    if fields.len() < 3 {
        return;
    }
    let kind = match fields[0].to_ascii_uppercase().as_str() {
        "DOMAIN" | "HOST" => "domain",
        "DOMAIN-SUFFIX" | "HOST-SUFFIX" => "domain-suffix",
        "DOMAIN-KEYWORD" | "HOST-KEYWORD" => "domain-keyword",
        "DOMAIN-REGEX" => "domain-regex",
        _ => return,
    };
    let rule = format!("{kind},{}", fields[1]);
    if direct_outbound(fields[2]) {
        policy.direct_rules.push(rule);
    } else {
        policy.proxy_rules.push(rule);
    }
}

fn add_native_rule(rule: &Value, policy: &mut DnsFrontendPolicy) {
    let mut domains = Vec::new();
    collect_domain_fields(rule, &mut domains);
    if domains.is_empty() {
        return;
    }
    let direct = rule
        .get("outbound")
        .and_then(Value::as_str)
        .is_some_and(direct_outbound);
    let unsafe_direct = direct && has_non_domain_matcher(rule);
    if rule.get("invert").and_then(Value::as_bool) == Some(true) {
        policy.complete = false;
        policy
            .warnings
            .push("inverted domain rule is not representable by the DNS frontend".into());
    }
    if direct && !unsafe_direct {
        policy.direct_rules.extend(domains);
    } else {
        policy.proxy_rules.extend(domains);
    }
}

fn collect_domain_fields(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, kind) in [
                ("domain", "domain"),
                ("domains", "domain"),
                ("domain_suffix", "domain-suffix"),
                ("domainSuffixes", "domain-suffix"),
                ("domain_keyword", "domain-keyword"),
                ("domainKeywords", "domain-keyword"),
                ("domain_regex", "domain-regex"),
                ("domainRegexes", "domain-regex"),
            ] {
                if let Some(value) = object.get(key) {
                    append_values(value, kind, output);
                }
            }
            for (key, value) in object {
                if matches!(
                    key.as_str(),
                    "domain"
                        | "domains"
                        | "domain_suffix"
                        | "domainSuffixes"
                        | "domain_keyword"
                        | "domainKeywords"
                        | "domain_regex"
                        | "domainRegexes"
                ) {
                    continue;
                }
                collect_domain_fields(value, output);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_domain_fields(value, output);
            }
        }
        _ => {}
    }
}

fn append_values(value: &Value, kind: &str, output: &mut Vec<String>) {
    match value {
        Value::String(value) if !value.trim().is_empty() => {
            output.push(format!("{kind},{}", value.trim()));
        }
        Value::Array(values) => {
            for value in values {
                append_values(value, kind, output);
            }
        }
        _ => {}
    }
}

fn has_non_domain_matcher(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "domain" | "domain_suffix" | "domain_keyword" | "domain_regex" | "action" | "outbound"
        )
    })
}

fn provider_domain_rules(content: &str) -> Vec<String> {
    let converted = convert_clash_rule_set(content, 4);
    let Some(rules) = converted.get("rules").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut output = Vec::new();
    for rule in rules {
        collect_domain_fields(rule, &mut output);
    }
    output
}

fn direct_outbound(value: &str) -> bool {
    matches!(value.trim(), "DIRECT" | "direct" | "🚀 直接连接")
}

fn normalize(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(value.to_ascii_lowercase()));
    values.sort_unstable_by_key(|value| value.to_ascii_lowercase());
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn managed_frontend_policy_is_enabled_on_windows_and_macos() {
        let profile: Profile = serde_json::from_value(json!({
            "dns": { "shared": { "systemDnsTakeoverEnabled": true } }
        }))
        .expect("profile");
        for target in ["sing-box-v14-windows", "sing-box-v13-macos"] {
            let policy =
                dns_frontend_policy(&profile, &Target::parse(target).expect("target"), &[])
                    .expect("DNS frontend policy");
            assert!(policy.enabled, "{target}");
            assert!(policy.complete, "{target}");
        }
    }

    #[test]
    fn extracts_safe_precedence_and_requires_every_provider_snapshot() {
        let profile: Profile = serde_json::from_value(json!({
            "dns": { "shared": { "systemDnsTakeoverEnabled": true, "fakeipEnabled": true } },
            "rules": [
                "DOMAIN,proxy.baidu.com,proxy",
                "DOMAIN-SUFFIX,direct.example,DIRECT",
                { "domain": "conditional.example", "process_name": ["git"], "outbound": "direct" }
            ],
            "rule_providers": [
                { "tag": "direct", "url": "https://rules.example/direct", "outbound": "DIRECT" },
                { "tag": "proxy", "url": "https://rules.example/proxy", "outbound": "proxy" },
                { "tag": "missing", "url": "https://rules.example/missing", "outbound": "proxy" }
            ],
            "private_access": { "connectors": [{ "routes": { "domains": ["private.example"] } }] }
        }))
        .expect("profile");
        let snapshots = vec![
            SourceSnapshot {
                source_id: rule_provider_snapshot_id("direct"),
                content: "payload:\n  - DOMAIN-SUFFIX,provider-direct.example\n".into(),
                content_hash: String::new(),
            },
            SourceSnapshot {
                source_id: rule_provider_snapshot_id("proxy"),
                content: "payload:\n  - DOMAIN-REGEX,^provider-[0-9]+\\.example$\n".into(),
                content_hash: String::new(),
            },
        ];
        let policy = dns_frontend_policy(
            &profile,
            &Target::parse("sing-box-v13-macos").expect("target"),
            &snapshots,
        )
        .expect("policy");
        assert!(!policy.complete);
        assert!(policy.enabled);
        assert!(policy.fakeip_enabled);
        assert_eq!(policy.core_listen_port, 1053);
        assert!(
            policy
                .proxy_rules
                .contains(&"domain,proxy.baidu.com".into())
        );
        assert!(
            policy
                .proxy_rules
                .contains(&"domain,conditional.example".into())
        );
        assert!(
            policy
                .proxy_rules
                .contains(&"domain,private.example".into())
        );
        assert!(
            policy
                .proxy_rules
                .contains(&"domain-regex,^provider-[0-9]+\\.example$".into())
        );
        assert!(
            policy
                .direct_rules
                .contains(&"domain-suffix,direct.example".into())
        );
        assert!(
            policy
                .direct_rules
                .contains(&"domain-suffix,provider-direct.example".into())
        );
        assert!(policy.warnings[0].contains("missing"));
    }
}
