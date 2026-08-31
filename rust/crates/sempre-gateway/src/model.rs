use std::net::Ipv4Addr;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::GatewayError;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct Config {
    pub schema: u32,
    pub topology: String,
    pub lan: LanConfig,
    pub dhcp: DhcpConfig,
    pub dns: DnsConfig,
    pub pve: PveConfig,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct LanConfig {
    pub interface: String,
    pub gateway_cidr: String,
    pub wan_interface: String,
    pub nat_enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct DhcpConfig {
    pub enabled: bool,
    pub range_start: String,
    pub range_end: String,
    pub lease_time: String,
    pub domain: String,
    pub reservations: Vec<DhcpReservation>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct DhcpReservation {
    pub mac: String,
    pub ip: String,
    pub hostname: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct DnsConfig {
    pub enabled: bool,
    pub listen_hosts: Vec<String>,
    pub listen_port: u16,
    pub local_upstreams: Vec<String>,
    pub remote_upstream: String,
    pub strategy: String,
    pub reject_https: bool,
    pub rule_sets: Vec<DnsRuleSet>,
    pub domestic_cidrs: Vec<String>,
    pub cache_ttl_seconds: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct DnsRuleSet {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub rules: Vec<String>,
    pub upstream: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct PveConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub key_path: String,
    pub fingerprint: String,
    pub apply_persistent: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RuntimeStatus {
    pub dns_running: bool,
    pub dhcp_running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub dhcp_leases: Vec<LeaseView>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub last_error: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct LeaseView {
    pub mac: String,
    pub ip: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hostname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub reserved: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub config: Config,
    pub runtime: RuntimeStatus,
    pub inventory: sempre_network::Inventory,
    pub validation_errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transparent_proxy: Option<Value>,
    pub host_plan_available: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            topology: "local-pve".into(),
            lan: LanConfig {
                gateway_cidr: "10.10.10.1/24".into(),
                ..LanConfig::default()
            },
            dhcp: DhcpConfig::default(),
            dns: DnsConfig::default(),
            pve: PveConfig::default(),
        }
    }
}

impl Default for DhcpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            range_start: "10.10.10.100".into(),
            range_end: "10.10.10.200".into(),
            lease_time: "12h".into(),
            domain: String::new(),
            reservations: Vec::new(),
        }
    }
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_hosts: vec!["10.10.10.1".into()],
            listen_port: 53,
            local_upstreams: vec!["223.5.5.5:53".into(), "223.6.6.6:53".into()],
            remote_upstream: "127.0.0.1:1053".into(),
            strategy: "local-first-classify".into(),
            reject_https: true,
            rule_sets: Vec::new(),
            domestic_cidrs: vec![
                "10.0.0.0/8".into(),
                "100.64.0.0/10".into(),
                "127.0.0.0/8".into(),
                "172.16.0.0/12".into(),
                "192.168.0.0/16".into(),
            ],
            cache_ttl_seconds: 300,
        }
    }
}

impl Default for PveConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 22,
            user: "root".into(),
            key_path: String::new(),
            fingerprint: String::new(),
            apply_persistent: false,
        }
    }
}

impl Config {
    pub fn normalize(&mut self) {
        let defaults = Self::default();
        if self.topology.is_empty() {
            self.topology = defaults.topology;
        }
        if self.lan.gateway_cidr.is_empty() {
            self.lan.gateway_cidr = defaults.lan.gateway_cidr;
        }
        if self.dhcp.range_start.is_empty() {
            self.dhcp.range_start = defaults.dhcp.range_start;
        }
        if self.dhcp.range_end.is_empty() {
            self.dhcp.range_end = defaults.dhcp.range_end;
        }
        if self.dhcp.lease_time.is_empty() {
            self.dhcp.lease_time = defaults.dhcp.lease_time;
        }
        if self.dns.listen_hosts.is_empty() {
            self.dns.listen_hosts = defaults.dns.listen_hosts;
        }
        if self.dns.listen_port == 0 {
            self.dns.listen_port = defaults.dns.listen_port;
        }
        if self.dns.local_upstreams.is_empty() {
            self.dns.local_upstreams = defaults.dns.local_upstreams;
        }
        if self.dns.remote_upstream.is_empty() {
            self.dns.remote_upstream = defaults.dns.remote_upstream;
        }
        if self.dns.strategy.is_empty() {
            self.dns.strategy = defaults.dns.strategy;
        }
        if self.dns.cache_ttl_seconds == 0 {
            self.dns.cache_ttl_seconds = defaults.dns.cache_ttl_seconds;
        }
        if self.pve.port == 0 {
            self.pve.port = defaults.pve.port;
        }
        if self.pve.user.is_empty() {
            self.pve.user = defaults.pve.user;
        }
    }

    pub fn validate(&self) -> Result<(), GatewayError> {
        let errors = validation_messages(self);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(GatewayError::invalid(errors.join("; ")))
        }
    }
}

