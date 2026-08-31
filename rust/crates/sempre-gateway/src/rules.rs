use std::time::Duration;

use crate::{GatewayError, domain_matcher::parse_adguard_domains, model::DnsConfig};

const MAX_RULE_SET_SIZE: usize = 4 << 20;

pub(crate) async fn resolve_rule_sets(mut config: DnsConfig) -> Result<DnsConfig, GatewayError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| GatewayError::invalid(format!("build DNS rule client: {error}")))?;
    for rule_set in &mut config.rule_sets {
        if !rule_set.enabled || rule_set.kind != "url" || rule_set.url.trim().is_empty() {
            continue;
        }
        let url = reqwest::Url::parse(&rule_set.url)
            .ok()
            .filter(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
            .ok_or_else(|| {
                GatewayError::invalid(format!("invalid DNS rule set URL {:?}", rule_set.url))
            })?;
        let response = client.get(url).send().await.map_err(|error| {
            GatewayError::invalid(format!("fetch DNS rule set {:?}: {error}", rule_set.name))
        })?;
        if !response.status().is_success() {
            return Err(GatewayError::invalid(format!(
                "fetch DNS rule set {:?}: HTTP {}",
                rule_set.name,
                response.status()
            )));
        }
        let data = response.bytes().await.map_err(|error| {
            GatewayError::invalid(format!("read DNS rule set {:?}: {error}", rule_set.name))
        })?;
        if data.len() > MAX_RULE_SET_SIZE {
            return Err(GatewayError::invalid(format!(
                "DNS rule set {:?} exceeds 4 MiB",
                rule_set.name
            )));
        }
        rule_set.rules = parse_rule_lines(&String::from_utf8_lossy(&data))?;
    }
    Ok(config)
}

fn parse_rule_lines(data: &str) -> Result<Vec<String>, GatewayError> {
    if data.lines().any(|line| line.trim().starts_with("[/")) {
        return parse_adguard_domains(data);
    }
    Ok(data
        .lines()
        .map(str::trim)
        .map(|line| line.strip_prefix("- ").unwrap_or(line))
        .map(|line| line.trim_matches(['\"', '\'']))
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with('#')
                && !line.ends_with(':')
                && !line.to_ascii_lowercase().starts_with("payload:")
        })
        .map(str::to_owned)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_yaml_payload_rules() {
        assert_eq!(
            parse_rule_lines("payload:\n  - 'domain,example.com'\n# note\nfoo.test\n")
                .expect("rules"),
            ["domain,example.com", "foo.test"]
        );
    }

    #[test]
    fn parses_adguard_upstream_domain_shape() {
        assert_eq!(
            parse_rule_lines("127.0.0.1:1053\n[/foo.cn/bar.cn/]127.0.0.1\n").expect("rules"),
            ["bar.cn", "foo.cn"]
        );
    }
}
