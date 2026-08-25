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
    apply_override(&mut config, profile, &target.core)?;
    let mut content = serde_json::to_string_pretty(&config)
        .map_err(|error| CompileError::Render(error.to_string()))?;
    content.push('\n');
    Ok((content, diffs, warnings))
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

fn apply_override(config: &mut Value, profile: &Profile, core: &str) -> Result<(), CompileError> {
    let Some(value) = profile.core_overrides.get(core) else {
        return Ok(());
    };
    if value.get("api").is_some() {
        return Err(CompileError::Render(
            "top-level api is managed by Sempre's internal core control".into(),
        ));
    }
    if value.get("inbounds").is_some() {
        return Err(CompileError::Render(
            "top-level inbounds are managed by Sempre's authenticated local proxy".into(),
        ));
    }
    deep_merge(config, value);
    Ok(())
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
