mod model;
mod outbound;
mod routing;

use std::collections::HashSet;

use serde_json::{Value, json};

use crate::{CompileError, FieldDiff, Profile, Proxy, Target};

use self::model::RuntimeModel;

pub(super) fn render(
    profile: &Profile,
    proxies: &[Proxy],
    target: &Target,
) -> Result<(String, Vec<FieldDiff>, Vec<String>), CompileError> {
    let modern = target.core == "xray";
    let mut outbounds = vec![
        json!({ "tag": "direct", "protocol": "freedom", "settings": { "domainStrategy": "UseIP" } }),
        json!({ "tag": "reject", "protocol": "blackhole", "settings": {} }),
        json!({ "tag": "dns-out", "protocol": "dns", "settings": {} }),
    ];
    let mut diffs = Vec::with_capacity(proxies.len());
    let mut warnings = Vec::new();
    let mut represented = HashSet::new();
    for proxy in proxies {
        let (outbound, diff) = outbound::convert(proxy, modern);
        warnings.extend(diff.warnings.iter().cloned());
        if let Some(outbound) = outbound {
            represented.insert(proxy.name.as_str());
            outbounds.push(outbound);
        }
        diffs.push(diff);
    }
    if represented.is_empty() {
        return Err(CompileError::Render(format!(
            "no nodes can be represented by {}",
            target.core
        )));
    }
    let supported = proxies
        .iter()
        .filter(|proxy| represented.contains(proxy.name.as_str()))
        .collect::<Vec<_>>();
    let model = RuntimeModel::new(profile, proxies, &represented, &target.core)?;
    let mut inbounds = outbound::local_inbounds(profile, modern);
    inbounds.extend(super::transparent::v2ray_inbounds(profile, target));
    let (routing, routing_warnings) = routing::render(&model);
    warnings.extend(routing_warnings);
    let dns_proxies = supported.iter().copied().cloned().collect::<Vec<_>>();
    let mut config = json!({
        "log": log(&profile.log_level, modern),
        "dns": super::dns::v2ray(profile, &dns_proxies, target),
        "inbounds": inbounds,
        "outbounds": outbounds,
        "routing": routing,
        "policy": { "system": {
            "statsInboundUplink": true, "statsInboundDownlink": true,
            "statsOutboundUplink": true, "statsOutboundDownlink": true
        }},
        "stats": {}
    });
    if let Some(observatory) = routing::observatory(&model) {
        config["observatory"] = observatory;
    }
    restore_direct_name(&mut config);
    let mut content = serde_json::to_string_pretty(&config)
        .map_err(|error| CompileError::Render(error.to_string()))?;
    content.push('\n');
    Ok((content, diffs, warnings))
}

fn restore_direct_name(config: &mut Value) {
    use super::outbound_names::{DIRECT, restore_field};

    if let Some(outbounds) = config.get_mut("outbounds").and_then(Value::as_array_mut) {
        for outbound in outbounds {
            if outbound.get("protocol").and_then(Value::as_str) == Some("freedom")
                && outbound.get("tag").and_then(Value::as_str) == Some("direct")
            {
                outbound["tag"] = Value::from(DIRECT);
            }
        }
    }
    if let Some(routing) = config.get_mut("routing") {
        if let Some(rules) = routing.get_mut("rules").and_then(Value::as_array_mut) {
            for rule in rules {
                restore_field(rule, "outboundTag");
            }
        }
        if let Some(balancers) = routing.get_mut("balancers").and_then(Value::as_array_mut) {
            for balancer in balancers {
                restore_field(balancer, "selector");
            }
        }
    }
    if let Some(observatory) = config.get_mut("observatory") {
        restore_field(observatory, "subjectSelector");
    }
}

fn log(level: &str, modern: bool) -> Value {
    let level = if level == "warn" {
        "warning"
    } else {
        match level {
            "off" | "none" => "none",
            "error" | "warning" | "info" | "debug" => level,
            _ => "warning",
        }
    };
    let mut result = json!({ "loglevel": level });
    if modern {
        result["dnsLog"] = json!(level == "debug");
    }
    result
}
