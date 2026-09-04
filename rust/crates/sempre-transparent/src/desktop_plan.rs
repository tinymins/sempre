use std::{fs, net::IpAddr, path::Path};

use sempre_converter::Profile;
use serde_json::{Value, json};

use crate::{Plan, TransparentError};

const WINDOWS_FRONTEND_PORT: u16 = 1054;
const WINDOWS_DNS_TARGET_PREFIX: &str = "192.0.2.1/32";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Platform {
    Macos,
    Windows,
    WindowsDivert,
}

pub(crate) fn prepare(
    platform: Platform,
    core: &str,
    core_version: &str,
    profile: &Profile,
    runtime_config: &Path,
    original_upstreams: Vec<String>,
) -> Result<Plan, TransparentError> {
    prepare_with_windows_ipv6(
        platform,
        core,
        core_version,
        profile,
        runtime_config,
        original_upstreams,
        windows_ipv6_default_route_available(),
    )
}

fn prepare_with_windows_ipv6(
    platform: Platform,
    core: &str,
    core_version: &str,
    profile: &Profile,
    runtime_config: &Path,
    original_upstreams: Vec<String>,
    windows_ipv6_available: bool,
) -> Result<Plan, TransparentError> {
    let Some(system_dns) = managed_frontend_plan(platform, core, profile, original_upstreams)
    else {
        return Ok(Plan::default());
    };
    let data = fs::read(runtime_config).map_err(|source| TransparentError::Io {
        context: "read desktop DNS frontend runtime configuration".into(),
        source,
    })?;
    let mut config = crate::decode(core, &data)?;
    if matches!(platform, Platform::Windows | Platform::WindowsDivert) && !windows_ipv6_available {
        disable_fake_ipv6(&mut config);
    }
    let plan = Plan {
        core: core.into(),
        system_dns: Some(system_dns),
        ..Plan::default()
    };
    crate::validate_system_dns_config(&plan, &config)?;
    match platform {
        Platform::Macos => insert_original_dns_bypass(&mut config, &plan)?,
        Platform::Windows => configure_windows_dns_redirect(&mut config, &plan, core_version)?,
        Platform::WindowsDivert => configure_windows_routes(&mut config, &plan)?,
    }
    let mut data =
        serde_json::to_vec_pretty(&config).map_err(|error| TransparentError::Encode {
            core: core.into(),
            detail: error.to_string(),
        })?;
    data.push(b'\n');
    sempre_state::write_atomic(runtime_config, &data, 0o600).map_err(|source| {
        TransparentError::Io {
            context: "write desktop DNS frontend runtime configuration".into(),
            source,
        }
    })?;
    Ok(plan)
}

#[cfg(target_os = "windows")]
fn windows_ipv6_default_route_available() -> bool {
    std::net::UdpSocket::bind("[::]:0")
        .and_then(|socket| socket.connect("[2001:4860:4860::8888]:53"))
        .is_ok()
}

#[cfg(not(target_os = "windows"))]
const fn windows_ipv6_default_route_available() -> bool {
    true
}

pub(crate) fn managed_frontend_plan(
    platform: Platform,
    core: &str,
    profile: &Profile,
    original_upstreams: Vec<String>,
) -> Option<crate::SystemDnsPlan> {
    if core != "sing-box" {
        return None;
    }
    let mut system_dns = crate::system_dns_intent(profile)?;
    system_dns.managed_frontend = true;
    system_dns.takeover_host = true;
    system_dns.core_listen_port = match profile.transparent_proxy.tproxy.dns_listen_port {
        0 => sempre_converter::DEFAULT_CORE_DNS_PORT,
        port => port,
    };
    if matches!(platform, Platform::Windows | Platform::WindowsDivert) {
        system_dns.listen_port = WINDOWS_FRONTEND_PORT;
    }
    system_dns.original_upstreams = original_upstreams;
    Some(system_dns)
}

pub(crate) const fn windows_platform() -> Platform {
    if cfg!(target_arch = "x86_64") {
        Platform::WindowsDivert
    } else {
        Platform::Windows
    }
}

fn disable_fake_ipv6(config: &mut Value) {
    let Some(dns) = config.get_mut("dns").and_then(Value::as_object_mut) else {
        return;
    };
    let mut removed_prefixes = Vec::new();
    if let Some(servers) = dns.get_mut("servers").and_then(Value::as_array_mut) {
        for server in servers
            .iter_mut()
            .filter(|server| server["type"] == "fakeip")
        {
            if let Some(prefix) = server
                .as_object_mut()
                .and_then(|server| server.remove("inet6_range"))
                .and_then(|value| value.as_str().map(str::to_owned))
            {
                removed_prefixes.push(prefix);
            }
        }
    }
    if let Some(fakeip) = dns.get_mut("fakeip").and_then(Value::as_object_mut)
        && let Some(prefix) = fakeip
            .remove("inet6_range")
            .and_then(|value| value.as_str().map(str::to_owned))
    {
        removed_prefixes.push(prefix);
    }
    if let Some(rules) = dns.get_mut("rules").and_then(Value::as_array_mut) {
        rules.retain_mut(restrict_fakeip_rule_to_ipv4);
    }
    for inbound in config
        .get_mut("inbounds")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter(|inbound| inbound["type"] == "tun")
    {
        if let Some(routes) = inbound
            .get_mut("route_address")
            .and_then(Value::as_array_mut)
        {
            routes.retain(|route| {
                route
                    .as_str()
                    .is_none_or(|route| !removed_prefixes.iter().any(|prefix| prefix == route))
            });
        }
    }
}

