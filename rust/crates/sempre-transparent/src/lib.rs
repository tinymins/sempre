mod command;
mod desktop_plan;
mod document;
mod host;
mod macos_dns;
mod nft;
mod policy;
mod prefix;
mod system_dns;
mod windows_dns;

use std::{fs, io, path::Path};

use sempre_converter::Profile;
use sempre_network::Inventory;
use serde_json::Value;
use thiserror::Error;

pub use host::Controller;

pub const ROUTE_MARK: u32 = 0x5350_0001;
pub const BYPASS_MARK: u32 = 0x5350_0002;
pub const ROUTE_TABLE: u32 = 20_240;
pub const RULE_PRIORITY: u32 = 20_240;
pub const POLICY_PROTOCOL: u8 = 0xfd;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Plan {
    pub core: String,
    pub mode: Mode,
    pub tun_interface: String,
    pub tun_address: String,
    pub route_exclusions: Vec<String>,
    pub tproxy_port: u16,
    pub dns_port: u16,
    pub capture_host: bool,
    pub lan_interfaces: Vec<String>,
    pub excluded_prefixes: Vec<String>,
    pub fake_ip_prefixes: Vec<String>,
    pub system_dns: Option<SystemDnsPlan>,
}

impl Plan {
    pub const fn enabled(&self) -> bool {
        matches!(self.mode, Mode::Tun | Mode::TProxy)
    }

