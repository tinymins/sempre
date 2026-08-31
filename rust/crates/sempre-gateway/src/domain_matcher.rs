use std::collections::HashSet;

use crate::GatewayError;

const BUNDLED_DOMAINS_MIN: &str = include_str!("../resources/domains-min.txt");

pub const DOMESTIC_DOMAIN_SOURCE: &str =
    "https://github.com/ohmywrt/ohmywrt/blob/master/package/base-files/files/etc/domains-min.txt";
pub const DOMESTIC_DOMAIN_SHA256: &str =
    "80aed7f0cbe1d0292f58284f5b0b91043e09950a9019c60da96bff3a6e8ba634";

pub fn bundled_domestic_domains() -> Result<Vec<String>, GatewayError> {
    parse_adguard_domains(BUNDLED_DOMAINS_MIN)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DomainMatcher {
    exact: HashSet<String>,
    suffixes: HashSet<String>,
    keywords: Vec<String>,
}

impl DomainMatcher {
    pub(crate) fn from_rules(rules: &[String]) -> Self {
        let mut matcher = Self::default();
        for rule in rules {
            matcher.insert_rule(rule);
        }
        matcher
    }

    pub(crate) fn matches(&self, name: &str) -> bool {
        let name = normalize_query(name);
        if self.exact.contains(&name) || self.keywords.iter().any(|value| name.contains(value)) {
            return true;
        }
        suffixes(&name).any(|suffix| self.suffixes.contains(suffix))
    }

    fn insert_rule(&mut self, rule: &str) {
        let rule = rule.trim();
        if rule.is_empty() || rule.starts_with('#') {
            return;
        }
        let (kind, value) = rule
            .split_once(',')
            .map_or(("domain-suffix", rule), |(kind, value)| {
                (kind.trim(), value.trim())
            });
        let Some(value) = normalize_domain(value) else {
            return;
        };
        match kind.to_ascii_lowercase().as_str() {
            "domain" => {
                self.exact.insert(value);
            }
            "domain-keyword" => {
                self.keywords.push(value);
            }
            _ => {
                self.suffixes.insert(value);
            }
        }
    }
}

pub(crate) fn parse_adguard_domains(data: &str) -> Result<Vec<String>, GatewayError> {
    let mut domains = HashSet::new();
    let mut found = false;
    for line in data.lines().map(str::trim) {
        if !line.starts_with("[/") {
            continue;
        }
        found = true;
        let (patterns, upstream) = line.split_once(']').ok_or_else(|| {
            GatewayError::invalid("AdGuard domain rule is missing closing bracket")
        })?;
        if upstream.trim().is_empty() {
            return Err(GatewayError::invalid("AdGuard domain rule has no upstream"));
        }
        let patterns = patterns
            .strip_prefix("[/")
            .and_then(|value| value.strip_suffix('/'))
            .ok_or_else(|| GatewayError::invalid("invalid AdGuard domain rule"))?;
        for value in patterns.split('/') {
            let domain = normalize_domain(value).ok_or_else(|| {
                GatewayError::invalid(format!("invalid AdGuard domain {value:?}"))
            })?;
            domains.insert(domain);
        }
    }
    if !found {
        return Err(GatewayError::invalid(
            "AdGuard domain list contains no upstream rules",
        ));
    }
    if domains.is_empty() {
        return Err(GatewayError::invalid("AdGuard domain list is empty"));
    }
    let mut domains = domains.into_iter().collect::<Vec<_>>();
    domains.sort_unstable();
    Ok(domains)
}

fn normalize_query(value: &str) -> String {
    value.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn normalize_domain(value: &str) -> Option<String> {
    let value = value
        .trim()
        .trim_start_matches("*.")
        .trim_start_matches('.')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if value.is_empty() || value.len() > 253 || !value.is_ascii() {
        return None;
    }
    if value.split('.').any(|label| {
        label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    }) {
        return None;
    }
    Some(value)
}

fn suffixes(mut value: &str) -> impl Iterator<Item = &str> {
    std::iter::from_fn(move || {
        if value.is_empty() {
            return None;
        }
        let current = value;
        value = value.split_once('.').map_or("", |(_, suffix)| suffix);
        Some(current)
    })
}

#[cfg(test)]
mod tests {
    use sha2::{Digest as _, Sha256};

    use super::*;

    #[test]
    fn matches_exact_suffix_and_keyword_without_label_false_positives() {
        let matcher = DomainMatcher::from_rules(&[
            "domain,exact.example".into(),
            "domain-suffix,example.com".into(),
            "domain-keyword,keyword".into(),
        ]);
        assert!(matcher.matches("exact.example."));
        assert!(matcher.matches("WWW.EXAMPLE.COM."));
        assert!(!matcher.matches("www.exact.example."));
        assert!(matcher.matches("example.com."));
        assert!(matcher.matches("www.example.com."));
        assert!(!matcher.matches("notexample.com."));
        assert!(matcher.matches("has-keyword.test."));
    }

    #[test]
    fn parses_large_adguard_shape_and_ignores_default_upstream() {
        let domains =
            parse_adguard_domains("127.0.0.1:1053\n[/Baidu.com/qq.com/example.cn/]127.0.0.1\n")
                .expect("AdGuard domains");
        assert_eq!(domains, ["baidu.com", "example.cn", "qq.com"]);
    }

    #[test]
    fn rejects_malformed_adguard_rules() {
        assert!(parse_adguard_domains("127.0.0.1:1053\n[/example.com/").is_err());
        assert!(parse_adguard_domains("[/example.com/]").is_err());
        assert!(parse_adguard_domains("[/bad_domain.test/]127.0.0.1").is_err());
    }

    #[test]
    fn bundled_snapshot_has_expected_identity_and_domain_set() {
        assert_eq!(
            format!("{:x}", Sha256::digest(BUNDLED_DOMAINS_MIN.as_bytes())),
            DOMESTIC_DOMAIN_SHA256
        );
        let domains = bundled_domestic_domains().expect("bundled domains");
        assert_eq!(domains.len(), 77_072);
        assert!(domains.binary_search(&"baidu.com".into()).is_ok());
        assert!(domains.binary_search(&"qq.com".into()).is_ok());
        assert!(domains.binary_search(&"github.com".into()).is_err());
        assert!(domains.binary_search(&"google.com".into()).is_err());
        assert!(domains.binary_search(&"openai.com".into()).is_err());
    }
}
