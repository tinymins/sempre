//! `OpenWrt` gateway capture. Rules live independently of the core process, so a
//! core restart or reload never lets LAN traffic and DNS fall through.
use std::{fmt::Write as _, fs, io, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::TransparentError;

mod discovery;
mod geoip;
mod host;
#[cfg(test)]
mod tests;

pub use geoip::parse_cidrs;
pub use host::CaptureController;

pub(crate) const TABLE: &str = "sempre_capture";
pub(crate) const MARK: &str = "0x2333";
pub(crate) const ROUTE_TABLE: u32 = 100;
pub(crate) const RULE_PRIORITY: u32 = 10_010;
const RESERVED: &[&str] = &[
    "0.0.0.0/8",
    "10.0.0.0/8",
    "127.0.0.0/8",
    "100.64.0.0/10",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "224.0.0.0/4",
    "240.0.0.0/4",
];
const DEFAULT_GEOIP_URL: &str =
    "https://raw.githubusercontent.com/17mon/china_ip_list/master/china_ip_list.txt";

/// Host choices the configuration cannot express. Ports come from the configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct CaptureSettings {
    pub enabled: bool,
    pub front_dns_port: Option<u16>,
    pub lan_interfaces: Vec<String>,
    pub bypass_macs: Vec<String>,
    pub software_offload: bool,
    pub offload_wan_interfaces: Vec<String>,
    pub geoip_update_url: String,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            front_dns_port: None,
            lan_interfaces: Vec::new(),
            bypass_macs: Vec::new(),
            software_offload: true,
            offload_wan_interfaces: Vec::new(),
            geoip_update_url: DEFAULT_GEOIP_URL.into(),
        }
    }
}

impl CaptureSettings {
    pub fn load(path: &Path) -> Result<Self, TransparentError> {
        match fs::read(path) {
            Ok(data) => serde_json::from_slice(&data).map_err(|error| {
                TransparentError::Invalid(format!("decode {}: {error}", path.display()))
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(TransparentError::Io {
                context: format!("read {}", path.display()),
                source,
            }),
        }
    }
}

/// What the runtime plan asks the coordinator to hold in place.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureIntent {
    pub settings: CaptureSettings,
    pub tproxy_port: u16,
    pub dns_port: u16,
}

impl CaptureIntent {
    /// The configuration is the only source of ports; a mismatch must not reach nft.
    pub fn from_config(
        settings: CaptureSettings,
        config: &Value,
    ) -> Result<Self, TransparentError> {
        let dns_port = settings
            .front_dns_port
            .map_or_else(|| inbound_port(config, "dns-in"), Ok)?;
        Ok(Self {
            tproxy_port: inbound_port(config, "tproxy-in")?,
            dns_port,
            settings,
        })
    }
}

fn inbound_port(config: &Value, tag: &str) -> Result<u16, TransparentError> {
    config["inbounds"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|inbound| inbound["tag"] == tag)
        .and_then(|inbound| inbound["listen_port"].as_u64())
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port != 0)
        .ok_or_else(|| {
            TransparentError::Invalid(format!(
                "gateway capture needs a {tag} inbound with a listen port"
            ))
        })
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Offload {
    pub devices: Vec<String>,
    pub lan: Vec<String>,
    pub wan: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Ruleset {
    pub tproxy_port: u16,
    pub dns_port: u16,
    pub lan_interfaces: Vec<String>,
    pub local_nets: Vec<String>,
    pub gateways: Vec<String>,
    pub bypass_macs: Vec<String>,
    pub cn_prefixes: Vec<String>,
    pub offload: Option<Offload>,
}

/// One transaction: readers see the old table or the new one, never a gap.
pub(crate) fn script(rules: &Ruleset) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "add table inet {TABLE}");
    let _ = writeln!(out, "delete table inet {TABLE}");
    let _ = writeln!(out, "table inet {TABLE} {{");
    if let Some(offload) = &rules.offload {
        offload_objects(&mut out, offload);
    }
    interval_set(&mut out, "cn_ip", &rules.cn_prefixes);
    interval_set(&mut out, "local_nets", &rules.local_nets);
    let reserved = RESERVED.iter().map(ToString::to_string).collect::<Vec<_>>();
    interval_set(&mut out, "reserved", &reserved);
    plain_set(&mut out, "gateways", "ipv4_addr", &rules.gateways, false);
    plain_set(
        &mut out,
        "bypass_macs",
        "ether_addr",
        &rules.bypass_macs,
        false,
    );
    plain_set(
        &mut out,
        "lan_devices",
        "ifname",
        &rules.lan_interfaces,
        true,
    );
    let _ = writeln!(
        out,
        "chain proxy {{ type filter hook prerouting priority -160; policy accept;"
    );
    out.push_str("meta nfproto != ipv4 return\n");
    out.push_str("udp dport 53 counter return\n");
    out.push_str("ip daddr @local_nets counter return\n");
    out.push_str("ip daddr @reserved counter return\n");
    out.push_str("ip daddr @cn_ip counter return\n");
    out.push_str("ether saddr @bypass_macs counter return\n");
    let _ = writeln!(
        out,
        "iifname @lan_devices meta l4proto {{ tcp, udp }} counter tproxy ip to :{} meta mark set {MARK} accept",
        rules.tproxy_port
    );
    out.push_str("}\n");
    let _ = writeln!(
        out,
        "chain dns {{ type nat hook prerouting priority -105; policy accept;"
    );
    out.push_str("ether saddr @bypass_macs counter return\n");
    let _ = writeln!(
        out,
        "ip daddr @gateways meta l4proto {{ tcp, udp }} th dport 53 counter redirect to :{}",
        rules.dns_port
    );
    let _ = writeln!(
        out,
        "meta nfproto ipv6 iifname @lan_devices meta l4proto {{ tcp, udp }} th dport 53 counter redirect to :{}",
        rules.dns_port
    );
    out.push_str("}\n");
    out.push_str("}\n");
    out
}

