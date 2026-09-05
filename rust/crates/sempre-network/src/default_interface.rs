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

fn interface(name: &str) -> DefaultInterface {
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
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{DefaultInterface, NetworkError, interface};

    pub fn detect() -> Result<DefaultInterface, NetworkError> {
        let inventory = crate::inventory()?;
        Ok(if inventory.default_interface.is_empty() {
            DefaultInterface {
                supported: inventory.supported,
                ..DefaultInterface::default()
            }
        } else {
            interface(&inventory.default_interface)
        })
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::{io, process::Command};

    use super::{DefaultInterface, NetworkError, interface};

    pub fn detect() -> Result<DefaultInterface, NetworkError> {
        let output = Command::new("/sbin/route")
            .args(["-n", "get", "default"])
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other("route -n get default failed").into());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let name = parse_route(&stdout)
            .ok_or_else(|| io::Error::other("default route has no interface"))?;
        Ok(interface(name))
    }

    fn parse_route(output: &str) -> Option<&str> {
        output.lines().find_map(|line| {
            let (key, value) = line.trim().split_once(':')?;
            (key == "interface").then_some(value.trim())
        })
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn parses_default_route_interface() {
            assert_eq!(
                super::parse_route("   route to: default\ninterface: en0\n"),
                Some("en0")
            );
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::{io, net::IpAddr, process::Command};

    use sysinfo::Networks;

    use super::{DefaultInterface, NetworkError};

    pub fn detect() -> Result<DefaultInterface, NetworkError> {
        let output = Command::new("route.exe").args(["PRINT", "-4"]).output()?;
        if !output.status.success() {
            return Err(io::Error::other("route PRINT -4 failed").into());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let address = parse_route(&stdout)
            .ok_or_else(|| io::Error::other("default route has no interface address"))?;
        let networks = Networks::new_with_refreshed_list();
        let Some((name, data)) = networks.iter().find(|(_, data)| {
            data.ip_networks()
                .iter()
                .any(|network| network.addr == address)
        }) else {
            return Err(io::Error::other("default route interface was not found").into());
        };
        let mut addresses: Vec<String> =
            data.ip_networks().iter().map(ToString::to_string).collect();
        addresses.sort();
        Ok(DefaultInterface {
            supported: true,
            name: name.clone(),
            addresses,
        })
    }

    fn parse_route(output: &str) -> Option<IpAddr> {
        output
            .lines()
            .filter_map(|line| {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                if fields.first() != Some(&"0.0.0.0") || fields.get(1) != Some(&"0.0.0.0") {
                    return None;
                }
                let address = fields.get(3)?.parse().ok()?;
                let metric = fields.get(4)?.parse::<u32>().ok()?;
                Some((metric, address))
            })
            .min_by_key(|(metric, _)| *metric)
            .map(|(_, address)| address)
    }

    #[cfg(test)]
    mod tests {
        use std::net::{IpAddr, Ipv4Addr};

        #[test]
        fn parses_lowest_metric_default_route() {
            let output =
                "0.0.0.0 0.0.0.0 10.0.0.1 10.0.0.5 35\n0.0.0.0 0.0.0.0 10.1.0.1 10.1.0.5 25\n";
            assert_eq!(
                super::parse_route(output),
                Some(IpAddr::V4(Ipv4Addr::new(10, 1, 0, 5)))
            );
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
