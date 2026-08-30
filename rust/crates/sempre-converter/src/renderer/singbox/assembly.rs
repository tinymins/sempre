use std::net::IpAddr;

use serde_json::{Value, json};

use crate::{CompileError, FieldDiff, Profile, Proxy, ProxyGroup, SourceSnapshot, Target};

use super::{config, convert_proxy, fields::deep_merge, private_access};

pub(super) fn render(
    profile: &Profile,
    proxies: &[Proxy],
    target: &Target,
    snapshots: &[SourceSnapshot],
) -> Result<(String, Vec<FieldDiff>, Vec<String>), CompileError> {
    let modern = target.version != "11";
    let private = private_access::resolve(
        &profile.private_access,
        modern,
        target.platform != "default",
    );
    let mut outbounds = vec![
        json!({ "type": "direct", "tag": "direct" }),
        json!({ "type": "block", "tag": "reject" }),
    ];
    if !modern {
        outbounds.push(json!({ "type": "dns", "tag": "dns-out" }));
    }
    let mut names = Vec::new();
    let mut diffs = Vec::new();
    let mut warnings = Vec::new();
    for proxy in proxies {
        let (converted, mut diff) = convert_proxy(proxy, target);
        if let Some(mut outbound) = converted {
            if modern && proxy.server.parse::<IpAddr>().is_err() {
                outbound["domain_resolver"] =
                    json!({ "server": "bootstrap", "strategy": "ipv4_only" });
            }
            names.push(proxy.name.clone());
            diff.outbound = Some(outbound.clone());
            outbounds.push(outbound);
        }
        warnings.extend(diff.warnings.clone());
        diffs.push(diff);
    }
    outbounds.extend(private.outbounds.iter().cloned());
    outbounds.extend(selector_outbounds(&profile.groups, &names)?);

    let mut dns = super::super::dns::sing_box(profile, proxies, target)?;
    if let Some(servers) = dns.get_mut("servers").and_then(Value::as_array_mut) {
        servers.extend(private.dns_servers.iter().cloned());
    }
    if let Some(rules) = dns.get_mut("rules").and_then(Value::as_array_mut) {
        let mut private_rules = private.dns_rules.clone();
        if !private.direct_domains.is_empty() {
            private_rules.insert(
                0,
                json!({ "domain": private.direct_domains, "action": "route", "server": "local" }),
            );
        }
        private_rules.append(rules);
        *rules = private_rules;
    }
    let route = config::route(profile, target, snapshots, &private, &mut warnings);
    let store_fakeip = target.platform == "default"
        && dns
            .get("servers")
            .and_then(Value::as_array)
            .is_some_and(|servers| {
                servers
                    .iter()
                    .any(|server| server.get("type").and_then(Value::as_str) == Some("fakeip"))
            });
    let mut output = json!({
        "log": config::log(&profile.log_level),
        "inbounds": config::inbounds(profile, target),
        "outbounds": outbounds,
        "dns": dns,
        "route": route,
        "experimental": config::experimental(profile, target, store_fakeip)
    });
    if !private.endpoints.is_empty() {
        output["endpoints"] = json!(private.endpoints);
    }
    if let Some(override_value) = profile.core_overrides.get("sing-box") {
        deep_merge(&mut output, override_value);
    }
    super::super::dns::apply_sing_box_platform_policy(profile, target, &mut output, &mut warnings);
    config::normalize_for_version(&mut output, target);
    let mut content = serde_json::to_string_pretty(&output)
        .map_err(|error| CompileError::Render(error.to_string()))?;
    content.push('\n');
    Ok((content, diffs, warnings))
}

fn selector_outbounds(groups: &[ProxyGroup], names: &[String]) -> Result<Vec<Value>, CompileError> {
    let configured = if groups.is_empty() {
        vec![ProxyGroup {
            name: "proxy".into(),
            group_type: "select".into(),
            include_all: true,
            ..ProxyGroup::default()
        }]
    } else {
        groups.to_vec()
    };
    let mut outbounds = Vec::new();
    for group in configured {
        if group.name.trim().is_empty() {
            return Err(CompileError::Render("proxy group name is required".into()));
        }
        if builtin(&group.name) {
            continue;
        }
        let mut members = group
            .proxies
            .iter()
            .map(|value| normalize(value))
            .collect::<Vec<_>>();
        if names.is_empty() {
            members = vec!["direct".into()];
        } else if !group.readonly || group.include_all || members.is_empty() {
            append_unique(&mut members, names);
        }
        if members.is_empty() {
            return Err(CompileError::Render(format!(
                "proxy group {:?} has no members",
                group.name
            )));
        }
        let kind = if group.group_type == "url-test" {
            "urltest"
        } else {
            "selector"
        };
        let mut outbound = json!({ "type": kind, "tag": group.name, "outbounds": members });
        if kind == "selector" {
            let default =
                selector_default(&group, outbound["outbounds"].as_array().expect("members"));
            outbound["default"] = json!(default);
            outbound["interrupt_exist_connections"] = json!(true);
        } else {
            outbound["url"] = json!(if group.url.is_empty() {
                "https://www.gstatic.com/generate_204"
            } else {
                &group.url
            });
            if group.interval > 0 {
                outbound["interval"] = json!(format!("{}s", group.interval));
            }
            if group.tolerance > 0 {
                outbound["tolerance"] = json!(group.tolerance);
            }
        }
        outbounds.push(outbound);
    }
    Ok(outbounds)
}

fn selector_default(group: &ProxyGroup, members: &[Value]) -> String {
    if !group.default.is_empty() {
        return normalize(&group.default);
    }
    if group.name == "🔰 国外流量"
        && let Some(value) = members
            .iter()
            .filter_map(Value::as_str)
            .find(|value| !builtin(value))
    {
        return value.into();
    }
    members[0].as_str().unwrap_or_default().into()
}

fn append_unique(target: &mut Vec<String>, values: &[String]) {
    for value in values {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

fn normalize(value: &str) -> String {
    match value {
        "DIRECT" | "🚀 直接连接" => "direct".into(),
        "REJECT" => "reject".into(),
        _ => value.into(),
    }
}

fn builtin(value: &str) -> bool {
    matches!(normalize(value).as_str(), "direct" | "reject" | "dns-out")
}