fn interval_set(out: &mut String, name: &str, values: &[String]) {
    let elements = if values.is_empty() {
        String::new()
    } else {
        format!(" elements = {{ {} }};", values.join(", "))
    };
    let _ = writeln!(
        out,
        "set {name} {{ type ipv4_addr; flags interval; auto-merge;{elements} }}"
    );
}

fn plain_set(out: &mut String, name: &str, kind: &str, values: &[String], quote: bool) {
    let elements = if values.is_empty() {
        String::new()
    } else {
        let joined = values
            .iter()
            .map(|value| {
                if quote {
                    format!("\"{value}\"")
                } else {
                    value.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(" elements = {{ {joined} }};")
    };
    let _ = writeln!(out, "set {name} {{ type {kind};{elements} }}");
}

fn names(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("\"{value}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Runs after the fw4 forward filter. Only established LAN to WAN TCP that bypasses
/// the proxy can enter, so DNS, DNAT, marked, VPN and proxied flows keep their path.
fn offload_objects(out: &mut String, offload: &Offload) {
    let _ = writeln!(
        out,
        "flowtable direct {{ hook ingress priority 0; devices = {{ {} }}; counter; }}",
        names(&offload.devices)
    );
    let _ = writeln!(
        out,
        "set offload_lan {{ type ifname; elements = {{ {} }}; }}",
        names(&offload.lan)
    );
    let _ = writeln!(
        out,
        "set offload_wan {{ type ifname; elements = {{ {} }}; }}",
        names(&offload.wan)
    );
    out.push_str("chain accelerate { type filter hook forward priority 10; policy accept;\n");
    for rule in [
        "meta nfproto != ipv4 return",
        "meta l4proto != tcp return",
        "ct state != established return",
        "ct direction != original return",
        "ct status dnat return",
        "meta mark != 0 return",
        "ct mark != 0 return",
        "tcp dport 53 return",
        "iifname != @offload_lan return",
        "oifname != @offload_wan return",
        "ip saddr != @local_nets return",
        "ip daddr @local_nets return",
        "ip daddr @reserved counter flow add @direct",
        "ip daddr @cn_ip counter flow add @direct",
        "ether saddr @bypass_macs counter flow add @direct",
    ] {
        out.push_str(rule);
        out.push('\n');
    }
    out.push_str("}\n");
}
