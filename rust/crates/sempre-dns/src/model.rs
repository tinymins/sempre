use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct DnsConfig {
    pub enabled: bool,
    pub listen_hosts: Vec<String>,
    pub listen_port: u16,
    pub local_upstreams: Vec<String>,
    pub remote_upstream: String,
    pub strategy: String,
    pub reject_https: bool,
    pub rule_sets: Vec<DnsRuleSet>,
    pub domestic_cidrs: Vec<String>,
    pub cache_ttl_seconds: u64,
    pub outbound_mark: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct DnsRuleSet {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub rules: Vec<String>,
    pub upstream: String,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_hosts: vec!["127.0.0.1".into()],
            listen_port: 1054,
            local_upstreams: Vec::new(),
            remote_upstream: "127.0.0.1:1053".into(),
            strategy: "rules-first".into(),
            reject_https: false,
            rule_sets: Vec::new(),
            domestic_cidrs: Vec::new(),
            cache_ttl_seconds: 300,
            outbound_mark: None,
        }
    }
}

pub(crate) fn validate(config: &DnsConfig, errors: &mut Vec<String>) {
    if config.listen_port == 0 {
        errors.push("DNS listen port must be between 1 and 65535".into());
    }
    for host in &config.listen_hosts {
        if host.parse::<Ipv4Addr>().is_err() {
            errors.push(format!("DNS listen host {host:?} must be an IPv4 address"));
        }
    }
    for upstream in config
        .local_upstreams
        .iter()
        .chain([&config.remote_upstream])
    {
        if !valid_upstream(upstream) {
            errors.push(format!("DNS upstream {upstream:?} must be host:port"));
        }
    }
    if !matches!(
        config.strategy.as_str(),
        "local-first-classify" | "rules-first"
    ) {
        errors.push(format!("invalid DNS strategy {:?}", config.strategy));
    }
    for cidr in &config.domestic_cidrs {
        if !valid_ipv4_prefix(cidr) {
            errors.push(format!("domestic CIDR {cidr:?} must be an IPv4 prefix"));
        }
    }
    for rule_set in &config.rule_sets {
        if rule_set.id.trim().is_empty() || rule_set.name.trim().is_empty() {
            errors.push("DNS rule sets require ID and name".into());
        }
        if !rule_set.upstream.is_empty()
            && !matches!(rule_set.upstream.as_str(), "local" | "remote")
            && !valid_upstream(&rule_set.upstream)
        {
            errors.push(format!(
                "DNS upstream {:?} must be host:port",
                rule_set.upstream
            ));
        }
        if !rule_set.kind.is_empty() && !matches!(rule_set.kind.as_str(), "inline" | "url") {
            errors.push(format!("unsupported DNS rule set type {:?}", rule_set.kind));
        }
    }
}

fn valid_ipv4_prefix(value: &str) -> bool {
    value.split_once('/').is_some_and(|(address, prefix)| {
        address.parse::<Ipv4Addr>().is_ok() && prefix.parse::<u8>().is_ok_and(|prefix| prefix <= 32)
    })
}

fn valid_upstream(value: &str) -> bool {
    let value = value.trim();
    if let Some((host, port)) = value
        .strip_prefix('[')
        .and_then(|value| value.split_once("]:"))
    {
        return !host.is_empty() && valid_port(port);
    }
    value
        .rsplit_once(':')
        .is_some_and(|(host, port)| !host.is_empty() && !host.contains(':') && valid_port(port))
}

fn valid_port(value: &str) -> bool {
    value.parse::<u16>().is_ok_and(|port| port != 0)
}
