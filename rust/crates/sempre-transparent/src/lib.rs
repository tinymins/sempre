mod cn_routes;
mod command;
mod desktop_plan;
mod document;
mod host;
mod intranet;
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

pub use cn_routes::{CnRouteIntent, CnRouteProgress, normalize_cn_prefixes};
pub use host::Controller;

pub const ROUTE_MARK: u32 = 0x5350_0001;
pub const BYPASS_MARK: u32 = 0x5350_0002;
pub const ROUTE_TABLE: u32 = 20_240;
pub const RULE_PRIORITY: u32 = 20_240;
pub const POLICY_PROTOCOL: u8 = 0xfd;
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Plan {
    pub core: String,
    pub cn_routes: Option<CnRouteIntent>,
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
        self.enabled() || self.system_dns.is_some() || self.cn_routes.is_some()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemDnsPlan {
    pub listen_port: u16,
    pub listen_hosts: Vec<String>,
    pub core_listen_port: u16,
    pub original_upstreams: Vec<String>,
    pub managed_frontend: bool,
    pub takeover_host: bool,
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
    prepare_with_upstreams(core, profile, runtime_config, inventory, &[])
}

pub(crate) fn prepare_with_upstreams(
    core: &str,
    profile: &Profile,
    runtime_config: &Path,
    inventory: &Inventory,
    upstreams: &[String],
) -> Result<Plan, TransparentError> {
    let mode = effective_mode(core, &profile.transparent_proxy.mode)?;
    let system_dns = system_dns_intent(profile);

    if mode == Mode::Disabled && system_dns.is_none() {
        return Ok(Plan::default());
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
        Mode::Tun => document::prepare_tun(&mut plan, profile, inventory, &mut config, upstreams)?,
        Mode::TProxy => {
            document::prepare_tproxy(&mut plan, profile, inventory, &mut config, upstreams)?;
        }
        Mode::Disabled => {}
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
    let managed_frontend = shared.get("managedDnsFrontend").and_then(Value::as_bool) == Some(true);
    let takeover_host = shared
        .get("systemDnsTakeoverHost")
        .and_then(Value::as_bool)
        .unwrap_or(true);
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
                core_listen_port: if managed_frontend {
                    sempre_converter::DEFAULT_CORE_DNS_PORT
                } else {
                    listen_port
                },
                original_upstreams: Vec::new(),
                managed_frontend,
                takeover_host,
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
        core_listen_port: if managed_frontend {
            sempre_converter::DEFAULT_CORE_DNS_PORT
        } else {
            listen_port
        },
        original_upstreams: Vec::new(),
        managed_frontend,
        takeover_host,
    })
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
