mod command;
mod document;
mod host;
mod nft;
mod policy;
mod prefix;

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
}

impl Plan {
    pub const fn enabled(&self) -> bool {
        matches!(self.mode, Mode::Tun | Mode::TProxy)
    }
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
    prepare_with_inventory(core, profile, runtime_config, &inventory)
}

pub fn prepare_with_inventory(
    core: &str,
    profile: &Profile,
    runtime_config: &Path,
    inventory: &Inventory,
) -> Result<Plan, TransparentError> {
    let mode = effective_mode(core, &profile.transparent_proxy.mode)?;
    if mode == Mode::Disabled {
        return Ok(Plan::default());
    }
    validate_profile(profile)?;
    let data = fs::read(runtime_config).map_err(|source| TransparentError::Io {
        context: "read transparent proxy runtime configuration".into(),
        source,
    })?;
    let mut config = decode(core, &data)?;
    let mut plan = Plan {
        core: core.into(),
        mode,
        ..Plan::default()
    };
    match mode {
        Mode::Tun => document::prepare_tun(&mut plan, profile, inventory, &mut config)?,
        Mode::TProxy => document::prepare_tproxy(&mut plan, profile, inventory, &mut config)?,
        Mode::Disabled => unreachable!("disabled returned before decoding"),
    }
    let encoded = encode(core, &config)?;
    sempre_state::write_atomic(runtime_config, &encoded, 0o600).map_err(|source| {
        TransparentError::Io {
            context: "write resolved transparent proxy runtime configuration".into(),
            source,
        }
    })?;
    Ok(plan)
}

fn decode(core: &str, data: &[u8]) -> Result<Value, TransparentError> {
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