    pub const fn active(&self) -> bool {
        self.enabled() || self.system_dns.is_some()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemDnsPlan {
    pub listen_port: u16,
    pub listen_hosts: Vec<String>,
    pub core_listen_port: u16,
    pub original_upstreams: Vec<String>,
    pub managed_frontend: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Mode {
    Tun,
    TProxy,
    #[default]
    Disabled,
}

#[derive(Debug, Error)]
pub enum TransparentError {
    #[error("inspect Linux network inventory: {0}")]
    Network(#[from] sempre_network::NetworkError),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
    #[error("decode {core} runtime configuration: {detail}")]
    Decode { core: String, detail: String },
    #[error("encode {core} runtime configuration: {detail}")]
    Encode { core: String, detail: String },
    #[error("{0}")]
    Invalid(String),
    #[error("start host command {program}: {source}")]
    CommandStart {
        program: String,
        #[source]
        source: io::Error,
    },
    #[error("host command {program} timed out")]
    CommandTimeout { program: String },
    #[error("host command {program} failed: {detail}")]
    CommandFailed { program: String, detail: String },
}

pub fn prepare(
    core: &str,
    profile: &Profile,
    runtime_config: &Path,
) -> Result<Plan, TransparentError> {
    if !cfg!(target_os = "linux") || !supported_core(core) {
        return Ok(Plan::default());
    }
    let inventory = sempre_network::inventory()?;
    prepare_with_inventory_authorized(core, profile, runtime_config, &inventory, false)
}

pub fn prepare_with_inventory(
    core: &str,
    profile: &Profile,
    runtime_config: &Path,
    inventory: &Inventory,
) -> Result<Plan, TransparentError> {
    prepare_with_inventory_authorized(core, profile, runtime_config, inventory, false)
}

pub(crate) fn prepare_with_inventory_authorized(
    core: &str,
    profile: &Profile,
    runtime_config: &Path,
    inventory: &Inventory,
    system_dns_allowed: bool,
) -> Result<Plan, TransparentError> {
    let mode = effective_mode(core, &profile.transparent_proxy.mode)?;
    let system_dns = system_dns_intent(profile);
    if system_dns.is_some() && (core != "sing-box" || !system_dns_allowed) {
        return Err(TransparentError::Invalid(
            "system DNS takeover is only available for Linux system sing-box runtime".into(),
        ));
    }
    if mode == Mode::Disabled && system_dns.is_none() {
        return Ok(Plan::default());
    }
    if mode != Mode::Disabled {
        validate_profile(profile)?;
    }
    let data = fs::read(runtime_config).map_err(|source| TransparentError::Io {
        context: "read transparent proxy runtime configuration".into(),
        source,
    })?;
    let mut config = decode(core, &data)?;
    let mut plan = Plan {
        core: core.into(),
        mode,
        system_dns,
        ..Plan::default()
    };
    match mode {
        Mode::Tun => document::prepare_tun(&mut plan, profile, inventory, &mut config)?,
        Mode::TProxy => document::prepare_tproxy(&mut plan, profile, inventory, &mut config)?,
        Mode::Disabled => {}
    }
    validate_system_dns_config(&plan, &config)?;
    let encoded = encode(core, &config)?;
    sempre_state::write_atomic(runtime_config, &encoded, 0o600).map_err(|source| {
        TransparentError::Io {
            context: "write resolved transparent proxy runtime configuration".into(),
            source,
        }
    })?;
    Ok(plan)
}

pub(crate) fn system_dns_intent(profile: &Profile) -> Option<SystemDnsPlan> {
    let shared = profile.dns.get("shared").unwrap_or(&profile.dns);
    if shared
        .get("systemDnsTakeoverEnabled")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return None;
    }
    let listen_port = shared
        .get("systemDnsListenPort")
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
        .unwrap_or(53);
    let mut listen_hosts = Vec::new();
    for host in shared
        .get("systemDnsListenHosts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|host| host.trim().parse::<std::net::Ipv4Addr>().ok())
        .map(|host| host.to_string())
    {
        if host == "0.0.0.0" {
            return Some(SystemDnsPlan {
                listen_port,
                listen_hosts: vec![host],
                core_listen_port: listen_port,
                original_upstreams: Vec::new(),
                managed_frontend: false,
            });
        }
        if !listen_hosts.contains(&host) {
            listen_hosts.push(host);
        }
    }
    if listen_hosts.is_empty() {
        listen_hosts.push("127.0.0.1".into());
    }
    Some(SystemDnsPlan {
        listen_port,
        listen_hosts,
        core_listen_port: listen_port,
        original_upstreams: Vec::new(),
        managed_frontend: false,
    })
}

pub(crate) fn validate_system_dns_config(
    plan: &Plan,
    config: &Value,
) -> Result<(), TransparentError> {
    let Some(system_dns) = &plan.system_dns else {
        return Ok(());
    };
    let inbounds = config
        .get("inbounds")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            TransparentError::Invalid("runtime configuration has no system DNS inbounds".into())
        })?;
    let rules = config
        .pointer("/route/rules")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            TransparentError::Invalid("runtime configuration has no system DNS route rules".into())
        })?;
    let listeners = if system_dns.managed_frontend {
        vec![(
            "sempre-dns-core-in".into(),
            "127.0.0.1".into(),
            system_dns.core_listen_port,
        )]
    } else {
        system_dns
            .listen_hosts
            .iter()
            .enumerate()
            .map(|(index, host)| {
                let tag = match host.as_str() {
                    "127.0.0.1" => "system-dns-in".into(),
                    "0.0.0.0" => "system-dns-in-any".into(),
                    _ => format!("system-dns-in-{index}"),
                };
                (tag, host.clone(), system_dns.listen_port)
            })
            .collect()
    };
    for (tag, host, port) in listeners {
        let valid_inbound = inbounds.iter().any(|inbound| {
            inbound["type"] == "direct"
                && inbound["tag"] == tag
                && inbound["listen"] == host
                && inbound["listen_port"] == port
                && inbound["override_address"] == "1.1.1.1"
                && inbound["override_port"] == 53
        });
        let valid_rules = rules.windows(2).any(|pair| {
            pair[0]["inbound"] == tag
                && pair[0]["action"] == "sniff"
                && pair[1]["inbound"] == tag
                && pair[1]["protocol"] == "dns"
                && pair[1]["action"] == "hijack-dns"
        });
        if !valid_inbound || !valid_rules {
            return Err(TransparentError::Invalid(format!(
                "runtime configuration is missing managed system DNS listener {host}:{port}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn decode(core: &str, data: &[u8]) -> Result<Value, TransparentError> {
    if matches!(core, "mihomo" | "clash-rs") {
        serde_yaml::from_slice(data).map_err(|error| TransparentError::Decode {
            core: core.into(),
            detail: error.to_string(),
        })
    } else {
        serde_json::from_slice(data).map_err(|error| TransparentError::Decode {
            core: core.into(),
            detail: error.to_string(),
        })
    }
}

fn encode(core: &str, config: &Value) -> Result<Vec<u8>, TransparentError> {
    if matches!(core, "mihomo" | "clash-rs") {
        serde_yaml::to_string(config)
            .map(String::into_bytes)
            .map_err(|error| TransparentError::Encode {
                core: core.into(),
                detail: error.to_string(),
            })
    } else {
        serde_json::to_vec_pretty(config)
            .map(|mut data| {
                data.push(b'\n');
                data
            })
            .map_err(|error| TransparentError::Encode {
                core: core.into(),
                detail: error.to_string(),
            })
    }
}

fn validate_profile(profile: &Profile) -> Result<(), TransparentError> {
    let transparent = &profile.transparent_proxy;
    let name = transparent.tun.interface_name.trim();
    if name.is_empty() || name.len() > 15 {
        return Err(TransparentError::Invalid(
            "TUN interface name must contain 1 to 15 characters".into(),
        ));
    }
    if !matches!(
        transparent.interface_mode.as_str(),
        "all" | "include" | "exclude"
    ) {
        return Err(TransparentError::Invalid(
            "TUN interface mode must be all, include, or exclude".into(),
        ));
    }
    if transparent.interface_mode != "all" && transparent.interfaces.is_empty() {
        return Err(TransparentError::Invalid(format!(
            "transparent interface mode {} requires at least one interface",
            transparent.interface_mode
        )));
    }
    validate_interfaces(&transparent.interfaces, "TUN")?;
    validate_interfaces(&transparent.lan_interfaces, "TProxy LAN")?;
    for value in &transparent.route_exclusions {
        value.parse::<ipnet::IpNet>().map_err(|_| {
            TransparentError::Invalid(format!("invalid TUN route exclusion {value:?}"))
        })?;
    }
    if transparent.tproxy.listen_port == 0 || transparent.tproxy.dns_listen_port == 0 {
        return Err(TransparentError::Invalid(
            "transparent proxy ports must be between 1 and 65535".into(),
        ));
    }
    if transparent.mode == "tproxy"
        && [
            profile.local_proxy.socks_port,
            profile.local_proxy.http_port,
        ]
        .into_iter()
        .any(|port| {
            port == transparent.tproxy.listen_port || port == transparent.tproxy.dns_listen_port
        })
    {
        return Err(TransparentError::Invalid(
            "local proxy ports must not conflict with transparent proxy ports".into(),
        ));
    }
    Ok(())
}

fn validate_interfaces(values: &[String], label: &str) -> Result<(), TransparentError> {
    let mut unique = std::collections::HashSet::new();
    for value in values {
        let name = value.trim();
        if name.is_empty() || name.len() > 15 || !unique.insert(name) {
            return Err(TransparentError::Invalid(format!(
                "{label} interfaces must be unique valid interface names"
            )));
        }
    }
    Ok(())
}

fn supported_core(core: &str) -> bool {
    matches!(core, "sing-box" | "mihomo" | "clash-rs" | "xray" | "v2ray")
}

fn effective_mode(core: &str, mode: &str) -> Result<Mode, TransparentError> {
    match mode {
        "tun-router" if core != "v2ray" => Ok(Mode::Tun),
        "tun-router" | "disabled" | "ebpf-router" | "" => Ok(Mode::Disabled),
        "tproxy" => Ok(Mode::TProxy),
        _ => Err(TransparentError::Invalid(format!(
            "unsupported transparent proxy mode {mode:?}"
        ))),
    }
}

#[cfg(test)]
mod system_plan_tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    fn profile() -> Profile {
        serde_json::from_value(json!({
            "name": "system DNS",
            "transparent_proxy": { "mode": "disabled" },
            "dns": { "shared": {
                "systemDnsTakeoverEnabled": true,
                "systemDnsListenPort": 53,
                "systemDnsListenHosts": ["127.0.0.1"]
            }}
        }))
        .expect("profile")
    }

    fn runtime() -> Value {
        json!({
            "inbounds": [{
                "type": "direct", "tag": "system-dns-in", "listen": "127.0.0.1",
                "listen_port": 53, "override_address": "1.1.1.1", "override_port": 53
            }],
            "route": { "rules": [
                { "inbound": "system-dns-in", "action": "sniff" },
                { "inbound": "system-dns-in", "protocol": "dns", "action": "hijack-dns" }
            ] }
        })
    }

    #[test]
    fn system_dns_only_plan_requires_authorization_and_managed_runtime() {
        let root = tempfile::tempdir().expect("directory");
        let config = root.path().join("config.json");
        fs::write(&config, serde_json::to_vec(&runtime()).expect("JSON")).expect("config");
        let denied = prepare_with_inventory_authorized(
            "sing-box",
            &profile(),
            &config,
            &Inventory::default(),
            false,
        )
        .expect_err("portable takeover must fail");
        assert!(denied.to_string().contains("Linux system sing-box"));

        let plan = prepare_with_inventory_authorized(
            "sing-box",
            &profile(),
            &config,
            &Inventory::default(),
            true,
        )
        .expect("system DNS plan");
        assert!(plan.active() && !plan.enabled());
        assert_eq!(
            plan.system_dns,
            Some(SystemDnsPlan {
                listen_port: 53,
                listen_hosts: vec!["127.0.0.1".into()],
                core_listen_port: 53,
                original_upstreams: Vec::new(),
                managed_frontend: false,
            })
        );

        let mut invalid = runtime();
        invalid["route"]["rules"] = json!([]);
        fs::write(&config, serde_json::to_vec(&invalid).expect("JSON")).expect("invalid config");
        assert!(
            prepare_with_inventory_authorized(
                "sing-box",
                &profile(),
                &config,
                &Inventory::default(),
                true,
            )
            .expect_err("missing route rules must fail")
            .to_string()
            .contains("managed system DNS listener")
        );
    }
}
