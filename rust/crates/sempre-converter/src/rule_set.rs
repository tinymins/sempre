use serde_json::{Map, Value, json};

pub fn rule_provider_snapshot_id(tag: &str) -> String {
    format!("rule-provider:{tag}")
}

pub fn convert_clash_rule_set(text: &str, version: u8) -> Value {
    let entries = entries(text);
    let mut domain = Vec::new();
    let mut domain_suffix = Vec::new();
    let mut domain_keyword = Vec::new();
    let mut domain_regex = Vec::new();
    let mut ip_cidr = Vec::new();
    let mut source_ip_cidr = Vec::new();
    let mut port = Vec::new();
    let mut source_port = Vec::new();

    for line in entries {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts = line.splitn(3, ',').collect::<Vec<_>>();
        if parts.len() == 1 {
            domain.push(parts[0].to_owned());
            continue;
        }
        let kind = parts[0].trim();
        let value = parts[1].trim();
        match kind {
            "DOMAIN" | "+" | "HOST" => domain.push(value.to_owned()),
            "DOMAIN-SUFFIX" | "HOST-SUFFIX" => domain_suffix.push(value.to_owned()),
            "DOMAIN-KEYWORD" | "HOST-KEYWORD" => domain_keyword.push(value.to_owned()),
            "DOMAIN-REGEX" => domain_regex.push(value.to_owned()),
            "IP-CIDR" | "IP-CIDR6" => ip_cidr.push(value.to_owned()),
            "SRC-IP-CIDR" => source_ip_cidr.push(value.to_owned()),
            "DST-PORT" => parse_port(value, &mut port),
            "SRC-PORT" => parse_port(value, &mut source_port),
            _ => {}
        }
    }

    let mut rule = Map::new();
    insert_nonempty(&mut rule, "domain", &domain);
    insert_nonempty(&mut rule, "domain_suffix", &domain_suffix);
    insert_nonempty(&mut rule, "domain_keyword", &domain_keyword);
    insert_nonempty(&mut rule, "domain_regex", &domain_regex);
    insert_nonempty(&mut rule, "ip_cidr", &ip_cidr);
    insert_nonempty(&mut rule, "source_ip_cidr", &source_ip_cidr);
    insert_nonempty(&mut rule, "port", &port);
    insert_nonempty(&mut rule, "source_port", &source_port);
    json!({ "version": version, "rules": [rule] })
}

pub(crate) fn inline_rules(text: &str) -> Option<Vec<Value>> {
    let converted = convert_clash_rule_set(text, 4);
    let rules = converted.get("rules")?.as_array()?.clone();
    rules
        .iter()
        .any(|rule| rule.as_object().is_some_and(|rule| !rule.is_empty()))
        .then_some(rules)
}

pub fn rule_provider_has_rules(text: &str) -> bool {
    inline_rules(text).is_some()
}

fn entries(text: &str) -> Vec<String> {
    let entries = match serde_yaml::from_str::<serde_yaml::Value>(text) {
        Ok(serde_yaml::Value::Sequence(values)) => strings(values),
        Ok(value) => value
            .get("payload")
            .and_then(serde_yaml::Value::as_sequence)
            .cloned()
            .map(strings)
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    if entries.is_empty() {
        text.lines().map(str::to_owned).collect()
    } else {
        entries
    }
}

fn strings(values: Vec<serde_yaml::Value>) -> Vec<String> {
    values
        .into_iter()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect()
}

fn parse_port(value: &str, output: &mut Vec<u16>) {
    if let Ok(value) = value.parse() {
        output.push(value);
    }
}

fn insert_nonempty<T: serde::Serialize>(rule: &mut Map<String, Value>, key: &str, values: &[T]) {
    if !values.is_empty() {
        rule.insert(key.into(), json!(values));
    }
}

#[cfg(test)]
mod tests {
    use super::convert_clash_rule_set;
    use serde_json::json;

    #[test]
    fn converts_ohmywrt_rule_types_and_skips_process_rules() {
        let output = convert_clash_rule_set(
            "payload:\n  - DOMAIN,exact.example\n  - DOMAIN-SUFFIX,suffix.example\n  - IP-CIDR,192.0.2.0/24,no-resolve\n  - SRC-IP-CIDR,198.51.100.0/24\n  - DST-PORT,443\n  - SRC-PORT,65536\n  - PROCESS-NAME,unsafe\n  - plain.example",
            4,
        );
        assert_eq!(output["version"], 4);
        assert_eq!(
            output["rules"][0]["domain"],
            json!(["exact.example", "plain.example"])
        );
        assert_eq!(
            output["rules"][0]["domain_suffix"],
            json!(["suffix.example"])
        );
        assert_eq!(output["rules"][0]["ip_cidr"], json!(["192.0.2.0/24"]));
        assert_eq!(
            output["rules"][0]["source_ip_cidr"],
            json!(["198.51.100.0/24"])
        );
        assert_eq!(output["rules"][0]["port"], json!([443]));
        assert!(output["rules"][0].get("source_port").is_none());
        assert!(!output.to_string().contains("unsafe"));
    }

    #[test]
    fn converts_plain_line_provider_documents() {
        let output =
            convert_clash_rule_set("DOMAIN,exact.example\nDOMAIN-SUFFIX,suffix.example", 4);
        assert_eq!(output["rules"][0]["domain"], json!(["exact.example"]));
        assert_eq!(
            output["rules"][0]["domain_suffix"],
            json!(["suffix.example"])
        );
    }
}