fn restrict_fakeip_rule_to_ipv4(rule: &mut Value) -> bool {
    if rule["server"] != "fakeip" {
        return true;
    }
    let Some(rule) = rule.as_object_mut() else {
        return true;
    };
    let Some(query_type) = rule.get_mut("query_type") else {
        rule.insert("query_type".into(), json!(["A"]));
        return true;
    };
    if let Some(query_types) = query_type.as_array_mut() {
        query_types.retain(|query_type| query_type != "AAAA");
        return !query_types.is_empty();
    }
    query_type != "AAAA"
}

fn configure_windows_routes(config: &mut Value, plan: &Plan) -> Result<(), TransparentError> {
    insert_original_dns_bypass(config, plan)?;
    let original_dns = upstream_prefixes(plan.system_dns.as_ref().expect("desktop DNS plan"))?;
    let fake_ip = crate::document::fake_ip_prefixes(&plan.core, config);
    // DNS interception is owned by Sempre. TUN only owns the selected traffic routes.
    if let Some(tun) = config
        .get_mut("inbounds")
        .and_then(Value::as_array_mut)
        .and_then(|inbounds| inbounds.iter_mut().find(|inbound| inbound["type"] == "tun"))
        .and_then(Value::as_object_mut)
    {
        tun.insert("strict_route".into(), json!(false));
        merge_array(tun, "route_exclude_address", original_dns);
        if fake_ip.is_empty() {
            tun.remove("route_address");
        } else {
            merge_array(tun, "route_address", fake_ip);
        }
    }
    Ok(())
}

fn configure_windows_dns_redirect(
    config: &mut Value,
    plan: &Plan,
    core_version: &str,
) -> Result<(), TransparentError> {
    if !supports_configurable_tun_dns(core_version) {
        return Err(TransparentError::Invalid(format!(
            "Windows managed DNS frontend requires sing-box 1.14 or newer; selected version is {core_version}"
        )));
    }
    let system_dns = plan
        .system_dns
        .as_ref()
        .expect("desktop DNS plan has system DNS");
    let original_dns = upstream_prefixes(system_dns)?;
    let fake_ip_prefixes = crate::document::fake_ip_prefixes(&plan.core, config);
    let inbounds = config
        .get_mut("inbounds")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| TransparentError::Invalid("runtime configuration has no inbounds".into()))?;
    let tun = inbounds
        .iter_mut()
        .find(|inbound| inbound["type"] == "tun" && inbound["tag"] == "tun-in")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            TransparentError::Invalid(
                "Windows managed DNS frontend requires the sing-box TUN inbound".into(),
            )
        })?;
    tun.insert("dns_mode".into(), json!("native"));
    tun.insert("dns_address".into(), json!(["192.0.2.1"]));
    tun.insert("strict_route".into(), json!(false));
    merge_array(tun, "route_exclude_address", original_dns);
    if fake_ip_prefixes.is_empty() {
        tun.remove("route_address");
    } else {
        let mut included = fake_ip_prefixes;
        let configured = tun
            .get("route_address")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        for prefix in configured {
            if !included.contains(&prefix) {
                included.push(prefix);
            }
        }
        if !included
            .iter()
            .any(|prefix| prefix == WINDOWS_DNS_TARGET_PREFIX)
        {
            included.push(WINDOWS_DNS_TARGET_PREFIX.into());
        }
        tun.insert("route_address".into(), json!(included));
    }
    let rules = config
        .pointer_mut("/route/rules")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            TransparentError::Invalid("runtime configuration has no route rules".into())
        })?;
    rules.insert(
        0,
        json!({
            "inbound": "tun-in", "network": ["tcp", "udp"], "port": [53],
            "action": "route", "outbound": "direct",
            "override_address": "127.0.0.1", "override_port": system_dns.listen_port,
            "udp_connect": true
        }),
    );
    Ok(())
}

fn supports_configurable_tun_dns(version: &str) -> bool {
    let mut parts = version.trim_start_matches('v').split('.');
    matches!(
        (
            parts.next().and_then(|value| value.parse::<u32>().ok()),
            parts.next().and_then(|value| value.parse::<u32>().ok()),
        ),
        (Some(1), Some(14..))
    )
}

fn merge_array(object: &mut serde_json::Map<String, Value>, key: &str, values: Vec<String>) {
    let target = object
        .entry(key)
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("sing-box route address field is an array");
    for value in values {
        if !target.iter().any(|current| current == &value) {
            target.push(json!(value));
        }
    }
}

fn upstream_prefixes(system_dns: &crate::SystemDnsPlan) -> Result<Vec<String>, TransparentError> {
    system_dns
        .original_upstreams
        .iter()
        .map(|value| {
            value
                .parse::<IpAddr>()
                .map(|address| match address {
                    IpAddr::V4(_) => format!("{address}/32"),
                    IpAddr::V6(_) => format!("{address}/128"),
                })
                .map_err(|_| {
                    TransparentError::Invalid(format!(
                        "desktop original DNS upstream {value:?} is not an IP address"
                    ))
                })
        })
        .collect()
}

fn insert_original_dns_bypass(config: &mut Value, plan: &Plan) -> Result<(), TransparentError> {
    let system_dns = plan
        .system_dns
        .as_ref()
        .expect("desktop DNS plan has system DNS");
    let prefixes = upstream_prefixes(system_dns)?;
    let rules = config
        .pointer_mut("/route/rules")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            TransparentError::Invalid("runtime configuration has no route rules".into())
        })?;
    rules.insert(
        0,
        json!({ "ip_cidr": prefixes, "port": [53], "action": "route", "outbound": "direct" }),
    );
    Ok(())
}

#[cfg(test)]
#[path = "desktop_plan_tests.rs"]
mod tests;
