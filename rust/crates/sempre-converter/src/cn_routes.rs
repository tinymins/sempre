use crate::Profile;

/// Host routing is a daemon policy, carried through compilation without IO.
pub fn cn_pre_routing(profile: &Profile) -> bool {
    profile
        .extra
        .get("sempre_cn_direct_routes")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

/// Only direct mode installs physical CN routes; proxy mode uses the TUN fallback.
pub fn cn_direct_routes(profile: &Profile) -> bool {
    cn_pre_routing(profile)
        && profile
            .extra
            .get("sempre_cn_routing_mode")
            .and_then(serde_json::Value::as_str)
            != Some("proxy")
}

pub const CN_PROXY_GROUP: &str = "DNS · CN pre-routing";

/// Apply the same proxy selector contract to locally compiled and remote configs.
/// A local compiler may place the reserved rule after explicit private access.
pub fn apply_cn_proxy(document: &mut serde_json::Value) -> Result<(), crate::CompileError> {
    use serde_json::json;
    let invalid = |message: &str| crate::CompileError::Render(message.into());
    let names = crate::dns_outbound_members(document);
    let outbounds = document["outbounds"]
        .as_array_mut()
        .ok_or_else(|| invalid("CN proxy routing requires outbounds"))?;
    if names.is_empty() {
        return Err(invalid("CN proxy routing has no proxy nodes to select"));
    }
    if let Some(group) = outbounds.iter().find(|item| item["tag"] == CN_PROXY_GROUP) {
        let valid = group["type"] == "selector"
            && group["outbounds"].as_array().is_some_and(|members| {
                !members.is_empty()
                    && members.iter().all(|member| {
                        member
                            .as_str()
                            .is_some_and(|name| names.iter().any(|n| n == name))
                    })
                    && members.contains(&group["default"])
            });
        if !valid {
            return Err(invalid("CN pre-routing proxy group name is reserved"));
        }
    } else {
        outbounds.push(json!({"type": "selector", "tag": CN_PROXY_GROUP,
            "outbounds": names, "default": names[0], "interrupt_exist_connections": true}));
    }
    let rules = document["route"]["rules"]
        .as_array_mut()
        .ok_or_else(|| invalid("CN proxy routing requires route rules"))?;
    let existing = rules
        .iter()
        .filter(|rule| rule["outbound"] == CN_PROXY_GROUP)
        .collect::<Vec<_>>();
    if !existing.is_empty() && (existing.len() != 1 || *existing[0] != proxy_rule()) {
        return Err(invalid(
            "CN routing proxy rule is reserved for original TUN IP traffic",
        ));
    }
    if existing.is_empty() {
        let at = rules
            .iter()
            .position(|rule| {
                !matches!(rule["action"].as_str(), Some("sniff" | "hijack-dns"))
                    && rule["protocol"] != "dns"
                    && rule["ip_is_private"] != true
            })
            .unwrap_or(rules.len());
        rules.insert(at, proxy_rule());
    }
    crate::populate_dns_selectors(document);
    Ok(())
}

/// `FakeIP` restoration clears the destination IP before sing-box sets `IPVersion`.
/// Resolved addresses must never turn a restored domain into CN packet traffic.
pub(crate) fn proxy_rule() -> serde_json::Value {
    serde_json::json!({
        "type": "logical", "mode": "and",
        "rules": [
            {"inbound": ["tun-in"]},
            {"type": "logical", "mode": "or", "rules": [{"ip_version": 4}, {"ip_version": 6}]},
            {"rule_set": ["geoip-cn"]}
        ],
        "outbound": CN_PROXY_GROUP
    })
}
