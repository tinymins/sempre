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
}

pub(crate) fn prepare(
    platform: Platform,
    core: &str,
    core_version: &str,
    profile: &Profile,
    runtime_config: &Path,
    original_upstreams: Vec<String>,
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
    let plan = Plan {
        core: core.into(),
        system_dns: Some(system_dns),
        ..Plan::default()
    };
    crate::validate_system_dns_config(&plan, &config)?;
    match platform {
        Platform::Macos => insert_original_dns_bypass(&mut config, &plan)?,
        Platform::Windows => configure_windows_dns_redirect(&mut config, &plan, core_version)?,
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
        0 => 1053,
        port => port,
    };
    if platform == Platform::Windows {
        system_dns.listen_port = WINDOWS_FRONTEND_PORT;
    }
    system_dns.original_upstreams = original_upstreams;
    Some(system_dns)
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
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn managed_frontend_plan_does_not_depend_on_core_runtime_state() {
        let mut profile = Profile {
            dns: json!({ "shared": {
                "systemDnsTakeoverEnabled": true,
                "managedDnsFrontend": true,
                "systemDnsListenPort": 53,
                "systemDnsListenHosts": ["127.0.0.1"]
            }}),
            ..Profile::default()
        };
        profile.transparent_proxy.tproxy.dns_listen_port = 2053;

        let plan = managed_frontend_plan(
            Platform::Macos,
            "sing-box",
            &profile,
            vec!["223.5.5.5".into()],
        )
        .expect("frontend plan");

        assert_eq!(plan.listen_port, 53);
        assert_eq!(plan.core_listen_port, 2053);
        assert_eq!(plan.original_upstreams, ["223.5.5.5"]);
    }

    #[test]
    fn writes_original_dns_bypass_before_global_hijack() {
        let root = tempfile::tempdir().expect("temporary directory");
        let config = root.path().join("config.json");
        fs::write(
            &config,
            serde_json::to_vec(&json!({
                "inbounds": [{
                    "type": "direct", "tag": "sempre-dns-core-in", "listen": "127.0.0.1",
                    "listen_port": 1053, "override_address": "1.1.1.1", "override_port": 53
                }],
                "route": { "rules": [
                    { "inbound": "sempre-dns-core-in", "action": "sniff" },
                    { "inbound": "sempre-dns-core-in", "protocol": "dns", "action": "hijack-dns" },
                    { "protocol": "dns", "action": "hijack-dns" }
                ] }
            }))
            .expect("encode config"),
        )
        .expect("write config");
        let profile: Profile = serde_json::from_value(json!({
            "dns": { "shared": { "systemDnsTakeoverEnabled": true } }
        }))
        .expect("profile");
        let plan = prepare(
            Platform::Macos,
            "sing-box",
            "1.13.18",
            &profile,
            &config,
            vec!["223.6.6.6".into(), "2400:3200::1".into()],
        )
        .expect("plan");
        assert_eq!(plan.system_dns.expect("system DNS").core_listen_port, 1053);
        let output: Value =
            serde_json::from_slice(&fs::read(config).expect("read config")).expect("decode config");
        assert_eq!(
            output["route"]["rules"][0],
            json!({
                "ip_cidr": ["223.6.6.6/32", "2400:3200::1/128"],
                "port": [53], "action": "route", "outbound": "direct"
            })
        );
    }

    #[test]
    fn windows_redirects_dns_to_non_privileged_frontend_and_routes_only_fake_ip() {
        let root = tempfile::tempdir().expect("temporary directory");
        let config = root.path().join("config.json");
        fs::write(
            &config,
            serde_json::to_vec(&json!({
                "dns": { "servers": [{
                    "type": "fakeip", "tag": "fakeip",
                    "inet4_range": "198.18.0.0/15", "inet6_range": "fc00::/18"
                }] },
                "inbounds": [
                    { "type": "tun", "tag": "tun-in", "auto_route": true },
                    {
                        "type": "direct", "tag": "sempre-dns-core-in", "listen": "127.0.0.1",
                        "listen_port": 1053, "override_address": "1.1.1.1", "override_port": 53
                    }
                ],
                "route": { "rules": [
                    { "inbound": "sempre-dns-core-in", "action": "sniff" },
                    { "inbound": "sempre-dns-core-in", "protocol": "dns", "action": "hijack-dns" },
                    { "protocol": "dns", "action": "hijack-dns" }
                ] }
            }))
            .expect("encode config"),
        )
        .expect("write config");
        let profile: Profile = serde_json::from_value(json!({
            "dns": { "shared": { "systemDnsTakeoverEnabled": true } }
        }))
        .expect("profile");
        let plan = prepare(
            Platform::Windows,
            "sing-box",
            "1.14.0-beta.13",
            &profile,
            &config,
            vec!["223.6.6.6".into()],
        )
        .expect("plan");
        assert_eq!(plan.system_dns.expect("system DNS").listen_port, 1054);
        let output: Value =
            serde_json::from_slice(&fs::read(config).expect("read config")).expect("decode config");
        assert_eq!(
            output["inbounds"][0]["route_exclude_address"],
            json!(["223.6.6.6/32"])
        );
        assert_eq!(output["inbounds"][0]["strict_route"], json!(false));
        assert_eq!(output["inbounds"][0]["dns_mode"], json!("native"));
        assert_eq!(output["inbounds"][0]["dns_address"], json!(["192.0.2.1"]));
        assert_eq!(
            output["inbounds"][0]["route_address"],
            json!(["198.18.0.0/15", "fc00::/18", "192.0.2.1/32"])
        );
        assert_eq!(
            output["route"]["rules"][0],
            json!({
                "inbound": "tun-in", "network": ["tcp", "udp"], "port": [53],
                "action": "route", "outbound": "direct",
                "override_address": "127.0.0.1", "override_port": 1054,
                "udp_connect": true
            })
        );
    }

    #[test]
    fn windows_real_ip_keeps_full_tun_routing() {
        let mut config = json!({
            "inbounds": [{
                "type": "tun", "tag": "tun-in", "auto_route": true,
                "route_address": ["198.18.0.0/15"]
            }],
            "route": { "rules": [{ "protocol": "dns", "action": "hijack-dns" }] }
        });
        let plan = Plan {
            core: "sing-box".into(),
            system_dns: Some(crate::SystemDnsPlan {
                listen_port: 1054,
                listen_hosts: vec!["127.0.0.1".into()],
                core_listen_port: 1053,
                original_upstreams: vec!["223.6.6.6".into()],
                managed_frontend: true,
                takeover_host: true,
            }),
            ..Plan::default()
        };
        configure_windows_dns_redirect(&mut config, &plan, "1.14.0-beta.13").expect("redirect");
        assert!(config["inbounds"][0]["route_address"].is_null());
        assert_eq!(config["inbounds"][0]["strict_route"], json!(false));
        assert_eq!(
            config["inbounds"][0]["route_exclude_address"],
            json!(["223.6.6.6/32"])
        );
    }

    #[test]
    fn windows_rejects_core_without_configurable_tun_dns() {
        let mut config = json!({
            "inbounds": [{ "type": "tun", "tag": "tun-in" }],
            "route": { "rules": [] }
        });
        let plan = Plan {
            core: "sing-box".into(),
            system_dns: Some(crate::SystemDnsPlan {
                listen_port: 1054,
                listen_hosts: vec!["127.0.0.1".into()],
                core_listen_port: 1053,
                original_upstreams: vec!["223.6.6.6".into()],
                managed_frontend: true,
                takeover_host: true,
            }),
            ..Plan::default()
        };
        let error = configure_windows_dns_redirect(&mut config, &plan, "1.13.18")
            .expect_err("sing-box 1.13 must be rejected");
        assert!(error.to_string().contains("requires sing-box 1.14"));
    }
}