pub fn validation_messages(config: &Config) -> Vec<String> {
    let mut errors = Vec::new();
    if config.schema != SCHEMA_VERSION {
        errors.push(format!("unsupported gateway schema {}", config.schema));
    }
    if !matches!(config.topology.as_str(), "local-pve" | "remote-pve") {
        errors.push(format!("invalid gateway topology {:?}", config.topology));
    }
    let gateway = parse_cidr(&config.lan.gateway_cidr);
    if gateway.is_none() {
        errors.push("LAN gateway CIDR must be an IPv4 prefix".into());
    }
    let start = config.dhcp.range_start.parse::<Ipv4Addr>().ok();
    let end = config.dhcp.range_end.parse::<Ipv4Addr>().ok();
    match (gateway, start, end) {
        (Some((network, prefix)), Some(start), Some(end))
            if in_prefix(start, network, prefix)
                && in_prefix(end, network, prefix)
                && u32::from(start) <= u32::from(end) => {}
        (_, Some(_), Some(_)) => {
            errors.push("DHCP range must be inside LAN gateway CIDR and ordered".into());
        }
        _ => errors.push("DHCP range must contain IPv4 addresses".into()),
    }
    if humantime::parse_duration(&config.dhcp.lease_time).is_err() {
        errors.push("invalid DHCP lease time".into());
    }
    for reservation in &config.dhcp.reservations {
        if !valid_mac(&reservation.mac) {
            errors.push(format!(
                "invalid DHCP reservation MAC {:?}",
                reservation.mac
            ));
        }
        if reservation.ip.parse::<Ipv4Addr>().is_err() {
            errors.push(format!("invalid DHCP reservation IP {:?}", reservation.ip));
        }
    }
    validate_dns(&config.dns, &mut errors);
    validate_host_fields(config, &mut errors);
    errors
}

pub(crate) fn validate_dns(config: &DnsConfig, errors: &mut Vec<String>) {
    if config.listen_port == 0 {
        errors.push("DNS listen port must be between 1 and 65535".into());
    }
    for host in &config.listen_hosts {
        if host.parse::<Ipv4Addr>().is_err() {
            errors.push(format!("DNS listen host {host:?} must be an IPv4 address"));
        }
    }
    for upstream in config
        .local_upstreams
        .iter()
        .chain([&config.remote_upstream])
    {
        if !valid_upstream(upstream) {
            errors.push(format!("DNS upstream {upstream:?} must be host:port"));
        }
    }
    if !matches!(
        config.strategy.as_str(),
        "local-first-classify" | "rules-first"
    ) {
        errors.push(format!("invalid DNS strategy {:?}", config.strategy));
    }
    for cidr in &config.domestic_cidrs {
        if parse_cidr(cidr).is_none() {
            errors.push(format!("domestic CIDR {cidr:?} must be an IPv4 prefix"));
        }
    }
    for rule_set in &config.rule_sets {
        if rule_set.id.trim().is_empty() || rule_set.name.trim().is_empty() {
            errors.push("DNS rule sets require ID and name".into());
        }
        if !rule_set.upstream.is_empty()
            && !matches!(rule_set.upstream.as_str(), "local" | "remote")
            && !valid_upstream(&rule_set.upstream)
        {
            errors.push(format!(
                "DNS upstream {:?} must be host:port",
                rule_set.upstream
            ));
        }
        if !rule_set.kind.is_empty() && !matches!(rule_set.kind.as_str(), "inline" | "url") {
            errors.push(format!("unsupported DNS rule set type {:?}", rule_set.kind));
        }
    }
}

fn validate_host_fields(config: &Config, errors: &mut Vec<String>) {
    let interface = Regex::new(r"^[A-Za-z0-9_.:-]*$").expect("static interface pattern");
    if !interface.is_match(&config.lan.interface) || !interface.is_match(&config.lan.wan_interface)
    {
        errors.push("LAN and WAN interfaces contain unsupported characters".into());
    }
    let ssh_name = Regex::new(r"^[A-Za-z0-9_.:@-]*$").expect("static SSH name pattern");
    if config.pve.host.starts_with('-')
        || config.pve.user.starts_with('-')
        || !ssh_name.is_match(&config.pve.host)
        || !ssh_name.is_match(&config.pve.user)
    {
        errors.push("PVE host and user contain unsupported characters".into());
    }
}

fn parse_cidr(value: &str) -> Option<(Ipv4Addr, u8)> {
    let (address, prefix) = value.split_once('/')?;
    let address = address.parse().ok()?;
    let prefix = prefix.parse().ok()?;
    (prefix <= 32).then_some((address, prefix))
}

fn in_prefix(address: Ipv4Addr, gateway: Ipv4Addr, prefix: u8) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    u32::from(address) & mask == u32::from(gateway) & mask
}

fn valid_mac(value: &str) -> bool {
    let parts: Vec<_> = value.split([':', '-']).collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|part| part.len() == 2 && u8::from_str_radix(part, 16).is_ok())
}

fn valid_upstream(value: &str) -> bool {
    let value = value.trim();
    if let Some((host, port)) = value
        .strip_prefix('[')
        .and_then(|value| value.split_once("]:"))
    {
        return !host.is_empty() && valid_port(port);
    }
    value
        .rsplit_once(':')
        .is_some_and(|(host, port)| !host.is_empty() && !host.contains(':') && valid_port(port))
}

fn valid_port(value: &str) -> bool {
    value.parse::<u16>().is_ok_and(|port| port != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_validate_and_unsafe_interfaces_are_rejected() {
        Config::default().validate().expect("defaults");
        let mut config = Config::default();
        config.lan.interface = "eth0; reboot".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn explicit_empty_domestic_cidrs_are_preserved() {
        let mut config = Config::default();
        config.dns.domestic_cidrs.clear();
        config.normalize();
        assert!(config.dns.domestic_cidrs.is_empty());
    }
}
