mod clash;
mod singbox;
mod v2ray;

use serde_json::Value;

use crate::{CompileError, Profile, Proxy, Target};

pub(super) fn sing_box(
    profile: &Profile,
    proxies: &[Proxy],
    target: &Target,
) -> Result<Value, CompileError> {
    let shared = SharedDns::resolve(&profile.dns);
    shared.validate(profile, target)?;
    Ok(singbox::render(profile, proxies, target, &shared))
}

pub(super) fn sing_box_system_inbounds(profile: &Profile, target: &Target) -> Vec<Value> {
    singbox::system_inbounds(profile, target, &SharedDns::resolve(&profile.dns))
}

pub(super) fn sing_box_system_route_rules(profile: &Profile, target: &Target) -> Vec<Value> {
    singbox::system_route_rules(profile, target, &SharedDns::resolve(&profile.dns))
}

pub(super) fn sing_box_fakeip_route_addresses(profile: &Profile, target: &Target) -> Vec<String> {
    let shared = SharedDns::resolve(&profile.dns);
    if managed_frontend(&shared, target) && shared.fakeip_enabled() {
        vec![shared.fakeip_ipv4_range, shared.fakeip_ipv6_range]
    } else {
        Vec::new()
    }
}

pub(super) fn sing_box_route_policy(profile: &Profile) -> (Vec<Value>, Option<Value>) {
    singbox::route_policy(profile)
}

pub(super) fn strip_fakeip(config: &mut Value) {
    singbox::strip_fakeip(config);
}

pub(super) fn apply_sing_box_platform_policy(
    profile: &Profile,
    target: &Target,
    config: &mut Value,
    warnings: &mut Vec<String>,
) {
    if target.platform != "macos" || managed_frontend(&SharedDns::resolve(&profile.dns), target) {
        return;
    }
    strip_fakeip(&mut config["dns"]);
    if profile
        .dns
        .pointer("/shared/fakeipEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        warnings.push("FakeIP is unavailable for standalone sing-box on macOS without system DNS integration; using the compatible real-IP mode".into());
    }
}

fn managed_frontend(shared: &SharedDns, target: &Target) -> bool {
    matches!(target.platform.as_str(), "macos" | "windows") && shared.system_takeover()
}

pub(super) fn clash(profile: &Profile, target: &Target, final_group: &str) -> Option<Value> {
    clash::render(
        profile,
        target,
        final_group,
        &SharedDns::resolve(&profile.dns),
    )
}

pub(super) fn v2ray(profile: &Profile, proxies: &[Proxy], target: &Target) -> Value {
    native_override(&profile.dns, &target.core)
        .unwrap_or_else(|| v2ray::render(proxies, &SharedDns::resolve(&profile.dns)))
}

pub(super) fn remote_address(profile: &Profile) -> String {
    SharedDns::resolve(&profile.dns).remote_dns
}

fn native_override(config: &Value, key: &str) -> Option<Value> {
    if config
        .pointer(&format!("/modes/{key}"))
        .and_then(Value::as_str)
        != Some("native")
    {
        return None;
    }
    config.pointer(&format!("/overrides/{key}")).cloned()
}

fn string(value: &Value, key: &str, fallback: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

fn boolean(value: &Value, key: &str, fallback: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}

fn integer(value: &Value, key: &str, fallback: u64) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(fallback)
}

#[derive(Debug, Clone)]
pub(super) struct SharedDns {
    local_dns: String,
    local_transport: String,
    local_port: u64,
    local_server_name: String,
    bootstrap_dns: String,
    bootstrap_port: u64,
    bootstrap_server_name: String,
    remote_dns: String,
    remote_port: u64,
    remote_server_name: String,
    remote_detour: String,
    fakeip_ipv4_range: String,
    fakeip_ipv6_range: String,
    fakeip_ttl: u64,
    system_dns_listen_port: u64,
    system_dns_listen_hosts: Vec<String>,
    flags: DnsFlags,
    cn_domain_rule_set: RuleSet,
    cn_ip_rule_set: RuleSet,
    hk_ip_rule_set: RuleSet,
}

#[derive(Debug, Clone)]
pub(super) struct RuleSet {
    enabled: bool,
    url: String,
    detour: String,
}

