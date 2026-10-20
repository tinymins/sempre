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

#[cfg(any(target_os = "linux", target_os = "macos"))]
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
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{DefaultInterface, NetworkError, warm_neighbor};

    pub fn detect() -> Result<DefaultInterface, NetworkError> {
        let route = crate::windows::default_route()?;
        let adapter = crate::windows::adapters()?
            .into_iter()
            .find(|adapter| adapter.index == route.index)
            .ok_or_else(|| std::io::Error::other("default route interface was not found"))?;
        let mut addresses = adapter.addresses;
        addresses.sort();
        warm_neighbor(&route.gateway);
        let gateway_mac =
            crate::windows::neighbor_mac(route.index, &route.gateway).unwrap_or_default();
        Ok(DefaultInterface {
            supported: true,
            name: adapter.name,
            addresses,
            gateway: route.gateway,
            gateway_mac,
        })
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
