use serde_json::{Map, Value, json};

use crate::{CompileError, FieldDiff, Profile, Proxy, ProxyGroup, Target};

pub(super) fn render(
    profile: &Profile,
    proxies: &[Proxy],
    target: &Target,
) -> Result<(String, Vec<FieldDiff>, Vec<String>), CompileError> {
    let names: Vec<String> = proxies.iter().map(|proxy| proxy.name.clone()).collect();
    let groups = groups(&profile.groups, &names);
    let final_group = groups
        .first()
        .and_then(|value| value["name"].as_str())
        .unwrap_or("proxy");
    let mut rules = profile.rules.clone();
    let mut providers = Map::new();
    for provider in &profile.rule_providers {
        let behavior = if provider.behavior.is_empty() {
            "classical"
        } else {
            &provider.behavior
        };
        let mut value = json!({
            "type": "http", "behavior": behavior, "url": provider.url,
            "path": format!("./rules/{}", provider.tag), "interval": 86400
        });
        if !provider.format.is_empty() {
            value["format"] = json!(provider.format);
        }
        providers.insert(provider.tag.clone(), value);
        let outbound = if provider.outbound.is_empty() {
            final_group
        } else {
            &provider.outbound
        };
        rules.push(format!("RULE-SET,{},{outbound}", provider.tag));
    }
    rules.extend([
        "DOMAIN-SUFFIX,local,DIRECT".into(),
        "GEOIP,LAN,DIRECT,no-resolve".into(),
        "GEOIP,CN,DIRECT,no-resolve".into(),
        format!("MATCH,{final_group}"),
    ]);
    let mut config = json!({
        "allow-lan": true,
        "mode": "Rule",
        "log-level": clash_log_level(&profile.log_level),
        "proxies": proxies.iter().map(Proxy::as_value).collect::<Vec<_>>(),
        "proxy-groups": groups,
        "rule-providers": providers,
        "rules": rules,
        "profile": { "store-selected": true, "store-fake-ip": true, "tracing": true }
    });
    if target.format != "clash" {
        let object = config.as_object_mut().expect("object");
        object.insert("unified-delay".into(), json!(true));
        object.insert("tcp-concurrent".into(), json!(true));
        object.insert("find-process-mode".into(), json!("strict"));
        object.insert("geodata-mode".into(), json!(true));
        object.insert("geo-auto-update".into(), json!(true));
        object.insert("geo-update-interval".into(), json!(24));
    }
    apply_runtime(profile, target, &mut config);
    if let Some(override_value) = profile.core_overrides.get(&target.core) {
        deep_merge(&mut config, override_value);
    }
    let content =
        serde_yaml::to_string(&config).map_err(|error| CompileError::Render(error.to_string()))?;
    let diffs = proxies
        .iter()
        .map(|proxy| FieldDiff {
            node: proxy.name.clone(),
            represented: true,
            consumed: proxy.extra.keys().cloned().collect(),
            ignored: vec![],
            dropped: vec![],
            warnings: vec![],
            outbound: Some(proxy.as_value()),
        })
        .collect();
    Ok((content, diffs, vec![]))
}

fn groups(configured: &[ProxyGroup], names: &[String]) -> Vec<Value> {
    if configured.is_empty() {
        return vec![
            json!({ "name": "proxy", "type": "select", "proxies": std::iter::once("DIRECT".into()).chain(names.iter().cloned()).collect::<Vec<String>>() }),
        ];
    }
    configured
        .iter()
        .map(|group| {
            let mut members = group.proxies.clone();
            if !group.readonly {
                for name in names {
                    if !members.contains(name) {
                        members.push(name.clone());
                    }
                }
            }
            if members.is_empty() {
                members.clone_from_slice(names);
            }
            if !group.default.is_empty() && members.contains(&group.default) {
                members.retain(|member| member != &group.default);
                members.insert(0, group.default.clone());
            }
            let mut value =
                json!({ "name": group.name, "type": group.group_type, "proxies": members });
            if !group.url.is_empty() {
                value["url"] = json!(group.url);
            }
            if group.interval > 0 {
                value["interval"] = json!(group.interval);
            }
            if group.tolerance > 0 {
                value["tolerance"] = json!(group.tolerance);
            }
            value
        })
        .collect()
}

fn apply_runtime(profile: &Profile, target: &Target, config: &mut Value) {
    if target.core != "mihomo" && target.core != "clash-rs" {
        return;
    }
    let object = config.as_object_mut().expect("object");
    object.insert("socks-port".into(), json!(profile.local_proxy.socks_port));
    object.insert("port".into(), json!(profile.local_proxy.http_port));
    object.insert("bind-address".into(), json!("127.0.0.1"));
    object.insert("allow-lan".into(), json!(false));
    if !profile.local_proxy.username.is_empty() {
        object.insert(
            "authentication".into(),
            json!([format!(
                "{}:{}",
                profile.local_proxy.username, profile.local_proxy.password
            )]),
        );
    }
    if profile.dns.is_object() {
        object.insert("dns".into(), profile.dns.clone());
    }
}

fn clash_log_level(level: &str) -> &str {
    match level {
        "off" => "silent",
        "warn" => "warning",
        "error" | "info" | "debug" => level,
        _ => "info",
    }
}

fn deep_merge(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Object(target), Value::Object(source)) => {
            for (key, value) in source {
                deep_merge(target.entry(key).or_insert(Value::Null), value);
            }
        }
        (target, source) => *target = source.clone(),
    }
}
