use std::{
    collections::{HashMap, HashSet},
    fs,
    net::SocketAddr,
    path::Path,
    time::Duration,
};

use sempre_subscription::{Fetcher, RuleSetSnapshot};
use serde_json::{Value, json};

use crate::ManagerError;

#[derive(Clone)]
struct RemoteRule {
    tag: String,
    url: String,
    format: String,
    interval: Duration,
}

#[derive(Default)]
pub(crate) struct RuleBootstrap {
    original: Value,
    pending: usize,
    resources: Vec<RemoteRule>,
}

impl RuleBootstrap {
    pub(crate) fn pending_count(&self) -> usize {
        self.pending
    }

    pub(crate) fn prepare(fetcher: &Fetcher, path: &Path) -> Result<Self, ManagerError> {
        let original = read_config(path)?;
        let (prepared, pending) = materialize(fetcher, &original, &HashMap::new())?;
        if prepared != original {
            let data = serde_json::to_vec(&prepared).map_err(config_error)?;
            sempre_state::write_atomic(path, &data, 0o600)
                .map_err(|error| ManagerError::io("prepare local rule sets", error))?;
        }
        let resources = remote_rules(&original)?;
        Ok(Self {
            original,
            pending: pending.len(),
            resources,
        })
    }
}

pub(crate) fn read_config(path: &Path) -> Result<Value, ManagerError> {
    let content =
        fs::read(path).map_err(|error| ManagerError::io("read core rule configuration", error))?;
    serde_json::from_slice(&content).map_err(config_error)
}

pub(crate) fn proxy_fetcher(fetcher: &Fetcher, config: &Value) -> Result<Fetcher, ManagerError> {
    let inbound = config["inbounds"]
        .as_array()
        .and_then(|inbounds| {
            inbounds
                .iter()
                .find(|inbound| matches!(inbound["type"].as_str(), Some("http" | "mixed")))
        })
        .ok_or_else(|| {
            ManagerError::RuntimeNotReady("core has no local HTTP proxy for rule downloads".into())
        })?;
    let port = inbound["listen_port"]
        .as_u64()
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port != 0)
        .ok_or_else(|| {
            ManagerError::RuntimeNotReady("core HTTP proxy port is unavailable".into())
        })?;
    let host = match inbound["listen"].as_str() {
        Some("::" | "::1") => "[::1]",
        _ => "127.0.0.1",
    };
    let address: SocketAddr = format!("{host}:{port}").parse().map_err(config_error)?;
    let user = &inbound["users"][0];
    fetcher
        .via_local_http_proxy(
            address,
            user["username"].as_str().unwrap_or_default(),
            user["password"].as_str().unwrap_or_default(),
        )
        .map_err(Into::into)
}

fn materialize(
    fetcher: &Fetcher,
    original: &Value,
    candidates: &HashMap<String, RuleSetSnapshot>,
) -> Result<(Value, Vec<RemoteRule>), ManagerError> {
    let mut config = original.clone();
    let mut pending = Vec::new();
    let Some(rule_sets) = config
        .get_mut("route")
        .and_then(|route| route.get_mut("rule_set"))
        .and_then(Value::as_array_mut)
    else {
        return Ok((config, pending));
    };
    let mut available = Vec::new();
    for rule in rule_sets.drain(..) {
        if rule["type"] != "remote" {
            available.push(rule);
            continue;
        }
        let remote = RemoteRule::parse(&rule)?;
        let snapshot = candidates.get(&remote.tag).cloned().or_else(|| {
            fetcher
                .cached_rule_set(&remote.url, &remote.format)
                .ok()
                .flatten()
        });
        let local = snapshot.map(|snapshot| local_rule(&remote, &snapshot));
        if let Some(local) = local {
            available.push(local);
        } else {
            pending.push(remote);
        }
    }
    *rule_sets = available;
    let missing = pending
        .iter()
        .map(|rule| rule.tag.as_str())
        .collect::<HashSet<_>>();
    for section in ["route", "dns"] {
        if let Some(rules) = config
            .get_mut(section)
            .and_then(|section| section.get_mut("rules"))
            .and_then(Value::as_array_mut)
        {
            // Remove the entire dependent expression, never broaden a logical condition.
            rules.retain(|rule| !references_missing(rule, &missing));
        }
    }
    Ok((config, pending))
}

impl RemoteRule {
    fn parse(rule: &Value) -> Result<Self, ManagerError> {
        let url = required_string(rule, "url")?;
        let inferred = url::Url::parse(&url).ok().is_some_and(|url| {
            Path::new(url.path())
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("srs"))
        });
        let format = rule["format"]
            .as_str()
            .unwrap_or(if inferred { "binary" } else { "source" });
        let interval = rule["update_interval"]
            .as_str()
            .map(humantime::parse_duration)
            .transpose()
            .map_err(config_error)?
            .filter(|value| !value.is_zero())
            .unwrap_or(Duration::from_hours(24));
        Ok(Self {
            tag: required_string(rule, "tag")?,
            url,
            format: format.into(),
            interval,
        })
    }
}

fn remote_rules(config: &Value) -> Result<Vec<RemoteRule>, ManagerError> {
    config["route"]["rule_set"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|rule| rule["type"] == "remote")
        .map(RemoteRule::parse)
        .collect()
}

fn local_rule(rule: &RemoteRule, snapshot: &RuleSetSnapshot) -> Value {
    json!({"type":"local", "tag":rule.tag, "format":rule.format, "path":snapshot.path})
}

fn normalize_rule(
    fetcher: &Fetcher,
    rule: &RemoteRule,
    snapshot: RuleSetSnapshot,
) -> Result<RuleSetSnapshot, ManagerError> {
    if rule.format == "binary" {
        return Ok(snapshot);
    }
    let text = std::str::from_utf8(&snapshot.content).map_err(config_error)?;
    if let Ok(native) = serde_json::from_str::<Value>(text)
        && native["rules"].is_array()
    {
        return Ok(snapshot);
    }
    if !sempre_converter::rule_provider_has_rules(text) {
        return Err(ManagerError::InvalidOperation(format!(
            "rule set {:?} has no usable rules",
            rule.tag
        )));
    }
    // The converted fields are supported by rule-set version 1, including older cores.
    let native = sempre_converter::convert_clash_rule_set(text, 1);
    fetcher
        .rule_set_candidate(serde_json::to_vec(&native).map_err(config_error)?)
        .map_err(Into::into)
}

fn references_missing(value: &Value, missing: &HashSet<&str>) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            if key == "rule_set" {
                value.as_str().is_some_and(|tag| missing.contains(tag))
                    || value.as_array().is_some_and(|tags| {
                        tags.iter()
                            .any(|tag| tag.as_str().is_some_and(|tag| missing.contains(tag)))
                    })
            } else {
                references_missing(value, missing)
            }
        }),
        Value::Array(values) => values
            .iter()
            .any(|value| references_missing(value, missing)),
        _ => false,
    }
}

fn required_string(value: &Value, key: &str) -> Result<String, ManagerError> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| ManagerError::InvalidOperation(format!("remote rule set has no {key}")))
}

fn config_error(error: impl std::fmt::Display) -> ManagerError {
    ManagerError::InvalidOperation(format!("prepare core rule sets: {error}"))
}

mod download;

#[cfg(test)]
mod tests;