#[derive(Debug, Clone, Copy)]
struct DnsFlags(u8);

impl DnsFlags {
    const FAKE_IP: u8 = 1;
    const REJECT_HTTPS: u8 = 1 << 1;
    const CN_DOMAIN_LOCAL: u8 = 1 << 2;
    const CN_IP_LOCAL: u8 = 1 << 3;
    const EXCLUDE_HK: u8 = 1 << 4;
    const PREFER_IPV4: u8 = 1 << 5;
    const SYSTEM_TAKEOVER: u8 = 1 << 6;

    fn resolve(shared: &Value) -> Self {
        let mut bits = 0;
        for (flag, key, fallback) in [
            (Self::FAKE_IP, "fakeipEnabled", true),
            (Self::REJECT_HTTPS, "rejectHttps", true),
            (Self::CN_DOMAIN_LOCAL, "cnDomainLocalDns", true),
            (Self::CN_IP_LOCAL, "cnIpLocalDns", true),
            (Self::EXCLUDE_HK, "excludeHkFromCnIp", true),
            (Self::PREFER_IPV4, "preferIpv4", true),
            (Self::SYSTEM_TAKEOVER, "systemDnsTakeoverEnabled", false),
        ] {
            if boolean(shared, key, fallback) {
                bits |= flag;
            }
        }
        Self(bits)
    }

    const fn has(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}

impl SharedDns {
    fn resolve(config: &Value) -> Self {
        let shared = config
            .get("shared")
            .filter(|value| value.is_object())
            .unwrap_or(config);
        let local_dns = string(shared, "localDns", "223.5.5.5");
        let local_transport = shared
            .get("localDnsTransport")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map_or_else(
                || {
                    if local_dns.eq_ignore_ascii_case("local") {
                        "system"
                    } else {
                        "udp"
                    }
                    .into()
                },
                str::to_owned,
            );
        Self {
            local_dns,
            local_transport,
            local_port: integer(shared, "localDnsPort", 53),
            local_server_name: string(shared, "localServerName", ""),
            bootstrap_dns: string(shared, "bootstrapDns", "223.5.5.5"),
            bootstrap_port: integer(shared, "bootstrapDnsPort", 853),
            bootstrap_server_name: string(shared, "bootstrapServerName", "dns.alidns.com"),
            remote_dns: string(shared, "remoteDns", "8.8.8.8"),
            remote_port: integer(shared, "remoteDnsPort", 853),
            remote_server_name: string(shared, "remoteServerName", "dns.google"),
            remote_detour: string(shared, "remoteDetour", ""),
            fakeip_ipv4_range: string(shared, "fakeipIpv4Range", "198.18.0.0/15"),
            fakeip_ipv6_range: string(shared, "fakeipIpv6Range", "fc00::/18"),
            fakeip_ttl: integer(shared, "fakeipTtl", 300),
            system_dns_listen_port: integer(shared, "systemDnsListenPort", 53),
            system_dns_listen_hosts: system_dns_hosts(shared),
            flags: DnsFlags::resolve(shared),
            cn_domain_rule_set: RuleSet::resolve(
                shared,
                "cnDomain",
                "https://cdn.jsdelivr.net/gh/SagerNet/sing-geosite@rule-set/geosite-cn.srs",
            ),
            cn_ip_rule_set: RuleSet::resolve(
                shared,
                "cnIp",
                "https://cdn.jsdelivr.net/gh/SagerNet/sing-geoip@rule-set/geoip-cn.srs",
            ),
            hk_ip_rule_set: RuleSet::resolve(
                shared,
                "hkIp",
                "https://cdn.jsdelivr.net/gh/SagerNet/sing-geoip@rule-set/geoip-hk.srs",
            ),
        }
    }

