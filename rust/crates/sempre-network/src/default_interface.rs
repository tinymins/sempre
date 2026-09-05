use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::NetworkError;

const CACHE_TTL: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct DefaultInterface {
    pub supported: bool,
    pub name: String,
    pub addresses: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub gateway: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub gateway_mac: String,
}

pub fn default_interface() -> Result<DefaultInterface, NetworkError> {
    static CACHE: OnceLock<Mutex<Option<(Instant, DefaultInterface)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(cached) = cache.lock()
        && let Some((checked_at, value)) = cached.as_ref()
        && checked_at.elapsed() < CACHE_TTL
    {
        return Ok(value.clone());
    }
    let value = platform::detect()?;
    if let Ok(mut cached) = cache.lock() {
        *cached = Some((Instant::now(), value.clone()));
    }
    Ok(value)
}

fn interface(name: &str, gateway: &str, gateway_mac: &str) -> DefaultInterface {
    use sysinfo::Networks;

    let networks = Networks::new_with_refreshed_list();
    let mut addresses: Vec<String> = networks
        .get(name)
        .map(|data| data.ip_networks().iter().map(ToString::to_string).collect())
        .unwrap_or_default();
    addresses.sort();
    DefaultInterface {
        supported: true,
        name: name.into(),
        addresses,
        gateway: gateway.into(),
        gateway_mac: gateway_mac.into(),
    }
}

pub fn normalize_mac(value: &str) -> Option<String> {
    let separated = value.trim().split([':', '-']).collect::<Vec<_>>();
    if separated.len() == 6
        && separated.iter().all(|part| {
            !part.is_empty() && part.len() <= 2 && part.chars().all(|c| c.is_ascii_hexdigit())
        })
    {
        let normalized = separated
            .iter()
            .map(|part| {
                u8::from_str_radix(part, 16)
                    .ok()
                    .map(|byte| format!("{byte:02x}"))
            })
            .collect::<Option<Vec<_>>>()?
            .join(":");
        return (!matches!(
            normalized.as_str(),
            "00:00:00:00:00:00" | "ff:ff:ff:ff:ff:ff"
        ))
        .then_some(normalized);
    }
    let compact = value
        .trim()
        .chars()
        .filter(|character| !matches!(character, ':' | '-'))
        .collect::<String>()
        .to_ascii_lowercase();
    if compact.len() != 12
        || !compact
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return None;
    }
    let normalized = (0..6)
        .map(|index| &compact[index * 2..index * 2 + 2])
        .collect::<Vec<_>>()
        .join(":");
    (!matches!(
        normalized.as_str(),
        "00:00:00:00:00:00" | "ff:ff:ff:ff:ff:ff"
    ))
    .then_some(normalized)
}

