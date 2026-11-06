use std::collections::BTreeSet;

use sempre_dns::DnsRuleSet;
use url::{Host, Url};

use crate::DnsSettings;

const RULE_SET_ID: &str = "tunnel-servers";

/// Tunnel servers carry the core's own endpoints, so the DNS frontend must
/// answer them from the direct upstreams instead of the core's fake-IP pool.
pub(crate) fn tunnel_server_rule_set(tunnels: &sempre_tunnel::Config) -> Option<DnsRuleSet> {
    let domains = tunnels
        .instances
        .iter()
        .filter_map(
            |instance| match Url::parse(&instance.server_url).ok()?.host()? {
                Host::Domain(domain) => Some(domain.trim_end_matches('.').to_ascii_lowercase()),
                Host::Ipv4(_) | Host::Ipv6(_) => None,
            },
        )
        .filter(|domain| !domain.is_empty())
        .collect::<BTreeSet<_>>();
    if domains.is_empty() {
        return None;
    }
    Some(DnsRuleSet {
        id: RULE_SET_ID.into(),
        name: RULE_SET_ID.into(),
        enabled: true,
        kind: "inline".into(),
        url: String::new(),
        rules: domains
            .into_iter()
            .map(|domain| format!("domain,{domain}"))
            .collect(),
        upstream: "local".into(),
    })
}

pub(crate) fn frontend_rule_sets(
    settings: &DnsSettings,
    tunnels: &sempre_tunnel::Config,
) -> Vec<DnsRuleSet> {
    tunnel_server_rule_set(tunnels)
        .into_iter()
        .chain(settings.frontend_rule_sets())
        .collect()
}

#[cfg(test)]
#[path = "dns_tunnel_rules_tests.rs"]
mod tests;