    fn validate(&self, profile: &Profile, target: &Target) -> Result<(), CompileError> {
        if !matches!(self.local_transport.as_str(), "system" | "udp" | "tls") {
            return Err(CompileError::Render(format!(
                "unsupported local DNS transport {:?}",
                self.local_transport
            )));
        }
        for (name, rule_set) in [
            ("CN domain", &self.cn_domain_rule_set),
            ("CN IP", &self.cn_ip_rule_set),
            ("HK IP", &self.hk_ip_rule_set),
        ] {
            if rule_set.enabled && rule_set.url.trim().is_empty() {
                return Err(CompileError::Render(format!(
                    "{name} DNS rule-set URL is required when enabled"
                )));
            }
        }
        if self.system_takeover() {
            let frontend = matches!(target.platform.as_str(), "macos" | "windows");
            if target.platform != "default" && !frontend {
                return Err(CompileError::Render(
                    "system DNS takeover is only available for Linux system or managed desktop sing-box runtime".into(),
                ));
            }
            if frontend && target.version == "11" {
                return Err(CompileError::Render(
                    "managed desktop DNS frontend requires sing-box 1.12 or newer".into(),
                ));
            }
            if self.system_dns_listen_port != 53 {
                return Err(CompileError::Render(
                    "system DNS takeover requires listen port 53 because resolv.conf cannot specify ports".into(),
                ));
            }
            if !self
                .system_dns_listen_hosts
                .iter()
                .any(|host| matches!(host.as_str(), "127.0.0.1" | "0.0.0.0"))
            {
                return Err(CompileError::Render(
                    "system DNS takeover listen hosts must include 127.0.0.1 or 0.0.0.0".into(),
                ));
            }
            if self.local_server().1 {
                return Err(CompileError::Render(
                    "system DNS takeover requires an explicit local DNS upstream instead of local"
                        .into(),
                ));
            }
            let managed_port = u64::from(profile.local_proxy.socks_port);
            let ports = [
                managed_port,
                u64::from(profile.local_proxy.http_port),
                u64::from(profile.transparent_proxy.tproxy.listen_port),
                u64::from(profile.transparent_proxy.tproxy.dns_listen_port),
            ];
            if ports.contains(&self.system_dns_listen_port) {
                return Err(CompileError::Render(format!(
                    "system DNS takeover port {} conflicts with another managed listener",
                    self.system_dns_listen_port
                )));
            }
        }
        Ok(())
    }

    fn local_server(&self) -> (&str, bool) {
        let server = self
            .local_dns
            .split(',')
            .map(str::trim)
            .find(|value| !value.is_empty())
            .unwrap_or("local");
        (
            server,
            self.local_transport == "system" || server.eq_ignore_ascii_case("local"),
        )
    }

    const fn fakeip_enabled(&self) -> bool {
        self.flags.has(DnsFlags::FAKE_IP)
    }
    const fn reject_https(&self) -> bool {
        self.flags.has(DnsFlags::REJECT_HTTPS)
    }
    const fn cn_domain_local_dns(&self) -> bool {
        self.flags.has(DnsFlags::CN_DOMAIN_LOCAL)
    }
    const fn cn_ip_local_dns(&self) -> bool {
        self.flags.has(DnsFlags::CN_IP_LOCAL)
    }
    const fn exclude_hk_from_cn_ip(&self) -> bool {
        self.flags.has(DnsFlags::EXCLUDE_HK)
    }
    const fn prefer_ipv4(&self) -> bool {
        self.flags.has(DnsFlags::PREFER_IPV4)
    }
    const fn system_takeover(&self) -> bool {
        self.flags.has(DnsFlags::SYSTEM_TAKEOVER)
    }
}

fn system_dns_hosts(shared: &Value) -> Vec<String> {
    let values = shared
        .get("systemDnsListenHosts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str);
    let mut hosts = Vec::new();
    for host in values.filter_map(|host| host.trim().parse::<std::net::Ipv4Addr>().ok()) {
        let host = host.to_string();
        if host == "0.0.0.0" {
            return vec![host];
        }
        if !hosts.contains(&host) {
            hosts.push(host);
        }
    }
    if hosts.is_empty() {
        hosts.push("127.0.0.1".into());
    }
    hosts
}

impl RuleSet {
    fn resolve(shared: &Value, prefix: &str, fallback_url: &str) -> Self {
        let enabled_key = format!("{prefix}RuleSetEnabled");
        let url_key = format!("{prefix}RuleSetUrl");
        let detour_key = format!("{prefix}RuleSetDetour");
        Self {
            enabled: boolean(shared, &enabled_key, true),
            url: string(shared, &url_key, fallback_url),
            detour: string(shared, &detour_key, "direct"),
        }
    }
}
