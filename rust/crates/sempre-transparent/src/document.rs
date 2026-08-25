use sempre_converter::{Profile, TransparentProxy};
use sempre_network::Inventory;
use serde_json::{Map, Value, json};

use crate::{BYPASS_MARK, Plan, TransparentError, prefix};

const RESERVED: [&str; 12] = [
    "0.0.0.0/8",
    "10.0.0.0/8",
    "100.64.0.0/10",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.0.0.0/24",
    "192.168.0.0/16",
    "224.0.0.0/4",
    "240.0.0.0/4",
    "::1/128",
    "fc00::/7",
];

pub(crate) fn prepare_tun(
    plan: &mut Plan,
    profile: &Profile,
    inventory: &Inventory,
    config: &mut Value,
) -> Result<(), TransparentError> {
    let transparent = &profile.transparent_proxy;
    let fake_ip_prefixes = fake_ip_prefixes(&plan.core, config);
    let mut exclusions = transparent.route_exclusions.clone();
    if transparent.auto_exclude_local_routes {
        exclusions.extend(inventory.local_prefixes.clone());
    }
    if transparent.auto_exclude_vpn_routes {
        exclusions.extend(inventory.vpn_prefixes.clone());
    }
    exclusions = prefix::filter_overlaps(exclusions, &fake_ip_prefixes);
    plan.fake_ip_prefixes = fake_ip_prefixes;
    plan.route_exclusions.clone_from(&exclusions);
    plan.tun_interface
        .clone_from(&transparent.tun.interface_name);
    match plan.core.as_str() {
        "sing-box" => prepare_sing_box_tun(plan, transparent, inventory, config, &exclusions)?,
        "mihomo" => prepare_mihomo_tun(transparent, config, &exclusions),
        "clash-rs" => prepare_clash_rs_tun(plan, transparent, inventory, config)?,
        "xray" => prepare_xray_tun(plan, transparent, inventory, config, &exclusions)?,
        _ => return Err(unsupported(&plan.core, "tun-router")),
    }
    plan.lan_interfaces
        .clone_from(&inventory.recommended_lan_interfaces);
    Ok(())
}

fn prepare_sing_box_tun(
    plan: &mut Plan,
    transparent: &TransparentProxy,
    inventory: &Inventory,
    config: &mut Value,
    exclusions: &[String],
) -> Result<(), TransparentError> {
    let address =
        prefix::resolve_tun_address(&transparent.tun.address, &inventory.occupied_prefixes)?;
    let inbound = find_inbound(config, "tun-in", "type", "tun")?;
    inbound["interface_name"] = json!(transparent.tun.interface_name);
    inbound["address"] = json!([address]);
    inbound["auto_route"] = json!(true);
    inbound["auto_redirect"] = json!(true);
    inbound["strict_route"] = json!(true);
    inbound["stack"] = json!("system");
    let inbound = inbound
        .as_object_mut()
        .expect("matched inbound is an object");
    set_interface_policy(
        inbound,
        transparent,
        "include_interface",
        "exclude_interface",
    );
    set_optional_array(inbound, "route_exclude_address", exclusions);
    object_mut(config, "route").insert("auto_detect_interface".into(), json!(true));
    plan.tun_address = address;
    Ok(())
}

fn prepare_mihomo_tun(transparent: &TransparentProxy, config: &mut Value, exclusions: &[String]) {
    let tun = object_mut(config, "tun");
    for (key, value) in [
        ("enable", json!(true)),
        ("device", json!(transparent.tun.interface_name)),
        ("stack", json!("system")),
        ("auto-route", json!(true)),
        ("auto-redirect", json!(true)),
        ("strict-route", json!(true)),
        ("auto-detect-interface", json!(true)),
        ("dns-hijack", json!(["any:53", "tcp://any:53"])),
    ] {
        tun.insert(key.into(), value);
    }
    set_interface_policy(tun, transparent, "include-interface", "exclude-interface");
    set_optional_array(tun, "route-exclude-address", exclusions);
}

fn prepare_clash_rs_tun(
    plan: &mut Plan,
    transparent: &TransparentProxy,
    inventory: &Inventory,
    config: &mut Value,
) -> Result<(), TransparentError> {
    let address =
        prefix::resolve_tun_address(&transparent.tun.address, &inventory.occupied_prefixes)?;
    let tun = object_mut(config, "tun");
    tun.insert("enable".into(), json!(true));
    tun.insert("device".into(), json!(transparent.tun.interface_name));
    tun.insert("gateway".into(), json!(address));
    tun.insert("route-all".into(), json!(true));
    tun.insert("dns-hijack".into(), json!(true));
    plan.tun_address = address;
    Ok(())
}

