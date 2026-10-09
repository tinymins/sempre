use std::borrow::Cow;

use crate::{DnsRoutingDomain, DnsRoutingRuleSet, DnsSettings, ManagerError};

pub(super) const BUILTIN_ID: &str = "builtin-domains-min";
pub(super) const BUILTIN_NAME: &str = "Mainland China domains";

impl DnsSettings {
    pub(super) fn routing_rule_sets(
        &self,
    ) -> Result<Vec<Cow<'_, DnsRoutingRuleSet>>, ManagerError> {
        let mut rules = self.rule_sets.iter().map(Cow::Borrowed).collect::<Vec<_>>();
        if self.domestic_domains.enabled {
            rules.push(Cow::Owned(DnsRoutingRuleSet {
                id: BUILTIN_ID.into(),
                name: BUILTIN_NAME.into(),
                mode: match self.domestic_domains.mode {
                    sempre_dns::DomesticDomainMode::Direct => "direct",
                    sempre_dns::DomesticDomainMode::Proxy => "proxy",
                }
                .into(),
                domains: sempre_dns::bundled_domestic_domains()?
                    .into_iter()
                    .map(|domain| DnsRoutingDomain {
                        id: String::new(),
                        domain,
                        include_subdomains: true,
                    })
                    .collect(),
            }));
        }
        Ok(rules)
    }
}