fn warm_neighbor(gateway: &str) {
    use std::net::UdpSocket;

    if gateway.is_empty() {
        return;
    }
    let bind = if gateway.contains(':') {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    };
    if let Ok(socket) = UdpSocket::bind(bind) {
        let destination = if gateway.contains(':') {
            format!("[{gateway}]:9")
        } else {
            format!("{gateway}:9")
        };
        if socket.connect(destination).is_ok() {
            let _ = socket.send(&[0]);
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::{fs, net::Ipv4Addr};

    use super::{DefaultInterface, NetworkError, interface, normalize_mac, warm_neighbor};

    pub fn detect() -> Result<DefaultInterface, NetworkError> {
        let inventory = crate::inventory()?;
        Ok(if inventory.default_interface.is_empty() {
            DefaultInterface {
                supported: inventory.supported,
                ..DefaultInterface::default()
            }
        } else {
            let gateway = default_gateway(&fs::read_to_string("/proc/net/route")?)
                .map_or_else(String::new, |value| value.to_string());
            warm_neighbor(&gateway);
            let gateway_mac = fs::read_to_string("/proc/net/arp")
                .ok()
                .and_then(|data| arp_mac(&data, &gateway))
                .unwrap_or_default();
            interface(&inventory.default_interface, &gateway, &gateway_mac)
        })
    }

    fn default_gateway(data: &str) -> Option<Ipv4Addr> {
        data.lines().skip(1).find_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.get(1) != Some(&"00000000") {
                return None;
            }
            let raw = u32::from_str_radix(fields.get(2)?, 16).ok()?;
            Some(Ipv4Addr::from(raw.to_le_bytes()))
        })
    }

    fn arp_mac(data: &str, gateway: &str) -> Option<String> {
        data.lines().skip(1).find_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            (fields.first() == Some(&gateway))
                .then(|| normalize_mac(fields.get(3)?))
                .flatten()
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn normalizes_cross_platform_mac_formats() {
        assert_eq!(
            super::normalize_mac("A0-B1-C2-D3-E4-F5"),
            Some("a0:b1:c2:d3:e4:f5".into())
        );
        assert_eq!(
            super::normalize_mac("10:8f:fe:6b:a0:7"),
            Some("10:8f:fe:6b:a0:07".into())
        );
        assert!(super::normalize_mac("00:00:00:00:00:00").is_none());
        assert!(super::normalize_mac("not-a-mac").is_none());
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::{io, process::Command};

    use super::{DefaultInterface, NetworkError, interface, normalize_mac, warm_neighbor};

    pub fn detect() -> Result<DefaultInterface, NetworkError> {
        let output = Command::new("/sbin/route")
            .args(["-n", "get", "default"])
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other("route -n get default failed").into());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let (name, gateway) = parse_route(&stdout)
            .ok_or_else(|| io::Error::other("default route has no interface or gateway"))?;
        warm_neighbor(gateway);
        let gateway_mac = Command::new("/usr/sbin/arp")
            .args(["-n", gateway])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| parse_arp(&String::from_utf8_lossy(&output.stdout)))
            .unwrap_or_default();
        Ok(interface(name, gateway, &gateway_mac))
    }

    fn parse_route(output: &str) -> Option<(&str, &str)> {
        let value = |target| {
            output.lines().find_map(|line| {
                let (key, value) = line.trim().split_once(':')?;
                (key == target).then_some(value.trim())
            })
        };
        Some((value("interface")?, value("gateway")?))
    }

    fn parse_arp(output: &str) -> Option<String> {
        output.split_whitespace().find_map(normalize_mac)
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn parses_default_route_interface() {
            assert_eq!(
                super::parse_route("route to: default\ngateway: 10.8.28.1\ninterface: en0\n"),
                Some(("en0", "10.8.28.1"))
            );
            assert_eq!(
                super::parse_arp("? (10.8.28.1) at a0:b1:c2:d3:e4:f5 on en0"),
                Some("a0:b1:c2:d3:e4:f5".into())
            );
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::{io, net::IpAddr, process::Command};

    use sysinfo::Networks;

    use super::{DefaultInterface, NetworkError, normalize_mac, warm_neighbor};

    pub fn detect() -> Result<DefaultInterface, NetworkError> {
        let output = Command::new("route.exe").args(["PRINT", "-4"]).output()?;
        if !output.status.success() {
            return Err(io::Error::other("route PRINT -4 failed").into());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let route = parse_route(&stdout)
            .ok_or_else(|| io::Error::other("default route has no interface address"))?;
        let networks = Networks::new_with_refreshed_list();
        let Some((name, data)) = networks.iter().find(|(_, data)| {
            data.ip_networks()
                .iter()
                .any(|network| network.addr == route.address)
        }) else {
            return Err(io::Error::other("default route interface was not found").into());
        };
        let mut addresses: Vec<String> =
            data.ip_networks().iter().map(ToString::to_string).collect();
        addresses.sort();
        warm_neighbor(&route.gateway.to_string());
        let gateway_mac = Command::new("arp.exe")
            .args(["-a", &route.gateway.to_string()])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| {
                parse_arp(
                    &String::from_utf8_lossy(&output.stdout),
                    &route.gateway.to_string(),
                )
            })
            .unwrap_or_default();
        Ok(DefaultInterface {
            supported: true,
            name: name.clone(),
            addresses,
            gateway: route.gateway.to_string(),
            gateway_mac,
        })
    }

    #[derive(Debug, Eq, PartialEq)]
    struct Route {
        gateway: IpAddr,
        address: IpAddr,
    }

    fn parse_route(output: &str) -> Option<Route> {
        output
            .lines()
            .filter_map(|line| {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                if fields.first() != Some(&"0.0.0.0") || fields.get(1) != Some(&"0.0.0.0") {
                    return None;
                }
                let gateway = fields.get(2)?.parse().ok()?;
                let address = fields.get(3)?.parse().ok()?;
                let metric = fields.get(4)?.parse::<u32>().ok()?;
                Some((metric, Route { gateway, address }))
            })
            .min_by_key(|(metric, _)| *metric)
            .map(|(_, route)| route)
    }

    fn parse_arp(output: &str, gateway: &str) -> Option<String> {
        output.lines().find_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            (fields.first() == Some(&gateway))
                .then(|| fields.get(1).and_then(|value| normalize_mac(value)))
                .flatten()
        })
    }

    #[cfg(test)]
    mod tests {
        use std::net::{IpAddr, Ipv4Addr};

        #[test]
        fn parses_lowest_metric_default_route() {
            let output =
                "0.0.0.0 0.0.0.0 10.0.0.1 10.0.0.5 35\n0.0.0.0 0.0.0.0 10.1.0.1 10.1.0.5 25\n";
            let route = super::parse_route(output).expect("route");
            assert_eq!(route.gateway, IpAddr::V4(Ipv4Addr::new(10, 1, 0, 1)));
            assert_eq!(route.address, IpAddr::V4(Ipv4Addr::new(10, 1, 0, 5)));
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    use super::{DefaultInterface, NetworkError};

    #[allow(clippy::unnecessary_wraps)]
    pub fn detect() -> Result<DefaultInterface, NetworkError> {
        Ok(DefaultInterface::default())
    }
}