fn prepare_xray_tun(
    plan: &mut Plan,
    transparent: &TransparentProxy,
    inventory: &Inventory,
    config: &mut Value,
    exclusions: &[String],
) -> Result<(), TransparentError> {
    let address =
        prefix::resolve_tun_address(&transparent.tun.address, &inventory.occupied_prefixes)?;
    let inbound = find_inbound(config, "tun-in", "protocol", "tun")?;
    let settings = object_mut(inbound, "settings");
    settings.insert("name".into(), json!(transparent.tun.interface_name));
    settings.insert("gateway".into(), json!([address]));
    settings.insert(
        "autoSystemRoutingTable".into(),
        json!(["0.0.0.0/0", "::/0"]),
    );
    settings.insert("autoOutboundsInterface".into(), json!("auto"));
    if !exclusions.is_empty() {
        let routing = object_mut(config, "routing");
        let rules = routing
            .entry("rules")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                TransparentError::Invalid("Xray routing.rules must be an array".into())
            })?;
        rules.insert(
            0,
            json!({ "type": "field", "ip": exclusions, "outboundTag": "direct" }),
        );
    }
    plan.tun_address = address;
    Ok(())
}

pub(crate) fn prepare_tproxy(
    plan: &mut Plan,
    profile: &Profile,
    inventory: &Inventory,
    config: &mut Value,
) -> Result<(), TransparentError> {
    let transparent = &profile.transparent_proxy;
    let interfaces = resolve_lan_interfaces(&transparent.lan_interfaces, inventory)?;
    if interfaces.is_empty() && !transparent.capture_host {
        return Err(TransparentError::Invalid(
            "TProxy mode needs a LAN interface or capture_host enabled".into(),
        ));
    }
    match plan.core.as_str() {
        "sing-box" => {
            find_inbound(config, "tproxy-in", "type", "tproxy")?;
            find_inbound(config, "dns-in", "type", "direct")?;
            let route = object_mut(config, "route");
            route.insert("default_mark".into(), json!(BYPASS_MARK));
            route.insert("auto_detect_interface".into(), json!(true));
        }
        "mihomo" | "clash-rs" => {
            require_number(config, "tproxy-port", transparent.tproxy.listen_port)?;
            require_listener(config, transparent.tproxy.dns_listen_port)?;
            config["routing-mark"] = json!(BYPASS_MARK);
        }
        "xray" | "v2ray" => {
            find_inbound(config, "tproxy-in", "protocol", "dokodemo-door")?;
            find_inbound(config, "dns-in", "protocol", "dokodemo-door")?;
            mark_v2ray_outbounds(config)?;
        }
        _ => return Err(unsupported(&plan.core, "tproxy")),
    }
    let mut excluded = RESERVED.into_iter().map(String::from).collect::<Vec<_>>();
    excluded.extend(["fe80::/10", "ff00::/8"].map(String::from));
    excluded.extend(inventory.local_prefixes.clone());
    excluded.extend(outbound_server_prefixes(config));
    plan.tproxy_port = transparent.tproxy.listen_port;
    plan.dns_port = transparent.tproxy.dns_listen_port;
    plan.capture_host = transparent.capture_host;
    plan.lan_interfaces = interfaces;
    plan.excluded_prefixes = prefix::normalized(excluded);
    Ok(())
}

fn resolve_lan_interfaces(
    configured: &[String],
    inventory: &Inventory,
) -> Result<Vec<String>, TransparentError> {
    let source = if configured.is_empty() {
        &inventory.recommended_lan_interfaces
    } else {
        configured
    };
    let mut result = Vec::new();
    for name in source {
        if !inventory.interfaces.iter().any(|item| item.name == *name) {
            return Err(TransparentError::Invalid(format!(
                "configured LAN interface {name:?} does not exist"
            )));
        }
        if !result.contains(name) {
            result.push(name.clone());
        }
    }
    Ok(result)
}

fn fake_ip_prefixes(core: &str, config: &Value) -> Vec<String> {
    let dns = config.get("dns").unwrap_or(&Value::Null);
    let mut values = Vec::new();
    if core == "sing-box" {
        if let Some(servers) = dns.get("servers").and_then(Value::as_array) {
            for server in servers.iter().filter(|item| item["type"] == "fakeip") {
                values.extend(strings(server, &["inet4_range", "inet6_range"]));
            }
        }
        if dns["fakeip"]["enabled"] != false {
            values.extend(strings(&dns["fakeip"], &["inet4_range", "inet6_range"]));
        }
    } else if matches!(core, "mihomo" | "clash-rs") && dns["enhanced-mode"] == "fake-ip" {
        values.extend(strings(dns, &["fake-ip-range", "fake-ip-range6"]));
    }
    prefix::normalized(values)
}

