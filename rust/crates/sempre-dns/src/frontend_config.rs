use crate::{
    DnsError,
    domain_matcher::bundled_domestic_domains,
    model::{DnsConfig, DnsRuleSet, validate},
};

impl DnsConfig {
    pub fn managed_frontend(
        listen_port: u16,
        local_upstreams: Vec<String>,
        remote_upstream: String,
        mut rule_sets: Vec<DnsRuleSet>,
    ) -> Result<Self, DnsError> {
        if local_upstreams.is_empty() {
            return Err(DnsError::invalid(
                "managed DNS frontend requires at least one original DNS upstream",
            ));
        }
        push_inline_rules(
            &mut rule_sets,
            "domestic-domains",
            bundled_domestic_domains()?,
            "local",
        );
        let config = Self {
            enabled: true,
            listen_hosts: vec!["127.0.0.1".into()],
            listen_port,
            local_upstreams,
            remote_upstream,
            strategy: "rules-first".into(),
            reject_https: false,
            rule_sets,
            domestic_cidrs: Vec::new(),
            cache_ttl_seconds: 300,
            outbound_mark: None,
        };
        let mut errors = Vec::new();
        validate(&config, &mut errors);
        if errors.is_empty() {
            Ok(config)
        } else {
            Err(DnsError::invalid(errors.join("; ")))
        }
    }
}

fn push_inline_rules(output: &mut Vec<DnsRuleSet>, id: &str, rules: Vec<String>, upstream: &str) {
    if rules.is_empty() {
        return;
    }
    output.push(DnsRuleSet {
        id: id.into(),
        name: id.into(),
        enabled: true,
        kind: "inline".into(),
        url: String::new(),
        rules,
        upstream: upstream.into(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_fixed_precedence_and_no_classification_fallback() {
        let config = DnsConfig::managed_frontend(
            1054,
            vec!["192.0.2.53:53".into()],
            "127.0.0.1:1053".into(),
            Vec::new(),
        )
        .expect("managed frontend");
        assert_eq!(config.listen_port, 1054);
        assert_eq!(config.strategy, "rules-first");
        assert!(config.domestic_cidrs.is_empty());
        assert_eq!(
            config
                .rule_sets
                .iter()
                .map(|rules| (rules.id.as_str(), rules.upstream.as_str()))
                .collect::<Vec<_>>(),
            [("domestic-domains", "local")]
        );
        assert!(config.rule_sets[0].rules.len() > 77_000);
    }

    #[test]
    fn requires_usable_upstreams() {
        assert!(
            DnsConfig::managed_frontend(1054, Vec::new(), "127.0.0.1:1053".into(), Vec::new(),)
                .is_err()
        );
        assert!(
            DnsConfig::managed_frontend(
                1054,
                vec!["223.5.5.5:not-a-port".into()],
                "127.0.0.1:1053".into(),
                Vec::new(),
            )
            .is_err()
        );
    }
}