fn outbound_server_prefixes(config: &Value) -> Vec<String> {
    let mut result = Vec::new();
    let Some(outbounds) = config.get("outbounds").and_then(Value::as_array) else {
        return result;
    };
    for outbound in outbounds {
        if let Some(value) = outbound.get("server").and_then(Value::as_str) {
            result.extend(prefix::host_prefix(value));
        }
        let settings = outbound.get("settings").unwrap_or(&Value::Null);
        if let Some(value) = settings.get("address").and_then(Value::as_str) {
            result.extend(prefix::host_prefix(value));
        }
        for key in ["servers", "vnext"] {
            if let Some(servers) = settings.get(key).and_then(Value::as_array) {
                for server in servers {
                    if let Some(value) = server.get("address").and_then(Value::as_str) {
                        result.extend(prefix::host_prefix(value));
                    }
                }
            }
        }
    }
    result
}

fn mark_v2ray_outbounds(config: &mut Value) -> Result<(), TransparentError> {
    let outbounds = config
        .get_mut("outbounds")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| TransparentError::Invalid("V2Ray runtime has no outbounds".into()))?;
    for outbound in outbounds {
        let stream = object_mut(outbound, "streamSettings");
        map_object_mut(stream, "sockopt").insert("mark".into(), json!(BYPASS_MARK));
    }
    Ok(())
}

fn find_inbound<'a>(
    config: &'a mut Value,
    tag: &str,
    kind_key: &str,
    kind: &str,
) -> Result<&'a mut Value, TransparentError> {
    config
        .get_mut("inbounds")
        .and_then(Value::as_array_mut)
        .and_then(|values| {
            values
                .iter_mut()
                .find(|item| item["tag"] == tag && item[kind_key] == kind)
        })
        .ok_or_else(|| {
            TransparentError::Invalid(format!(
                "runtime configuration is missing {tag} {kind} inbound"
            ))
        })
}

fn require_number(config: &Value, key: &str, expected: u16) -> Result<(), TransparentError> {
    if config.get(key).and_then(Value::as_u64) == Some(u64::from(expected)) {
        Ok(())
    } else {
        Err(TransparentError::Invalid(format!(
            "runtime configuration is missing {key} {expected}"
        )))
    }
}

fn require_listener(config: &Value, port: u16) -> Result<(), TransparentError> {
    if config["listeners"].as_array().is_some_and(|listeners| {
        listeners
            .iter()
            .any(|item| item["type"] == "tproxy" && item["port"] == port)
    }) {
        Ok(())
    } else {
        Err(TransparentError::Invalid(format!(
            "runtime configuration is missing DNS TProxy listener {port}"
        )))
    }
}

fn object_mut<'a>(config: &'a mut Value, key: &str) -> &'a mut Map<String, Value> {
    if !config.get(key).is_some_and(Value::is_object) {
        config[key] = json!({});
    }
    config[key]
        .as_object_mut()
        .expect("object helper creates an object")
}

fn map_object_mut<'a>(config: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    if !config.get(key).is_some_and(Value::is_object) {
        config.insert(key.into(), json!({}));
    }
    config
        .get_mut(key)
        .and_then(Value::as_object_mut)
        .expect("map helper creates an object")
}

fn set_interface_policy(
    object: &mut Map<String, Value>,
    transparent: &TransparentProxy,
    include_key: &str,
    exclude_key: &str,
) {
    object.remove(include_key);
    object.remove(exclude_key);
    match transparent.interface_mode.as_str() {
        "include" => object.insert(include_key.into(), json!(transparent.interfaces)),
        "exclude" => object.insert(exclude_key.into(), json!(transparent.interfaces)),
        _ => None,
    };
}

fn set_optional_array(object: &mut Map<String, Value>, key: &str, values: &[String]) {
    if values.is_empty() {
        object.remove(key);
    } else {
        object.insert(key.into(), json!(values));
    }
}

fn strings(value: &Value, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .filter_map(|key| value.get(key).and_then(Value::as_str))
        .filter(|value| !value.is_empty())
        .map(String::from)
        .collect()
}

fn unsupported(core: &str, mode: &str) -> TransparentError {
    TransparentError::Invalid(format!("{core} does not support {mode} mode"))
}
